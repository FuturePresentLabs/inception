---
name: inception-attach
description: Attach to a remote Claude Code session via Inception registry. Spawns a new session if no ID provided.
---

# /inception:attach

Attach to a remote Claude Code session. All subsequent commands will be routed to that session.

## Usage

```
/inception:attach                    # Spawn new session and attach
/inception:attach <session-id>       # Attach to existing session
```

## Examples

**Spawn a new session:**
```
/inception:attach
```
Spawns a fresh Claude Code session on your registered host and attaches to it.

**Attach to existing session:**
```
/inception:attach sess-abc123
```
Attaches to session `sess-abc123` that was previously spawned.

## What Happens

1. If no session ID provided:
   - Requests Inception registry to spawn new session
   - Registry contacts your agent daemon (running on your Mac)
   - Daemon starts new Claude Code process
   - Returns session ID and WebSocket URL

2. If session ID provided:
   - Validates session exists and is active
   - Retrieves WebSocket connection details

3. Plugin enters "attached" mode:
   - All tool calls routed to remote session
   - Responses streamed back from remote
   - Local session remains idle

## Session Lifecycle

Sessions persist until:
- You run `/inception:detach`
- Session times out (configurable, default 30 min idle)
- You explicitly kill the session via registry

## See Also

- `/inception:detach` — Detach from remote session
- `/inception:status` — Show current attachment status
