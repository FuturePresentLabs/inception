# Inception Registry

A Rust-based registry and orchestrator for Claude Code sessions, enabling persistent, warm agent sessions that feel as native as OpenClaw sub-agents.

## Architecture

```
┌─────────────────┐     HTTP/gRPC      ┌─────────────────┐     WebSocket      ┌─────────────────┐
│  OpenClaw       │◄──────────────────►│  Inception      │◄──────────────────►│  inception-     │
│  Gateway        │       mTLS         │  Registry       │       mTLS         │  agent (Mac)    │
│                 │                    │  (This Service) │                    │                 │
└─────────────────┘                    └─────────────────┘                    └─────────────────┘
                                              │
                                              ▼ SQLite/Postgres
                                        ┌─────────────────┐
                                        │  Session Store  │
                                        └─────────────────┘
```

## Quick Start

### Running with Docker

```bash
docker run -p 8080:8080 -p 9090:9090 \
  -v ./data:/data \
  -v ./certs:/certs:ro \
  futurepresentlabs/inception-registry:latest
```

### Building from Source

```bash
# Clone the repo
git clone https://github.com/FuturePresentLabs/inception.git
cd inception/registry

# Build
cargo build --release

# Run
./target/release/inception-registry
```

## Configuration

Configuration is via environment variables or a config file:

```yaml
# config.yaml
server:
  host: "0.0.0.0"
  port: 8080
  tls:
    enabled: true
    cert: "/certs/server.crt"
    key: "/certs/server.key"
    ca: "/certs/ca.crt"

database:
  url: "sqlite:///data/inception.db"
  # or for production:
  # url: "postgres://user:pass@localhost/inception"

metrics:
  enabled: true
  port: 9090

tracing:
  enabled: true
  jaeger_endpoint: "http://jaeger:14268/api/traces"
```

## API

### Sessions

**Spawn a new session:**
```bash
POST /v1/sessions
{
  "agent_type": "claude-code",
  "capabilities": ["rust", "python"],
  "metadata": {
    "host": "macbook-pro.local"
  }
}

Response:
{
  "id": "sess-abc123",
  "status": "spawning",
  "websocket_url": "wss://registry:8081/v1/sessions/sess-abc123/ws"
}
```

**List sessions:**
```bash
GET /v1/sessions?status=idle

Response:
{
  "sessions": [
    {
      "id": "sess-abc123",
      "status": "idle",
      "agent_type": "claude-code",
      "created_at": "2026-03-22T20:00:00Z"
    }
  ]
}
```

**Send message to session:**
```bash
POST /v1/sessions/sess-abc123/messages
{
  "content": "Refactor this file",
  "context": {
    "files": ["/path/to/file.rs"]
  }
}
```

## Development

### Running Tests

```bash
cargo test
```

### Running with Tracing

```bash
RUST_LOG=debug cargo run
```

### Generating mTLS Certs (Development)

```bash
./scripts/generate-certs.sh
```

## Metrics

Prometheus metrics exposed on `:9090/metrics`:

- `inception_sessions_total` — Total sessions by status
- `inception_messages_sent_total` — Messages sent
- `inception_message_latency_seconds` — Message latency
- `inception_websocket_connections` — Active WebSocket connections

## License

AGPL-3.0 — See [LICENSE](../LICENSE)
