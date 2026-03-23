---
name: inception-status
description: Show current Inception attachment status and available sessions
---

# /inception:status

Display current attachment status and list available remote sessions.

## Usage

```
/inception:status
```

## Output

**When attached:**
```
Currently attached to: sess-abc123
Host: macbook-pro.local
Status: idle
Uptime: 12m 34s
Capabilities: rust, python, typescript
```

**When detached:**
```
Not attached to any remote session.

Available sessions:
  sess-abc123  idle   12m  macbook-pro.local  [rust, python]
  sess-def456  busy   45m  macbook-pro.local  [python, node]
```

## Session States

- `spawning` — Session is being created
- `idle` — Session ready, waiting for work
- `busy` — Session actively processing
- `disconnected` — Host temporarily offline
- `terminated` — Session ended
