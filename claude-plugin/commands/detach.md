---
name: inception-detach
description: Detach from remote Claude Code session and return to local session
---

# /inception:detach

Detach from the current remote session. Returns control to your local Claude Code session.

## Usage

```
/inception:detach [--close]
```

## Options

- `--close` — Also terminate the remote session (default: keep running)

## Examples

**Detach but keep session running:**
```
/inception:detach
```
Session remains active in background. Re-attach later with `/inception:attach <id>`.

**Detach and close session:**
```
/inception:detach --close
```
Terminates the remote Claude Code process.

## What Happens

1. MCP server closes WebSocket connection to registry
2. Registry stops routing messages to that session
3. Local Claude Code resumes normal operation
4. Remote session either:
   - Continues running (if not `--close`)
   - Receives shutdown signal (if `--close`)

## Session Persistence

Detached sessions remain in the registry for later re-attachment unless:
- They time out from inactivity
- Explicitly killed via `/inception:kill` or registry API
- Host machine goes offline
