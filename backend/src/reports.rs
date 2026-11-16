//! Reports, CSV export and checks for imported data (docs/SPEC.md, sections 4.3, 5 and 4.4).
//! Pure functions on [`UserData`]; days are local days in the user's time zone.

use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde::Serialize;
use uuid::Uuid;

use crate::corrections::MAX_RANGE_DAYS;
use crate::domain::{
    CSV_ROUNDINGS, DomainError, Entry, UserData, WeekStart, invalid, normalize_tags,
    normalize_title,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub from: Date,
    pub to: Date,
    pub total_seconds: i64,
    /// One element per day of the range, also for days without time.
    pub days: Vec<DayTotal>,
    /// Tasks with time in the range, most time first.
    pub by_task: Vec<TaskTotal>,
    /// Variant A: every tag of a task gets the task's full time; `tag: null` is untagged.
    pub by_tag: Vec<TagTotal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayTotal {
    pub date: Date,
    pub total_seconds: i64,
    pub by_task: Vec<TaskSeconds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskSeconds {
    pub task_id: Uuid,
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTotal {
    pub task_id: Uuid,
    pub title: String,
    pub tags: Vec<String>,
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagTotal {
    pub tag: Option<String>,
    pub seconds: i64,
}

/// A piece of an entry inside one local day.
struct Segment<'a> {
    date: Date,
    start: Timestamp,
    end: Timestamp,
    task: usize,
    entry: &'a Entry,
}

fn day_start(date: Date, tz: &TimeZone) -> Result<Timestamp, DomainError> {
    date.to_zoned_datetime(tz.clone())
        .map(|z| z.timestamp())
        .map_err(|_| invalid("range_invalid", "Date out of range."))
}

fn next_day(date: Date) -> Result<Date, DomainError> {
    date.tomorrow()
        .map_err(|_| invalid("range_invalid", "Date out of range."))
}

pub fn check_range(from: Date, to: Date) -> Result<(), DomainError> {
    if to < from {
        return Err(invalid("range_invalid", "`to` must not be before `from`."));
    }
    if from.until(to).map(|s| s.get_days()).unwrap_or(i32::MAX) >= MAX_RANGE_DAYS {
        return Err(invalid(
            "range_too_long",
            format!("A range can cover at most {MAX_RANGE_DAYS} days."),
        ));
    }
    Ok(())
}

impl UserData {
    /// Entries cut at local midnights, limited to `from..=to`, oldest first.
    fn segments(
        &self,
        from: Date,
        to: Date,
        now: Timestamp,
    ) -> Result<Vec<Segment<'_>>, DomainError> {
        check_range(from, to)?;
        let tz = self.settings.time_zone();
        let range_start = day_start(from, &tz)?;
        let range_end = day_start(next_day(to)?, &tz)?;
        let mut out = Vec::new();
        for (t, task) in self.tasks.iter().enumerate() {
            for entry in &task.entries {
                let start = entry.start.max(range_start);
                let end = entry.end.unwrap_or(now).min(range_end);
                if end <= start {
                    continue;
                }
                let mut date = start.to_zoned_datetime(tz.clone()).date();
                let mut cursor = start;
                while cursor < end {
                    let boundary = day_start(next_day(date)?, &tz)?;
                    let piece_end = end.min(boundary);
                    out.push(Segment {
                        date,
                        start: cursor,
                        end: piece_end,
                        task: t,
                        entry,
                    });
                    cursor = piece_end;
                    date = next_day(date)?;
                }
            }
        }
        out.sort_by_key(|s| s.start);
        Ok(out)
    }

    pub fn report(&self, from: Date, to: Date, now: Timestamp) -> Result<Report, DomainError> {
        let segments = self.segments(from, to, now)?;
        let mut days: BTreeMap<Date, BTreeMap<usize, i64>> = BTreeMap::new();
        let mut date = from;
        loop {
            days.insert(date, BTreeMap::new());
            if date == to {
                break;
            }
            date = next_day(date)?;
        }
        let mut by_task: HashMap<usize, i64> = HashMap::new();
        let mut by_tag: HashMap<Option<String>, i64> = HashMap::new();
        let mut total = 0;
        for s in &segments {
            let seconds = s.end.as_second() - s.start.as_second();
            total += seconds;
            *days.entry(s.date).or_default().entry(s.task).or_default() += seconds;
            *by_task.entry(s.task).or_default() += seconds;
            let tags = &self.tasks[s.task].tags;
            if tags.is_empty() {
                *by_tag.entry(None).or_default() += seconds;
            }
            for tag in tags {
                *by_tag.entry(Some(tag.clone())).or_default() += seconds;
            }
        }
        let mut by_task: Vec<TaskTotal> = by_task
            .into_iter()
            .map(|(t, seconds)| TaskTotal {
                task_id: self.tasks[t].id,
                title: self.tasks[t].title.clone(),
                tags: self.tasks[t].tags.clone(),
                seconds,
            })
            .collect();
        by_task.sort_by(|a, b| {
            b.seconds
                .cmp(&a.seconds)
                .then_with(|| a.title.cmp(&b.title))
        });
        let mut by_tag: Vec<TagTotal> = by_tag
            .into_iter()
            .map(|(tag, seconds)| TagTotal { tag, seconds })
            .collect();
        // Untagged last among equals.
        by_tag.sort_by(|a, b| {
            b.seconds
                .cmp(&a.seconds)
                .then_with(|| a.tag.is_none().cmp(&b.tag.is_none()))
                .then_with(|| a.tag.cmp(&b.tag))
        });
        Ok(Report {
            from,
            to,
            total_seconds: total,
            days: days
                .into_iter()
                .map(|(date, tasks)| DayTotal {
                    date,
                    total_seconds: tasks.values().sum(),
                    by_task: tasks
                        .into_iter()
                        .map(|(t, seconds)| TaskSeconds {
                            task_id: self.tasks[t].id,
                            seconds,
                        })
                        .collect(),
                })
                .collect(),
            by_task,
            by_tag,
        })
    }

    /// CSV (section 5): one row per entry and day, `duration_minutes` rounded to `round` minutes
    /// (0 = exact). Times are local `HH:MM`; a piece ending at midnight ends at `24:00`.
    pub fn csv(
        &self,
        from: Date,
        to: Date,
        round: u8,
        now: Timestamp,
    ) -> Result<String, DomainError> {
        if !CSV_ROUNDINGS.contains(&round) {
            return Err(invalid(
                "round_invalid",
                "Rounding must be 0, 5 or 15 minutes.",
            ));
        }
        let tz = self.settings.time_zone();
        let mut out = String::from("date,task,tags,start,end,duration_minutes,note\n");
        for s in self.segments(from, to, now)? {
            let task = &self.tasks[s.task];
            let end_local = s.end.to_zoned_datetime(tz.clone());
            let end = if end_local.date() != s.date {
                "24:00".to_owned()
            } else {
                end_local.strftime("%H:%M").to_string()
            };
            let seconds = s.end.as_second() - s.start.as_second();
            let row = [
                s.date.to_string(),
                task.title.clone(),
                task.tags.join(" "),
                s.start
                    .to_zoned_datetime(tz.clone())
                    .strftime("%H:%M")
                    .to_string(),
                end,
                round_minutes(seconds, round).to_string(),
                s.entry.note.clone().unwrap_or_default(),
            ];
            let cells: Vec<String> = row.iter().map(|c| csv_cell(c)).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        Ok(out)
    }

    pub fn set_preferences(
        &mut self,
        week_start: Option<WeekStart>,
        csv_rounding: Option<u8>,
    ) -> Result<(), DomainError> {
        if let Some(r) = csv_rounding
            && !CSV_ROUNDINGS.contains(&r)
        {
            return Err(invalid(
                "round_invalid",
                "Rounding must be 0, 5 or 15 minutes.",
            ));
        }
        if let Some(w) = week_start {
            self.settings.week_start = w;
        }
        if let Some(r) = csv_rounding {
            self.settings.csv_rounding = r;
        }
        Ok(())
    }

    /// Checks imported data against the rules a running clockopus keeps: valid titles and tags,
    /// unique ids, every entry `start < end`, at most one running entry (R1), no overlaps (R5).
    pub fn validate(&self) -> Result<(), DomainError> {
        let mut ids = HashSet::new();
        let mut spans: Vec<(Timestamp, Option<Timestamp>, &str)> = Vec::new();
        for task in &self.tasks {
            normalize_title(&task.title)?;
            normalize_tags(&task.tags)?;
            if !ids.insert(task.id) {
                return Err(invalid(
                    "import_invalid",
                    format!("Task id {} appears twice.", task.id),
                ));
            }
            for e in &task.entries {
                if !ids.insert(e.id) {
                    return Err(invalid(
                        "import_invalid",
                        format!("Entry id {} appears twice.", e.id),
                    ));
                }
                if e.end.is_some_and(|end| end <= e.start) {
                    return Err(invalid(
                        "import_invalid",
                        format!("An entry of “{}” ends before it starts.", task.title),
                    ));
                }
                spans.push((e.start, e.end, &task.title));
            }
        }
        if spans.iter().filter(|s| s.1.is_none()).count() > 1 {
            return Err(invalid("import_invalid", "More than one timer is running."));
        }
        spans.sort_by_key(|s| s.0);
        for pair in spans.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.1.is_none_or(|end| end > b.0) {
                return Err(invalid(
                    "import_invalid",
                    format!("Entries of “{}” and “{}” overlap.", a.2, b.2),
                ));
            }
        }
        Ok(())
    }
}

fn round_minutes(seconds: i64, step: u8) -> i64 {
    let minutes = (seconds as f64) / 60.0;
    if step == 0 {
        return minutes.round() as i64;
    }
    let step = f64::from(step);
    ((minutes / step).round() * step) as i64
}

/// Quotes a CSV cell when needed and defuses spreadsheet formulas (`=`, `+`, `-`, `@`, tab, CR at
/// the start), so a task title cannot run as a formula in Excel or LibreOffice.
fn csv_cell(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if value.contains([',', '"', '\n', '\r']) || value.starts_with(' ') || value.ends_with(' ') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewTask;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    fn day(s: &str) -> Date {
        s.parse().unwrap()
    }

    /// Berlin user: "Review" #review #team-a 09:00–10:30 on the 16th, "Mail" (no tags)
    /// 23:00 on the 16th – 01:00 on the 17th (local).
    fn setup() -> UserData {
        let mut d = UserData::default();
        d.set_timezone("Europe/Berlin").unwrap();
        let now = ts("2026-11-18T12:00:00Z");
        let review = d
            .create_task(
                NewTask {
                    title: "Review".into(),
                    tags: vec!["review".into(), "team-a".into()],
                    ..Default::default()
                },
                ts("2026-11-16T07:00:00Z"),
            )
            .unwrap();
        let mail = d
            .create_task(
                NewTask {
                    title: "Mail".into(),
                    ..Default::default()
                },
                ts("2026-11-16T07:00:00Z"),
            )
            .unwrap();
        d.add_entry(
            review,
            ts("2026-11-16T08:00:00Z"),
            ts("2026-11-16T09:30:00Z"),
            None,
            now,
        )
        .unwrap();
        d.add_entry(
            mail,
            ts("2026-11-16T22:00:00Z"),
            ts("2026-11-17T00:00:00Z"),
            Some("=1+1"),
            now,
        )
        .unwrap();
        d
    }

    #[test]
    fn report_splits_at_local_midnight_and_counts_tags_fully() {
        let d = setup();
        let r = d
            .report(
                day("2026-11-16"),
                day("2026-11-18"),
                ts("2026-11-18T12:00:00Z"),
            )
            .unwrap();
        assert_eq!(r.total_seconds, 90 * 60 + 120 * 60);
        let per_day: Vec<(String, i64)> = r
            .days
            .iter()
            .map(|d| (d.date.to_string(), d.total_seconds))
            .collect();
        assert_eq!(
            per_day,
            vec![
                ("2026-11-16".into(), 90 * 60 + 60 * 60),
                ("2026-11-17".into(), 60 * 60),
                ("2026-11-18".into(), 0),
            ]
        );
        let tasks: Vec<(&str, i64)> = r
            .by_task
            .iter()
            .map(|t| (t.title.as_str(), t.seconds))
            .collect();
        assert_eq!(tasks, vec![("Mail", 7200), ("Review", 5400)]);
        let tags: Vec<(Option<&str>, i64)> = r
            .by_tag
            .iter()
            .map(|t| (t.tag.as_deref(), t.seconds))
            .collect();
        assert_eq!(
            tags,
            vec![(None, 7200), (Some("review"), 5400), (Some("team-a"), 5400)]
        );
    }

    #[test]
    fn report_counts_a_running_entry_until_now_and_checks_the_range() {
        let mut d = setup();
        let mail = d.tasks[1].id;
        d.start_task(mail, ts("2026-11-18T09:00:00Z")).unwrap();
        let r = d
            .report(
                day("2026-11-18"),
                day("2026-11-18"),
                ts("2026-11-18T09:45:00Z"),
            )
            .unwrap();
        assert_eq!(r.total_seconds, 45 * 60);
        assert!(
            d.report(
                day("2026-11-18"),
                day("2026-11-17"),
                ts("2026-11-18T12:00:00Z")
            )
            .is_err()
        );
        assert!(
            d.report(
                day("2025-01-01"),
                day("2026-11-17"),
                ts("2026-11-18T12:00:00Z")
            )
            .is_err()
        );
    }

    #[test]
    fn report_on_the_25_hour_day() {
        let mut d = UserData::default();
        d.set_timezone("Europe/Berlin").unwrap();
        let a = d
            .create_task(
                NewTask {
                    title: "Night".into(),
                    ..Default::default()
                },
                ts("2026-10-24T20:00:00Z"),
            )
            .unwrap();
        // The whole local 25 October: 22:00 UTC on the 24th to 23:00 UTC on the 25th.
        d.add_entry(
            a,
            ts("2026-10-24T22:00:00Z"),
            ts("2026-10-25T23:00:00Z"),
            None,
            ts("2026-11-01T00:00:00Z"),
        )
        .unwrap();
        let r = d
            .report(
                day("2026-10-24"),
                day("2026-10-26"),
                ts("2026-11-01T00:00:00Z"),
            )
            .unwrap();
        let per_day: Vec<i64> = r.days.iter().map(|d| d.total_seconds).collect();
        assert_eq!(per_day, vec![0, 25 * 3600, 0]);
    }

    #[test]
    fn csv_rows_per_day_with_local_times_and_safe_cells() {
        let d = setup();
        let csv = d
            .csv(
                day("2026-11-16"),
                day("2026-11-17"),
                0,
                ts("2026-11-18T12:00:00Z"),
            )
            .unwrap();
        assert_eq!(
            csv,
            "date,task,tags,start,end,duration_minutes,note\n\
             2026-11-16,Review,review team-a,09:00,10:30,90,\n\
             2026-11-16,Mail,,23:00,24:00,60,'=1+1\n\
             2026-11-17,Mail,,00:00,01:00,60,'=1+1\n"
        );
    }

    #[test]
    fn csv_rounding_to_the_nearest_step() {
        assert_eq!(round_minutes(7 * 60, 5), 5);
        assert_eq!(round_minutes(8 * 60, 5), 10);
        assert_eq!(round_minutes(22 * 60 + 29, 15), 15);
        assert_eq!(round_minutes(22 * 60 + 30, 15), 30);
        assert_eq!(round_minutes(89, 0), 1);
        let d = setup();
        assert!(
            d.csv(
                day("2026-11-16"),
                day("2026-11-16"),
                10,
                ts("2026-11-18T12:00:00Z")
            )
            .is_err()
        );
    }

    #[test]
    fn csv_cells_are_quoted_when_needed() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a, b"), "\"a, b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("-2"), "'-2");
        assert_eq!(csv_cell("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_cell("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn preferences_are_checked() {
        let mut d = UserData::default();
        d.set_preferences(Some(WeekStart::Sunday), Some(15))
            .unwrap();
        assert_eq!(d.settings.week_start, WeekStart::Sunday);
        assert_eq!(d.settings.csv_rounding, 15);
        assert!(d.set_preferences(None, Some(7)).is_err());
        assert_eq!(d.settings.csv_rounding, 15);
    }

    #[test]
    fn validate_accepts_consistent_data_and_names_problems() {
        let d = setup();
        d.validate().unwrap();

        let mut overlap = d.clone();
        overlap.tasks[0].entries[0].end = Some(ts("2026-11-16T23:00:00Z"));
        let err = format!("{:?}", overlap.validate());
        assert!(err.contains("overlap"), "{err}");

        let mut two_running = d.clone();
        two_running.tasks[0].entries[0].end = None;
        two_running.tasks[1].entries[0].end = None;
        assert!(format!("{:?}", two_running.validate()).contains("More than one timer"));

        let mut dup = d.clone();
        dup.tasks[1].id = dup.tasks[0].id;
        assert!(format!("{:?}", dup.validate()).contains("appears twice"));

        let mut backwards = d.clone();
        backwards.tasks[0].entries[0].end = Some(ts("2026-11-16T07:00:00Z"));
        assert!(format!("{:?}", backwards.validate()).contains("ends before it starts"));
    }
}
