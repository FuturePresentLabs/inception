//! Test helpers for integration and e2e tests

use inception_registry::{
    api::{create_router, AppState},
    session::SqliteSessionStore,
};
use std::sync::Arc;

/// Create a test app with in-memory database
pub async fn create_test_app() -> axum::Router {
    let store = SqliteSessionStore::new_in_memory().await.unwrap();
    let state = Arc::new(AppState {
        store: Arc::new(store),
    });
    create_router(state)
}

/// Create a test app with custom configuration
pub async fn create_test_app_with_config<F>(config_fn: F) -> axum::Router
where
    F: FnOnce(&mut AppState),
{
    let store = SqliteSessionStore::new_in_memory().await.unwrap();
    let mut state = AppState {
        store: Arc::new(store),
    };
    config_fn(&mut state);
    create_router(Arc::new(state))
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

/// Helper to create a JSON request body
pub fn json_body<T>(data: &T) -> axum::body::Body
where
    T: serde::Serialize,
{
    axum::body::Body::from(serde_json::to_string(data).unwrap())
}

/// Wait for a condition with timeout
pub async fn wait_for<F, Fut>(mut condition: F, timeout_ms: u64)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if condition().await {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    panic!("Timeout waiting for condition");
}
