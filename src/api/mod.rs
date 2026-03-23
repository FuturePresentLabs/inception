use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::{
    models::{CreateSessionRequest, CreateSessionResponse, Message, Session, SessionId, SessionStatus},
    session::SessionStore,
};

/// Application state shared across handlers
pub struct AppState {
    pub store: Arc<dyn SessionStore>,
}

/// Create a new router
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/sessions", post(create_session).get(list_sessions))
        .route("/v1/sessions/:id", get(get_session).patch(update_session))
        .route("/v1/sessions/:id/messages", post(send_message))
        .route("/v1/sessions/:id/status", post(update_status))
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> StatusCode {
    StatusCode::OK
}

/// Create a new session
async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, StatusCode> {
    let session = Session::new(req.agent_type)
        .with_capabilities(req.capabilities)
        .with_metadata(req.metadata);

    state
        .store
        .create(&session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = CreateSessionResponse {
        id: session.id.0.clone(),
        status: session.status,
        websocket_url: format!("/v1/sessions/{}/ws", session.id.0),
    };

    Ok(Json(response))
}

/// List all sessions
async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Session>>, StatusCode> {
    let sessions = state
        .store
        .list(None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(sessions))
}

/// Get a specific session
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    let session_id = SessionId(id);
    let session = state
        .store
        .get(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(session))
}

/// Send a message to a session
async fn send_message(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
    Json(_msg): Json<Message>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Implement message routing to WebSocket
    Ok(StatusCode::ACCEPTED)
}

use crate::models::{UpdateSessionRequest, UpdateStatusRequest};

/// Update session metadata
async fn update_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> Result<Json<Session>, StatusCode> {
    let session_id = SessionId(id);

    // Get existing session
    let session_opt = state
        .store
        .get(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let Some(mut session) = session_opt else {
        return Err(StatusCode::NOT_FOUND);
    };

    // Update fields if provided
    if let Some(metadata) = req.metadata {
        session.metadata = metadata;
    }
    if let Some(capabilities) = req.capabilities {
        session.capabilities = capabilities;
    }
    if let Some(current_task) = req.current_task {
        session.current_task = Some(current_task);
    }

    // Update last_activity
    session.last_activity = chrono::Utc::now();

    Ok(Json(session))
}

/// Update session status
async fn update_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<Session>, StatusCode> {
    let session_id = SessionId(id);

    // Update status in store
    state
        .store
        .update_status(&session_id, req.status.clone())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get updated session
    let session_opt = state
        .store
        .get(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let Some(mut session) = session_opt else {
        return Err(StatusCode::NOT_FOUND);
    };

    // Update agent state if provided
    if let Some(agent_state) = req.agent_state {
        session.agent_state = Some(agent_state);
    }
    if let Some(progress) = req.progress {
        session.progress = Some(progress);
    }

    session.last_activity = chrono::Utc::now();

    Ok(Json(session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SqliteSessionStore;
    use tower::ServiceExt;

    async fn create_test_app() -> Router {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let state = Arc::new(AppState {
            store: Arc::new(store),
        });
        create_router(state)
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = create_test_app().await;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
