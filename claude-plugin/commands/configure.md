---
name: inception-configure
description: Configure the Inception registry endpoint (URL, port, DNS name, auth token)
---

# /inception:configure

Set or view the Inception registry connection settings.

## Usage

```
/inception:configure
/inception:configure --url https://inception.example.com:8080
/inception:configure --url https://inception.example.com:8080 --token abc123
```

## Options

- `--url` — Registry URL (including port)
- `--token` — Authentication token

## Examples

**View current configuration:**
```
/inception:configure
```

**Set registry URL:**
```
/inception:configure --url https://inception.example.com:8080
```

**Set URL and token:**
```
/inception:configure --url https://inception.example.com:8080 --token my-api-token
```

**Local development:**
```
/inception:configure --url http://localhost:18080
```

## Configuration Persistence

Settings are stored in memory for the current Claude Code session. To persist across sessions, set environment variables:

```bash
export INCEPTION_REGISTRY_URL="https://inception.example.com:8080"
export INCEPTION_TOKEN="your-token"
```

Or add to `~/.claude/.env`.

## See Also

- `/inception:attach` — Attach to a session
- `/inception:status` — Check connection status
