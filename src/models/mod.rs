use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(format!("sess-{}", Uuid::new_v4().to_string().split('-').next().unwrap()))
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session is being created
    Spawning,
    /// Session is ready but idle
    Idle,
    /// Session is actively processing
    Busy,
    /// Session is disconnected
    Disconnected,
    /// Session has been terminated
    Terminated,
}

impl Default for SessionStatus {
    fn default() -> Self {
        SessionStatus::Spawning
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Spawning => write!(f, "spawning"),
            SessionStatus::Idle => write!(f, "idle"),
            SessionStatus::Busy => write!(f, "busy"),
            SessionStatus::Disconnected => write!(f, "disconnected"),
            SessionStatus::Terminated => write!(f, "terminated"),
        }
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::ClaudeCode => write!(f, "claude_code"),
            AgentType::Custom(s) => write!(f, "custom({})", s),
        }
    }
}

/// Agent type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    ClaudeCode,
    Custom(String),
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub agent_type: AgentType,
    pub status: SessionStatus,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub last_activity: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_state: Option<AgentState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
}

impl Session {
    pub fn new(agent_type: AgentType) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            agent_type,
            status: SessionStatus::Spawning,
            capabilities: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_heartbeat: None,
            last_activity: now,
            current_task: None,
            agent_state: Some(AgentState::Idle),
            progress: None,
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::Idle | SessionStatus::Busy | SessionStatus::Spawning
        )
    }
}

/// Request to create a new session
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub agent_type: AgentType,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

/// Response after creating a session
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub id: String,
    pub status: SessionStatus,
    pub websocket_url: String,
}

/// Request to update session metadata
#[derive(Debug, Deserialize)]
pub struct UpdateSessionRequest {
    #[serde(default)]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub current_task: Option<String>,
}

/// Request to update session status
#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: SessionStatus,
    #[serde(default)]
    pub agent_state: Option<AgentState>,
    #[serde(default)]
    pub progress: Option<f32>,
}

/// Agent execution state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Thinking,
    Executing,
    WaitingForUser,
    Error,
}

/// Token request
#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Token response
#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub token: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Permission request from MCP server
#[derive(Debug, Deserialize)]
pub struct PermissionRequest {
    pub request_id: String,
    pub tool_name: String,
    pub description: String,
    pub input_preview: String,
}

/// Permission verdict from user
#[derive(Debug, Deserialize)]
pub struct PermissionVerdict {
    pub request_id: String,
    pub behavior: String, // "allow" or "deny"
}

/// Query parameters for listing sessions
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub connected_only: Option<bool>,
}

/// Message sent to/from a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HashMap<String, serde_json::Value>>,
    pub timestamp: DateTime<Utc>,
}

impl Message {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content: content.into(),
            context: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_context(mut self, context: HashMap<String, serde_json::Value>) -> Self {
        self.context = Some(context);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1.0, id2.0);
        assert!(id1.0.starts_with("sess-"));
    }

    #[test]
    fn test_session_creation() {
        let session = Session::new(AgentType::ClaudeCode);
        assert!(session.id.0.starts_with("sess-"));
        assert_eq!(session.status, SessionStatus::Spawning);
        assert!(session.is_active());
    }

    #[test]
    fn test_session_with_capabilities() {
        let session = Session::new(AgentType::ClaudeCode)
            .with_capabilities(vec!["rust".to_string(), "python".to_string()]);
        assert_eq!(session.capabilities.len(), 2);
        assert!(session.capabilities.contains(&"rust".to_string()));
    }

    #[test]
    fn test_session_active_status() {
        let mut session = Session::new(AgentType::ClaudeCode);
        assert!(session.is_active());

        session.status = SessionStatus::Terminated;
        assert!(!session.is_active());

        session.status = SessionStatus::Disconnected;
        assert!(!session.is_active());
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::new("Hello, world!");
        assert_eq!(msg.content, "Hello, world!");
        assert!(msg.context.is_none());
    }

    #[test]
    fn test_message_with_context() {
        let mut context = HashMap::new();
        context.insert("file".to_string(), serde_json::json!("/path/to/file.rs"));
        
        let msg = Message::new("Refactor this").with_context(context);
        assert!(msg.context.is_some());
        assert_eq!(
            msg.context.unwrap()["file"],
            "/path/to/file.rs"
        );
    }
}
