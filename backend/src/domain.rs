//! The rules of clockopus (docs/SPEC.md, section 3) as pure functions on one user's data.
//!
//! Storage backends load a [`UserData`], apply one operation and persist the result; they never
//! implement rules themselves. `now` is always passed in, so tests control time.

use std::cmp::Reverse;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_TAG_LEN: usize = 40;
pub const MAX_TAGS_PER_TASK: usize = 10;
pub const MAX_LINK_LEN: usize = 2000;
/// Tags with the most tracked time in this window fill the quick-access bar.
pub const TAG_USAGE_WINDOW: SignedDuration = SignedDuration::from_hours(30 * 24);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Active,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntrySource {
    Timer,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub tags: Vec<String>,
    pub link: Option<String>,
    pub status: TaskStatus,
    pub pinned: bool,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Uuid,
    pub start: Timestamp,
    /// `None` while the timer runs.
    pub end: Option<Timestamp>,
    pub source: EntrySource,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// IANA time zone; `None` until the browser has sent one. Days are then computed in UTC.
    pub timezone: Option<String>,
}

impl Settings {
    pub fn time_zone(&self) -> TimeZone {
        self.timezone
            .as_deref()
            .and_then(|name| TimeZone::get(name).ok())
            .unwrap_or(TimeZone::UTC)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserData {
    pub settings: Settings,
    pub tasks: Vec<Task>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Input is invalid; `code` is stable for clients, `message` is for people.
    Invalid {
        code: &'static str,
        message: String,
    },
    NotFound,
}

fn invalid(code: &'static str, message: impl Into<String>) -> DomainError {
    DomainError::Invalid {
        code,
        message: message.into(),
    }
}

/// Timestamps are kept to the second: readable in YAML, precise enough for time tracking.
pub fn truncate(ts: Timestamp) -> Timestamp {
    Timestamp::from_second(ts.as_second()).expect("a valid timestamp stays valid")
}

// ---------------------------------------------------------------------------------------------
// Input normalisation
// ---------------------------------------------------------------------------------------------

pub fn normalize_title(title: &str) -> Result<String, DomainError> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err(invalid("title_empty", "The task needs a title."));
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(invalid(
            "title_too_long",
            format!("The title can have at most {MAX_TITLE_LEN} characters."),
        ));
    }
    Ok(title)
}

/// Trims a leading `#`, rejects whitespace, removes case-insensitive duplicates (first wins).
pub fn normalize_tags(tags: &[String]) -> Result<Vec<String>, DomainError> {
    let mut out: Vec<String> = Vec::new();
    for raw in tags {
        let tag = raw.trim().trim_start_matches('#');
        if tag.is_empty() {
            continue;
        }
        if tag.chars().any(char::is_whitespace) {
            return Err(invalid(
                "tag_whitespace",
                format!("Tag `{tag}` must not contain spaces."),
            ));
        }
        if tag.chars().count() > MAX_TAG_LEN {
            return Err(invalid(
                "tag_too_long",
                format!("A tag can have at most {MAX_TAG_LEN} characters."),
            ));
        }
        // Case-insensitive, also beyond ASCII (`Über` and `über` are the same tag).
        let lower = tag.to_lowercase();
        if !out.iter().any(|t| t.to_lowercase() == lower) {
            out.push(tag.to_owned());
        }
    }
    if out.len() > MAX_TAGS_PER_TASK {
        return Err(invalid(
            "too_many_tags",
            format!("A task can have at most {MAX_TAGS_PER_TASK} tags."),
        ));
    }
    Ok(out)
}

pub fn normalize_link(link: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(link) = link.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    let scheme_ok = link.starts_with("https://") || link.starts_with("http://");
    if !scheme_ok || link.chars().any(char::is_whitespace) || link.len() > MAX_LINK_LEN {
        return Err(invalid(
            "link_invalid",
            "The link must be a web address starting with http:// or https://.",
        ));
    }
    Ok(Some(link.to_owned()))
}

pub fn validate_timezone(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    TimeZone::get(name)
        .map(|_| name.to_owned())
        .map_err(|_| invalid("timezone_unknown", format!("Unknown time zone `{name}`.")))
}

// ---------------------------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub title: String,
    pub tags: Vec<String>,
    pub link: Option<String>,
    pub start: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    /// `Some("")` removes the link.
    pub link: Option<String>,
    pub pinned: Option<bool>,
    pub status: Option<TaskStatus>,
}

/// The currently running entry, if any (R1: there is at most one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Running {
    pub task_id: Uuid,
    pub entry_id: Uuid,
    pub start: Timestamp,
}

impl UserData {
    pub fn running(&self) -> Option<Running> {
        self.tasks.iter().find_map(|task| {
            task.entries
                .iter()
                .find(|e| e.end.is_none())
                .map(|e| Running {
                    task_id: task.id,
                    entry_id: e.id,
                    start: e.start,
                })
        })
    }

    fn task_mut(&mut self, id: Uuid) -> Result<&mut Task, DomainError> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(DomainError::NotFound)
    }

    /// Uses the spelling of an existing tag (`support` typed, `Support` known → `Support`) and
    /// registers new tags.
    fn adopt_tags(&mut self, tags: Vec<String>) -> Vec<String> {
        tags.into_iter()
            .map(|tag| {
                let lower = tag.to_lowercase();
                if let Some(known) = self.tags.iter().find(|t| t.name.to_lowercase() == lower) {
                    known.name.clone()
                } else {
                    self.tags.push(Tag {
                        name: tag.clone(),
                        color: None,
                    });
                    tag
                }
            })
            .collect()
    }

    pub fn create_task(&mut self, new: NewTask, now: Timestamp) -> Result<Uuid, DomainError> {
        let now = truncate(now);
        let title = normalize_title(&new.title)?;
        let tags = normalize_tags(&new.tags)?;
        let link = normalize_link(new.link.as_deref())?;
        let tags = self.adopt_tags(tags);
        let id = Uuid::now_v7();
        self.tasks.push(Task {
            id,
            title,
            tags,
            link,
            status: TaskStatus::Active,
            pinned: false,
            created_at: now,
            last_used_at: None,
            entries: Vec::new(),
        });
        if new.start {
            self.start_task(id, now)?;
        }
        Ok(id)
    }

    /// R2: ends the running entry and starts a new one for `id` at the same instant.
    /// Starting the task that already runs changes nothing. Starting a done task reopens it.
    pub fn start_task(&mut self, id: Uuid, now: Timestamp) -> Result<Running, DomainError> {
        let now = truncate(now);
        if !self.tasks.iter().any(|t| t.id == id) {
            return Err(DomainError::NotFound);
        }
        if let Some(running) = self.running() {
            if running.task_id == id {
                return Ok(running);
            }
            self.stop_timer(None, now)?;
        }
        let task = self.task_mut(id)?;
        let entry_id = Uuid::now_v7();
        task.status = TaskStatus::Active;
        task.last_used_at = Some(now);
        task.entries.push(Entry {
            id: entry_id,
            start: now,
            end: None,
            source: EntrySource::Timer,
            note: None,
        });
        Ok(Running {
            task_id: id,
            entry_id,
            start: now,
        })
    }

    /// R3: ends the running entry at `end` (default: now). Returns the stopped entry, or `None`
    /// if nothing was running. An entry that would end at its start is removed.
    pub fn stop_timer(
        &mut self,
        end: Option<Timestamp>,
        now: Timestamp,
    ) -> Result<Option<Running>, DomainError> {
        let now = truncate(now);
        let Some(running) = self.running() else {
            return Ok(None);
        };
        let end = truncate(end.unwrap_or(now));
        if end < running.start {
            return Err(invalid(
                "end_before_start",
                "The end cannot be before the start of the running timer.",
            ));
        }
        if end > now {
            return Err(invalid("end_in_future", "The end cannot be in the future."));
        }
        let task = self.task_mut(running.task_id)?;
        if end == running.start {
            task.entries.retain(|e| e.id != running.entry_id);
        } else if let Some(entry) = task.entries.iter_mut().find(|e| e.id == running.entry_id) {
            entry.end = Some(end);
        }
        Ok(Some(running))
    }

    pub fn update_task(
        &mut self,
        id: Uuid,
        patch: TaskPatch,
        now: Timestamp,
    ) -> Result<(), DomainError> {
        // Validate everything before changing anything.
        let title = patch.title.as_deref().map(normalize_title).transpose()?;
        let tags = patch.tags.as_deref().map(normalize_tags).transpose()?;
        let link = patch
            .link
            .as_deref()
            .map(|l| normalize_link(Some(l)))
            .transpose()?;
        if !self.tasks.iter().any(|t| t.id == id) {
            return Err(DomainError::NotFound);
        }
        // A task marked done stops its running timer first.
        if patch.status == Some(TaskStatus::Done) && self.running().is_some_and(|r| r.task_id == id)
        {
            self.stop_timer(None, now)?;
        }
        let tags = tags.map(|t| self.adopt_tags(t));
        let task = self.task_mut(id)?;
        if let Some(title) = title {
            task.title = title;
        }
        if let Some(tags) = tags {
            task.tags = tags;
        }
        if let Some(link) = link {
            task.link = link;
        }
        if let Some(pinned) = patch.pinned {
            task.pinned = pinned;
        }
        if let Some(status) = patch.status {
            task.status = status;
        }
        Ok(())
    }

    /// Deletes the task with all its entries (also a running one).
    pub fn delete_task(&mut self, id: Uuid) -> Result<(), DomainError> {
        let before = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        if self.tasks.len() == before {
            return Err(DomainError::NotFound);
        }
        Ok(())
    }

    pub fn set_timezone(&mut self, name: &str) -> Result<(), DomainError> {
        self.settings.timezone = Some(validate_timezone(name)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------------------------

/// Seconds of `entry` inside `[from, to)`, a running entry counted until `now`.
pub fn seconds_within(entry: &Entry, from: Timestamp, to: Timestamp, now: Timestamp) -> i64 {
    let end = entry.end.unwrap_or(now);
    let start = entry.start.max(from);
    let end = end.min(to);
    (end.as_second() - start.as_second()).max(0)
}

/// Start of today and of tomorrow in `tz` (23 or 25 hours apart on daylight-saving days).
pub fn today(now: Timestamp, tz: &TimeZone) -> (Timestamp, Timestamp) {
    let local = now.to_zoned_datetime(tz.clone());
    let start = local.start_of_day().expect("start of today exists");
    let next = local
        .tomorrow()
        .and_then(|t| t.start_of_day())
        .expect("start of tomorrow exists");
    (start.timestamp(), next.timestamp())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskView {
    pub id: Uuid,
    pub title: String,
    pub tags: Vec<String>,
    pub link: Option<String>,
    pub status: TaskStatus,
    pub pinned: bool,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub running_since: Option<Timestamp>,
    pub today_seconds: i64,
    pub total_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    Active,
    Done,
    All,
}

impl UserData {
    /// Board order: running task, pinned tasks, then most recently used, then newest.
    pub fn task_views(&self, filter: StatusFilter, now: Timestamp) -> Vec<TaskView> {
        let (day_start, day_end) = today(now, &self.settings.time_zone());
        let mut views: Vec<TaskView> = self
            .tasks
            .iter()
            .filter(|t| match filter {
                StatusFilter::Active => t.status == TaskStatus::Active,
                StatusFilter::Done => t.status == TaskStatus::Done,
                StatusFilter::All => true,
            })
            .map(|t| TaskView {
                id: t.id,
                title: t.title.clone(),
                tags: t.tags.clone(),
                link: t.link.clone(),
                status: t.status,
                pinned: t.pinned,
                created_at: t.created_at,
                last_used_at: t.last_used_at,
                running_since: t.entries.iter().find(|e| e.end.is_none()).map(|e| e.start),
                today_seconds: t
                    .entries
                    .iter()
                    .map(|e| seconds_within(e, day_start, day_end, now))
                    .sum(),
                total_seconds: t
                    .entries
                    .iter()
                    .map(|e| seconds_within(e, Timestamp::MIN, Timestamp::MAX, now))
                    .sum(),
            })
            .collect();
        views.sort_by_key(|v| {
            (
                v.running_since.is_none(),
                !v.pinned,
                Reverse(v.last_used_at),
                Reverse(v.created_at),
            )
        });
        views
    }

    /// All tags, most tracked time in the last 30 days first; ties by number of tasks, then name.
    pub fn tag_views(&self, now: Timestamp) -> Vec<TagView> {
        let from = now - TAG_USAGE_WINDOW;
        let mut views: Vec<TagView> = self
            .tags
            .iter()
            .map(|tag| {
                let lower = tag.name.to_lowercase();
                let tasks = self
                    .tasks
                    .iter()
                    .filter(|t| t.tags.iter().any(|n| n.to_lowercase() == lower));
                let (mut count, mut seconds) = (0, 0);
                for t in tasks {
                    count += 1;
                    seconds += t
                        .entries
                        .iter()
                        .map(|e| seconds_within(e, from, now, now))
                        .sum::<i64>();
                }
                TagView {
                    name: tag.name.clone(),
                    color: tag.color.clone(),
                    task_count: count,
                    recent_seconds: seconds,
                }
            })
            .collect();
        views.sort_by(|a, b| {
            b.recent_seconds
                .cmp(&a.recent_seconds)
                .then(b.task_count.cmp(&a.task_count))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        views
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagView {
    pub name: String,
    pub color: Option<String>,
    pub task_count: usize,
    /// Tracked seconds on tasks with this tag in the last 30 days.
    pub recent_seconds: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn task(data: &mut UserData, title: &str, tags: &[&str], now: &str) -> Uuid {
        data.create_task(
            NewTask {
                title: title.into(),
                tags: tags.iter().map(|t| (*t).to_owned()).collect(),
                ..Default::default()
            },
            ts(now),
        )
        .unwrap()
    }

    fn running_count(data: &UserData) -> usize {
        data.tasks
            .iter()
            .flat_map(|t| &t.entries)
            .filter(|e| e.end.is_none())
            .count()
    }

    #[test]
    fn titles_are_trimmed_and_checked() {
        assert_eq!(
            normalize_title("  Fix   login\tbug ").unwrap(),
            "Fix login bug"
        );
        assert!(matches!(
            normalize_title("   "),
            Err(DomainError::Invalid {
                code: "title_empty",
                ..
            })
        ));
        assert!(normalize_title(&"x".repeat(MAX_TITLE_LEN)).is_ok());
        assert!(normalize_title(&"x".repeat(MAX_TITLE_LEN + 1)).is_err());
    }

    #[test]
    fn tags_lose_the_hash_and_duplicates() {
        let tags = normalize_tags(&[
            "#support".into(),
            "Support".into(),
            "".into(),
            "kunde-x".into(),
        ])
        .unwrap();
        assert_eq!(tags, vec!["support", "kunde-x"]);
        assert!(normalize_tags(&["two words".into()]).is_err());
        assert!(normalize_tags(&vec!["t".to_owned(); 1]).is_ok());
        let many: Vec<String> = (0..=MAX_TAGS_PER_TASK).map(|i| format!("t{i}")).collect();
        assert!(normalize_tags(&many).is_err());
    }

    #[test]
    fn links_must_be_web_addresses() {
        assert_eq!(normalize_link(Some(" ")).unwrap(), None);
        assert_eq!(
            normalize_link(Some("https://jira.example.com/ABC-1"))
                .unwrap()
                .as_deref(),
            Some("https://jira.example.com/ABC-1")
        );
        assert!(normalize_link(Some("javascript:alert(1)")).is_err());
        assert!(normalize_link(Some("https://a b")).is_err());
    }

    #[test]
    fn existing_tag_spelling_is_reused_and_new_tags_are_registered() {
        let mut d = UserData::default();
        task(&mut d, "A", &["Support"], "2026-11-17T08:00:00Z");
        task(&mut d, "B", &["support", "new"], "2026-11-17T08:01:00Z");
        assert_eq!(d.tasks[1].tags, vec!["Support", "new"]);
        let names: Vec<_> = d.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Support", "new"]);
    }

    #[test]
    fn r1_r2_starting_switches_without_gap_or_overlap() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        let b = task(&mut d, "B", &[], "2026-11-17T08:00:00Z");

        d.start_task(a, ts("2026-11-17T09:00:00Z")).unwrap();
        let switched = d.start_task(b, ts("2026-11-17T09:30:00.750Z")).unwrap();

        assert_eq!(running_count(&d), 1);
        assert_eq!(switched.task_id, b);
        let a_entry = &d.tasks[0].entries[0];
        assert_eq!(a_entry.end, Some(ts("2026-11-17T09:30:00Z")));
        assert_eq!(
            switched.start,
            ts("2026-11-17T09:30:00Z"),
            "same instant, seconds"
        );
    }

    #[test]
    fn starting_the_running_task_again_changes_nothing() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        let first = d.start_task(a, ts("2026-11-17T09:00:00Z")).unwrap();
        let again = d.start_task(a, ts("2026-11-17T09:10:00Z")).unwrap();
        assert_eq!(first, again);
        assert_eq!(d.tasks[0].entries.len(), 1);
    }

    #[test]
    fn starting_a_done_task_reopens_it() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        d.update_task(
            a,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            ts("2026-11-17T08:00:00Z"),
        )
        .unwrap();
        d.start_task(a, ts("2026-11-17T09:00:00Z")).unwrap();
        assert_eq!(d.tasks[0].status, TaskStatus::Active);
        assert_eq!(d.tasks[0].last_used_at, Some(ts("2026-11-17T09:00:00Z")));
    }

    #[test]
    fn create_with_start_runs_immediately_and_stops_the_previous() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        d.start_task(a, ts("2026-11-17T08:00:00Z")).unwrap();
        let b = d
            .create_task(
                NewTask {
                    title: "B".into(),
                    start: true,
                    ..Default::default()
                },
                ts("2026-11-17T08:20:00Z"),
            )
            .unwrap();
        assert_eq!(d.running().unwrap().task_id, b);
        assert_eq!(d.tasks[0].entries[0].end, Some(ts("2026-11-17T08:20:00Z")));
        assert_eq!(running_count(&d), 1);
    }

    #[test]
    fn r3_stop_now_or_at_a_past_time() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        assert_eq!(
            d.stop_timer(None, ts("2026-11-17T08:00:00Z")).unwrap(),
            None
        );

        d.start_task(a, ts("2026-11-17T09:00:00Z")).unwrap();
        let now = ts("2026-11-17T12:00:00Z");
        assert!(matches!(
            d.stop_timer(Some(ts("2026-11-17T08:59:59Z")), now),
            Err(DomainError::Invalid {
                code: "end_before_start",
                ..
            })
        ));
        assert!(matches!(
            d.stop_timer(Some(ts("2026-11-17T12:00:01Z")), now),
            Err(DomainError::Invalid {
                code: "end_in_future",
                ..
            })
        ));
        assert!(d.running().is_some(), "failed stops change nothing");

        d.stop_timer(Some(ts("2026-11-17T11:00:00Z")), now).unwrap();
        assert_eq!(d.tasks[0].entries[0].end, Some(ts("2026-11-17T11:00:00Z")));
        assert_eq!(d.running(), None);
    }

    #[test]
    fn a_zero_length_entry_is_dropped() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        d.start_task(a, ts("2026-11-17T09:00:00.100Z")).unwrap();
        d.stop_timer(None, ts("2026-11-17T09:00:00.900Z")).unwrap();
        assert!(d.tasks[0].entries.is_empty());
    }

    #[test]
    fn marking_the_running_task_done_stops_it() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        d.start_task(a, ts("2026-11-17T09:00:00Z")).unwrap();
        d.update_task(
            a,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            ts("2026-11-17T10:00:00Z"),
        )
        .unwrap();
        assert_eq!(d.running(), None);
        assert_eq!(d.tasks[0].entries[0].end, Some(ts("2026-11-17T10:00:00Z")));
    }

    #[test]
    fn invalid_patch_changes_nothing() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        let before = d.clone();
        let err = d.update_task(
            a,
            TaskPatch {
                title: Some("New".into()),
                link: Some("ftp://x".into()),
                ..Default::default()
            },
            ts("2026-11-17T09:00:00Z"),
        );
        assert!(err.is_err());
        assert_eq!(d, before);
    }

    #[test]
    fn unknown_tasks_are_not_found() {
        let mut d = UserData::default();
        let id = Uuid::now_v7();
        let now = ts("2026-11-17T08:00:00Z");
        assert_eq!(d.start_task(id, now), Err(DomainError::NotFound));
        assert_eq!(d.delete_task(id), Err(DomainError::NotFound));
        assert_eq!(
            d.update_task(id, TaskPatch::default(), now),
            Err(DomainError::NotFound)
        );
    }

    #[test]
    fn today_counts_in_the_users_time_zone_and_running_until_now() {
        let mut d = UserData::default();
        d.set_timezone("Europe/Berlin").unwrap();
        let a = task(&mut d, "A", &[], "2026-11-16T20:00:00Z");
        // 22:30-01:30 Berlin (21:30-00:30 UTC): 1.5 h yesterday, 1.5 h today (Berlin).
        d.start_task(a, ts("2026-11-16T21:30:00Z")).unwrap();
        d.stop_timer(None, ts("2026-11-17T00:30:00Z")).unwrap();
        // Running since 08:00 Berlin, now 09:00 Berlin.
        d.start_task(a, ts("2026-11-17T07:00:00Z")).unwrap();

        let view = &d.task_views(StatusFilter::Active, ts("2026-11-17T08:00:00Z"))[0];
        assert_eq!(view.today_seconds, 90 * 60 + 60 * 60);
        assert_eq!(view.total_seconds, 180 * 60 + 60 * 60);
        assert_eq!(view.running_since, Some(ts("2026-11-17T07:00:00Z")));

        // In UTC the first entry is 2.5 h on the 16th and 0.5 h on the 17th.
        d.settings.timezone = None;
        let view = &d.task_views(StatusFilter::Active, ts("2026-11-17T08:00:00Z"))[0];
        assert_eq!(view.today_seconds, 30 * 60 + 60 * 60);
    }

    #[test]
    fn today_has_25_hours_when_daylight_saving_ends() {
        let tz = TimeZone::get("Europe/Berlin").unwrap();
        let (start, end) = today(ts("2026-10-25T12:00:00Z"), &tz);
        assert_eq!(start, ts("2026-10-24T22:00:00Z"));
        assert_eq!(end, ts("2026-10-25T23:00:00Z"));
        assert_eq!(end.as_second() - start.as_second(), 25 * 3600);
    }

    #[test]
    fn board_order_running_pinned_recent_new() {
        let mut d = UserData::default();
        let old = task(&mut d, "old", &[], "2026-11-17T08:00:00Z");
        let newer = task(&mut d, "newer", &[], "2026-11-17T08:05:00Z");
        let pinned = task(&mut d, "pinned", &[], "2026-11-17T08:01:00Z");
        let used = task(&mut d, "used", &[], "2026-11-17T08:02:00Z");
        let running = task(&mut d, "running", &[], "2026-11-17T08:03:00Z");
        d.update_task(
            pinned,
            TaskPatch {
                pinned: Some(true),
                ..Default::default()
            },
            ts("2026-11-17T08:10:00Z"),
        )
        .unwrap();
        d.start_task(used, ts("2026-11-17T09:00:00Z")).unwrap();
        d.start_task(running, ts("2026-11-17T09:10:00Z")).unwrap();

        let order: Vec<Uuid> = d
            .task_views(StatusFilter::Active, ts("2026-11-17T10:00:00Z"))
            .iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(order, vec![running, pinned, used, newer, old]);
    }

    #[test]
    fn status_filter_hides_done_tasks_by_default() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &[], "2026-11-17T08:00:00Z");
        task(&mut d, "B", &[], "2026-11-17T08:00:00Z");
        d.update_task(
            a,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            ts("2026-11-17T08:00:00Z"),
        )
        .unwrap();
        let now = ts("2026-11-17T09:00:00Z");
        assert_eq!(d.task_views(StatusFilter::Active, now).len(), 1);
        assert_eq!(d.task_views(StatusFilter::Done, now).len(), 1);
        assert_eq!(d.task_views(StatusFilter::All, now).len(), 2);
    }

    #[test]
    fn tags_ranked_by_recent_time_then_task_count() {
        let mut d = UserData::default();
        let a = task(&mut d, "A", &["review"], "2026-10-01T08:00:00Z");
        task(&mut d, "B", &["meeting"], "2026-10-01T08:00:00Z");
        task(&mut d, "C", &["meeting"], "2026-10-01T08:00:00Z");
        let old = task(&mut d, "D", &["legacy"], "2026-10-01T08:00:00Z");
        // Old time (outside 30 days) does not count.
        d.start_task(old, ts("2026-10-01T08:00:00Z")).unwrap();
        d.stop_timer(None, ts("2026-10-01T18:00:00Z")).unwrap();
        d.start_task(a, ts("2026-11-17T08:00:00Z")).unwrap();
        d.stop_timer(None, ts("2026-11-17T09:00:00Z")).unwrap();

        let tags = d.tag_views(ts("2026-11-17T10:00:00Z"));
        let order: Vec<(&str, i64, usize)> = tags
            .iter()
            .map(|t| (t.name.as_str(), t.recent_seconds, t.task_count))
            .collect();
        assert_eq!(
            order,
            vec![("review", 3600, 1), ("meeting", 0, 2), ("legacy", 0, 1)]
        );
    }
}
