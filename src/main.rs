mod api;
mod config;
mod models;
mod session;

use std::sync::Arc;
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
    info!("Configuration loaded: {:?}", config);

    // Initialize session store
    let store = SqliteSessionStore::new(&config.database.url).await?;
    info!("Session store initialized");

    // Create app state
    let state = Arc::new(api::AppState {
        store: Arc::new(store),
    });

    // Create router
    let app = api::create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(config.server.bind_addr()).await?;
    info!("Inception Registry ready on {}", config.server.bind_addr());

    axum::serve(listener, app).await?;

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
