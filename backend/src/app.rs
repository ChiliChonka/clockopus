//! HTTP routes: health probes, the JSON API under `/api/v1` and the Angular app.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{debug, error};

use crate::api;
use crate::auth::{AuthError, OidcVerifier};
use crate::config::{AuthMode, Config, Storage};
use crate::domain::DomainError;
use crate::store::{PgStore, Store, StoreError, YamlStore};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Arc<Store>,
    /// Set in auth mode `oidc`.
    pub auth: Option<Arc<OidcVerifier>>,
}

/// Opens the configured backend; PostgreSQL connects and applies the migrations.
pub async fn open_store(config: &Config) -> anyhow::Result<Store> {
    Ok(match &config.storage {
        Storage::Yaml { data_dir } => Store::Yaml(YamlStore::new(data_dir)),
        Storage::Postgres { database_url } => {
            Store::Postgres(PgStore::connect(database_url).await?)
        }
    })
}

pub fn router(config: Config, store: Store) -> Router {
    let index = config.static_dir.join("index.html");
    // Unknown paths fall back to index.html, so deep links into the app work.
    let spa = ServeDir::new(&config.static_dir).fallback(ServeFile::new(index));
    let auth = match &config.auth {
        AuthMode::None => None,
        AuthMode::Oidc {
            issuer, audience, ..
        } => Some(Arc::new(
            // Only building the HTTP client can fail here; there is no network access yet.
            OidcVerifier::new(issuer, audience).expect("HTTP client for the OIDC issuer"),
        )),
    };
    let state = AppState {
        config: Arc::new(config),
        store: Arc::new(store),
        auth,
    };

    let api = Router::new()
        .route("/config", get(public_config))
        .route("/me", get(me).delete(api::delete_me))
        .route(
            "/me/settings",
            get(api::get_settings).put(api::put_settings),
        )
        .route("/tasks", get(api::list_tasks).post(api::create_task))
        .route(
            "/tasks/{id}",
            patch(api::update_task).delete(api::delete_task),
        )
        .route("/tasks/{id}/start", post(api::start_task))
        .route(
            "/tasks/{id}/entries",
            get(api::list_task_entries).post(api::add_entry),
        )
        .route("/entries", get(api::list_entries))
        .route(
            "/entries/{id}",
            patch(api::update_entry).delete(api::delete_entry),
        )
        .route("/timer", get(api::get_timer))
        .route("/timer/stop", post(api::stop_timer))
        .route("/timer/keep-running", post(api::keep_running))
        .route("/tags", get(api::list_tags))
        .route("/reports", get(api::get_report))
        .route("/export.csv", get(api::export_csv))
        .route("/export", get(api::export_data))
        .route("/import", post(api::import_data))
        // Unknown API paths answer 404 instead of the app's index.html.
        .fallback(api_not_found);

    Router::new()
        .route("/healthz", get(ok))
        .route("/readyz", get(ready))
        .nest("/api/v1", api)
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn ok() -> &'static str {
    "ok"
}

/// Ready when the storage answers (the database for PostgreSQL).
async fn ready(State(state): State<AppState>) -> (StatusCode, &'static str) {
    if state.store.ready().await {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "storage not ready")
    }
}

/// Public runtime configuration for the frontend. Contains nothing secret.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "auth", rename_all = "lowercase")]
pub enum PublicConfig {
    None,
    Oidc { issuer: String, client_id: String },
}

/// `GET /api/v1/config`: the running version (shown in the frontend) and the auth settings.
#[derive(Debug, Serialize)]
pub struct ConfigBody {
    version: &'static str,
    #[serde(flatten)]
    auth: PublicConfig,
}

async fn public_config(State(state): State<AppState>) -> Json<ConfigBody> {
    let auth = match &state.config.auth {
        AuthMode::None => PublicConfig::None,
        AuthMode::Oidc {
            issuer, client_id, ..
        } => PublicConfig::Oidc {
            issuer: issuer.clone(),
            client_id: client_id.clone(),
        },
    };
    Json(ConfigBody {
        version: env!("CARGO_PKG_VERSION"),
        auth,
    })
}

/// The signed-in user. Its `key` names the user's data in the store.
#[derive(Debug, Clone)]
pub struct User {
    pub key: String,
    pub name: String,
}

impl FromRequestParts<AppState> for User {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let Some(verifier) = &state.auth else {
            return Ok(User {
                key: "local".to_owned(),
                name: "Local user".to_owned(),
            });
        };
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.strip_prefix("Bearer ")
                    .or_else(|| v.strip_prefix("bearer "))
            })
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let result = match token {
            Some(token) => verifier.verify(token).await,
            None => Err(AuthError::Missing),
        };
        match result {
            Ok(id) => Ok(User {
                key: id.key,
                name: id.name,
            }),
            Err(AuthError::Missing) => Err(ApiError::Unauthorized),
            Err(AuthError::Invalid(reason)) => {
                debug!(reason, "bearer token rejected");
                Err(ApiError::Unauthorized)
            }
            Err(AuthError::Unavailable) => Err(ApiError::Unavailable),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Me {
    pub id: String,
    pub name: String,
}

async fn me(user: User) -> Json<Me> {
    Json(Me {
        id: user.key,
        name: user.name,
    })
}

async fn api_not_found() -> ApiError {
    ApiError::NotFound
}

/// Unwraps a JSON body or answers with a JSON error instead of axum's plain-text rejection.
pub fn body<T>(payload: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    payload.map(|Json(v)| v).map_err(|e| ApiError::Invalid {
        code: "bad_request",
        message: e.body_text(),
    })
}

#[derive(Debug)]
pub enum ApiError {
    Invalid {
        code: &'static str,
        message: String,
    },
    NotFound,
    Conflict {
        code: &'static str,
        message: String,
        entry_id: uuid::Uuid,
    },
    Unauthorized,
    /// The sign-in provider cannot be reached.
    Unavailable,
    Internal,
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Domain(d) => d.into(),
            StoreError::Io(e) => {
                error!(error = format!("{e:#}"), "storage failed");
                ApiError::Internal
            }
        }
    }
}

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::Invalid { code, message } => ApiError::Invalid { code, message },
            DomainError::NotFound => ApiError::NotFound,
            DomainError::Conflict {
                code,
                message,
                entry_id,
            } => ApiError::Conflict {
                code,
                message,
                entry_id,
            },
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// The other entry of an overlap (R5).
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_id: Option<uuid::Uuid>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        if let ApiError::Conflict {
            code,
            message,
            entry_id,
        } = self
        {
            let body = ErrorBody {
                error: code,
                message: Some(message),
                entry_id: Some(entry_id),
            };
            return (StatusCode::CONFLICT, Json(body)).into_response();
        }
        let (status, error, message) = match self {
            ApiError::Invalid { code, message } => (StatusCode::BAD_REQUEST, code, Some(message)),
            ApiError::Conflict { .. } => unreachable!("handled above"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", None),
            ApiError::Unauthorized => {
                let body = ErrorBody {
                    error: "unauthorized",
                    message: None,
                    entry_id: None,
                };
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "Bearer")],
                    Json(body),
                )
                    .into_response();
            }
            ApiError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "auth_unavailable",
                Some("Sign-in is not available right now. Please try again in a moment.".into()),
            ),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                Some("Your data could not be read or saved. Details are in the server log.".into()),
            ),
        };
        let body = ErrorBody {
            error,
            message,
            entry_id: None,
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    pub struct TestApp {
        pub app: Router,
        pub data: tempfile::TempDir,
        _static: tempfile::TempDir,
    }

    pub fn test_app(auth: AuthMode) -> TestApp {
        let static_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            static_dir.path().join("index.html"),
            "<app-root></app-root>",
        )
        .unwrap();
        std::fs::write(static_dir.path().join("main.js"), "console.log(1)").unwrap();
        let data = tempfile::tempdir().unwrap();
        let app = router(
            Config {
                listen: "127.0.0.1:0".parse().unwrap(),
                auth,
                storage: Storage::Yaml {
                    data_dir: data.path().to_owned(),
                },
                static_dir: static_dir.path().to_owned(),
            },
            Store::Yaml(YamlStore::new(data.path())),
        );
        TestApp {
            app,
            data,
            _static: static_dir,
        }
    }

    pub async fn call(
        app: &Router,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> (StatusCode, String) {
        call_as(app, None, method, path, body).await.0
    }

    /// Like [`call`], with an optional bearer token; also returns the WWW-Authenticate header.
    pub async fn call_as(
        app: &Router,
        token: Option<&str>,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> ((StatusCode, String), Option<String>) {
        let mut req = Request::builder().method(method).uri(path);
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let req = req
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let challenge = res
            .headers()
            .get("www-authenticate")
            .map(|v| v.to_str().unwrap().to_owned());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            (status, String::from_utf8(bytes.to_vec()).unwrap()),
            challenge,
        )
    }

    fn oidc() -> AuthMode {
        AuthMode::Oidc {
            issuer: "https://auth.example.com/realms/x".into(),
            client_id: "clockopus".into(),
            audience: "secret-irrelevant".into(),
        }
    }

    #[tokio::test]
    async fn health_probes_answer_ok() {
        let t = test_app(AuthMode::None);
        assert_eq!(
            call(&t.app, "GET", "/healthz", None).await,
            (StatusCode::OK, "ok".into())
        );
        assert_eq!(
            call(&t.app, "GET", "/readyz", None).await,
            (StatusCode::OK, "ok".into())
        );
    }

    #[tokio::test]
    async fn config_tells_the_frontend_the_version_and_auth_mode() {
        let v = env!("CARGO_PKG_VERSION");
        let t = test_app(AuthMode::None);
        let (status, body) = call(&t.app, "GET", "/api/v1/config", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, format!(r#"{{"version":"{v}","auth":"none"}}"#));

        let t = test_app(oidc());
        let (_, body) = call(&t.app, "GET", "/api/v1/config", None).await;
        assert_eq!(
            body,
            format!(
                r#"{{"version":"{v}","auth":"oidc","issuer":"https://auth.example.com/realms/x","client_id":"clockopus"}}"#
            )
        );
    }

    #[tokio::test]
    async fn me_is_the_local_user_without_sign_in() {
        let t = test_app(AuthMode::None);
        let (status, body) = call(&t.app, "GET", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"id":"local","name":"Local user"}"#);
    }

    #[tokio::test]
    async fn without_a_token_every_user_endpoint_is_401() {
        let t = test_app(oidc());
        let mut refused = 0;
        for (method, path, body) in [
            ("GET", "/api/v1/me", None),
            ("GET", "/api/v1/tasks", None),
            ("POST", "/api/v1/tasks", Some(r#"{"title":"x"}"#)),
            ("GET", "/api/v1/timer", None),
            ("POST", "/api/v1/timer/stop", Some("{}")),
            ("GET", "/api/v1/tags", None),
            ("GET", "/api/v1/me/settings", None),
        ] {
            let (status, _) = call(&t.app, method, path, body).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            refused += 1;
        }
        assert_eq!(refused, 7);
        assert!(
            std::fs::read_dir(t.data.path()).unwrap().next().is_none(),
            "nothing written"
        );
    }

    #[tokio::test]
    async fn unknown_api_path_is_404_json_not_the_app() {
        let t = test_app(AuthMode::None);
        let (status, body) = call(&t.app, "GET", "/api/v1/nope", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, r#"{"error":"not_found"}"#);
    }

    #[tokio::test]
    async fn app_files_and_deep_links_are_served() {
        let t = test_app(AuthMode::None);
        assert_eq!(
            call(&t.app, "GET", "/main.js", None).await,
            (StatusCode::OK, "console.log(1)".into())
        );
        // A deep link into the app gets index.html so the Angular router can take over.
        let (status, body) = call(&t.app, "GET", "/reports/week", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "<app-root></app-root>");
    }

    fn oidc_for(issuer: &str) -> AuthMode {
        AuthMode::Oidc {
            issuer: issuer.to_owned(),
            client_id: "clockopus".into(),
            audience: "clockopus".into(),
        }
    }

    #[tokio::test]
    async fn a_valid_token_signs_the_user_in() {
        let idp = crate::auth::testing::TestIssuer::start().await;
        let t = test_app(oidc_for(&idp.issuer));
        let token = idp.token("6f1c2b8e-1111-4222-8333-944455556666");
        let ((status, body), _) = call_as(&t.app, Some(&token), "GET", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            r#"{"id":"6f1c2b8e-1111-4222-8333-944455556666","name":"Anna Example"}"#
        );

        let ((status, _), challenge) = call_as(&t.app, None, "GET", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(challenge.as_deref(), Some("Bearer"));
        let forged = idp.sign(
            "key-1",
            &[7; 32],
            &idp.claims("6f1c2b8e-1111-4222-8333-944455556666"),
        );
        let ((status, _), _) = call_as(&t.app, Some(&forged), "GET", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn users_cannot_see_or_touch_each_others_data() {
        let idp = crate::auth::testing::TestIssuer::start().await;
        let t = test_app(oidc_for(&idp.issuer));
        let (anna, ben) = (idp.token("anna"), idp.token("ben"));
        let ((status, body), _) = call_as(
            &t.app,
            Some(&anna),
            "POST",
            "/api/v1/tasks",
            Some(r#"{"title":"Anna's task","tags":["private"],"start":true}"#),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let mut checked = 0;
        for (path, expected) in [
            ("/api/v1/tasks?status=all", "[]"),
            ("/api/v1/tags", "[]"),
            ("/api/v1/timer", r#"{"running":null,"forgotten":null}"#),
        ] {
            let ((status, body), _) = call_as(&t.app, Some(&ben), "GET", path, None).await;
            assert_eq!(
                (status, body.as_str()),
                (StatusCode::OK, expected),
                "{path}"
            );
            checked += 1;
        }
        for (method, path, body) in [
            (
                "PATCH",
                format!("/api/v1/tasks/{id}"),
                Some(r#"{"title":"mine now"}"#),
            ),
            ("DELETE", format!("/api/v1/tasks/{id}"), None),
            ("POST", format!("/api/v1/tasks/{id}/start"), None),
            ("GET", format!("/api/v1/tasks/{id}/entries"), None),
        ] {
            let ((status, _), _) = call_as(&t.app, Some(&ben), method, &path, body).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
            checked += 1;
        }
        assert_eq!(checked, 7);
        // Anna's task is untouched.
        let ((_, body), _) =
            call_as(&t.app, Some(&anna), "GET", "/api/v1/tasks?status=all", None).await;
        let tasks: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(tasks[0]["title"], "Anna's task");
        assert!(tasks[0]["running_since"].is_string());
        // Each user has their own directory.
        assert!(t.data.path().join("anna/tasks.yaml").exists());
        assert!(!t.data.path().join("ben").exists());
    }

    #[tokio::test]
    async fn an_unreachable_issuer_is_503_not_401() {
        let idp = crate::auth::testing::TestIssuer::start().await;
        let t = test_app(oidc_for("http://127.0.0.1:9/realms/none"));
        let ((status, body), _) =
            call_as(&t.app, Some(&idp.token("anna")), "GET", "/api/v1/me", None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("auth_unavailable"), "{body}");
    }
}
