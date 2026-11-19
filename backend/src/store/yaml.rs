//! YAML backend (ADR-0001): one directory per user with `tasks.yaml`, `tags.yaml` and
//! `settings.yaml`, meant to be read by people. Times are written as local time with the offset
//! of the user's time zone.
//!
//! All data of a user lives in memory after the first access. A change is applied to a copy,
//! the copy is written atomically (temporary file, fsync, rename), and only then replaces the
//! data in memory: a failed write or a rejected operation leaves files and memory unchanged.
//! The files are small, so they are written synchronously while the lock is held.

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use jiff::civil::Time;

use crate::domain::{
    CSV_ROUNDINGS, Change, ChangeReason, DEFAULT_FORGOTTEN_AFTER_HOURS, DEFAULT_WORKDAY_END,
    DomainError, Entry, EntrySource, Settings, Tag, Task, TaskStatus, UserData, WeekStart,
};

const FORMAT_VERSION: u32 = 1;
const TASKS_HEADER: &str = "# Written by clockopus. Times are local with UTC offset (RFC 3339).\n# Edit by hand only while clockopus is stopped.\n";
const TAGS_HEADER: &str = "# Written by clockopus.\n";
const SETTINGS_HEADER: &str = "# Written by clockopus.\n";

#[derive(Debug)]
pub enum StoreError {
    Domain(DomainError),
    Io(anyhow::Error),
}

impl From<DomainError> for StoreError {
    fn from(e: DomainError) -> Self {
        StoreError::Domain(e)
    }
}

pub struct YamlStore {
    dir: PathBuf,
    users: Mutex<HashMap<String, UserData>>,
}

impl YamlStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            users: Mutex::new(HashMap::new()),
        }
    }

    fn user_dir(&self, user: &str) -> anyhow::Result<PathBuf> {
        // User keys become directory names; never let one escape the data directory.
        let valid = !user.is_empty()
            && user.len() <= 64
            && user
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            bail!("invalid user key {user:?}");
        }
        Ok(self.dir.join(user))
    }

    pub async fn read<R>(
        &self,
        user: &str,
        f: impl FnOnce(&UserData) -> R,
    ) -> Result<R, StoreError> {
        let mut users = self.users.lock().await;
        let data = self.loaded(&mut users, user)?;
        Ok(f(data))
    }

    pub async fn write<R>(
        &self,
        user: &str,
        f: impl FnOnce(&mut UserData) -> Result<R, DomainError>,
    ) -> Result<R, StoreError> {
        let mut users = self.users.lock().await;
        let mut copy = self.loaded(&mut users, user)?.clone();
        let result = f(&mut copy)?;
        save(&self.user_dir(user).map_err(StoreError::Io)?, &copy).map_err(StoreError::Io)?;
        users.insert(user.to_owned(), copy);
        Ok(result)
    }

    /// Removes all data of a user: files and memory (`DELETE /me`).
    pub async fn delete_user(&self, user: &str) -> Result<(), StoreError> {
        let dir = self.user_dir(user).map_err(StoreError::Io)?;
        let mut users = self.users.lock().await;
        match fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(StoreError::Io(
                    anyhow::Error::new(e).context(format!("cannot delete {}", dir.display())),
                ));
            }
        }
        users.remove(user);
        Ok(())
    }

    fn loaded<'a>(
        &self,
        users: &'a mut HashMap<String, UserData>,
        user: &str,
    ) -> Result<&'a mut UserData, StoreError> {
        if !users.contains_key(user) {
            let data =
                load(&self.user_dir(user).map_err(StoreError::Io)?).map_err(StoreError::Io)?;
            users.insert(user.to_owned(), data);
        }
        Ok(users.get_mut(user).expect("inserted above"))
    }
}

// ---------------------------------------------------------------------------------------------
// File format
// ---------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TasksFile {
    version: u32,
    #[serde(default)]
    tasks: Vec<TaskDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskDto {
    id: Uuid,
    title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link: Option<String>,
    /// Several lines; missing in files from before 0.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes_changed_at: Option<String>,
    status: TaskStatus,
    #[serde(default)]
    pinned: bool,
    created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_used_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entries: Vec<EntryDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDto {
    id: Uuid,
    start: String,
    /// `null` while the timer runs.
    end: Option<String>,
    source: EntrySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    keep_running: bool,
    /// Previous values, oldest first (R6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<ChangeDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeDto {
    changed_at: String,
    reason: ChangeReason,
    start: String,
    end: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TagsFile {
    version: u32,
    #[serde(default)]
    tags: Vec<TagDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TagDto {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    /// `HH:MM`; missing in files from before M3, then the default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workday_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    forgotten_after_hours: Option<u8>,
    /// Missing in files from before M4, then the default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    week_start: Option<WeekStart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    csv_rounding: Option<u8>,
}

impl SettingsFile {
    fn empty() -> Self {
        Self {
            version: FORMAT_VERSION,
            timezone: None,
            workday_end: None,
            forgotten_after_hours: None,
            week_start: None,
            csv_rounding: None,
        }
    }
}

/// `GET /export` and `POST /import`: everything of one user in one readable YAML document,
/// in the same shapes as the files in the data directory.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportFile {
    version: u32,
    exported_at: String,
    settings: SettingsFile,
    #[serde(default)]
    tags: Vec<TagDto>,
    #[serde(default)]
    tasks: Vec<TaskDto>,
}

const EXPORT_HEADER: &str = "# clockopus export. Import it into an empty account (Settings → Import).\n# Times are local with UTC offset (RFC 3339).\n";

pub fn export_yaml(data: &UserData, now: Timestamp) -> anyhow::Result<String> {
    let (tasks, tags, settings) = to_files(data);
    let tz = data.settings.time_zone();
    let file = ExportFile {
        version: FORMAT_VERSION,
        exported_at: local(now, &tz),
        settings,
        tags: tags.tags,
        tasks: tasks.tasks,
    };
    let body = serde_saphyr::to_string(&file).context("cannot serialise the export")?;
    Ok(format!("{EXPORT_HEADER}{body}"))
}

/// Reads an export. Problems are the uploader's, so they come back as `import_invalid`.
pub fn import_yaml(text: &str) -> Result<UserData, DomainError> {
    let bad = |message: String| DomainError::Invalid {
        code: "import_invalid",
        message,
    };
    let file: ExportFile = serde_saphyr::from_str(text)
        .map_err(|e| bad(format!("This is not a clockopus export: {e}")))?;
    if file.version != FORMAT_VERSION {
        return Err(bad(format!(
            "Export format version {} is not supported (expected {FORMAT_VERSION}).",
            file.version
        )));
    }
    let data = from_files(
        TasksFile {
            version: FORMAT_VERSION,
            tasks: file.tasks,
        },
        TagsFile {
            version: FORMAT_VERSION,
            tags: file.tags,
        },
        file.settings,
    )
    .map_err(|e| bad(format!("{e:#}")))?;
    data.validate()?;
    Ok(data)
}

fn local(ts: Timestamp, tz: &TimeZone) -> String {
    ts.display_with_offset(tz.to_offset(ts)).to_string()
}

fn parse_ts(s: &str) -> anyhow::Result<Timestamp> {
    s.parse()
        .with_context(|| format!("`{s}` is not a time with offset (RFC 3339)"))
}

fn to_files(data: &UserData) -> (TasksFile, TagsFile, SettingsFile) {
    let tz = data.settings.time_zone();
    let tasks = data
        .tasks
        .iter()
        .map(|t| TaskDto {
            id: t.id,
            title: t.title.clone(),
            tags: t.tags.clone(),
            link: t.link.clone(),
            notes: t.notes.clone(),
            notes_changed_at: t.notes_changed_at.map(|ts| local(ts, &tz)),
            status: t.status,
            pinned: t.pinned,
            created_at: local(t.created_at, &tz),
            last_used_at: t.last_used_at.map(|ts| local(ts, &tz)),
            entries: t
                .entries
                .iter()
                .map(|e| EntryDto {
                    id: e.id,
                    start: local(e.start, &tz),
                    end: e.end.map(|ts| local(ts, &tz)),
                    source: e.source,
                    note: e.note.clone(),
                    keep_running: e.keep_running,
                    history: e
                        .history
                        .iter()
                        .map(|c| ChangeDto {
                            changed_at: local(c.changed_at, &tz),
                            reason: c.reason,
                            start: local(c.start, &tz),
                            end: c.end.map(|ts| local(ts, &tz)),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let tags = data
        .tags
        .iter()
        .map(|t| TagDto {
            name: t.name.clone(),
            color: t.color.clone(),
        })
        .collect();
    (
        TasksFile {
            version: FORMAT_VERSION,
            tasks,
        },
        TagsFile {
            version: FORMAT_VERSION,
            tags,
        },
        SettingsFile {
            version: FORMAT_VERSION,
            timezone: data.settings.timezone.clone(),
            workday_end: Some(data.settings.workday_end.strftime("%H:%M").to_string()),
            forgotten_after_hours: Some(data.settings.forgotten_after_hours),
            week_start: Some(data.settings.week_start),
            csv_rounding: Some(data.settings.csv_rounding),
        },
    )
}

fn from_files(
    tasks: TasksFile,
    tags: TagsFile,
    settings: SettingsFile,
) -> anyhow::Result<UserData> {
    for (name, version) in [
        ("tasks.yaml", tasks.version),
        ("tags.yaml", tags.version),
        ("settings.yaml", settings.version),
    ] {
        if version != FORMAT_VERSION {
            bail!("{name} has format version {version}, this clockopus reads {FORMAT_VERSION}");
        }
    }
    let tasks = tasks
        .tasks
        .into_iter()
        .map(|t| {
            Ok(Task {
                id: t.id,
                title: t.title,
                tags: t.tags,
                link: t.link,
                notes: t.notes,
                notes_changed_at: t.notes_changed_at.as_deref().map(parse_ts).transpose()?,
                status: t.status,
                pinned: t.pinned,
                created_at: parse_ts(&t.created_at)?,
                last_used_at: t.last_used_at.as_deref().map(parse_ts).transpose()?,
                entries: t
                    .entries
                    .into_iter()
                    .map(|e| {
                        Ok(Entry {
                            id: e.id,
                            start: parse_ts(&e.start)?,
                            end: e.end.as_deref().map(parse_ts).transpose()?,
                            source: e.source,
                            note: e.note,
                            keep_running: e.keep_running,
                            history: e
                                .history
                                .into_iter()
                                .map(|c| {
                                    Ok(Change {
                                        changed_at: parse_ts(&c.changed_at)?,
                                        reason: c.reason,
                                        start: parse_ts(&c.start)?,
                                        end: c.end.as_deref().map(parse_ts).transpose()?,
                                    })
                                })
                                .collect::<anyhow::Result<_>>()?,
                        })
                    })
                    .collect::<anyhow::Result<_>>()?,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    let mut data = UserData {
        settings: Settings {
            timezone: settings.timezone,
            workday_end: match settings.workday_end.as_deref() {
                Some(t) => t
                    .parse::<Time>()
                    .with_context(|| format!("settings.yaml: workday_end `{t}` is not HH:MM"))?,
                None => DEFAULT_WORKDAY_END,
            },
            forgotten_after_hours: settings
                .forgotten_after_hours
                .unwrap_or(DEFAULT_FORGOTTEN_AFTER_HOURS),
            week_start: settings.week_start.unwrap_or_default(),
            csv_rounding: match settings.csv_rounding {
                Some(r) if CSV_ROUNDINGS.contains(&r) => r,
                Some(r) => bail!("settings.yaml: csv_rounding {r} is not 0, 5 or 15"),
                None => 0,
            },
        },
        tasks,
        tags: tags
            .tags
            .into_iter()
            .map(|t| Tag {
                name: t.name,
                color: t.color,
            })
            .collect(),
    };
    // Files (and exports) from before 0.2 may hold mixed-case tags; the next write stores them
    // in lower case.
    data.lowercase_tags();
    Ok(data)
}

// ---------------------------------------------------------------------------------------------
// Disk access
// ---------------------------------------------------------------------------------------------

fn read_file<T: for<'de> Deserialize<'de>>(path: &Path, empty: T) -> anyhow::Result<T> {
    match fs::read_to_string(path) {
        Ok(text) => {
            serde_saphyr::from_str(&text).with_context(|| format!("cannot read {}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(empty),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn load(dir: &Path) -> anyhow::Result<UserData> {
    let tasks = read_file(
        &dir.join("tasks.yaml"),
        TasksFile {
            version: FORMAT_VERSION,
            tasks: vec![],
        },
    )?;
    let tags = read_file(
        &dir.join("tags.yaml"),
        TagsFile {
            version: FORMAT_VERSION,
            tags: vec![],
        },
    )?;
    let settings = read_file(&dir.join("settings.yaml"), SettingsFile::empty())?;
    from_files(tasks, tags, settings).with_context(|| format!("invalid data in {}", dir.display()))
}

fn write_atomically(path: &Path, header: &str, value: &impl Serialize) -> anyhow::Result<()> {
    let body = serde_saphyr::to_string(value).context("cannot serialise YAML")?;
    let tmp = path.with_extension("yaml.tmp");
    {
        let mut file =
            fs::File::create(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
        file.write_all(header.as_bytes())?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

fn save(dir: &Path, data: &UserData) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let (tasks, tags, settings) = to_files(data);
    // Tags first: a task never refers to a tag that is not written yet.
    write_atomically(&dir.join("tags.yaml"), TAGS_HEADER, &tags)?;
    write_atomically(&dir.join("settings.yaml"), SETTINGS_HEADER, &settings)?;
    write_atomically(&dir.join("tasks.yaml"), TASKS_HEADER, &tasks)?;
    // Make the renames durable.
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewTask;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    async fn sample(store: &YamlStore) -> (Uuid, Uuid) {
        store
            .write("local", |d| {
                d.set_timezone("Europe/Berlin")?;
                let a = d.create_task(
                    NewTask {
                        title: "Fix login bug".into(),
                        tags: vec!["support".into(), "#customer-x".into()],
                        link: Some("https://jira.example.com/browse/ABC-123".into()),
                        start: false,
                    },
                    ts("2026-11-17T08:12:00Z"),
                )?;
                let b = d.create_task(
                    NewTask {
                        title: "Review".into(),
                        ..Default::default()
                    },
                    ts("2026-11-17T08:13:00Z"),
                )?;
                d.start_task(a, ts("2026-11-17T08:12:00Z"))?;
                d.start_task(b, ts("2026-11-17T09:40:00Z"))?;
                Ok((a, b))
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn files_are_readable_local_time_with_offset() {
        let dir = tempfile::tempdir().unwrap();
        let store = YamlStore::new(dir.path());
        let (a, _) = sample(&store).await;

        let tasks = fs::read_to_string(dir.path().join("local/tasks.yaml")).unwrap();
        assert!(tasks.starts_with("# Written by clockopus."), "{tasks}");
        assert!(tasks.contains(&format!("id: {a}")), "{tasks}");
        assert!(tasks.contains("title: Fix login bug"), "{tasks}");
        // 08:12 UTC is 09:12 in Berlin in November.
        assert!(
            tasks.contains(r#"start: "2026-11-17T09:12:00+01:00""#),
            "{tasks}"
        );
        assert!(
            tasks.contains(r#"end: "2026-11-17T10:40:00+01:00""#),
            "{tasks}"
        );
        assert!(tasks.contains("end: null"), "running entry: {tasks}");
        assert!(tasks.contains("source: timer"), "{tasks}");
        assert!(!tasks.contains("Z\n"), "no UTC timestamps: {tasks}");

        let tags = fs::read_to_string(dir.path().join("local/tags.yaml")).unwrap();
        assert!(tags.contains("name: support"), "{tags}");
        assert!(tags.contains("name: customer-x"), "{tags}");
        let settings = fs::read_to_string(dir.path().join("local/settings.yaml")).unwrap();
        assert!(settings.contains("timezone: Europe/Berlin"), "{settings}");
    }

    #[tokio::test]
    async fn data_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let before = {
            let store = YamlStore::new(dir.path());
            sample(&store).await;
            store.read("local", Clone::clone).await.unwrap()
        };
        let after = YamlStore::new(dir.path())
            .read("local", Clone::clone)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(after.tasks.len(), 2);
        assert!(after.running().is_some());
    }

    #[tokio::test]
    async fn a_new_user_starts_empty_without_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = YamlStore::new(dir.path());
        let data = store.read("local", Clone::clone).await.unwrap();
        assert_eq!(data, UserData::default());
        assert!(!dir.path().join("local").exists(), "reading writes nothing");
    }

    #[tokio::test]
    async fn a_rejected_operation_changes_neither_memory_nor_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = YamlStore::new(dir.path());
        sample(&store).await;
        let file = dir.path().join("local/tasks.yaml");
        let before_file = fs::read_to_string(&file).unwrap();
        let before = store.read("local", Clone::clone).await.unwrap();

        let err = store
            .write("local", |d| {
                // First a valid change, then a failing one: neither may stay.
                d.create_task(
                    NewTask {
                        title: "half done".into(),
                        ..Default::default()
                    },
                    ts("2026-11-17T10:00:00Z"),
                )?;
                d.create_task(NewTask::default(), ts("2026-11-17T10:00:00Z"))
            })
            .await;
        assert!(matches!(err, Err(StoreError::Domain(_))));
        assert_eq!(store.read("local", Clone::clone).await.unwrap(), before);
        assert_eq!(fs::read_to_string(&file).unwrap(), before_file);
    }

    #[tokio::test]
    async fn a_broken_file_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("local")).unwrap();
        let file = dir.path().join("local/tasks.yaml");
        fs::write(&file, "version: 1\ntasks: [ this is: not valid").unwrap();

        let store = YamlStore::new(dir.path());
        let err = store
            .write("local", |d| {
                d.create_task(
                    NewTask {
                        title: "x".into(),
                        ..Default::default()
                    },
                    ts("2026-11-17T10:00:00Z"),
                )
            })
            .await;
        match err {
            Err(StoreError::Io(e)) => assert!(format!("{e:#}").contains("tasks.yaml"), "{e:#}"),
            other => panic!("expected an I/O error, got {other:?}"),
        }
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "version: 1\ntasks: [ this is: not valid"
        );
    }

    #[tokio::test]
    async fn unknown_format_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("local")).unwrap();
        fs::write(dir.path().join("local/tags.yaml"), "version: 2\ntags: []\n").unwrap();
        let err = YamlStore::new(dir.path()).read("local", |_| ()).await;
        match err {
            Err(StoreError::Io(e)) => assert!(format!("{e:#}").contains("version 2"), "{e:#}"),
            other => panic!("expected a version error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_file_written_by_hand_is_read() {
        // The example from docs/SPEC.md, section 6.1, minus the history (M3).
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("local")).unwrap();
        fs::write(
            dir.path().join("local/tasks.yaml"),
            "version: 1
tasks:
  - id: 0193a1c2-7d40-7b1e-9c3a-2f1d5e6a8b90
    title: Fix login bug
    tags: [support, customer-x]
    link: https://jira.example.com/browse/ABC-123
    status: active
    pinned: false
    created_at: 2026-11-17T09:12:00+01:00
    entries:
      - id: 0193a1c2-7d41-7c00-8d2e-1a2b3c4d5e6f
        start: 2026-11-17T09:12:00+01:00
        end: 2026-11-17T10:40:00+01:00
        source: timer
",
        )
        .unwrap();
        let data = YamlStore::new(dir.path())
            .read("local", Clone::clone)
            .await
            .unwrap();
        assert_eq!(data.tasks[0].tags, vec!["support", "customer-x"]);
        assert_eq!(data.tasks[0].entries[0].start, ts("2026-11-17T08:12:00Z"));
        assert_eq!(
            data.tasks[0].entries[0].end,
            Some(ts("2026-11-17T09:40:00Z"))
        );
    }

    #[tokio::test]
    async fn user_keys_cannot_escape_the_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = YamlStore::new(dir.path());
        for bad in ["../x", "a/b", "", ".", "x y"] {
            assert!(
                matches!(store.read(bad, |_| ()).await, Err(StoreError::Io(_))),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn history_settings_and_keep_running_survive_a_restart() {
        use crate::corrections::EntryPatch;
        use crate::domain::ChangeReason;
        let dir = tempfile::tempdir().unwrap();
        let before = {
            let store = YamlStore::new(dir.path());
            let (a, _) = sample(&store).await;
            store
                .write("local", |d| {
                    let first = d.tasks.iter().find(|t| t.id == a).unwrap().entries[0].id;
                    d.update_entry(
                        first,
                        EntryPatch {
                            end: Some(ts("2026-11-17T09:00:00Z")),
                            note: Some("shorter".into()),
                            ..Default::default()
                        },
                        ChangeReason::Edit,
                        ts("2026-11-18T08:00:00Z"),
                    )?;
                    d.keep_running()?;
                    d.set_workday(Some(jiff::civil::Time::constant(16, 30, 0, 0)), Some(8))
                })
                .await
                .unwrap();
            store.read("local", Clone::clone).await.unwrap()
        };
        let tasks = fs::read_to_string(dir.path().join("local/tasks.yaml")).unwrap();
        assert!(tasks.contains("reason: edit"), "{tasks}");
        assert!(tasks.contains("keep_running: true"), "{tasks}");
        assert!(tasks.contains("note: shorter"), "{tasks}");
        let settings = fs::read_to_string(dir.path().join("local/settings.yaml")).unwrap();
        assert!(
            settings.contains("workday_end: 16:30") || settings.contains("workday_end: \"16:30\""),
            "{settings}"
        );
        assert!(settings.contains("forgotten_after_hours: 8"), "{settings}");

        let after = YamlStore::new(dir.path())
            .read("local", Clone::clone)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(after.tasks[0].entries[0].history.len(), 1);
    }

    #[tokio::test]
    async fn mixed_case_tags_from_older_files_are_lowered() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("local")).unwrap();
        fs::write(
            dir.path().join("local/tags.yaml"),
            "version: 1
tags:
  - name: Support
  - name: Team-A
",
        )
        .unwrap();
        fs::write(
            dir.path().join("local/tasks.yaml"),
            r#"version: 1
tasks:
  - id: 0193a1c2-7d40-7b1e-9c3a-2f1d5e6a8b90
    title: Fix
    tags: [Support, Team-A]
    status: active
    created_at: "2026-11-17T09:12:00+01:00"
"#,
        )
        .unwrap();
        let data = YamlStore::new(dir.path())
            .read("local", Clone::clone)
            .await
            .unwrap();
        assert_eq!(data.tasks[0].tags, vec!["support", "team-a"]);
        let names: Vec<_> = data.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["support", "team-a"]);
    }
}
