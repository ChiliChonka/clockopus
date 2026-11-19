//! Handlers for tasks, timer, tags and settings (docs/SPEC.md, section 7).
//!
//! Wire format: JSON, times in RFC 3339 (UTC with `Z`), durations in whole seconds.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::{ApiError, AppState, User, body};
use jiff::civil::{Date, Time};

use crate::corrections::{EntryPatch, EntryView, Forgotten};
use crate::domain::{
    ChangeReason, DomainError, NewTask, Running, StatusFilter, TagView, TaskPatch, TaskStatus,
    TaskView, UserData, WeekStart,
};
use crate::notes::NoteView;
use crate::reports::Report;
use crate::store::yaml::{export_yaml, import_yaml};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    status: Option<String>,
}

pub async fn list_tasks(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TaskView>>, ApiError> {
    let filter = match q.status.as_deref().unwrap_or("active") {
        "active" => StatusFilter::Active,
        "done" => StatusFilter::Done,
        "all" => StatusFilter::All,
        other => {
            return Err(ApiError::Invalid {
                code: "status_invalid",
                message: format!("status must be active, done or all, not `{other}`"),
            });
        }
    };
    let now = Timestamp::now();
    Ok(Json(
        state
            .store
            .read(&user.key, |d| d.task_views(filter, now))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTask {
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    link: Option<String>,
    /// Start the timer for the new task right away (R2).
    #[serde(default)]
    start: bool,
}

pub async fn create_task(
    State(state): State<AppState>,
    user: User,
    payload: Result<Json<CreateTask>, JsonRejection>,
) -> Result<(StatusCode, Json<TaskView>), ApiError> {
    let input = body(payload)?;
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            let id = d.create_task(
                NewTask {
                    title: input.title,
                    tags: input.tags,
                    link: input.link,
                    start: input.start,
                },
                now,
            )?;
            Ok(task_view(d, id, now))
        })
        .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

fn task_view(d: &crate::domain::UserData, id: Uuid, now: Timestamp) -> TaskView {
    d.task_views(StatusFilter::All, now)
        .into_iter()
        .find(|v| v.id == id)
        .expect("the task exists")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTask {
    title: Option<String>,
    tags: Option<Vec<String>>,
    /// An empty string removes the link.
    link: Option<String>,
    /// Several lines; an empty string removes the notes.
    notes: Option<String>,
    pinned: Option<bool>,
    status: Option<TaskStatus>,
}

pub async fn update_task(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
    payload: Result<Json<UpdateTask>, JsonRejection>,
) -> Result<Json<TaskView>, ApiError> {
    let input = body(payload)?;
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            d.update_task(
                id,
                TaskPatch {
                    title: input.title,
                    tags: input.tags,
                    link: input.link,
                    notes: input.notes,
                    pinned: input.pinned,
                    status: input.status,
                },
                now,
            )?;
            Ok(task_view(d, id, now))
        })
        .await?;
    Ok(Json(view))
}

pub async fn delete_task(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.store.write(&user.key, |d| d.delete_task(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `{"running": null | {…}, "forgotten": null | {…}}` — `forgotten` is set when R7 applies.
#[derive(Debug, Serialize)]
pub struct TimerView {
    running: Option<RunningView>,
    forgotten: Option<Forgotten>,
}

#[derive(Debug, Serialize)]
pub struct RunningView {
    task_id: Uuid,
    entry_id: Uuid,
    start: Timestamp,
}

fn timer_view(d: &UserData, now: Timestamp) -> TimerView {
    TimerView {
        running: d.running().map(|r: Running| RunningView {
            task_id: r.task_id,
            entry_id: r.entry_id,
            start: r.start,
        }),
        forgotten: d.forgotten(now),
    }
}

pub async fn start_task(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
) -> Result<Json<TimerView>, ApiError> {
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            d.start_task(id, now)?;
            Ok(timer_view(d, now))
        })
        .await?;
    Ok(Json(view))
}

pub async fn get_timer(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<TimerView>, ApiError> {
    let now = Timestamp::now();
    Ok(Json(
        state.store.read(&user.key, |d| timer_view(d, now)).await?,
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopTimer {
    /// Optional end in the past; default now.
    #[serde(default)]
    end: Option<Timestamp>,
    /// `forgotten-timer` for the R8 correction; it needs `end`.
    #[serde(default)]
    reason: Option<ChangeReason>,
}

/// Stops the running timer. Stopping when nothing runs is not an error: the result is the same.
pub async fn stop_timer(
    State(state): State<AppState>,
    user: User,
    payload: Option<Json<StopTimer>>,
) -> Result<Json<TimerView>, ApiError> {
    let input = payload.map(|Json(v)| v).unwrap_or_default();
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            match (input.reason, input.end) {
                (Some(ChangeReason::ForgottenTimer), Some(end)) => d.stop_forgotten(end, now)?,
                (Some(ChangeReason::ForgottenTimer), None) => {
                    return Err(DomainError::Invalid {
                        code: "end_required",
                        message: "Correcting a forgotten timer needs an end time.".into(),
                    });
                }
                (Some(ChangeReason::Edit), _) | (None, _) => {
                    d.stop_timer(input.end, now)?;
                }
            }
            Ok(timer_view(d, now))
        })
        .await?;
    Ok(Json(view))
}

/// R8 “Keep running”.
pub async fn keep_running(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<TimerView>, ApiError> {
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            d.keep_running()?;
            Ok(timer_view(d, now))
        })
        .await?;
    Ok(Json(view))
}

pub async fn list_task_entries(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<EntryView>>, ApiError> {
    let entries = state
        .store
        .read(&user.key, |d| d.entries_of_task(id))
        .await??;
    Ok(Json(entries))
}

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    from: Date,
    to: Date,
}

/// Entries touching the local days `from..=to` (`YYYY-MM-DD`, user's time zone).
pub async fn list_entries(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<EntryView>>, ApiError> {
    let now = Timestamp::now();
    let entries = state
        .store
        .read(&user.key, |d| d.entries_between(q.from, q.to, now))
        .await??;
    Ok(Json(entries))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddEntry {
    start: Timestamp,
    end: Timestamp,
    #[serde(default)]
    note: Option<String>,
}

pub async fn add_entry(
    State(state): State<AppState>,
    user: User,
    Path(task_id): Path<Uuid>,
    payload: Result<Json<AddEntry>, JsonRejection>,
) -> Result<(StatusCode, Json<EntryView>), ApiError> {
    let input = body(payload)?;
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            let id = d.add_entry(task_id, input.start, input.end, input.note.as_deref(), now)?;
            entry_view(d, task_id, id)
        })
        .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

fn entry_view(d: &UserData, task_id: Uuid, id: Uuid) -> Result<EntryView, DomainError> {
    d.entries_of_task(task_id)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or(DomainError::NotFound)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateEntry {
    start: Option<Timestamp>,
    end: Option<Timestamp>,
    /// An empty string removes the note.
    note: Option<String>,
}

pub async fn update_entry(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
    payload: Result<Json<UpdateEntry>, JsonRejection>,
) -> Result<Json<EntryView>, ApiError> {
    let input = body(payload)?;
    let now = Timestamp::now();
    let view = state
        .store
        .write(&user.key, |d| {
            d.update_entry(
                id,
                EntryPatch {
                    start: input.start,
                    end: input.end,
                    note: input.note,
                },
                ChangeReason::Edit,
                now,
            )?;
            let task_id = d
                .tasks
                .iter()
                .find(|t| t.entries.iter().any(|e| e.id == id))
                .map(|t| t.id)
                .ok_or(DomainError::NotFound)?;
            entry_view(d, task_id, id)
        })
        .await?;
    Ok(Json(view))
}

pub async fn delete_entry(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.store.write(&user.key, |d| d.delete_entry(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_tags(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<Vec<TagView>>, ApiError> {
    let now = Timestamp::now();
    Ok(Json(
        state.store.read(&user.key, |d| d.tag_views(now)).await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    timezone: Option<String>,
    /// `HH:MM`
    workday_end: String,
    forgotten_after_hours: u8,
    week_start: WeekStart,
    /// Default CSV rounding in minutes: 0, 5 or 15.
    csv_rounding: u8,
}

fn settings_view(d: &UserData) -> SettingsView {
    SettingsView {
        timezone: d.settings.timezone.clone(),
        workday_end: d.settings.workday_end.strftime("%H:%M").to_string(),
        forgotten_after_hours: d.settings.forgotten_after_hours,
        week_start: d.settings.week_start,
        csv_rounding: d.settings.csv_rounding,
    }
}

/// Only the given fields change.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    workday_end: Option<String>,
    #[serde(default)]
    forgotten_after_hours: Option<u8>,
    #[serde(default)]
    week_start: Option<WeekStart>,
    #[serde(default)]
    csv_rounding: Option<u8>,
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(state.store.read(&user.key, settings_view).await?))
}

pub async fn put_settings(
    State(state): State<AppState>,
    user: User,
    payload: Result<Json<SettingsPatch>, JsonRejection>,
) -> Result<Json<SettingsView>, ApiError> {
    let input = body(payload)?;
    let workday_end = input
        .workday_end
        .as_deref()
        .map(|t| {
            t.parse::<Time>().map_err(|_| ApiError::Invalid {
                code: "workday_end_invalid",
                message: format!("`{t}` is not a time like 17:00."),
            })
        })
        .transpose()?;
    let view = state
        .store
        .write(&user.key, |d| {
            if let Some(tz) = input.timezone.as_deref() {
                d.set_timezone(tz)?;
            }
            d.set_workday(workday_end, input.forgotten_after_hours)?;
            d.set_preferences(input.week_start, input.csv_rounding)?;
            Ok(settings_view(d))
        })
        .await?;
    Ok(Json(view))
}
/// Totals for the local days `from..=to` (week, month or any range up to 366 days).
pub async fn get_report(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Report>, ApiError> {
    let now = Timestamp::now();
    let report = state
        .store
        .read(&user.key, |d| d.report(q.from, q.to, now))
        .await??;
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct NotesQuery {
    /// One tag; missing or empty: all tags.
    #[serde(default)]
    tag: Option<String>,
    /// Local days `from..=to`; both missing: all time.
    #[serde(default)]
    from: Option<Date>,
    #[serde(default)]
    to: Option<Date>,
}

/// Tasks with notes for the notes view, newest activity first.
pub async fn list_notes(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<NotesQuery>,
) -> Result<Json<Vec<NoteView>>, ApiError> {
    let period = match (q.from, q.to) {
        (Some(from), Some(to)) => Some((from, to)),
        (None, None) => None,
        _ => {
            return Err(ApiError::Invalid {
                code: "range_invalid",
                message: "Give both `from` and `to`, or neither for all time.".into(),
            });
        }
    };
    let now = Timestamp::now();
    let notes = state
        .store
        .read(&user.key, |d| d.notes(q.tag.as_deref(), period, now))
        .await??;
    Ok(Json(notes))
}

#[derive(Debug, Deserialize)]
pub struct CsvQuery {
    from: Date,
    to: Date,
    /// Minutes: 0, 5 or 15; default from the settings.
    #[serde(default)]
    round: Option<u8>,
}

pub async fn export_csv(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<CsvQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let now = Timestamp::now();
    let csv = state
        .store
        .read(&user.key, |d| {
            d.csv(
                q.from,
                q.to,
                q.round.unwrap_or(d.settings.csv_rounding),
                now,
            )
        })
        .await??;
    let name = format!("clockopus-{}-{}.csv", q.from, q.to);
    Ok(download(csv, "text/csv; charset=utf-8", &name))
}

/// Everything of the user as one YAML document (settings, tags, tasks with entries).
pub async fn export_data(
    State(state): State<AppState>,
    user: User,
) -> Result<impl IntoResponse, ApiError> {
    let now = Timestamp::now();
    let (yaml, date) = state
        .store
        .read(&user.key, |d| {
            let date = now.to_zoned_datetime(d.settings.time_zone()).date();
            (export_yaml(d, now), date)
        })
        .await?;
    let yaml = yaml.map_err(|e| {
        tracing::error!(error = format!("{e:#}"), "export failed");
        ApiError::Internal
    })?;
    Ok(download(
        yaml,
        "application/yaml; charset=utf-8",
        &format!("clockopus-export-{date}.yaml"),
    ))
}

/// Imports an export into an empty account; anything else is refused so nothing is mixed.
pub async fn import_data(
    State(state): State<AppState>,
    user: User,
    body: String,
) -> Result<StatusCode, ApiError> {
    let imported = import_yaml(&body)?;
    state
        .store
        .write(&user.key, |d| {
            if !d.tasks.is_empty() || !d.tags.is_empty() {
                return Err(DomainError::Invalid {
                    code: "account_not_empty",
                    message:
                        "Import works only into an empty account. Delete the current data first."
                            .into(),
                });
            }
            *d = imported;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes all data of the user (`DELETE /me`).
pub async fn delete_me(State(state): State<AppState>, user: User) -> Result<StatusCode, ApiError> {
    state.store.delete_user(&user.key).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn download(body: String, content_type: &'static str, filename: &str) -> axum::response::Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use crate::app::tests::{call, test_app};
    use crate::config::AuthMode;
    use axum::http::StatusCode;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    fn json(s: &str) -> Value {
        serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
    }

    /// The literals the frontend sends (frontend/src/app/core/wire.ts and its spec).
    const CREATE_AND_START: &str =
        r#"{"title":"Review payment PR","tags":["review","team-a"],"start":true}"#;
    const CREATE_ONLY: &str = r#"{"title":"Write notes","tags":[],"start":false}"#;

    #[tokio::test]
    async fn create_start_switch_stop_through_the_api() {
        let t = test_app(AuthMode::None);
        let app = &t.app;

        let (status, body) = call(app, "POST", "/api/v1/tasks", Some(CREATE_AND_START)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let a = json(&body);
        assert_eq!(a["title"], "Review payment PR");
        assert_eq!(a["tags"], serde_json::json!(["review", "team-a"]));
        assert_eq!(a["status"], "active");
        assert!(a["running_since"].is_string(), "{a}");
        let a_id = a["id"].as_str().unwrap().to_owned();

        let (status, body) = call(app, "POST", "/api/v1/tasks", Some(CREATE_ONLY)).await;
        assert_eq!(status, StatusCode::CREATED);
        let b_id = json(&body)["id"].as_str().unwrap().to_owned();
        assert!(json(&body)["running_since"].is_null());

        // Switch: B runs, A stopped.
        let (status, body) = call(app, "POST", &format!("/api/v1/tasks/{b_id}/start"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json(&body)["running"]["task_id"], b_id.as_str());

        let (_, body) = call(app, "GET", "/api/v1/tasks", None).await;
        let tasks = json(&body);
        assert_eq!(tasks.as_array().unwrap().len(), 2);
        assert_eq!(tasks[0]["id"], b_id.as_str(), "running task first");
        assert!(tasks[1]["running_since"].is_null());
        assert_eq!(tasks[1]["id"], a_id.as_str());

        let (status, body) = call(app, "POST", "/api/v1/timer/stop", Some("{}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"running":null,"forgotten":null}"#);
        let (_, body) = call(app, "GET", "/api/v1/timer", None).await;
        assert_eq!(body, r#"{"running":null,"forgotten":null}"#);

        // Stop without a body works too, and stopping twice is fine.
        let (status, _) = call(app, "POST", "/api/v1/timer/stop", None).await;
        assert_eq!(status, StatusCode::OK);

        // The data is on disk, readable.
        let file = std::fs::read_to_string(t.data.path().join("local/tasks.yaml")).unwrap();
        assert!(file.contains("title: Review payment PR"), "{file}");
    }

    #[tokio::test]
    async fn tags_list_and_quick_access_order() {
        let t = test_app(AuthMode::None);
        call(&t.app, "POST", "/api/v1/tasks", Some(CREATE_AND_START)).await;
        let (status, body) = call(&t.app, "GET", "/api/v1/tags", None).await;
        assert_eq!(status, StatusCode::OK);
        let tags = json(&body);
        let names: Vec<&str> = tags
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["review", "team-a"]);
        assert_eq!(tags[0]["task_count"], 1);
    }

    #[tokio::test]
    async fn invalid_input_is_a_400_with_a_code() {
        let t = test_app(AuthMode::None);
        let mut checked = 0;
        for (body, code) in [
            (r#"{"title":"  "}"#, "title_empty"),
            (r#"{"title":"x","tags":["two words"]}"#, "tag_whitespace"),
            (r#"{"title":"x","link":"ftp://x"}"#, "link_invalid"),
            (r#"{"title":"x","unknown":1}"#, "bad_request"),
            (r#"{"title":"#, "bad_request"),
        ] {
            let (status, res) = call(&t.app, "POST", "/api/v1/tasks", Some(body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(json(&res)["error"], code, "{body}");
            assert!(json(&res)["message"].is_string(), "{res}");
            checked += 1;
        }
        assert_eq!(checked, 5);
        let (_, body) = call(&t.app, "GET", "/api/v1/tasks?status=all", None).await;
        assert_eq!(body, "[]", "nothing was created");
    }

    #[tokio::test]
    async fn unknown_task_is_404() {
        let t = test_app(AuthMode::None);
        let id = uuid::Uuid::now_v7();
        for (method, path, body) in [
            ("POST", format!("/api/v1/tasks/{id}/start"), None),
            (
                "PATCH",
                format!("/api/v1/tasks/{id}"),
                Some(r#"{"pinned":true}"#),
            ),
            ("DELETE", format!("/api/v1/tasks/{id}"), None),
        ] {
            let (status, _) = call(&t.app, method, &path, body).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        }
    }

    #[tokio::test]
    async fn pin_done_reopen_delete() {
        let t = test_app(AuthMode::None);
        let (_, body) = call(&t.app, "POST", "/api/v1/tasks", Some(CREATE_AND_START)).await;
        let id = json(&body)["id"].as_str().unwrap().to_owned();
        let path = format!("/api/v1/tasks/{id}");

        let (_, body) = call(&t.app, "PATCH", &path, Some(r#"{"pinned":true}"#)).await;
        assert_eq!(json(&body)["pinned"], true);

        let (_, body) = call(&t.app, "PATCH", &path, Some(r#"{"status":"done"}"#)).await;
        assert_eq!(json(&body)["status"], "done");
        assert!(
            json(&body)["running_since"].is_null(),
            "done stops the timer"
        );
        let (_, body) = call(&t.app, "GET", "/api/v1/tasks", None).await;
        assert_eq!(body, "[]", "done tasks are not on the board");
        let (_, body) = call(&t.app, "GET", "/api/v1/tasks?status=done", None).await;
        assert_eq!(json(&body).as_array().unwrap().len(), 1);

        // Starting a done task reopens it.
        call(&t.app, "POST", &format!("{path}/start"), None).await;
        let (_, body) = call(&t.app, "GET", "/api/v1/tasks", None).await;
        assert_eq!(json(&body)[0]["status"], "active");

        let (status, _) = call(&t.app, "DELETE", &path, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, body) = call(&t.app, "GET", "/api/v1/tasks?status=all", None).await;
        assert_eq!(body, "[]");
        let (_, body) = call(&t.app, "GET", "/api/v1/timer", None).await;
        assert_eq!(body, r#"{"running":null,"forgotten":null}"#);
    }

    #[tokio::test]
    async fn settings_change_only_the_given_fields() {
        let t = test_app(AuthMode::None);
        let (_, body) = call(&t.app, "GET", "/api/v1/me/settings", None).await;
        assert_eq!(
            body,
            r#"{"timezone":null,"workday_end":"17:00","forgotten_after_hours":10,"week_start":"monday","csv_rounding":0}"#
        );
        let (status, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"timezone":"Europe/Berlin"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            r#"{"timezone":"Europe/Berlin","workday_end":"17:00","forgotten_after_hours":10,"week_start":"monday","csv_rounding":0}"#
        );
        let (_, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"workday_end":"16:30","forgotten_after_hours":8,"week_start":"monday","csv_rounding":0}"#),
        )
        .await;
        assert_eq!(
            body,
            r#"{"timezone":"Europe/Berlin","workday_end":"16:30","forgotten_after_hours":8,"week_start":"monday","csv_rounding":0}"#
        );
        let mut rejected = 0;
        for (input, code) in [
            (r#"{"timezone":"Mars/Olympus"}"#, "timezone_unknown"),
            (r#"{"workday_end":"5 pm"}"#, "workday_end_invalid"),
            (r#"{"forgotten_after_hours":0}"#, "forgotten_after_invalid"),
        ] {
            let (status, body) = call(&t.app, "PUT", "/api/v1/me/settings", Some(input)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{input}");
            assert_eq!(json(&body)["error"], code, "{input}");
            rejected += 1;
        }
        assert_eq!(rejected, 3);
        let (_, body) = call(&t.app, "GET", "/api/v1/me/settings", None).await;
        assert_eq!(
            body,
            r#"{"timezone":"Europe/Berlin","workday_end":"16:30","forgotten_after_hours":8,"week_start":"monday","csv_rounding":0}"#
        );
    }

    #[tokio::test]
    async fn stop_rejects_an_end_in_the_future() {
        let t = test_app(AuthMode::None);
        call(&t.app, "POST", "/api/v1/tasks", Some(CREATE_AND_START)).await;
        let (status, body) = call(
            &t.app,
            "POST",
            "/api/v1/timer/stop",
            Some(r#"{"end":"2999-01-01T00:00:00Z"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json(&body)["error"], "end_in_future");
    }

    /// Literals the frontend sends (frontend/src/app/core/wire.spec.ts): `Date.toISOString()`
    /// writes milliseconds.
    const ADD_ENTRY: &str = r#"{"start":"2026-11-10T08:00:00.000Z","end":"2026-11-10T09:30:00.000Z","note":"call with Anna"}"#;
    const ADD_OVERLAPPING: &str =
        r#"{"start":"2026-11-10T09:00:00.000Z","end":"2026-11-10T10:00:00.000Z"}"#;
    const MOVE_END: &str = r#"{"end":"2026-11-10T10:00:00.000Z"}"#;

    async fn task_id(app: &axum::Router) -> String {
        let (_, body) = call(app, "POST", "/api/v1/tasks", Some(CREATE_ONLY)).await;
        json(&body)["id"].as_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn manual_entries_overlaps_and_history() {
        let t = test_app(AuthMode::None);
        let app = &t.app;
        let task = task_id(app).await;
        let entries = format!("/api/v1/tasks/{task}/entries");

        let (status, body) = call(app, "POST", &entries, Some(ADD_ENTRY)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let entry = json(&body);
        assert_eq!(entry["start"], "2026-11-10T08:00:00Z");
        assert_eq!(entry["end"], "2026-11-10T09:30:00Z");
        assert_eq!(entry["source"], "manual");
        assert_eq!(entry["note"], "call with Anna");
        assert_eq!(entry["task_title"], "Write notes");
        assert_eq!(entry["history"], serde_json::json!([]));
        let entry_id = entry["id"].as_str().unwrap().to_owned();

        // R5: 409 names the other entry.
        let (status, body) = call(app, "POST", &entries, Some(ADD_OVERLAPPING)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(json(&body)["error"], "overlap");
        assert_eq!(json(&body)["entry_id"], entry_id.as_str());

        // R6: moving the end keeps the old values.
        let path = format!("/api/v1/entries/{entry_id}");
        let (status, body) = call(app, "PATCH", &path, Some(MOVE_END)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let changed = json(&body);
        assert_eq!(changed["end"], "2026-11-10T10:00:00Z");
        assert_eq!(changed["history"][0]["reason"], "edit");
        assert_eq!(changed["history"][0]["end"], "2026-11-10T09:30:00Z");

        let (_, body) = call(app, "GET", &entries, None).await;
        assert_eq!(json(&body).as_array().unwrap().len(), 1);

        // Day view: the entry lies on 2026-11-10 in UTC (no time zone set yet).
        let (status, body) = call(
            app,
            "GET",
            "/api/v1/entries?from=2026-11-10&to=2026-11-10",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json(&body).as_array().unwrap().len(), 1);
        let (_, body) = call(
            app,
            "GET",
            "/api/v1/entries?from=2026-11-11&to=2026-11-12",
            None,
        )
        .await;
        assert_eq!(body, "[]");

        let (status, _) = call(app, "DELETE", &path, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = call(app, "DELETE", &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// A timer running since 2026-11-10 08:00 Berlin, written as the YAML backend would.
    fn forgotten_timer_files(dir: &std::path::Path) {
        let user = dir.join("local");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(
            user.join("settings.yaml"),
            "version: 1\ntimezone: Europe/Berlin\n",
        )
        .unwrap();
        std::fs::write(
            user.join("tasks.yaml"),
            r#"version: 1
tasks:
  - id: 0193a1c2-7d40-7b1e-9c3a-2f1d5e6a8b90
    title: Fix login bug
    status: active
    created_at: "2026-11-10T08:00:00+01:00"
    entries:
      - id: 0193a1c2-7d41-7c00-8d2e-1a2b3c4d5e6f
        start: "2026-11-10T08:00:00+01:00"
        end: null
        source: timer
"#,
        )
        .unwrap();
    }

    #[tokio::test]
    async fn forgotten_timer_is_reported_and_corrected_with_history() {
        let t = test_app(AuthMode::None);
        forgotten_timer_files(t.data.path());
        let (_, body) = call(&t.app, "GET", "/api/v1/timer", None).await;
        let timer = json(&body);
        assert_eq!(
            timer["running"]["entry_id"],
            "0193a1c2-7d41-7c00-8d2e-1a2b3c4d5e6f"
        );
        let f = &timer["forgotten"];
        assert_eq!(f["task_title"], "Fix login bug");
        assert_eq!(f["start"], "2026-11-10T07:00:00Z");
        assert_eq!(f["suggested_end"], "2026-11-10T16:00:00Z", "17:00 Berlin");
        assert!(f["running_seconds"].as_i64().unwrap() > 24 * 3600);

        // Without an end the correction is refused.
        let (status, body) = call(
            &t.app,
            "POST",
            "/api/v1/timer/stop",
            Some(r#"{"reason":"forgotten-timer"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json(&body)["error"], "end_required");

        let (status, body) = call(
            &t.app,
            "POST",
            "/api/v1/timer/stop",
            Some(r#"{"end":"2026-11-10T16:00:00.000Z","reason":"forgotten-timer"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, r#"{"running":null,"forgotten":null}"#);

        let (_, body) = call(
            &t.app,
            "GET",
            "/api/v1/tasks/0193a1c2-7d40-7b1e-9c3a-2f1d5e6a8b90/entries",
            None,
        )
        .await;
        let entry = &json(&body)[0];
        assert_eq!(entry["end"], "2026-11-10T16:00:00Z");
        assert_eq!(entry["history"][0]["reason"], "forgotten-timer");
        assert!(entry["history"][0]["end"].is_null());

        // The YAML file shows the correction in local time.
        let file = std::fs::read_to_string(t.data.path().join("local/tasks.yaml")).unwrap();
        assert!(file.contains("reason: forgotten-timer"), "{file}");
        assert!(
            file.contains(r#"end: "2026-11-10T17:00:00+01:00""#),
            "{file}"
        );
    }

    #[tokio::test]
    async fn keep_running_silences_the_prompt() {
        let t = test_app(AuthMode::None);
        forgotten_timer_files(t.data.path());
        let (status, body) = call(&t.app, "POST", "/api/v1/timer/keep-running", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(json(&body)["running"].is_object());
        assert!(json(&body)["forgotten"].is_null());
        let (_, body) = call(&t.app, "GET", "/api/v1/timer", None).await;
        assert!(json(&body)["forgotten"].is_null());
    }

    #[tokio::test]
    async fn week_start_and_csv_rounding_are_settings() {
        let t = test_app(AuthMode::None);
        let (status, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"week_start":"sunday","csv_rounding":15}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(json(&body)["week_start"], "sunday");
        assert_eq!(json(&body)["csv_rounding"], 15);
        let (status, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"csv_rounding":10}"#),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json(&body)["error"], "round_invalid");
    }

    /// One task with an entry on 2026-11-10 08:00–09:30 UTC (no time zone set: days in UTC).
    async fn with_entry(app: &axum::Router) -> String {
        let task = task_id(app).await;
        let (status, _) = call(
            app,
            "POST",
            &format!("/api/v1/tasks/{task}/entries"),
            Some(ADD_ENTRY),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        task
    }

    #[tokio::test]
    async fn report_for_a_range() {
        let t = test_app(AuthMode::None);
        let task = with_entry(&t.app).await;
        let (status, body) = call(
            &t.app,
            "GET",
            "/api/v1/reports?from=2026-11-09&to=2026-11-11",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let r = json(&body);
        assert_eq!(r["total_seconds"], 5400);
        assert_eq!(r["days"].as_array().unwrap().len(), 3);
        assert_eq!(r["days"][1]["date"], "2026-11-10");
        assert_eq!(r["days"][1]["by_task"][0]["task_id"], task.as_str());
        assert_eq!(r["by_task"][0]["title"], "Write notes");
        assert_eq!(r["by_tag"][0]["tag"], serde_json::Value::Null);
        let (status, _) = call(
            &t.app,
            "GET",
            "/api/v1/reports?from=2026-11-11&to=2026-11-09",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn csv_download() {
        let t = test_app(AuthMode::None);
        with_entry(&t.app).await;
        let res = t
            .app
            .clone()
            .oneshot(
                axum::http::Request::get(
                    "/api/v1/export.csv?from=2026-11-10&to=2026-11-10&round=15",
                )
                .body(axum::body::Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["content-type"], "text/csv; charset=utf-8");
        assert_eq!(
            res.headers()["content-disposition"],
            "attachment; filename=\"clockopus-2026-11-10-2026-11-10.csv\""
        );
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            String::from_utf8(body.to_vec()).unwrap(),
            "date,task,tags,start,end,duration_minutes,note\n2026-11-10,Write notes,,08:00,09:30,90,call with Anna\n"
        );
    }

    #[tokio::test]
    async fn export_import_round_trip_into_an_empty_account_only() {
        let source = test_app(AuthMode::None);
        call(
            &source.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"timezone":"Europe/Berlin"}"#),
        )
        .await;
        with_entry(&source.app).await;
        let (status, export) = call(&source.app, "GET", "/api/v1/export", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(export.starts_with("# clockopus export."), "{export}");
        assert!(export.contains("title: Write notes"), "{export}");
        assert!(
            export.contains(r#"start: "2026-11-10T09:00:00+01:00""#),
            "{export}"
        );

        // Not into an account with data.
        let (status, body) = call(&source.app, "POST", "/api/v1/import", Some(&export)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json(&body)["error"], "account_not_empty");

        let target = test_app(AuthMode::None);
        let (status, body) = call(&target.app, "POST", "/api/v1/import", Some(&export)).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let (_, a) = call(&source.app, "GET", "/api/v1/tasks?status=all", None).await;
        let (_, b) = call(&target.app, "GET", "/api/v1/tasks?status=all", None).await;
        assert_eq!(json(&a)[0]["id"], json(&b)[0]["id"]);
        assert_eq!(json(&a)[0]["total_seconds"], json(&b)[0]["total_seconds"]);
        let (_, settings) = call(&target.app, "GET", "/api/v1/me/settings", None).await;
        assert_eq!(json(&settings)["timezone"], "Europe/Berlin");
        // And on disk.
        assert!(target.data.path().join("local/tasks.yaml").exists());
    }

    #[tokio::test]
    async fn a_broken_import_is_the_uploaders_problem() {
        let t = test_app(AuthMode::None);
        let mut checked = 0;
        for body in [
            "not: [yaml",
            "version: 1\nexported_at: x\n",
            "version: 9\nexported_at: x\nsettings: {version: 1}\n",
        ] {
            let (status, res) = call(&t.app, "POST", "/api/v1/import", Some(body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {res}");
            assert_eq!(json(&res)["error"], "import_invalid", "{body}");
            checked += 1;
        }
        assert_eq!(checked, 3);
        let (_, tasks) = call(&t.app, "GET", "/api/v1/tasks?status=all", None).await;
        assert_eq!(tasks, "[]");
    }

    #[tokio::test]
    async fn deleting_the_account_removes_everything() {
        let t = test_app(AuthMode::None);
        with_entry(&t.app).await;
        assert!(t.data.path().join("local").exists());
        let (status, _) = call(&t.app, "DELETE", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!t.data.path().join("local").exists(), "files gone");
        let (_, tasks) = call(&t.app, "GET", "/api/v1/tasks?status=all", None).await;
        assert_eq!(tasks, "[]", "memory gone");
        let (_, tags) = call(&t.app, "GET", "/api/v1/tags", None).await;
        assert_eq!(tags, "[]");
    }

    /// What the edit dialog sends on “Save task” (frontend/src/app/core/wire.spec.ts).
    const SAVE_WITH_NOTES: &str =
        r#"{"title":"Planning","tags":["team-a"],"link":"","notes":"Agenda\n  - budget"}"#;

    #[tokio::test]
    async fn notes_are_saved_with_the_task_and_listed_by_tag() {
        let t = test_app(AuthMode::None);
        let app = &t.app;
        let (_, body) = call(app, "POST", "/api/v1/tasks", Some(CREATE_ONLY)).await;
        let id = json(&body)["id"].as_str().unwrap().to_owned();
        assert!(json(&body)["notes"].is_null());

        let (status, body) = call(
            app,
            "PATCH",
            &format!("/api/v1/tasks/{id}"),
            Some(SAVE_WITH_NOTES),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let task = json(&body);
        assert_eq!(task["notes"], "Agenda\n  - budget");
        assert!(task["notes_changed_at"].is_string(), "{task}");

        // Another task with the tag but without notes stays out.
        call(app, "POST", "/api/v1/tasks", Some(CREATE_AND_START)).await;

        let today = jiff::Timestamp::now()
            .to_zoned_datetime(jiff::tz::TimeZone::UTC)
            .date();
        for query in [
            "tag=team-a".to_owned(),
            format!("tag=%23Team-A&from={today}&to={today}"),
            String::new(),
        ] {
            let (status, body) = call(app, "GET", &format!("/api/v1/notes?{query}"), None).await;
            assert_eq!(status, StatusCode::OK, "{query}: {body}");
            let list = json(&body);
            assert_eq!(list.as_array().unwrap().len(), 1, "{query}: {list}");
            assert_eq!(list[0]["id"], id.as_str());
            assert_eq!(list[0]["notes"], "Agenda\n  - budget");
            assert_eq!(list[0]["period_seconds"], 0);
        }
        let (_, body) = call(app, "GET", "/api/v1/notes?tag=review", None).await;
        assert_eq!(body, "[]");

        // Only one end of a period, or a reversed one: 400.
        for query in [
            format!("from={today}"),
            format!("from={today}&to=2000-01-01"),
        ] {
            let (status, body) = call(app, "GET", &format!("/api/v1/notes?{query}"), None).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
            assert_eq!(json(&body)["error"], "range_invalid");
        }

        // In the YAML file the notes are readable as they were written.
        let file = std::fs::read_to_string(t.data.path().join("local/tasks.yaml")).unwrap();
        assert!(!file.contains("\\n"), "no escaped line breaks: {file}");
        assert!(file.lines().any(|l| l.trim() == "Agenda"), "{file}");
        assert!(file.lines().any(|l| l.trim() == "- budget"), "{file}");

        // An empty string removes them.
        let (_, body) = call(
            app,
            "PATCH",
            &format!("/api/v1/tasks/{id}"),
            Some(r#"{"notes":""}"#),
        )
        .await;
        assert!(json(&body)["notes"].is_null(), "{body}");
        let (_, body) = call(app, "GET", "/api/v1/notes", None).await;
        assert_eq!(body, "[]");
    }
}
