---
name: inception
emoji: 🧠
description: Manage Inception Registry sessions for distributed agent orchestration. List sessions, check status, attach to remote Claude Code instances, and monitor agent activity across the fleet.
---

# Inception Skill

Manage Inception Registry sessions for distributed agent orchestration.

## Configuration

Set environment variables:
```bash
export INCEPTION_REGISTRY_URL="http://localhost:18080"
export INCEPTION_TOKEN="your-token"  # optional
```

Or use the configure command:
```bash
openclaw inception configure --url http://localhost:18080
```

## Commands

### List Sessions
```bash
openclaw inception list [--status idle|busy|disconnected]
```

### Get Session Details
```bash
openclaw inception get <session-id>
```

### Attach to Session
```bash
openclaw inception attach [session-id]
```
Spawns new session if no ID provided.

### Check Session Status
```bash
openclaw inception status
```

### Update Session Status
```bash
openclaw inception update-status <session-id> --state thinking --progress 0.5 --task "Refactoring auth module"
```

## Quick Reference

| Command | Description |
|---------|-------------|
| `list` | Show all sessions |
| `get` | Session details |
| `attach` | Attach to session |
| `detach` | Detach from session |
| `status` | Current status |
| `configure` | Set registry URL |

## Session States

- **spawning** - Session starting up
- **idle** - Ready for work
- **busy** - Processing task
- **disconnected** - Agent offline
- **terminated** - Session ended

## Agent States

- **idle** - Waiting for input
- **thinking** - Processing/planning
- **executing** - Running tools
- **waiting_for_user** - Needs input
- **error** - Error state
