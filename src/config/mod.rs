use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub tls: TlsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 18080,
            tls: TlsConfig::default(),
        }
    }
}

impl ServerConfig {
    pub fn bind_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Invalid bind address")
    }
}

/// TLS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub ca: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert: None,
            key: None,
            ca: None,
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite::memory:".to_string(),
            max_connections: 10,
        }
    }
}

fn default_max_connections() -> u32 {
    10
}

/// Metrics configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_metrics_port")]
    pub port: u16,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 19090,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_metrics_port() -> u16 {
    9090
}

/// Tracing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TracingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub jaeger_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            jaeger_endpoint: None,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub admin_token: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            admin_token: None,
        }
    }
}

/// Webhook configuration for OpenClaw gateway integration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: None,
            enabled: false,
        }
    }
}

/// Application configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub webhook: WebhookConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
            security: SecurityConfig::default(),
            webhook: WebhookConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> anyhow::Result<Self> {
        let mut config = Config::default();

        // Server config
        if let Ok(host) = std::env::var("INCEPTION_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("INCEPTION_PORT") {
            config.server.port = port.parse()?;
        }

        // TLS config
        if std::env::var("INCEPTION_TLS_ENABLED").is_ok() {
            config.server.tls.enabled = true;
            config.server.tls.cert = std::env::var("INCEPTION_TLS_CERT").ok();
            config.server.tls.key = std::env::var("INCEPTION_TLS_KEY").ok();
            config.server.tls.ca = std::env::var("INCEPTION_TLS_CA").ok();
        }

        // Database config
        if let Ok(url) = std::env::var("INCEPTION_DATABASE_URL") {
            config.database.url = url;
        }

        // Metrics config
        if let Ok(enabled) = std::env::var("INCEPTION_METRICS_ENABLED") {
            config.metrics.enabled = enabled.parse()?;
        }
        if let Ok(port) = std::env::var("INCEPTION_METRICS_PORT") {
            config.metrics.port = port.parse()?;
        }

        // Security config - auto-generate if not set
        config.security.admin_token = Self::load_or_generate_admin_token();

        // Webhook config
        if let Ok(url) = std::env::var("INCEPTION_WEBHOOK_URL") {
            config.webhook.url = Some(url);
            config.webhook.enabled = true;
        }

        Ok(config)
    }

    /// Load admin token from file or generate new one
    fn load_or_generate_admin_token() -> Option<String> {
        let token_file = std::path::PathBuf::from("./data/admin_token");
        
        // Check environment variable first
        if let Ok(token) = std::env::var("INCEPTION_ADMIN_TOKEN") {
            // Save to file for persistence
            if let Some(parent) = token_file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&token_file, &token);
            return Some(token);
        }
        
        // Try to load from file
        if let Ok(token) = std::fs::read_to_string(&token_file) {
            if !token.trim().is_empty() {
                return Some(token.trim().to_string());
            }
        }
        
        // Generate new token
        let token = format!("admin_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        
        // Save to file
        if let Some(parent) = token_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&token_file, &token).is_ok() {
            tracing::info!("Generated new admin token and saved to {:?}", token_file);
            tracing::info!("Admin token: {}", token);
        }
        
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 18080);
        assert!(!config.server.tls.enabled);
        assert_eq!(config.metrics.port, 19090);
        assert!(config.metrics.enabled);
    }

    #[test]
    fn test_bind_addr() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            tls: TlsConfig::default(),
        };
        let addr = config.bind_addr();
        assert_eq!(addr.to_string(), "127.0.0.1:3000");
    }

    #[test]
    fn test_config_from_env() {
        std::env::set_var("INCEPTION_HOST", "localhost");
        std::env::set_var("INCEPTION_PORT", "9000");
        std::env::set_var("INCEPTION_DATABASE_URL", "postgres://localhost/test");

        let config = Config::from_env().unwrap();
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.database.url, "postgres://localhost/test");

        // Cleanup
        std::env::remove_var("INCEPTION_HOST");
        std::env::remove_var("INCEPTION_PORT");
        std::env::remove_var("INCEPTION_DATABASE_URL");
    }
}
