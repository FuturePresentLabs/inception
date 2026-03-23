use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::{
    models::{CreateSessionRequest, CreateSessionResponse, Message, Session, SessionId, SessionStatus},
    session::SessionStore,
    websocket::{WebSocketManager, AgentConnection},
};

/// Application state shared across handlers
pub struct AppState {
    pub store: Arc<dyn SessionStore>,
    pub ws_manager: Arc<RwLock<WebSocketManager>>,
}

/// Create a new router
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/tokens", post(create_token))
        .route("/v1/sessions", post(create_session).get(list_sessions))
        .route("/v1/sessions/:id", get(get_session).patch(update_session))
        .route("/v1/sessions/:id/messages", post(send_message))
        .route("/v1/sessions/:id/status", post(update_status))
        .route("/v1/sessions/:id/ws", get(websocket_handler))
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> StatusCode {
    StatusCode::OK
}

use crate::models::{CreateTokenRequest, CreateTokenResponse};
use uuid::Uuid;

/// Create a new API token
async fn create_token(
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    // Generate token
    let token = format!("inc_{}", Uuid::new_v4().to_string().replace("-", ""));
    
    // TODO: Store token in database with metadata
    // For now, just return it (ephemeral)
    
    let response = CreateTokenResponse {
        token: token.clone(),
        name: req.name,
        created_at: chrono::Utc::now(),
        expires_at: req.expires_at,
    };
    
    Ok(Json(response))
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
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(msg): Json<Message>,
) -> Result<StatusCode, StatusCode> {
    let session_id = SessionId(id);
    
    // Try to send via WebSocket if agent is connected
    let ws_manager = state.ws_manager.read().await;
    if let Some(conn) = ws_manager.get_connection(&session_id).await {
        // Send to connected agent
        if conn.send_message(&msg).await.is_ok() {
            return Ok(StatusCode::ACCEPTED);
        }
    }
    drop(ws_manager);
    
    // Queue for later delivery if agent is offline
    // TODO: Implement message queue
    
    Ok(StatusCode::ACCEPTED)
}

/// WebSocket handler for agent connections
async fn websocket_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let session_id = SessionId(id);
    
    // Verify session exists
    let session = state
        .store
        .get(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    if session.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    Ok(ws.on_upgrade(move |socket| {
        handle_agent_socket(state, session_id, socket)
    }))
}

async fn handle_agent_socket(
    state: Arc<AppState>,
    session_id: SessionId,
    mut socket: axum::extract::ws::WebSocket,
) {
    use axum::extract::ws::Message as WsMessage;
    use tokio::sync::mpsc;
    
    // Create channel for sending messages to this agent
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register connection
    let conn = crate::websocket::AgentConnection { sender: tx };
    {
        let mut manager = state.ws_manager.write().await;
        manager.register_connection(session_id.clone(), conn).await;
    }

    // Update session status to idle (agent connected)
    let _ = state
        .store
        .update_status(&session_id, SessionStatus::Idle)
        .await;

    // Spawn task to forward messages from channel to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if socket.send(WsMessage::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages from agent
    let recv_task = tokio::spawn(async move {
        // TODO: Handle incoming messages from agent
        // This would include responses, heartbeats, status updates
    });

    // Wait for either task to complete (connection closed)
    tokio::select! {
        _ = &mut send_task => {},
        _ = recv_task => {},
    }

    // Unregister connection
    {
        let mut manager = state.ws_manager.write().await;
        manager.remove_connection(&session_id).await;
    }

    // Update session status to disconnected
    let _ = state
        .store
        .update_status(&session_id, SessionStatus::Disconnected)
        .await;
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

    // Update agent state if provided (ephemeral, not persisted to DB)
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
        let ws_manager = Arc::new(RwLock::new(crate::websocket::WebSocketManager::new()));
        let state = Arc::new(AppState {
            store: Arc::new(store),
            ws_manager,
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
