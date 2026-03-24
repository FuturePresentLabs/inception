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

    /// Send a message notification to the webhook (OpenClaw /hooks/agent format)
    pub async fn send_message(&self, session_id: &SessionId, routing_key: Option<&str>, message: &Message) {
        if !self.enabled {
            tracing::debug!("Webhook disabled, skipping message for session {}", session_id.0);
            return;
        }

        let Some(url) = &self.webhook_url else {
            tracing::warn!("Webhook URL not configured, skipping message for session {}", session_id.0);
            return;
        };

        tracing::info!("Sending webhook for session {} to URL: {}", session_id.0, url);

        // Use routing_key if available, otherwise default to inception:session_id
        let session_key = routing_key.map(|s| s.to_string())
            .unwrap_or_else(|| format!("inception:{}", session_id.0));

        tracing::debug!("Using session_key: {} for session {}", session_key, session_id.0);

        // OpenClaw /hooks/agent format
        let payload = serde_json::json!({
            "message": message.content,
            "name": "Inception",
            "agentId": "inception",
            "sessionKey": session_key,
            "wakeMode": "now",
            "deliver": true,
            "channel": "last",
            "meta": {
                "session_id": session_id.0,
                "message_id": message.id,
                "timestamp": message.timestamp,
                "source": "claude_code"
            }
        });

        tracing::debug!("Webhook payload: {}", payload.to_string());

        let mut request = self.client.post(url).json(&payload);
        
        // Add bearer token if configured
        if let Some(token) = &self.webhook_token {
            tracing::debug!("Adding Authorization header with token");
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        tracing::info!("Sending POST request to webhook...");

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    tracing::info!("Webhook SUCCESS for session {}: HTTP {}", session_id.0, status);
                } else {
                    let body = response.text().await.unwrap_or_default();
                    tracing::warn!(
                        "Webhook FAILED for session {}: HTTP {} - Body: {}",
                        session_id.0,
                        status,
                        body
                    );
                }
            }
            Err(e) => {
                tracing::error!("Webhook ERROR for session {}: {}", session_id.0, e);
            }
        }
    }

    /// Send a permission request notification (OpenClaw /hooks/agent format)
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

        // OpenClaw /hooks/agent format for permission requests
        let payload = serde_json::json!({
            "message": format!("Claude needs permission to use {}: {}", tool_name, description),
            "name": "Inception-Permission",
            "agentId": "inception",
            "sessionKey": format!("inception:{}", session_id.0),
            "wakeMode": "now",
            "deliver": true,
            "channel": "last",
            "meta": {
                "session_id": session_id.0,
                "request_id": request_id,
                "tool_name": tool_name,
                "description": description,
                "event": "permission_request"
            }
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
