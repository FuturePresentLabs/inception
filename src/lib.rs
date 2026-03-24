pub mod api;
pub mod config;
pub mod models;
pub mod session;
pub mod webhook;
pub mod websocket;

// Re-export commonly used items
pub use api::{create_router, AppState};
pub use config::Config;
pub use models::{Session, SessionId, SessionStatus, AgentType, Message, CreateSessionRequest, CreateSessionResponse};
pub use session::{SessionStore, SqliteSessionStore};

/// Test helpers (available in test builds)
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use crate::{api::{create_router, AppState}, session::SqliteSessionStore, websocket::WebSocketManager};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Create a test app with in-memory database
    pub async fn create_test_app() -> axum::Router {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let ws_manager = Arc::new(RwLock::new(WebSocketManager::new()));
        let config = crate::config::Config::default();
        let webhook = crate::webhook::WebhookClient::new(&config);
        #[allow(deprecated)]
        let message_store = Arc::new(crate::api::InMemoryMessageStore::new());
        let state = Arc::new(AppState {
            store: Arc::new(store),
            ws_manager,
            webhook,
            message_store,
            config,
        });
        create_router(state)
    }

    /// Helper to extract JSON body from response
    pub async fn extract_json<T>(response: axum::response::Response) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }
}
