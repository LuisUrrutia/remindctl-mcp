mod config;
mod error;
mod models;
mod remindctl;
mod resolve;
mod server;

use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::middleware;
use config::Config;
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;
use server::{AppServer, RuntimeState, auth_middleware};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let config = Config::from_env()?;
    config.log_startup();

    let state = Arc::new(RuntimeState::new(config)?);
    let shutdown = CancellationToken::new();

    let mcp_service: StreamableHttpService<AppServer, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let state = Arc::clone(&state);
                move || Ok(AppServer::new(Arc::clone(&state)))
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig {
                cancellation_token: shutdown.child_token(),
                ..Default::default()
            },
        );

    let app =
        Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                Arc::clone(&state),
                auth_middleware,
            ));

    let listener = tokio::net::TcpListener::bind(state.config.bind_addr).await?;
    tracing::info!(addr = %state.config.bind_addr, "mcp server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(error = %err, "failed waiting for shutdown signal");
            }
            shutdown.cancel();
        })
        .await?;

    Ok(())
}
