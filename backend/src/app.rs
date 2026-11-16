//! HTTP routes: health probes, the JSON API under `/api/v1` and the Angular app.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::{AuthMode, Config};

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
}

pub fn router(config: Config) -> Router {
    let index = config.static_dir.join("index.html");
    // Unknown paths fall back to index.html, so deep links into the app work.
    let spa = ServeDir::new(&config.static_dir).fallback(ServeFile::new(index));
    let state = AppState {
        config: Arc::new(config),
    };

    let api = Router::new()
        .route("/config", get(public_config))
        .route("/me", get(me))
        // Unknown API paths answer 404 instead of the app's index.html.
        .fallback(api_not_found);

    Router::new()
        .route("/healthz", get(ok))
        .route("/readyz", get(ok))
        .nest("/api/v1", api)
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn ok() -> &'static str {
    "ok"
}

/// Public runtime configuration for the frontend. Contains nothing secret.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "auth", rename_all = "lowercase")]
pub enum PublicConfig {
    None,
    Oidc { issuer: String, client_id: String },
}

async fn public_config(State(state): State<AppState>) -> Json<PublicConfig> {
    Json(match &state.config.auth {
        AuthMode::None => PublicConfig::None,
        AuthMode::Oidc {
            issuer, client_id, ..
        } => PublicConfig::Oidc {
            issuer: issuer.clone(),
            client_id: client_id.clone(),
        },
    })
}

#[derive(Debug, Serialize)]
pub struct Me {
    pub id: String,
    pub name: String,
}

async fn me(State(state): State<AppState>) -> Result<Json<Me>, ApiError> {
    match state.config.auth {
        AuthMode::None => Ok(Json(Me {
            id: "local".to_owned(),
            name: "Local user".to_owned(),
        })),
        // Token validation arrives with milestone M5; until then nothing is served as anyone.
        AuthMode::Oidc { .. } => Err(ApiError::Unauthorized),
    }
}

async fn api_not_found() -> ApiError {
    ApiError::NotFound
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    Unauthorized,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        (status, Json(serde_json::json!({ "error": error }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Storage;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn config(auth: AuthMode, static_dir: &std::path::Path) -> Config {
        Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            auth,
            storage: Storage::Yaml {
                data_dir: "/tmp/unused".into(),
            },
            static_dir: static_dir.to_owned(),
        }
    }

    async fn get(app: Router, path: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    fn static_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<app-root></app-root>").unwrap();
        std::fs::write(dir.path().join("main.js"), "console.log(1)").unwrap();
        dir
    }

    #[tokio::test]
    async fn health_probes_answer_ok() {
        let dir = static_dir();
        let app = router(config(AuthMode::None, dir.path()));
        assert_eq!(
            get(app.clone(), "/healthz").await,
            (StatusCode::OK, "ok".into())
        );
        assert_eq!(get(app, "/readyz").await, (StatusCode::OK, "ok".into()));
    }

    #[tokio::test]
    async fn config_tells_the_frontend_the_auth_mode() {
        let dir = static_dir();
        let (status, body) =
            get(router(config(AuthMode::None, dir.path())), "/api/v1/config").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"auth":"none"}"#);

        let oidc = AuthMode::Oidc {
            issuer: "https://auth.example.com/realms/x".into(),
            client_id: "clockopus".into(),
            audience: "secret-irrelevant".into(),
        };
        let (_, body) = get(router(config(oidc, dir.path())), "/api/v1/config").await;
        assert_eq!(
            body,
            r#"{"auth":"oidc","issuer":"https://auth.example.com/realms/x","client_id":"clockopus"}"#
        );
    }

    #[tokio::test]
    async fn me_is_the_local_user_without_sign_in() {
        let dir = static_dir();
        let (status, body) = get(router(config(AuthMode::None, dir.path())), "/api/v1/me").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"id":"local","name":"Local user"}"#);
    }

    #[tokio::test]
    async fn me_is_refused_in_oidc_mode_until_tokens_are_checked() {
        let dir = static_dir();
        let oidc = AuthMode::Oidc {
            issuer: "https://auth.example.com".into(),
            client_id: "clockopus".into(),
            audience: "clockopus".into(),
        };
        let (status, _) = get(router(config(oidc, dir.path())), "/api/v1/me").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_api_path_is_404_json_not_the_app() {
        let dir = static_dir();
        let (status, body) = get(router(config(AuthMode::None, dir.path())), "/api/v1/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, r#"{"error":"not_found"}"#);
    }

    #[tokio::test]
    async fn app_files_and_deep_links_are_served() {
        let dir = static_dir();
        let app = router(config(AuthMode::None, dir.path()));
        assert_eq!(
            get(app.clone(), "/main.js").await,
            (StatusCode::OK, "console.log(1)".into())
        );
        // A deep link into the app gets index.html so the Angular router can take over.
        let (status, body) = get(app, "/reports/week").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "<app-root></app-root>");
    }
}
