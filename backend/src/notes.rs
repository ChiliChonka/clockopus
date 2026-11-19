//! The notes view (docs/SPEC.md, section 4.5): notes of the tasks with one tag, in a period.
//! Pure functions on [`UserData`]; days are local days in the user's time zone.

use std::cmp::Reverse;

use jiff::Timestamp;
use jiff::civil::Date;
use serde::Serialize;

use crate::domain::{DomainError, StatusFilter, TaskView, UserData, invalid, seconds_within};
use crate::reports::{day_start, next_day};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteView {
    #[serde(flatten)]
    pub task: TaskView,
    /// Time on the task in the period (all time without a period), a running entry until now.
    pub period_seconds: i64,
}

impl UserData {
    /// Tasks with notes, newest activity first.
    ///
    /// `tag`: only tasks with this tag (`None`: all). `period`: local days `from..=to`; a task
    /// belongs to it when it has time in it, its notes changed in it or it was created in it.
    /// `None`: all tasks with notes.
    pub fn notes(
        &self,
        tag: Option<&str>,
        period: Option<(Date, Date)>,
        now: Timestamp,
    ) -> Result<Vec<NoteView>, DomainError> {
        let tag = tag
            .map(|t| t.trim().trim_start_matches('#').to_lowercase())
            .filter(|t| !t.is_empty());
        let bounds = match period {
            None => (Timestamp::MIN, Timestamp::MAX),
            Some((from, to)) => {
                if to < from {
                    return Err(invalid("range_invalid", "`to` must not be before `from`."));
                }
                let tz = self.settings.time_zone();
                (day_start(from, &tz)?, day_start(next_day(to)?, &tz)?)
            }
        };
        let within = |ts: Timestamp| bounds.0 <= ts && ts < bounds.1;
        let views = self.task_views(StatusFilter::All, now);

        let mut out: Vec<(Timestamp, NoteView)> = self
            .tasks
            .iter()
            .filter(|t| t.notes.is_some())
            .filter(|t| tag.as_ref().is_none_or(|tag| t.tags.contains(tag)))
            .filter_map(|t| {
                let seconds: i64 = t
                    .entries
                    .iter()
                    .map(|e| seconds_within(e, bounds.0, bounds.1, now))
                    .sum();
                let in_period = period.is_none()
                    || seconds > 0
                    || t.notes_changed_at.is_some_and(within)
                    || within(t.created_at);
                if !in_period {
                    return None;
                }
                // Hand-entered time does not move `last_used_at`, so its entries count too.
                let last_entry = t.entries.iter().map(|e| e.end.unwrap_or(now)).max();
                let latest = [
                    Some(t.created_at),
                    t.last_used_at,
                    t.notes_changed_at,
                    last_entry,
                ]
                .into_iter()
                .flatten()
                .max()
                .unwrap_or(t.created_at);
                let view = views.iter().find(|v| v.id == t.id)?.clone();
                Some((
                    latest,
                    NoteView {
                        task: view,
                        period_seconds: seconds,
                    },
                ))
            })
            .collect();
        out.sort_by_key(|(latest, v)| (Reverse(*latest), v.task.title.clone()));
        Ok(out.into_iter().map(|(_, v)| v).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{NewTask, TaskPatch};

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn date(s: &str) -> Date {
        s.parse().unwrap()
    }

    /// Berlin time. Three tasks with notes, one without:
    /// - "Old review" #review, created and worked on in September, notes written then.
    /// - "Payment PR" #review #team-a, created in September, worked on 16 Nov.
    /// - "Planning" #team-a, created in October, no time, notes changed 10 Nov.
    /// - "No notes" #review, worked on 16 Nov.
    fn setup() -> UserData {
        let mut d = UserData::default();
        d.set_timezone("Europe/Berlin").unwrap();
        let add = |d: &mut UserData, title: &str, tags: &[&str], created: &str| {
            d.create_task(
                NewTask {
                    title: title.into(),
                    tags: tags.iter().map(|t| (*t).to_owned()).collect(),
                    ..Default::default()
                },
                ts(created),
            )
            .unwrap()
        };
        let old = add(&mut d, "Old review", &["review"], "2026-09-01T08:00:00Z");
        let pay = add(
            &mut d,
            "Payment PR",
            &["review", "team-a"],
            "2026-09-02T08:00:00Z",
        );
        let plan = add(&mut d, "Planning", &["team-a"], "2026-10-01T08:00:00Z");
        let none = add(&mut d, "No notes", &["review"], "2026-09-03T08:00:00Z");
        let now = ts("2026-11-18T12:00:00Z");
        d.add_entry(
            old,
            ts("2026-09-01T09:00:00Z"),
            ts("2026-09-01T10:00:00Z"),
            None,
            now,
        )
        .unwrap();
        d.add_entry(
            pay,
            ts("2026-11-16T09:00:00Z"),
            ts("2026-11-16T10:30:00Z"),
            None,
            now,
        )
        .unwrap();
        d.add_entry(
            none,
            ts("2026-11-16T11:00:00Z"),
            ts("2026-11-16T12:00:00Z"),
            None,
            now,
        )
        .unwrap();
        for (id, text, at) in [
            (old, "Old findings", "2026-09-01T10:00:00Z"),
            (pay, "Check rounding\n  - in CSV", "2026-09-02T09:00:00Z"),
            (plan, "Agenda for Q1", "2026-11-10T08:00:00Z"),
        ] {
            d.update_task(
                id,
                TaskPatch {
                    notes: Some(text.into()),
                    ..Default::default()
                },
                ts(at),
            )
            .unwrap();
        }
        d
    }

    fn titles(v: &[NoteView]) -> Vec<&str> {
        v.iter().map(|n| n.task.title.as_str()).collect()
    }

    const NOW: &str = "2026-11-18T12:00:00Z";

    #[test]
    fn a_tag_and_a_period_pick_the_tasks_with_notes() {
        let d = setup();
        let period = Some((date("2026-10-19"), date("2026-11-18")));
        // #review in the last month: only "Payment PR" (time on 16 Nov). "Old review" is
        // older, "No notes" has none.
        let v = d.notes(Some("review"), period, ts(NOW)).unwrap();
        assert_eq!(titles(&v), ["Payment PR"]);
        assert_eq!(v[0].period_seconds, 90 * 60);
        assert_eq!(
            v[0].task.notes.as_deref(),
            Some("Check rounding\n  - in CSV")
        );

        // #team-a: "Planning" has no time, but its notes changed in the period. Newest first.
        let v = d.notes(Some("#Team-A"), period, ts(NOW)).unwrap();
        assert_eq!(titles(&v), ["Payment PR", "Planning"]);
        assert_eq!(v[1].period_seconds, 0);
    }

    #[test]
    fn without_a_period_all_notes_count_and_without_a_tag_all_tags() {
        let d = setup();
        let v = d.notes(Some("review"), None, ts(NOW)).unwrap();
        assert_eq!(titles(&v), ["Payment PR", "Old review"]);
        assert_eq!(v[1].period_seconds, 3600, "all time");

        let v = d.notes(None, None, ts(NOW)).unwrap();
        assert_eq!(titles(&v), ["Payment PR", "Planning", "Old review"]);

        // A period with nothing in it, an unknown tag: empty, not an error.
        let empty = Some((date("2026-08-01"), date("2026-08-31")));
        assert!(d.notes(None, empty, ts(NOW)).unwrap().is_empty());
        assert!(d.notes(Some("nope"), None, ts(NOW)).unwrap().is_empty());
    }

    #[test]
    fn the_period_uses_local_days() {
        let d = setup();
        // "Planning" notes changed 2026-11-10 08:00 UTC = 09:00 Berlin: on the local 10th.
        let v = d
            .notes(
                Some("team-a"),
                Some((date("2026-11-10"), date("2026-11-10"))),
                ts(NOW),
            )
            .unwrap();
        assert_eq!(titles(&v), ["Planning"]);
        let v = d
            .notes(
                Some("team-a"),
                Some((date("2026-11-11"), date("2026-11-15"))),
                ts(NOW),
            )
            .unwrap();
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn to_before_from_is_refused() {
        let d = setup();
        let err = d
            .notes(
                None,
                Some((date("2026-11-18"), date("2026-11-01"))),
                ts(NOW),
            )
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::Invalid {
                code: "range_invalid",
                ..
            }
        ));
    }
}
