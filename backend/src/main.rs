mod api;
mod app;
mod config;
mod corrections;
mod domain;
mod reports;
mod store;

use anyhow::Context;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::{AuthMode, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        // Colour codes only on a terminal, not in container logs.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let config = Config::from_env()?;
    config.ensure_supported()?;
    if config.auth == AuthMode::None {
        warn!("CLOCKOPUS_AUTH=none: no sign-in, everyone who reaches this server is the same user");
    }

    let store = app::open_store(&config).await?;
    let storage = match &config.storage {
        config::Storage::Yaml { .. } => "yaml",
        config::Storage::Postgres { .. } => "postgres",
    };

    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("cannot listen on {}", config.listen))?;
    info!(
        listen = %config.listen,
        static_dir = %config.static_dir.display(),
        storage,
        "clockopus {} ready",
        env!("CARGO_PKG_VERSION")
    );

    axum::serve(listener, app::router(config, store))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
