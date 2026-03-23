use inception_registry::{api, config, session, websocket::WebSocketManager};

use std::sync::Arc;
use tokio::sync::RwLock;
use config::Config;
use session::SqliteSessionStore;
use tracing::{info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
        )
        .with(filter)
        .init();

    info!("Starting Inception Registry v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = Config::from_env()?;
    info!("Configuration loaded");
    
    // Log admin token (for initial setup)
    if let Some(ref token) = config.security.admin_token {
        info!("Admin token: {}", token);
        info!("Use this token to generate API tokens: Authorization: Bearer {}", token);
    }

    // Initialize session store
    let store = SqliteSessionStore::new(&config.database.url).await?;
    info!("Session store initialized");

    // Initialize WebSocket manager
    let ws_manager = Arc::new(RwLock::new(WebSocketManager::new()));
    info!("WebSocket manager initialized");

    // Create app state
    let state = Arc::new(api::AppState {
        store: Arc::new(store),
        ws_manager,
        config: config.clone(),
    });

    // Create router
    let app = api::create_router(state);

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(config.server.bind_addr()).await?;
    info!("Inception Registry ready on {}", config.server.bind_addr());

    // Handle shutdown signals
    let shutdown = async {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to create SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to create SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM, shutting down gracefully..."),
            _ = sigint.recv() => info!("Received SIGINT, shutting down gracefully..."),
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    info!("Shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }
}
