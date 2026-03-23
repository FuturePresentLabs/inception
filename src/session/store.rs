use crate::models::{Session, SessionId, SessionStatus};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::sync::Arc;

/// Session storage trait
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create a new session
    async fn create(&self, session: &Session) -> anyhow::Result<()>;
    
    /// Get a session by ID
    async fn get(&self, id: &SessionId) -> anyhow::Result<Option<Session>>;
    
    /// Update session status
    async fn update_status(&self, id: &SessionId, status: SessionStatus) -> anyhow::Result<()>;
    
    /// Update session heartbeat
    async fn update_heartbeat(&self, id: &SessionId) -> anyhow::Result<()>;
    
    /// List sessions with optional status filter
    async fn list(&self, status: Option<SessionStatus>) -> anyhow::Result<Vec<Session>>;
    
    /// Delete a session
    async fn delete(&self, id: &SessionId) -> anyhow::Result<()>;
}

/// SQLite session store
pub struct SqliteSessionStore {
    pool: Pool<Sqlite>,
}

impl SqliteSessionStore {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        
        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;
        
        Ok(Self { pool })
    }

    pub async fn new_in_memory() -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(":memory:").await?;
        
        // Create tables manually for in-memory testing
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                agent_type TEXT NOT NULL,
                status TEXT NOT NULL,
                capabilities TEXT NOT NULL DEFAULT '[]',
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_heartbeat TEXT
            )
            "#,
        )
        .execute(&pool)
        .await?;
        
        Ok(Self { pool })
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(&self, session: &Session) -> anyhow::Result<()> {
        let capabilities = serde_json::to_string(&session.capabilities)?;
        let metadata = serde_json::to_string(&session.metadata)?;
        let agent_type_str = match &session.agent_type {
            crate::models::AgentType::ClaudeCode => "claude_code".to_string(),
            crate::models::AgentType::Custom(s) => format!("custom({})", s),
        };
        let status_str = match session.status {
            SessionStatus::Spawning => "spawning",
            SessionStatus::Idle => "idle",
            SessionStatus::Busy => "busy",
            SessionStatus::Disconnected => "disconnected",
            SessionStatus::Terminated => "terminated",
        };
        
        sqlx::query(
            r#"
            INSERT INTO sessions (id, agent_type, status, capabilities, metadata, created_at, updated_at, last_heartbeat)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&session.id.0)
        .bind(agent_type_str)
        .bind(status_str)
        .bind(capabilities)
        .bind(metadata)
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .bind(session.last_heartbeat.map(|t| t.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    async fn get(&self, id: &SessionId) -> anyhow::Result<Option<Session>> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT id, agent_type, status, capabilities, metadata, created_at, updated_at, last_heartbeat
            FROM sessions
            WHERE id = ?1
            "#,
        )
        .bind(&id.0)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| r.to_session()))
    }

    async fn update_status(&self, id: &SessionId, status: SessionStatus) -> anyhow::Result<()> {
        let status_str = match status {
            SessionStatus::Spawning => "spawning",
            SessionStatus::Idle => "idle",
            SessionStatus::Busy => "busy",
            SessionStatus::Disconnected => "disconnected",
            SessionStatus::Terminated => "terminated",
        };
        sqlx::query(
            r#"
            UPDATE sessions
            SET status = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(status_str)
        .bind(Utc::now().to_rfc3339())
        .bind(&id.0)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    async fn update_heartbeat(&self, id: &SessionId) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions
            SET last_heartbeat = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(&id.0)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    async fn list(&self, status: Option<SessionStatus>) -> anyhow::Result<Vec<Session>> {
        let rows = if let Some(status) = status {
            let status_str = match status {
                SessionStatus::Spawning => "spawning",
                SessionStatus::Idle => "idle",
                SessionStatus::Busy => "busy",
                SessionStatus::Disconnected => "disconnected",
                SessionStatus::Terminated => "terminated",
            };
            sqlx::query_as::<_, SessionRow>(
                r#"
                SELECT id, agent_type, status, capabilities, metadata, created_at, updated_at, last_heartbeat
                FROM sessions
                WHERE status = ?1
                ORDER BY created_at DESC
                "#,
            )
            .bind(status_str)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, SessionRow>(
                r#"
                SELECT id, agent_type, status, capabilities, metadata, created_at, updated_at, last_heartbeat
                FROM sessions
                ORDER BY created_at DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        
        Ok(rows.into_iter().map(|r| r.to_session()).collect())
    }

    async fn delete(&self, id: &SessionId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?1")
            .bind(&id.0)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}

/// Database row for session
#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    agent_type: String,
    status: String,
    capabilities: String,
    metadata: String,
    created_at: String,
    updated_at: String,
    last_heartbeat: Option<String>,
}

impl SessionRow {
    fn to_session(self) -> Session {
        let now = Utc::now();
        Session {
            id: SessionId(self.id),
            agent_type: match self.agent_type.as_str() {
                "claude_code" => crate::models::AgentType::ClaudeCode,
                s if s.starts_with("custom(") => {
                    let inner = s.trim_start_matches("custom(").trim_end_matches(")");
                    crate::models::AgentType::Custom(inner.to_string())
                }
                _ => crate::models::AgentType::ClaudeCode,
            },
            status: match self.status.as_str() {
                "spawning" => SessionStatus::Spawning,
                "idle" => SessionStatus::Idle,
                "busy" => SessionStatus::Busy,
                "disconnected" => SessionStatus::Disconnected,
                "terminated" => SessionStatus::Terminated,
                _ => SessionStatus::Spawning,
            },
            capabilities: serde_json::from_str(&self.capabilities).unwrap_or_default(),
            metadata: serde_json::from_str(&self.metadata).unwrap_or_default(),
            created_at: self.created_at.parse().unwrap_or_else(|_| now),
            updated_at: self.updated_at.parse().unwrap_or_else(|_| now),
            last_heartbeat: self.last_heartbeat.and_then(|t| t.parse().ok()),
            last_activity: self.updated_at.parse().unwrap_or(now),
            current_task: None,
            agent_state: Some(crate::models::AgentState::Idle),
            progress: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AgentType;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let session = Session::new(AgentType::ClaudeCode);
        
        store.create(&session).await.unwrap();
        
        let retrieved = store.get(&session.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id.0, session.id.0);
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let session = Session::new(AgentType::ClaudeCode);
        
        store.create(&session).await.unwrap();
        store.update_status(&session.id, SessionStatus::Idle).await.unwrap();
        
        let retrieved = store.get(&session.id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, SessionStatus::Idle);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        
        let session1 = Session::new(AgentType::ClaudeCode);
        let session2 = Session::new(AgentType::ClaudeCode);
        
        store.create(&session1).await.unwrap();
        store.create(&session2).await.unwrap();
        
        let sessions = store.list(None).await.unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = SqliteSessionStore::new_in_memory().await.unwrap();
        let session = Session::new(AgentType::ClaudeCode);
        
        store.create(&session).await.unwrap();
        store.delete(&session.id).await.unwrap();
        
        let retrieved = store.get(&session.id).await.unwrap();
        assert!(retrieved.is_none());
    }
}
