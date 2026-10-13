//! HTTP service that renders HTML and CSS to PDF. See the crate's library for the API and
//! [`Config`] for the environment variables.

use std::io::IsTerminal;

use anyhow::Context;
use rustyprint_server::{Config, app};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        // Colors only on a terminal, not in container logs.
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().map_err(anyhow::Error::msg)?;
    let addr = config.addr;
    // Load the fonts and the UA stylesheet before the first request needs them.
    tokio::task::spawn_blocking(rustyprint::warm_up).await?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(
        %addr,
        concurrency = config.concurrency,
        allowed_hosts = ?config.allowed_hosts,
        "listening"
    );
    axum::serve(listener, app(config))
        .with_graceful_shutdown(shutdown())
        .await?;
    tracing::info!("stopped");
    Ok(())
}

/// Resolves on Ctrl+C, or SIGTERM where there is one, as container runtimes send to stop.
async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
    tracing::info!("stopping: finishing the requests in progress");
}
