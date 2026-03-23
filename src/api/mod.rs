use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
    middleware,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    models::{CreateSessionRequest, CreateSessionResponse, Message, Session, SessionId, SessionStatus},
    session::SessionStore,
    webhook::WebhookClient,
    websocket::{WebSocketManager, AgentConnection},
};

/// Application state shared across handlers
pub struct AppState {
    pub store: Arc<dyn SessionStore>,
    pub ws_manager: Arc<RwLock<WebSocketManager>>,
    pub webhook: WebhookClient,
    pub config: crate::config::Config,
}

/// Create a new router
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/v1/tokens", post(create_token))
        .route("/v1/sessions", post(create_session).get(list_sessions))
        .route("/v1/sessions/:id", get(get_session).patch(update_session).delete(delete_session))
        .route("/v1/sessions/:id/messages", post(send_message))
        .route("/v1/sessions/:id/status", post(update_status))
        .route("/v1/sessions/:id/heartbeat", post(heartbeat))
        .route("/v1/sessions/:id/permissions", post(create_permission_request))
        .route("/v1/sessions/:id/verdict", post(submit_verdict))
        .route("/v1/sessions/:id/ws", get(websocket_handler))
        .layer(middleware::from_fn(logging_middleware))
        .with_state(state)
}

/// Logging middleware for all requests
async fn logging_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    
    tracing::info!("→ {} {}", method, uri);
    
    let response = next.run(req).await;
    
    tracing::info!("← {} {} - {}", method, uri, response.status());
    
    response
}

/// Health check endpoint
async fn health_check() -> StatusCode {
    StatusCode::OK
}

use crate::models::{CreateTokenRequest, CreateTokenResponse};
use uuid::Uuid;

/// Create a new API token (requires admin token)
async fn create_token(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    // Check admin token if configured
    if let Some(ref admin_token) = state.config.security.admin_token {
        let provided_token = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        
        if provided_token != Some(admin_token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    
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

/// List all sessions with optional filtering
async fn list_sessions(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<Vec<Session>>, StatusCode> {
    let sessions = state
        .store
        .list(None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Apply filters
    let filtered: Vec<Session> = sessions.into_iter()
        .filter(|s| {
            // Filter by status
            if let Some(ref status) = params.status {
                if s.status.to_string() != *status {
                    return false;
                }
            }
            
            // Filter by agent_type
            if let Some(ref agent_type) = params.agent_type {
                if s.agent_type.to_string() != *agent_type {
                    return false;
                }
            }
            
            // Filter by capability
            if let Some(ref capability) = params.capability {
                if !s.capabilities.contains(capability) {
                    return false;
                }
            }
            
            // Filter connected sessions only
            if params.connected_only.unwrap_or(false) {
                // This would need to check WebSocket manager
                // For now, just check if status is not disconnected
                if s.status == SessionStatus::Disconnected {
                    return false;
                }
            }
            
            true
        })
        .collect();

    Ok(Json(filtered))
}

use crate::models::ListSessionsQuery;

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
            tracing::info!("Message sent via WebSocket to session {}", session_id.0);
            return Ok(StatusCode::ACCEPTED);
        }
    }
    drop(ws_manager);
    
    // Queue for later delivery if agent is offline
    tracing::info!("Message queued for offline session {}", session_id.0);
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
    
    tracing::info!("WebSocket connection established for session {}", session_id.0);
    
    // Create channel for sending messages to this agent
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register connection
    let conn = crate::websocket::AgentConnection { sender: tx };
    {
        let mut manager = state.ws_manager.write().await;
        manager.register_connection(session_id.clone(), conn).await;
        tracing::info!("Session {} registered in WebSocket manager", session_id.0);
    }

    // Update session status to idle (agent connected)
    let _ = state
        .store
        .update_status(&session_id, SessionStatus::Idle)
        .await;
    tracing::info!("Session {} status updated to idle", session_id.0);

    // Handle both directions
    loop {
        tokio::select! {
            // Forward messages from channel to WebSocket
            Some(msg) = rx.recv() => {
                if socket.send(WsMessage::Text(msg)).await.is_err() {
                    break;
                }
            }
            // Handle incoming messages from agent
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        // Process incoming message from agent
                        // Trigger webhook if configured
                        if let Ok(message) = serde_json::from_str::<crate::models::Message>(&text) {
                            state.webhook.send_message(&session_id, &message).await;
                        }
                    }
                    Ok(WsMessage::Close(_)) | Err(_) => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Unregister connection
    {
        let mut manager = state.ws_manager.write().await;
        manager.remove_connection(&session_id).await;
        tracing::info!("Session {} unregistered from WebSocket manager", session_id.0);
    }

    // Update session status to disconnected
    let _ = state
        .store
        .update_status(&session_id, SessionStatus::Disconnected)
        .await;
    tracing::info!("WebSocket connection closed for session {}", session_id.0);
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

/// Delete/terminate a session
async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let session_id = SessionId(id);
    
    state
        .store
        .delete(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    // Also remove from WebSocket manager if connected
    {
        let mut manager = state.ws_manager.write().await;
        manager.remove_connection(&session_id).await;
    }
    
    Ok(StatusCode::NO_CONTENT)
}

/// Record heartbeat from agent
async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    let session_id = SessionId(id);
    
    let session_opt = state
        .store
        .get(&session_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    let Some(mut session) = session_opt else {
        return Err(StatusCode::NOT_FOUND);
    };
    
    // Update heartbeat timestamp
    session.last_heartbeat = Some(chrono::Utc::now());
    session.last_activity = chrono::Utc::now();
    
    // If was disconnected, mark as idle
    if session.status == SessionStatus::Disconnected {
        session.status = SessionStatus::Idle;
    }
    
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

use crate::models::{PermissionRequest, PermissionVerdict};
use tokio::sync::mpsc;

/// Store pending permission requests
static PERMISSION_REQUESTS: once_cell::sync::Lazy<RwLock<HashMap<String, mpsc::Sender<PermissionVerdict>>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

/// Create a permission request (from MCP server)
async fn create_permission_request(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PermissionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
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
    
    // Forward to WebSocket if connected
    let ws_manager = state.ws_manager.read().await;
    if let Some(conn) = ws_manager.get_connection(&session_id).await {
        let msg = serde_json::json!({
            "type": "permission_request",
            "request_id": req.request_id,
            "tool_name": req.tool_name,
            "description": req.description,
            "input_preview": req.input_preview,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        if let Ok(json_str) = serde_json::to_string(&msg) {
            let _ = conn.sender.send(json_str);
        }
    }
    drop(ws_manager);
    
    tracing::info!(
        "Permission request {} for session {}: {}",
        req.request_id,
        session_id.0,
        req.tool_name
    );
    
    Ok(Json(serde_json::json!({
        "status": "forwarded",
        "request_id": req.request_id,
    })))
}

/// Submit a verdict for a permission request
async fn submit_verdict(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PermissionVerdict>,
) -> Result<Json<serde_json::Value>, StatusCode> {
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
    
    // Forward verdict to WebSocket
    let ws_manager = state.ws_manager.read().await;
    if let Some(conn) = ws_manager.get_connection(&session_id).await {
        let msg = serde_json::json!({
            "type": "permission_verdict",
            "request_id": req.request_id,
            "behavior": req.behavior,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        if let Ok(json_str) = serde_json::to_string(&msg) {
            let _ = conn.sender.send(json_str);
        }
    }
    drop(ws_manager);
    
    tracing::info!(
        "Permission verdict {} for session {}: {}",
        req.request_id,
        session_id.0,
        req.behavior
    );
    
    Ok(Json(serde_json::json!({
        "status": "submitted",
        "request_id": req.request_id,
        "behavior": req.behavior,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SqliteSessionStore;
    use tower::ServiceExt;

    async fn create_test_app() -> Router {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let ws_manager = Arc::new(RwLock::new(crate::websocket::WebSocketManager::new()));
        let config = crate::config::Config::default();
        let webhook = crate::webhook::WebhookClient::new(&config);
        let state = Arc::new(AppState {
            store: Arc::new(store),
            ws_manager,
            webhook,
            config,
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
