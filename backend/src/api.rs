//! Handlers for tasks, timer, tags and settings (docs/SPEC.md, section 7).
//!
//! Wire format: JSON, times in RFC 3339 (UTC with `Z`), durations in whole seconds.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::{ApiError, AppState, User, body};
use crate::domain::{NewTask, Running, StatusFilter, TagView, TaskPatch, TaskStatus, TaskView};

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

/// `{"running": null}` or `{"running": {"task_id": …, "entry_id": …, "start": …}}`.
#[derive(Debug, Serialize)]
pub struct TimerView {
    running: Option<RunningView>,
}

#[derive(Debug, Serialize)]
pub struct RunningView {
    task_id: Uuid,
    entry_id: Uuid,
    start: Timestamp,
}

impl From<Option<Running>> for TimerView {
    fn from(r: Option<Running>) -> Self {
        TimerView {
            running: r.map(|r| RunningView {
                task_id: r.task_id,
                entry_id: r.entry_id,
                start: r.start,
            }),
        }
    }
}

pub async fn start_task(
    State(state): State<AppState>,
    user: User,
    Path(id): Path<Uuid>,
) -> Result<Json<TimerView>, ApiError> {
    let now = Timestamp::now();
    let running = state
        .store
        .write(&user.key, |d| d.start_task(id, now))
        .await?;
    Ok(Json(Some(running).into()))
}

pub async fn get_timer(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<TimerView>, ApiError> {
    let running = state.store.read(&user.key, |d| d.running()).await?;
    Ok(Json(running.into()))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopTimer {
    /// Optional end in the past; default now.
    #[serde(default)]
    end: Option<Timestamp>,
}

/// Stops the running timer. Stopping when nothing runs is not an error: the result is the same.
pub async fn stop_timer(
    State(state): State<AppState>,
    user: User,
    payload: Option<Json<StopTimer>>,
) -> Result<Json<TimerView>, ApiError> {
    let input = payload.map(|Json(v)| v).unwrap_or_default();
    let now = Timestamp::now();
    state
        .store
        .write(&user.key, |d| d.stop_timer(input.end, now))
        .await?;
    Ok(Json(None.into()))
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsBody {
    timezone: Option<String>,
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<SettingsBody>, ApiError> {
    let timezone = state
        .store
        .read(&user.key, |d| d.settings.timezone.clone())
        .await?;
    Ok(Json(SettingsBody { timezone }))
}

pub async fn put_settings(
    State(state): State<AppState>,
    user: User,
    payload: Result<Json<SettingsBody>, JsonRejection>,
) -> Result<Json<SettingsBody>, ApiError> {
    let input = body(payload)?;
    let timezone = state
        .store
        .write(&user.key, |d| {
            if let Some(tz) = input.timezone.as_deref() {
                d.set_timezone(tz)?;
            }
            Ok(d.settings.timezone.clone())
        })
        .await?;
    Ok(Json(SettingsBody { timezone }))
}

#[cfg(test)]
mod tests {
    use crate::app::tests::{call, test_app};
    use crate::config::AuthMode;
    use axum::http::StatusCode;
    use serde_json::Value;

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
        assert_eq!(body, r#"{"running":null}"#);
        let (_, body) = call(app, "GET", "/api/v1/timer", None).await;
        assert_eq!(body, r#"{"running":null}"#);

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
        assert_eq!(body, r#"{"running":null}"#);
    }

    #[tokio::test]
    async fn settings_keep_a_valid_time_zone() {
        let t = test_app(AuthMode::None);
        let (_, body) = call(&t.app, "GET", "/api/v1/me/settings", None).await;
        assert_eq!(body, r#"{"timezone":null}"#);
        let (status, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"timezone":"Europe/Berlin"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"timezone":"Europe/Berlin"}"#);
        let (status, body) = call(
            &t.app,
            "PUT",
            "/api/v1/me/settings",
            Some(r#"{"timezone":"Mars/Olympus"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json(&body)["error"], "timezone_unknown");
        let (_, body) = call(&t.app, "GET", "/api/v1/me/settings", None).await;
        assert_eq!(body, r#"{"timezone":"Europe/Berlin"}"#);
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
}
