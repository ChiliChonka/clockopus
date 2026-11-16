//! Corrections (docs/SPEC.md, sections 3.2 and 3.3): manual entries, editing with history (R4–R6)
//! and the forgotten timer (R7/R8). Pure functions on [`UserData`], like `domain.rs`.

use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::{
    Change, ChangeReason, DomainError, Entry, EntrySource, UserData, invalid, truncate,
};

pub const MAX_NOTE_LEN: usize = 500;
/// The longest range the entry list answers, to keep responses small.
pub const MAX_RANGE_DAYS: i32 = 366;

pub fn normalize_note(note: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > MAX_NOTE_LEN {
        return Err(invalid(
            "note_too_long",
            format!("A note can have at most {MAX_NOTE_LEN} characters."),
        ));
    }
    Ok(Some(note.to_owned()))
}

#[derive(Debug, Clone, Default)]
pub struct EntryPatch {
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
    /// `Some("")` removes the note.
    pub note: Option<String>,
}

/// A time entry with its task, for the edit dialog and the day view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryView {
    pub id: Uuid,
    pub task_id: Uuid,
    pub task_title: String,
    pub tags: Vec<String>,
    pub start: Timestamp,
    pub end: Option<Timestamp>,
    pub source: EntrySource,
    pub note: Option<String>,
    pub history: Vec<Change>,
}

/// R7: the running timer looks forgotten; R8 offers `suggested_end`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Forgotten {
    pub task_id: Uuid,
    pub task_title: String,
    pub entry_id: Uuid,
    pub start: Timestamp,
    pub running_seconds: i64,
    /// Workday end on the start day, if that lies between start and now.
    pub suggested_end: Option<Timestamp>,
}

fn local_hm(ts: Timestamp, tz: &TimeZone) -> String {
    ts.to_zoned_datetime(tz.clone())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

impl UserData {
    fn entry_position(&self, id: Uuid) -> Option<(usize, usize)> {
        self.tasks
            .iter()
            .enumerate()
            .find_map(|(t, task)| task.entries.iter().position(|e| e.id == id).map(|e| (t, e)))
    }

    /// R5: `[start, end)` (a running span until `now`) must not overlap any other entry.
    fn check_free(
        &self,
        start: Timestamp,
        end: Option<Timestamp>,
        now: Timestamp,
        except: Option<Uuid>,
    ) -> Result<(), DomainError> {
        let end = end.unwrap_or(now);
        let tz = self.settings.time_zone();
        for task in &self.tasks {
            for e in &task.entries {
                if Some(e.id) == except {
                    continue;
                }
                let e_end = e.end.unwrap_or(now);
                if start < e_end && e.start < end {
                    return Err(DomainError::Conflict {
                        code: "overlap",
                        message: format!(
                            "Overlaps with “{}” from {} to {}.",
                            task.title,
                            local_hm(e.start, &tz),
                            if e.end.is_some() {
                                local_hm(e_end, &tz)
                            } else {
                                "now (running)".to_owned()
                            }
                        ),
                        entry_id: e.id,
                    });
                }
            }
        }
        Ok(())
    }

    /// R4 for a finished span.
    fn check_span(start: Timestamp, end: Timestamp, now: Timestamp) -> Result<(), DomainError> {
        if end <= start {
            return Err(invalid(
                "end_not_after_start",
                "The end must be after the start.",
            ));
        }
        if end > now {
            return Err(invalid("end_in_future", "The end cannot be in the future."));
        }
        Ok(())
    }

    fn sort_entries(&mut self, task: usize) {
        self.tasks[task].entries.sort_by_key(|e| e.start);
    }

    /// Adds a manual entry (R4, R5). Does not change `last_used_at`: past work is not “now”.
    pub fn add_entry(
        &mut self,
        task_id: Uuid,
        start: Timestamp,
        end: Timestamp,
        note: Option<&str>,
        now: Timestamp,
    ) -> Result<Uuid, DomainError> {
        let (start, end, now) = (truncate(start), truncate(end), truncate(now));
        let note = normalize_note(note)?;
        let t = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(DomainError::NotFound)?;
        Self::check_span(start, end, now)?;
        self.check_free(start, Some(end), now, None)?;
        let id = Uuid::now_v7();
        self.tasks[t].entries.push(Entry {
            id,
            start,
            end: Some(end),
            source: EntrySource::Manual,
            note,
            history: Vec::new(),
            keep_running: false,
        });
        self.sort_entries(t);
        Ok(id)
    }

    /// Changes start, end or note of an entry. A changed span is checked (R4, R5) and the old
    /// values go to the history (R6). The end of a running entry is set by stopping it.
    pub fn update_entry(
        &mut self,
        id: Uuid,
        patch: EntryPatch,
        reason: ChangeReason,
        now: Timestamp,
    ) -> Result<(), DomainError> {
        let now = truncate(now);
        let note = patch
            .note
            .as_deref()
            .map(|n| normalize_note(Some(n)))
            .transpose()?;
        let (t, e) = self.entry_position(id).ok_or(DomainError::NotFound)?;
        let old = self.tasks[t].entries[e].clone();
        let start = patch.start.map(truncate).unwrap_or(old.start);
        let end = match (old.end, patch.end.map(truncate)) {
            (None, Some(_)) => {
                return Err(invalid(
                    "entry_running",
                    "This timer is still running. Stop it to set its end.",
                ));
            }
            (None, None) => None,
            (Some(old_end), new_end) => Some(new_end.unwrap_or(old_end)),
        };
        match end {
            Some(end) => Self::check_span(start, end, now)?,
            None if start > now => {
                return Err(invalid(
                    "start_in_future",
                    "The start cannot be in the future.",
                ));
            }
            None => {}
        }
        self.check_free(start, end, now, Some(id))?;

        let entry = &mut self.tasks[t].entries[e];
        if (start, end) != (old.start, old.end) {
            entry.history.push(Change {
                changed_at: now,
                reason,
                start: old.start,
                end: old.end,
            });
            entry.start = start;
            entry.end = end;
        }
        if let Some(note) = note {
            entry.note = note;
        }
        self.sort_entries(t);
        Ok(())
    }

    pub fn delete_entry(&mut self, id: Uuid) -> Result<(), DomainError> {
        let (t, e) = self.entry_position(id).ok_or(DomainError::NotFound)?;
        self.tasks[t].entries.remove(e);
        Ok(())
    }

    /// R7: the running entry is forgotten if it started before today or runs longer than the
    /// threshold — unless the user chose “Keep running”.
    pub fn forgotten(&self, now: Timestamp) -> Option<Forgotten> {
        let running = self.running()?;
        let task = self.tasks.iter().find(|t| t.id == running.task_id)?;
        let entry = task.entries.iter().find(|e| e.id == running.entry_id)?;
        if entry.keep_running {
            return None;
        }
        let tz = self.settings.time_zone();
        let start_local = running.start.to_zoned_datetime(tz.clone());
        let today = now.to_zoned_datetime(tz.clone()).date();
        let threshold = SignedDuration::from_hours(i64::from(self.settings.forgotten_after_hours));
        let running_for = now.duration_since(running.start);
        if start_local.date() >= today && running_for <= threshold {
            return None;
        }
        Some(Forgotten {
            task_id: task.id,
            task_title: task.title.clone(),
            entry_id: entry.id,
            start: running.start,
            running_seconds: running_for.as_secs(),
            suggested_end: workday_end_on(start_local.date(), self.settings.workday_end, &tz)
                .filter(|end| *end > running.start && *end <= now),
        })
    }

    /// R8 “Stop at …” or “Pick end time”: stops the running entry at `end` and records that it
    /// was a forgotten-timer correction.
    pub fn stop_forgotten(&mut self, end: Timestamp, now: Timestamp) -> Result<(), DomainError> {
        let running = self
            .running()
            .ok_or_else(|| invalid("no_running_timer", "No timer is running."))?;
        self.stop_timer(Some(end), now)?;
        // The entry is gone if it ended at its start; then there is nothing to annotate.
        if let Some((t, e)) = self.entry_position(running.entry_id) {
            self.tasks[t].entries[e].history.push(Change {
                changed_at: truncate(now),
                reason: ChangeReason::ForgottenTimer,
                start: running.start,
                end: None,
            });
        }
        Ok(())
    }

    /// R8 “Keep running”: no prompt for this entry until it is stopped.
    pub fn keep_running(&mut self) -> Result<(), DomainError> {
        let running = self
            .running()
            .ok_or_else(|| invalid("no_running_timer", "No timer is running."))?;
        let (t, e) = self
            .entry_position(running.entry_id)
            .ok_or(DomainError::NotFound)?;
        self.tasks[t].entries[e].keep_running = true;
        Ok(())
    }

    fn entry_view(&self, t: usize, e: &Entry) -> EntryView {
        let task = &self.tasks[t];
        EntryView {
            id: e.id,
            task_id: task.id,
            task_title: task.title.clone(),
            tags: task.tags.clone(),
            start: e.start,
            end: e.end,
            source: e.source,
            note: e.note.clone(),
            history: e.history.clone(),
        }
    }

    /// All entries of one task, newest first.
    pub fn entries_of_task(&self, task_id: Uuid) -> Result<Vec<EntryView>, DomainError> {
        let t = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(DomainError::NotFound)?;
        let mut out: Vec<EntryView> = self.tasks[t]
            .entries
            .iter()
            .map(|e| self.entry_view(t, e))
            .collect();
        out.sort_by_key(|e| std::cmp::Reverse(e.start));
        Ok(out)
    }

    /// Entries touching the local days `from..=to` (user's time zone), oldest first.
    pub fn entries_between(
        &self,
        from: Date,
        to: Date,
        now: Timestamp,
    ) -> Result<Vec<EntryView>, DomainError> {
        if to < from {
            return Err(invalid("range_invalid", "`to` must not be before `from`."));
        }
        if from.until(to).map(|s| s.get_days()).unwrap_or(i32::MAX) >= MAX_RANGE_DAYS {
            return Err(invalid(
                "range_too_long",
                format!("A range can cover at most {MAX_RANGE_DAYS} days."),
            ));
        }
        let tz = self.settings.time_zone();
        let range_start = start_of(from, &tz)?;
        let range_end = start_of(
            to.tomorrow()
                .map_err(|_| invalid("range_invalid", "Date out of range."))?,
            &tz,
        )?;
        let mut out: Vec<EntryView> = self
            .tasks
            .iter()
            .enumerate()
            .flat_map(|(t, task)| {
                task.entries
                    .iter()
                    .filter(|e| e.start < range_end && e.end.unwrap_or(now) > range_start)
                    .map(move |e| (t, e))
            })
            .map(|(t, e)| self.entry_view(t, e))
            .collect();
        out.sort_by_key(|e| e.start);
        Ok(out)
    }

    pub fn set_workday(
        &mut self,
        workday_end: Option<Time>,
        forgotten_after_hours: Option<u8>,
    ) -> Result<(), DomainError> {
        if let Some(h) = forgotten_after_hours
            && !(1..=24).contains(&h)
        {
            return Err(invalid(
                "forgotten_after_invalid",
                "The forgotten-timer threshold must be between 1 and 24 hours.",
            ));
        }
        if let Some(t) = workday_end {
            self.settings.workday_end = Time::constant(t.hour(), t.minute(), 0, 0);
        }
        if let Some(h) = forgotten_after_hours {
            self.settings.forgotten_after_hours = h;
        }
        Ok(())
    }
}

/// Start of a local day; on a day that starts in a DST gap, the first existing instant.
fn start_of(date: Date, tz: &TimeZone) -> Result<Timestamp, DomainError> {
    date.to_zoned_datetime(tz.clone())
        .map(|z| z.timestamp())
        .map_err(|_| invalid("range_invalid", "Date out of range."))
}

fn workday_end_on(date: Date, time: Time, tz: &TimeZone) -> Option<Timestamp> {
    date.to_datetime(time)
        .to_zoned_datetime(tz.clone())
        .ok()
        .map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewTask;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    /// Berlin user with one task "A" and one finished entry 09:00–10:00 local (08:00–09:00 UTC).
    fn setup() -> (UserData, Uuid, Uuid) {
        let mut d = UserData::default();
        d.set_timezone("Europe/Berlin").unwrap();
        let a = d
            .create_task(
                NewTask {
                    title: "A".into(),
                    ..Default::default()
                },
                ts("2026-11-17T07:00:00Z"),
            )
            .unwrap();
        let e = d
            .add_entry(
                a,
                ts("2026-11-17T08:00:00Z"),
                ts("2026-11-17T09:00:00Z"),
                None,
                ts("2026-11-17T12:00:00Z"),
            )
            .unwrap();
        (d, a, e)
    }

    fn code(r: Result<impl std::fmt::Debug, DomainError>) -> &'static str {
        match r {
            Err(DomainError::Invalid { code, .. }) | Err(DomainError::Conflict { code, .. }) => {
                code
            }
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn r4_manual_entries_need_a_valid_span() {
        let (mut d, a, _) = setup();
        let now = ts("2026-11-17T12:00:00Z");
        assert_eq!(
            code(d.add_entry(
                a,
                ts("2026-11-17T10:00:00Z"),
                ts("2026-11-17T10:00:00Z"),
                None,
                now
            )),
            "end_not_after_start"
        );
        assert_eq!(
            code(d.add_entry(
                a,
                ts("2026-11-17T11:00:00Z"),
                ts("2026-11-17T12:00:01Z"),
                None,
                now
            )),
            "end_in_future"
        );
        let id = d
            .add_entry(
                a,
                ts("2026-11-17T11:00:00Z"),
                ts("2026-11-17T12:00:00Z"),
                Some(" call "),
                now,
            )
            .unwrap();
        let entry = d.tasks[0].entries.iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.source, EntrySource::Manual);
        assert_eq!(entry.note.as_deref(), Some("call"));
        assert_eq!(
            d.tasks[0].last_used_at, None,
            "manual entries do not count as use"
        );
    }

    #[test]
    fn r5_overlaps_are_rejected_and_name_the_other_entry() {
        let (mut d, a, existing) = setup();
        let now = ts("2026-11-17T12:00:00Z");
        let err = d.add_entry(
            a,
            ts("2026-11-17T08:30:00Z"),
            ts("2026-11-17T09:30:00Z"),
            None,
            now,
        );
        match err {
            Err(DomainError::Conflict {
                code,
                entry_id,
                message,
            }) => {
                assert_eq!(code, "overlap");
                assert_eq!(entry_id, existing);
                assert!(
                    message.contains("2026-11-17 09:00 to 2026-11-17 10:00"),
                    "{message}"
                );
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        // Touching edges are fine: [08:00, 09:00) and [09:00, 10:00).
        d.add_entry(
            a,
            ts("2026-11-17T09:00:00Z"),
            ts("2026-11-17T10:00:00Z"),
            None,
            now,
        )
        .unwrap();
        d.add_entry(
            a,
            ts("2026-11-17T07:00:00Z"),
            ts("2026-11-17T08:00:00Z"),
            None,
            now,
        )
        .unwrap();
        assert_eq!(d.tasks[0].entries.len(), 3);
        let starts: Vec<_> = d.tasks[0].entries.iter().map(|e| e.start).collect();
        assert!(starts.windows(2).all(|w| w[0] < w[1]), "kept in order");
    }

    #[test]
    fn r5_the_running_entry_counts_until_now() {
        let (mut d, a, _) = setup();
        d.start_task(a, ts("2026-11-17T11:00:00Z")).unwrap();
        let now = ts("2026-11-17T12:00:00Z");
        let err = d.add_entry(
            a,
            ts("2026-11-17T11:30:00Z"),
            ts("2026-11-17T11:45:00Z"),
            None,
            now,
        );
        assert!(matches!(err, Err(DomainError::Conflict { .. })), "{err:?}");
        assert!(format!("{err:?}").contains("now (running)"), "{err:?}");
    }

    #[test]
    fn r6_changing_the_span_keeps_the_old_values() {
        let (mut d, _, e) = setup();
        let now = ts("2026-11-18T07:00:00Z");
        d.update_entry(
            e,
            EntryPatch {
                end: Some(ts("2026-11-17T09:30:00Z")),
                ..Default::default()
            },
            ChangeReason::Edit,
            now,
        )
        .unwrap();
        let entry = &d.tasks[0].entries[0];
        assert_eq!(entry.end, Some(ts("2026-11-17T09:30:00Z")));
        assert_eq!(
            entry.history,
            vec![Change {
                changed_at: now,
                reason: ChangeReason::Edit,
                start: ts("2026-11-17T08:00:00Z"),
                end: Some(ts("2026-11-17T09:00:00Z")),
            }]
        );
    }

    #[test]
    fn r6_a_note_alone_leaves_no_history() {
        let (mut d, _, e) = setup();
        d.update_entry(
            e,
            EntryPatch {
                note: Some("pairing".into()),
                ..Default::default()
            },
            ChangeReason::Edit,
            ts("2026-11-18T07:00:00Z"),
        )
        .unwrap();
        assert_eq!(d.tasks[0].entries[0].note.as_deref(), Some("pairing"));
        assert!(d.tasks[0].entries[0].history.is_empty());
        d.update_entry(
            e,
            EntryPatch {
                note: Some("".into()),
                ..Default::default()
            },
            ChangeReason::Edit,
            ts("2026-11-18T07:00:00Z"),
        )
        .unwrap();
        assert_eq!(d.tasks[0].entries[0].note, None);
    }

    #[test]
    fn editing_checks_r4_r5_against_the_others_only() {
        let (mut d, a, e) = setup();
        let now = ts("2026-11-17T12:00:00Z");
        let other = d
            .add_entry(
                a,
                ts("2026-11-17T10:00:00Z"),
                ts("2026-11-17T11:00:00Z"),
                None,
                now,
            )
            .unwrap();
        // Moving within its own old span is fine (no conflict with itself).
        d.update_entry(
            e,
            EntryPatch {
                start: Some(ts("2026-11-17T08:15:00Z")),
                ..Default::default()
            },
            ChangeReason::Edit,
            now,
        )
        .unwrap();
        let before = d.clone();
        let err = d.update_entry(
            e,
            EntryPatch {
                end: Some(ts("2026-11-17T10:30:00Z")),
                ..Default::default()
            },
            ChangeReason::Edit,
            now,
        );
        assert!(matches!(err, Err(DomainError::Conflict { entry_id, .. }) if entry_id == other));
        assert_eq!(
            code(d.update_entry(
                e,
                EntryPatch {
                    start: Some(ts("2026-11-17T09:00:00Z")),
                    ..Default::default()
                },
                ChangeReason::Edit,
                now,
            )),
            "end_not_after_start"
        );
        assert_eq!(d, before, "rejected edits change nothing");
    }

    #[test]
    fn a_running_entry_can_move_its_start_but_not_get_an_end() {
        let (mut d, a, _) = setup();
        let r = d.start_task(a, ts("2026-11-17T10:00:00Z")).unwrap();
        let now = ts("2026-11-17T11:00:00Z");
        assert_eq!(
            code(d.update_entry(
                r.entry_id,
                EntryPatch {
                    end: Some(ts("2026-11-17T10:30:00Z")),
                    ..Default::default()
                },
                ChangeReason::Edit,
                now,
            )),
            "entry_running"
        );
        assert_eq!(
            code(d.update_entry(
                r.entry_id,
                EntryPatch {
                    start: Some(ts("2026-11-17T08:30:00Z")),
                    ..Default::default()
                },
                ChangeReason::Edit,
                now,
            )),
            "overlap"
        );
        d.update_entry(
            r.entry_id,
            EntryPatch {
                start: Some(ts("2026-11-17T09:30:00Z")),
                ..Default::default()
            },
            ChangeReason::Edit,
            now,
        )
        .unwrap();
        assert_eq!(d.running().unwrap().start, ts("2026-11-17T09:30:00Z"));
        assert_eq!(d.tasks[0].entries[1].history[0].end, None);
    }

    #[test]
    fn deleting_an_entry() {
        let (mut d, _, e) = setup();
        d.delete_entry(e).unwrap();
        assert!(d.tasks[0].entries.is_empty());
        assert_eq!(d.delete_entry(e), Err(DomainError::NotFound));
    }

    #[test]
    fn r7_forgotten_after_midnight_or_threshold_but_not_before() {
        let (mut d, a, _) = setup();
        // Started 08:00 Berlin (07:00 UTC) on the 18th.
        d.start_task(a, ts("2026-11-18T07:00:00Z")).unwrap();
        assert_eq!(
            d.forgotten(ts("2026-11-18T16:59:00Z")),
            None,
            "9 h 59 min, same day"
        );
        let f = d.forgotten(ts("2026-11-18T17:00:01Z")).expect("over 10 h");
        assert_eq!(f.running_seconds, 10 * 3600 + 1);
        assert_eq!(
            f.suggested_end,
            Some(ts("2026-11-18T16:00:00Z")),
            "17:00 Berlin"
        );

        // Started 22:00 Berlin, now 00:30 Berlin next day: only 2.5 h, but a new day.
        let (mut d, a, _) = setup();
        d.start_task(a, ts("2026-11-18T21:00:00Z")).unwrap();
        let f = d
            .forgotten(ts("2026-11-18T23:30:00Z"))
            .expect("past midnight");
        assert_eq!(f.suggested_end, None, "17:00 lies before the start");
    }

    #[test]
    fn r7_threshold_and_workday_end_come_from_the_settings() {
        let (mut d, a, _) = setup();
        d.set_workday(Some(Time::constant(16, 30, 0, 0)), Some(4))
            .unwrap();
        d.start_task(a, ts("2026-11-18T07:00:00Z")).unwrap();
        let f = d.forgotten(ts("2026-11-18T11:00:01Z")).expect("over 4 h");
        assert_eq!(f.suggested_end, None, "16:30 Berlin is still ahead");
        let f = d.forgotten(ts("2026-11-18T16:00:00Z")).unwrap();
        assert_eq!(f.suggested_end, Some(ts("2026-11-18T15:30:00Z")));
        assert_eq!(
            code(d.set_workday(None, Some(0))),
            "forgotten_after_invalid"
        );
        assert_eq!(
            code(d.set_workday(None, Some(25))),
            "forgotten_after_invalid"
        );
    }

    #[test]
    fn r8_stop_forgotten_records_the_correction() {
        let (mut d, a, _) = setup();
        d.start_task(a, ts("2026-11-18T07:00:00Z")).unwrap();
        let now = ts("2026-11-19T07:00:00Z");
        let f = d.forgotten(now).unwrap();
        d.stop_forgotten(f.suggested_end.unwrap(), now).unwrap();
        assert_eq!(d.running(), None);
        let entry = d.tasks[0].entries.last().unwrap();
        assert_eq!(entry.end, Some(ts("2026-11-18T16:00:00Z")));
        assert_eq!(
            entry.history,
            vec![Change {
                changed_at: now,
                reason: ChangeReason::ForgottenTimer,
                start: ts("2026-11-18T07:00:00Z"),
                end: None,
            }]
        );
        assert_eq!(code(d.stop_forgotten(now, now)), "no_running_timer");
    }

    #[test]
    fn r8_keep_running_silences_until_stopped() {
        let (mut d, a, _) = setup();
        d.start_task(a, ts("2026-11-18T07:00:00Z")).unwrap();
        let now = ts("2026-11-19T07:00:00Z");
        d.keep_running().unwrap();
        assert_eq!(d.forgotten(now), None);
        d.stop_timer(None, now).unwrap();
        assert!(
            !d.tasks[0].entries.last().unwrap().keep_running,
            "flag cleared"
        );
        d.start_task(a, ts("2026-11-19T07:00:00Z")).unwrap();
        assert!(
            d.forgotten(ts("2026-11-20T07:00:00Z")).is_some(),
            "new entry asks again"
        );
    }

    #[test]
    fn entries_between_uses_local_days_and_includes_crossing_entries() {
        let (mut d, a, _) = setup();
        let now = ts("2026-11-19T12:00:00Z");
        // 23:00–01:00 Berlin across midnight into the 18th.
        d.add_entry(
            a,
            ts("2026-11-17T22:00:00Z"),
            ts("2026-11-18T00:00:00Z"),
            None,
            now,
        )
        .unwrap();
        d.add_entry(
            a,
            ts("2026-11-18T09:00:00Z"),
            ts("2026-11-18T10:00:00Z"),
            None,
            now,
        )
        .unwrap();
        let day = |s: &str| s.parse::<Date>().unwrap();
        let on_18 = d
            .entries_between(day("2026-11-18"), day("2026-11-18"), now)
            .unwrap();
        assert_eq!(on_18.len(), 2, "crossing entry and the one on the 18th");
        assert_eq!(on_18[0].start, ts("2026-11-17T22:00:00Z"));
        let on_17 = d
            .entries_between(day("2026-11-17"), day("2026-11-17"), now)
            .unwrap();
        assert_eq!(on_17.len(), 2, "setup entry and the crossing one");
        assert_eq!(
            code(d.entries_between(day("2026-11-18"), day("2026-11-17"), now)),
            "range_invalid"
        );
        assert_eq!(
            code(d.entries_between(day("2025-01-01"), day("2026-11-17"), now)),
            "range_too_long"
        );
    }

    #[test]
    fn entries_of_task_newest_first() {
        let (mut d, a, first) = setup();
        let now = ts("2026-11-17T12:00:00Z");
        let second = d
            .add_entry(
                a,
                ts("2026-11-17T10:00:00Z"),
                ts("2026-11-17T11:00:00Z"),
                None,
                now,
            )
            .unwrap();
        let ids: Vec<Uuid> = d.entries_of_task(a).unwrap().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![second, first]);
        assert_eq!(
            d.entries_of_task(Uuid::now_v7()),
            Err(DomainError::NotFound)
        );
    }
}
