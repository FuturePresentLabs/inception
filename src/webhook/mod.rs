use crate::config::Config;
use crate::models::{Message, SessionId};

/// Webhook client for sending notifications to OpenClaw gateway
pub struct WebhookClient {
    client: reqwest::Client,
    webhook_url: Option<String>,
    webhook_token: Option<String>,
    enabled: bool,
}

impl WebhookClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: reqwest::Client::new(),
            webhook_url: config.webhook.url.clone(),
            webhook_token: config.webhook.token.clone(),
            enabled: config.webhook.enabled,
        }
    }

    /// Send a message notification to the webhook
    pub async fn send_message(&self, session_id: &SessionId, message: &Message) {
        if !self.enabled {
            return;
        }

        let Some(url) = &self.webhook_url else {
            return;
        };

        let payload = serde_json::json!({
            "event": "message",
            "session_id": session_id.0,
            "message": {
                "id": message.id,
                "content": message.content,
                "timestamp": message.timestamp,
            },
        });

        let mut request = self.client.post(url).json(&payload);
        
        // Add bearer token if configured
        if let Some(token) = &self.webhook_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("Webhook sent for session {}", session_id.0);
                } else {
                    tracing::warn!(
                        "Webhook returned error status: {} for session {}",
                        response.status(),
                        session_id.0
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to send webhook: {}", e);
            }
        }
    }

    /// Send a permission request notification
    pub async fn send_permission_request(
        &self,
        session_id: &SessionId,
        request_id: &str,
        tool_name: &str,
        description: &str,
    ) {
        if !self.enabled {
            return;
        }

        let Some(url) = &self.webhook_url else {
            return;
        };

        let payload = serde_json::json!({
            "event": "permission_request",
            "session_id": session_id.0,
            "request": {
                "request_id": request_id,
                "tool_name": tool_name,
                "description": description,
            },
        });

        let mut request = self.client.post(url).json(&payload);
        
        // Add bearer token if configured
        if let Some(token) = &self.webhook_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        match request.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!(
                        "Permission request webhook sent for session {}",
                        session_id.0
                    );
                } else {
                    tracing::warn!(
                        "Permission webhook returned error: {} for session {}",
                        response.status(),
                        session_id.0
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to send permission webhook: {}", e);
            }
        }
    }
}
