use crate::models::{Message, SessionId};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite, SqlitePool};

/// Message storage trait
#[async_trait]
pub trait MessageStore: Send + Sync {
    /// Store a message
    async fn store(&self, session_id: &SessionId, message: &Message) -> anyhow::Result<()>;
    
    /// Get messages for a session (ordered by timestamp)
    async fn get_for_session(&self, session_id: &SessionId, limit: i64) -> anyhow::Result<Vec<Message>>;
}

/// SQLite message store
pub struct SqliteMessageStore {
    pool: Pool<Sqlite>,
}

impl SqliteMessageStore {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        
        // Create messages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                source TEXT,
                in_reply_to TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            "#
        )
        .execute(&pool)
        .await?;
        
        Ok(Self { pool })
    }
}

#[async_trait]
impl MessageStore for SqliteMessageStore {
    async fn store(&self, session_id: &SessionId, message: &Message) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO messages (id, session_id, content, timestamp, source, in_reply_to)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                timestamp = excluded.timestamp
            "#
        )
        .bind(&message.id)
        .bind(&session_id.0)
        .bind(&message.content)
        .bind(message.timestamp.to_rfc3339())
        .bind(message.source.as_ref().unwrap_or(&"unknown".to_string()))
        .bind(message.in_reply_to.as_ref())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn get_for_session(&self, session_id: &SessionId, limit: i64) -> anyhow::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, content, timestamp, source, in_reply_to
            FROM messages
            WHERE session_id = ?1
            ORDER BY timestamp ASC
            LIMIT ?2
            "#
        )
        .bind(&session_id.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|r| r.to_message()).collect())
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    content: String,
    timestamp: String,
    source: String,
    in_reply_to: Option<String>,
}

impl MessageRow {
    fn to_message(self) -> Message {
        use chrono::DateTime;
        Message {
            id: self.id,
            content: self.content,
            timestamp: DateTime::parse_from_rfc3339(&self.timestamp)
                .unwrap_or_else(|_| chrono::Utc::now().into())
                .into(),
            context: None,
            source: Some(self.source),
            in_reply_to: self.in_reply_to,
        }
    }
}
