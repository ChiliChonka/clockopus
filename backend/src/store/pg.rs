//! PostgreSQL backend (ADR-0001). Same contract as the YAML backend: an operation runs on one
//! user's [`UserData`] and the result is persisted, or nothing changes.
//!
//! - **Reads** check the user's `version` (one small query) and use the cached data while it is
//!   current; another replica's change bumps the version and causes a reload.
//! - **Writes** lock the user's row (`SELECT … FOR UPDATE`), so changes of one user are serialised
//!   across replicas, apply the operation to a copy and write only what differs.
//! - Timestamps cross the boundary as epoch seconds (`to_timestamp` / `extract(epoch …)`); the
//!   domain keeps whole seconds anyway.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, anyhow};
use jiff::Timestamp;
use jiff::civil::Time;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnection, PgPoolOptions};
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{
    Change, ChangeReason, DomainError, Entry, EntrySource, Settings, Tag, Task, TaskStatus,
    UserData, WeekStart,
};
use crate::store::StoreError;

pub struct PgStore {
    pool: PgPool,
    cache: Mutex<HashMap<String, (i64, UserData)>>,
}

fn io(e: impl Into<anyhow::Error>) -> StoreError {
    StoreError::Io(e.into())
}

impl PgStore {
    /// Connects and applies the migrations in `backend/migrations` (embedded at build time).
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .context("cannot connect to PostgreSQL")?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("database migration failed")?;
        Ok(Self {
            pool,
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub async fn ready(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }

    pub async fn read<R>(
        &self,
        user: &str,
        f: impl FnOnce(&UserData) -> R,
    ) -> Result<R, StoreError> {
        let version: Option<i64> = sqlx::query_scalar("SELECT version FROM users WHERE key = $1")
            .bind(user)
            .fetch_optional(&self.pool)
            .await
            .map_err(io)?;
        let Some(version) = version else {
            // Reading creates nothing, like the YAML backend.
            return Ok(f(&UserData::default()));
        };
        if let Some((v, data)) = self.cache.lock().await.get(user)
            && *v == version
        {
            return Ok(f(data));
        }
        let mut conn = self.pool.acquire().await.map_err(io)?;
        let data = load(&mut conn, user).await.map_err(io)?;
        let result = f(&data);
        self.cache
            .lock()
            .await
            .insert(user.to_owned(), (version, data));
        Ok(result)
    }

    pub async fn write<R>(
        &self,
        user: &str,
        f: impl FnOnce(&mut UserData) -> Result<R, DomainError>,
    ) -> Result<R, StoreError> {
        let mut tx = self.pool.begin().await.map_err(io)?;
        sqlx::query("INSERT INTO users (key) VALUES ($1) ON CONFLICT (key) DO NOTHING")
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(io)?;
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM users WHERE key = $1 FOR UPDATE")
                .bind(user)
                .fetch_one(&mut *tx)
                .await
                .map_err(io)?;
        let cached = self
            .cache
            .lock()
            .await
            .get(user)
            .filter(|(v, _)| *v == version)
            .map(|(_, d)| d.clone());
        let old = match cached {
            Some(d) => d,
            None => load(&mut tx, user).await.map_err(io)?,
        };
        let mut new = old.clone();
        // A rejected operation drops the transaction: nothing is written, not even the user row.
        let result = f(&mut new)?;
        persist(&mut tx, user, &old, &new).await.map_err(io)?;
        sqlx::query("UPDATE users SET version = version + 1 WHERE key = $1")
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(io)?;
        tx.commit().await.map_err(io)?;
        self.cache
            .lock()
            .await
            .insert(user.to_owned(), (version + 1, new));
        Ok(result)
    }

    pub async fn delete_user(&self, user: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM users WHERE key = $1")
            .bind(user)
            .execute(&self.pool)
            .await
            .map_err(io)?;
        self.cache.lock().await.remove(user);
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct HistoryRow {
    changed_at: i64,
    reason: ChangeReason,
    start: i64,
    end: Option<i64>,
}

fn ts(seconds: i64) -> anyhow::Result<Timestamp> {
    Timestamp::from_second(seconds).map_err(|e| anyhow!("timestamp out of range: {e}"))
}

fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Active => "active",
        TaskStatus::Done => "done",
    }
}

fn source_str(s: EntrySource) -> &'static str {
    match s {
        EntrySource::Timer => "timer",
        EntrySource::Manual => "manual",
    }
}

fn week_str(w: WeekStart) -> &'static str {
    match w {
        WeekStart::Monday => "monday",
        WeekStart::Sunday => "sunday",
    }
}

async fn load(conn: &mut PgConnection, user: &str) -> anyhow::Result<UserData> {
    let row = sqlx::query(
        "SELECT timezone, workday_end, forgotten_after_hours, week_start, csv_rounding
         FROM users WHERE key = $1",
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?;
    let workday_end: String = row.get("workday_end");
    let settings = Settings {
        timezone: row.get("timezone"),
        workday_end: workday_end
            .parse::<Time>()
            .with_context(|| format!("workday_end `{workday_end}`"))?,
        forgotten_after_hours: u8::try_from(row.get::<i16, _>("forgotten_after_hours"))?,
        week_start: match row.get::<String, _>("week_start").as_str() {
            "sunday" => WeekStart::Sunday,
            _ => WeekStart::Monday,
        },
        csv_rounding: u8::try_from(row.get::<i16, _>("csv_rounding"))?,
    };

    let tags = sqlx::query("SELECT name, color FROM tags WHERE user_key = $1 ORDER BY position")
        .bind(user)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| Tag {
            name: r.get("name"),
            color: r.get("color"),
        })
        .collect();

    let mut entries: HashMap<Uuid, Vec<Entry>> = HashMap::new();
    for r in sqlx::query(
        "SELECT id, task_id, extract(epoch FROM start_at)::bigint AS start_s,
                extract(epoch FROM end_at)::bigint AS end_s, source, note, keep_running, history
         FROM entries WHERE user_key = $1 ORDER BY start_at, id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?
    {
        let history: Vec<HistoryRow> = serde_json::from_value(r.get("history"))?;
        let entry = Entry {
            id: r.get("id"),
            start: ts(r.get("start_s"))?,
            end: r.get::<Option<i64>, _>("end_s").map(ts).transpose()?,
            source: match r.get::<String, _>("source").as_str() {
                "manual" => EntrySource::Manual,
                _ => EntrySource::Timer,
            },
            note: r.get("note"),
            history: history
                .into_iter()
                .map(|h| {
                    Ok(Change {
                        changed_at: ts(h.changed_at)?,
                        reason: h.reason,
                        start: ts(h.start)?,
                        end: h.end.map(ts).transpose()?,
                    })
                })
                .collect::<anyhow::Result<_>>()?,
            keep_running: r.get("keep_running"),
        };
        entries.entry(r.get("task_id")).or_default().push(entry);
    }

    let tasks = sqlx::query(
        "SELECT id, title, tags, link, status, pinned,
                extract(epoch FROM created_at)::bigint AS created_s,
                extract(epoch FROM last_used_at)::bigint AS used_s
         FROM tasks WHERE user_key = $1 ORDER BY created_at, id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| {
        let id: Uuid = r.get("id");
        Ok(Task {
            id,
            title: r.get("title"),
            tags: r.get("tags"),
            link: r.get("link"),
            status: match r.get::<String, _>("status").as_str() {
                "done" => TaskStatus::Done,
                _ => TaskStatus::Active,
            },
            pinned: r.get("pinned"),
            created_at: ts(r.get("created_s"))?,
            last_used_at: r.get::<Option<i64>, _>("used_s").map(ts).transpose()?,
            entries: entries.remove(&id).unwrap_or_default(),
        })
    })
    .collect::<anyhow::Result<_>>()?;

    Ok(UserData {
        settings,
        tasks,
        tags,
    })
}

/// Writes the difference between `old` and `new`: deletes first, then updates, then inserts, so
/// a switch (one entry stops, another starts) never has two running entries at once.
async fn persist(
    conn: &mut PgConnection,
    user: &str,
    old: &UserData,
    new: &UserData,
) -> anyhow::Result<()> {
    if old.settings != new.settings {
        let s = &new.settings;
        sqlx::query(
            "UPDATE users SET timezone = $2, workday_end = $3, forgotten_after_hours = $4,
                              week_start = $5, csv_rounding = $6
             WHERE key = $1",
        )
        .bind(user)
        .bind(&s.timezone)
        .bind(s.workday_end.strftime("%H:%M").to_string())
        .bind(i16::from(s.forgotten_after_hours))
        .bind(week_str(s.week_start))
        .bind(i16::from(s.csv_rounding))
        .execute(&mut *conn)
        .await?;
    }

    if old.tags != new.tags {
        sqlx::query("DELETE FROM tags WHERE user_key = $1")
            .bind(user)
            .execute(&mut *conn)
            .await?;
        for (i, tag) in new.tags.iter().enumerate() {
            sqlx::query(
                "INSERT INTO tags (user_key, name, color, position) VALUES ($1, $2, $3, $4)",
            )
            .bind(user)
            .bind(&tag.name)
            .bind(&tag.color)
            .bind(i32::try_from(i)?)
            .execute(&mut *conn)
            .await?;
        }
    }

    let old_tasks: HashMap<Uuid, &Task> = old.tasks.iter().map(|t| (t.id, t)).collect();
    let new_ids: HashSet<Uuid> = new.tasks.iter().map(|t| t.id).collect();
    let removed: Vec<Uuid> = old_tasks
        .keys()
        .filter(|id| !new_ids.contains(id))
        .copied()
        .collect();
    if !removed.is_empty() {
        sqlx::query("DELETE FROM tasks WHERE user_key = $1 AND id = ANY($2)")
            .bind(user)
            .bind(&removed)
            .execute(&mut *conn)
            .await?;
    }

    let old_entries: HashMap<Uuid, (Uuid, &Entry)> = old
        .tasks
        .iter()
        .flat_map(|t| t.entries.iter().map(move |e| (e.id, (t.id, e))))
        .collect();
    let new_entries: HashMap<Uuid, (Uuid, &Entry)> = new
        .tasks
        .iter()
        .flat_map(|t| t.entries.iter().map(move |e| (e.id, (t.id, e))))
        .collect();
    let gone: Vec<Uuid> = old_entries
        .keys()
        .filter(|id| !new_entries.contains_key(id))
        .copied()
        .collect();
    if !gone.is_empty() {
        sqlx::query("DELETE FROM entries WHERE user_key = $1 AND id = ANY($2)")
            .bind(user)
            .bind(&gone)
            .execute(&mut *conn)
            .await?;
    }

    // Tasks before entries (foreign key).
    for task in &new.tasks {
        let unchanged = old_tasks.get(&task.id).is_some_and(|o| {
            o.title == task.title
                && o.tags == task.tags
                && o.link == task.link
                && o.status == task.status
                && o.pinned == task.pinned
                && o.created_at == task.created_at
                && o.last_used_at == task.last_used_at
        });
        if unchanged {
            continue;
        }
        // The WHERE keeps another user's row untouched if an id collides (e.g. an import).
        let affected = sqlx::query(
            "INSERT INTO tasks (id, user_key, title, tags, link, status, pinned, created_at, last_used_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8), to_timestamp($9))
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, tags = EXCLUDED.tags,
                 link = EXCLUDED.link, status = EXCLUDED.status, pinned = EXCLUDED.pinned,
                 created_at = EXCLUDED.created_at, last_used_at = EXCLUDED.last_used_at
             WHERE tasks.user_key = EXCLUDED.user_key",
        )
        .bind(task.id)
        .bind(user)
        .bind(&task.title)
        .bind(&task.tags)
        .bind(&task.link)
        .bind(status_str(task.status))
        .bind(task.pinned)
        .bind(task.created_at.as_second())
        .bind(task.last_used_at.map(|t| t.as_second()))
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if affected != 1 {
            anyhow::bail!("task id {} is already used by another account", task.id);
        }
    }

    // Updates (changed entries) before inserts (new ones).
    let mut inserts = Vec::new();
    for (id, (task_id, entry)) in &new_entries {
        match old_entries.get(id) {
            Some((old_task, old_entry)) if old_task == task_id && *old_entry == *entry => {}
            Some(_) => write_entry(conn, user, *task_id, entry, true).await?,
            None => inserts.push((*task_id, *entry)),
        }
    }
    for (task_id, entry) in inserts {
        write_entry(conn, user, task_id, entry, false).await?;
    }
    Ok(())
}

async fn write_entry(
    conn: &mut PgConnection,
    user: &str,
    task_id: Uuid,
    e: &Entry,
    update: bool,
) -> anyhow::Result<()> {
    let history = serde_json::to_value(
        e.history
            .iter()
            .map(|c| HistoryRow {
                changed_at: c.changed_at.as_second(),
                reason: c.reason,
                start: c.start.as_second(),
                end: c.end.map(|t| t.as_second()),
            })
            .collect::<Vec<_>>(),
    )?;
    let sql = if update {
        "UPDATE entries SET task_id = $2, start_at = to_timestamp($4), end_at = to_timestamp($5),
                source = $6, note = $7, keep_running = $8, history = $9
         WHERE id = $1 AND user_key = $3"
    } else {
        "INSERT INTO entries (id, task_id, user_key, start_at, end_at, source, note, keep_running, history)
         VALUES ($1, $2, $3, to_timestamp($4), to_timestamp($5), $6, $7, $8, $9)
         ON CONFLICT (id) DO NOTHING"
    };
    let affected = sqlx::query(sql)
        .bind(e.id)
        .bind(task_id)
        .bind(user)
        .bind(e.start.as_second())
        .bind(e.end.map(|t| t.as_second()))
        .bind(source_str(e.source))
        .bind(&e.note)
        .bind(e.keep_running)
        .bind(history)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if affected != 1 {
        anyhow::bail!("entry id {} is already used by another account", e.id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_entry(
        pool: &PgPool,
        task: Uuid,
        user: &str,
        start: &str,
        end: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO entries (id, task_id, user_key, start_at, end_at, source)
             VALUES ($1, $2, $3, $4::timestamptz, $5::timestamptz, 'manual')",
        )
        .bind(Uuid::now_v7())
        .bind(task)
        .bind(user)
        .bind(start)
        .bind(end)
        .execute(pool)
        .await
        .map(|_| ())
    }

    /// The database rejects what the domain would never write: a backstop for bugs and for
    /// anyone writing to the tables directly (R1, R4, R5).
    #[tokio::test]
    async fn the_database_enforces_r1_r4_r5_on_its_own() {
        let url = std::env::var("CLOCKOPUS_TEST_DATABASE_URL").expect(
            "CLOCKOPUS_TEST_DATABASE_URL is not set; start a test database with `task db:test`",
        );
        let store = PgStore::connect(&url).await.unwrap();
        let pool = &store.pool;
        let user = format!("t{}", Uuid::now_v7().simple());
        let task = Uuid::now_v7();
        sqlx::query("INSERT INTO users (key) VALUES ($1)")
            .bind(&user)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_key, title, status, created_at)
             VALUES ($1, $2, 'A', 'active', now())",
        )
        .bind(task)
        .bind(&user)
        .execute(pool)
        .await
        .unwrap();
        insert_entry(
            pool,
            task,
            &user,
            "2026-11-16T08:00:00Z",
            Some("2026-11-16T09:00:00Z"),
        )
        .await
        .unwrap();

        let mut refused = 0;
        // R5 (checked at commit; a single statement commits at once).
        let err = insert_entry(
            pool,
            task,
            &user,
            "2026-11-16T08:30:00Z",
            Some("2026-11-16T09:30:00Z"),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("entries_no_overlap"), "{err}");
        refused += 1;
        // R4
        let err = insert_entry(
            pool,
            task,
            &user,
            "2026-11-16T10:00:00Z",
            Some("2026-11-16T09:00:00Z"),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("entries_check"), "{err}");
        refused += 1;
        // R1: a second running entry.
        insert_entry(pool, task, &user, "2026-11-16T12:00:00Z", None)
            .await
            .unwrap();
        let err = insert_entry(pool, task, &user, "2026-11-16T11:00:00Z", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("entries_one_running"), "{err}");
        refused += 1;
        assert_eq!(refused, 3);
        store.delete_user(&user).await.unwrap();
    }
}
