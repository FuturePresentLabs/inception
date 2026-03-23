use axum::extract::ws::{Message as WsMessage, WebSocket};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::{
    models::{Message, SessionId},
};

/// Manages WebSocket connections for agent sessions
pub struct WebSocketManager {
    connections: HashMap<SessionId, AgentConnection>,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub async fn register_connection(&mut self, session_id: SessionId, conn: AgentConnection) {
        self.connections.insert(session_id, conn);
    }

    pub async fn remove_connection(&mut self, session_id: &SessionId) {
        self.connections.remove(session_id);
    }

    pub async fn get_connection(&self, session_id: &SessionId) -> Option<&AgentConnection> {
        self.connections.get(session_id)
    }

    pub async fn is_connected(&self, session_id: &SessionId) -> bool {
        self.connections.contains_key(session_id)
    }
}

/// Represents a connected agent
#[derive(Clone)]
pub struct AgentConnection {
    pub sender: mpsc::UnboundedSender<String>,
}

impl AgentConnection {
    pub async fn send_message(&self, msg: &Message) -> anyhow::Result<()> {
        let json = serde_json::to_string(msg)?;
        self.sender.send(json)?;
        Ok(())
    }
}
