//! Contract tests: every scenario runs against the YAML and the PostgreSQL backend.
//!
//! PostgreSQL comes from `CLOCKOPUS_TEST_DATABASE_URL` (`task db:test` starts one in Docker; CI has
//! a service container). Without it the PostgreSQL half **fails** instead of being skipped: a
//! skipped contract test would be green without saying anything.

use jiff::Timestamp;
use uuid::Uuid;

use super::{PgStore, Store, StoreError, YamlStore};
use crate::corrections::EntryPatch;
use crate::domain::{ChangeReason, DomainError, NewTask, StatusFilter, TaskPatch, WeekStart};

/// Where a backend keeps its data, so a second instance (another replica, or the server after a
/// restart) can be opened on the same data.
enum Backend {
    Yaml(tempfile::TempDir),
    Postgres(String),
}

impl Backend {
    fn yaml() -> Self {
        Backend::Yaml(tempfile::tempdir().unwrap())
    }

    fn postgres() -> Self {
        Backend::Postgres(std::env::var("CLOCKOPUS_TEST_DATABASE_URL").expect(
            "CLOCKOPUS_TEST_DATABASE_URL is not set; start a test database with `task db:test`",
        ))
    }

    async fn open(&self) -> Store {
        match self {
            Backend::Yaml(dir) => Store::Yaml(YamlStore::new(dir.path())),
            Backend::Postgres(url) => Store::Postgres(PgStore::connect(url).await.unwrap()),
        }
    }
}

macro_rules! both {
    ($($scenario:ident),* $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn yaml() {
                    super::$scenario(&super::Backend::yaml()).await;
                }

                #[tokio::test]
                async fn postgres() {
                    super::$scenario(&super::Backend::postgres()).await;
                }
            }
        )*
    };
}

both!(
    create_start_switch_stop_and_reopen,
    rejected_operations_change_nothing,
    corrections_history_and_settings_round_trip,
    deleting_and_reading_unknown_users,
);

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

/// A fresh user per test, so tests share one database without seeing each other.
fn user() -> String {
    format!("t{}", Uuid::now_v7().simple())
}

fn task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        ..Default::default()
    }
}

async fn create_start_switch_stop_and_reopen(backend: &Backend) {
    let store = backend.open().await;
    let u = user();
    let (a, b) = store
        .write(&u, |d| {
            d.set_timezone("Europe/Berlin")?;
            let a = d.create_task(
                NewTask {
                    title: "Review".into(),
                    tags: vec!["review".into(), "Team-A".into()],
                    start: true,
                    ..Default::default()
                },
                ts("2026-11-16T08:00:00Z"),
            )?;
            let b = d.create_task(task("Mail"), ts("2026-11-16T08:00:00Z"))?;
            Ok((a, b))
        })
        .await
        .unwrap();
    store
        .write(&u, |d| d.start_task(b, ts("2026-11-16T09:30:00Z")))
        .await
        .unwrap();
    store
        .write(&u, |d| d.stop_timer(None, ts("2026-11-16T10:00:00Z")))
        .await
        .unwrap();

    // Another instance sees exactly the same data.
    let seen = backend.open().await.read(&u, Clone::clone).await.unwrap();
    assert_eq!(seen, store.read(&u, Clone::clone).await.unwrap());
    let totals: Vec<(uuid::Uuid, i64)> = seen
        .task_views(StatusFilter::All, ts("2026-11-16T12:00:00Z"))
        .iter()
        .map(|v| (v.id, v.total_seconds))
        .collect();
    assert_eq!(totals.len(), 2);
    assert!(totals.contains(&(a, 90 * 60)), "{totals:?}");
    assert!(totals.contains(&(b, 30 * 60)), "{totals:?}");
    assert_eq!(seen.running(), None);
    let review = seen.tasks.iter().find(|t| t.id == a).unwrap();
    // Tags are stored in lower case (0.2).
    assert_eq!(review.tags, vec!["review", "team-a"]);
    assert_eq!(seen.settings.timezone.as_deref(), Some("Europe/Berlin"));
}

async fn rejected_operations_change_nothing(backend: &Backend) {
    let store = backend.open().await;
    let u = user();
    let a = store
        .write(&u, |d| d.create_task(task("A"), ts("2026-11-16T08:00:00Z")))
        .await
        .unwrap();
    let before = store.read(&u, Clone::clone).await.unwrap();
    let now = ts("2026-11-16T10:00:00Z");
    let err = store
        .write(&u, |d| {
            let pin = TaskPatch {
                pinned: Some(true),
                ..Default::default()
            };
            d.update_task(a, pin, now)?;
            // An R5 violation after a valid change: neither may stay.
            d.add_entry(
                a,
                ts("2026-11-16T08:00:00Z"),
                ts("2026-11-16T09:00:00Z"),
                None,
                now,
            )?;
            d.add_entry(
                a,
                ts("2026-11-16T08:30:00Z"),
                ts("2026-11-16T09:30:00Z"),
                None,
                now,
            )
        })
        .await;
    assert!(
        matches!(err, Err(StoreError::Domain(DomainError::Conflict { .. }))),
        "{err:?}"
    );
    assert_eq!(store.read(&u, Clone::clone).await.unwrap(), before);
    assert_eq!(
        backend.open().await.read(&u, Clone::clone).await.unwrap(),
        before
    );
}

async fn corrections_history_and_settings_round_trip(backend: &Backend) {
    let store = backend.open().await;
    let u = user();
    let now = ts("2026-11-16T12:00:00Z");
    store
        .write(&u, |d| {
            let a = d.create_task(
                NewTask {
                    title: "Fix".into(),
                    link: Some("https://jira.example.com/ABC-1".into()),
                    ..Default::default()
                },
                ts("2026-11-16T07:00:00Z"),
            )?;
            let e = d.add_entry(
                a,
                ts("2026-11-16T08:00:00Z"),
                ts("2026-11-16T09:00:00Z"),
                Some("call"),
                now,
            )?;
            let longer = EntryPatch {
                end: Some(ts("2026-11-16T09:30:00Z")),
                ..Default::default()
            };
            d.update_entry(e, longer, ChangeReason::Edit, now)?;
            d.start_task(a, ts("2026-11-16T10:00:00Z"))?;
            d.keep_running()?;
            d.set_workday(Some(jiff::civil::Time::constant(16, 30, 0, 0)), Some(8))?;
            d.set_preferences(Some(WeekStart::Sunday), Some(15))
        })
        .await
        .unwrap();
    let mine = store.read(&u, Clone::clone).await.unwrap();
    let theirs = backend.open().await.read(&u, Clone::clone).await.unwrap();
    assert_eq!(mine, theirs);
    let t = &theirs.tasks[0];
    assert_eq!(t.link.as_deref(), Some("https://jira.example.com/ABC-1"));
    assert_eq!(t.entries.len(), 2);
    assert_eq!(t.entries[0].history.len(), 1);
    assert_eq!(t.entries[0].note.as_deref(), Some("call"));
    assert!(t.entries[1].keep_running && t.entries[1].end.is_none());
    assert_eq!(theirs.settings.forgotten_after_hours, 8);
    assert_eq!(theirs.settings.week_start, WeekStart::Sunday);
    assert_eq!(theirs.settings.csv_rounding, 15);
    assert_eq!(t.notes, None);

    // A change of only the notes is written too (PostgreSQL writes only changed rows).
    let id = t.id;
    for (text, at) in [
        ("Agenda\n  - budget", "2026-11-16T13:00:00Z"),
        ("Agenda\n  - budget\n  - über", "2026-11-16T14:00:00Z"),
    ] {
        store
            .write(&u, |d| {
                d.update_task(
                    id,
                    TaskPatch {
                        notes: Some(text.into()),
                        ..Default::default()
                    },
                    ts(at),
                )
            })
            .await
            .unwrap();
        let fresh = backend.open().await.read(&u, Clone::clone).await.unwrap();
        assert_eq!(fresh.tasks[0].notes.as_deref(), Some(text));
        assert_eq!(fresh.tasks[0].notes_changed_at, Some(ts(at)));
    }
}

async fn deleting_and_reading_unknown_users(backend: &Backend) {
    let store = backend.open().await;
    let u = user();
    assert!(store.read(&u, |d| d.tasks.is_empty()).await.unwrap());
    store
        .write(&u, |d| d.create_task(task("x"), ts("2026-11-16T07:00:00Z")))
        .await
        .unwrap();
    // Users do not see each other.
    let other = user();
    assert!(store.read(&other, |d| d.tasks.is_empty()).await.unwrap());
    store.delete_user(&u).await.unwrap();
    assert!(store.read(&u, |d| d.tasks.is_empty()).await.unwrap());
    assert!(
        backend
            .open()
            .await
            .read(&u, |d| d.tasks.is_empty())
            .await
            .unwrap()
    );
}
