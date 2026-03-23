# Inception Claude Plugin

MCP server and commands for connecting Claude Code to remote sessions via Inception registry.

## Installation

```bash
/plugin install inception@FuturePresentLabs
```

## Configuration

Set environment variables:

```bash
export INCEPTION_REGISTRY_URL="https://inception.example.com:8080"
export INCEPTION_TOKEN="your-api-token"
```

Or create `~/.claude/.env`:

```
INCEPTION_REGISTRY_URL=https://inception.example.com:8080
INCEPTION_TOKEN=your-api-token
```

## Usage

### Spawn and attach to new session

```
/inception:attach
```

### Attach to existing session

```
/inception:attach sess-abc123
```

### Check status

```
/inception:status
```

### Detach

```
/inception:detach
```

## How It Works

1. **MCP Server** (`mcp-server/`) runs as a sidecar process
2. **Commands** (`commands/`) provide CLI interface
3. **Registry** handles session routing and WebSocket connections
4. **Agent Daemon** runs on your machine, manages Claude Code processes

## Architecture

```
Claude Code (local)
    ↓ MCP (stdio)
Inception MCP Server
    ↓ HTTP/WebSocket
Inception Registry
    ↓ WebSocket
inception-agent (your Mac)
    ↓ stdin/stdout
Claude Code (remote session)
```
