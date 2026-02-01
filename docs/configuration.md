# RemoShell Configuration Guide

This document describes how to configure the RemoShell daemon and client.

## Configuration Precedence

Configuration values are applied in the following order (highest priority first):

1. **Environment variables** - Override all other sources
2. **Configuration file** - User-specified settings
3. **Default values** - Built-in fallbacks

## Configuration File

### Location

The default configuration file location is:
- **Linux/macOS**: `~/.config/remoshell/config.toml`
- **Windows**: `%APPDATA%\remoshell\config.toml`

You can specify a custom path with the `--config` flag:
```bash
remoshell --config /path/to/config.toml start
```

### Format

Configuration uses TOML format. Here's a complete example with all options:

```toml
[daemon]
# Directory for storing daemon data (keys, sessions, etc.)
data_dir = "~/.local/share/remoshell"

# Logging level: trace, debug, info, warn, error
log_level = "info"

[network]
# URL of the signaling server (must start with ws:// or wss://)
signaling_url = "wss://remoshell-signaling.moukrea.workers.dev"

# STUN servers for NAT traversal
stun_servers = [
    "stun:stun.l.google.com:19302",
    "stun:stun1.l.google.com:19302"
]

[session]
# Default shell for new sessions
default_shell = "/bin/bash"

# Maximum concurrent sessions (1-1000)
max_sessions = 10

[file]
# Paths allowed for file transfers (empty = all paths allowed)
allowed_paths = []

# Maximum file size in bytes (must be > 0)
max_size = 104857600  # 100MB

[security]
# Require manual approval for new device connections
require_approval = true

# Timeout in seconds for approval requests (0-3600, 0 = no timeout)
approval_timeout = 300
```

## Environment Variables

### Daemon Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `REMOSHELL_SIGNALING_URL` | Override signaling server URL | `wss://remoshell-signaling.moukrea.workers.dev` |
| `REMOSHELL_LOG_LEVEL` | Override log level | `info` |

### Client Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `REMOSHELL_SERVER_ADDRESS` | Server address for Tauri client | `localhost:9000` |

### Examples

```bash
# Use a custom signaling server
export REMOSHELL_SIGNALING_URL="wss://my-signaling.example.com"

# Enable debug logging
export REMOSHELL_LOG_LEVEL="debug"

# Connect client to remote server
export REMOSHELL_SERVER_ADDRESS="192.168.1.100:9000"
```

## Configuration Options Reference

### [daemon] Section

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `data_dir` | path | `~/.local/share/remoshell` | Data storage directory |
| `log_level` | string | `info` | Logging verbosity |

### [network] Section

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `signaling_url` | string | `wss://remoshell-signaling.moukrea.workers.dev` | WebSocket URL for signaling |
| `stun_servers` | array | Google STUN servers | STUN servers for NAT traversal |

### [session] Section

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `default_shell` | string | `$SHELL` or `/bin/sh` | Shell for new sessions |
| `max_sessions` | integer | `10` | Max concurrent sessions |

### [file] Section

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowed_paths` | array | `[]` (all allowed) | Restrict file transfer paths |
| `max_size` | integer | `104857600` | Max file size in bytes |

### [security] Section

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `require_approval` | boolean | `true` | Require device approval |
| `approval_timeout` | integer | `300` | Approval timeout in seconds |

## Validation Rules

The configuration is validated when loaded. Invalid values will cause the daemon to exit with an error.

### Numeric Ranges

| Setting | Valid Range | Error Message |
|---------|-------------|---------------|
| `max_sessions` | 1-1000 | "max_sessions must be between 1 and 1000" |
| `approval_timeout` | 0-3600 | "approval_timeout must be between 0 and 3600 seconds" |
| `max_size` | > 0 | "max_size must be greater than 0" |

### Format Validation

| Setting | Requirement | Error Message |
|---------|-------------|---------------|
| `signaling_url` | Must start with `ws://` or `wss://` | "signaling_url must start with ws:// or wss://" |
| `default_shell` | Path must exist (absolute) or be in PATH | "default_shell path does not exist" |
| `log_level` | Must be: trace, debug, info, warn, error | "log_level must be one of: trace, debug, info, warn, error" |

## Common Use Cases

### Development Setup

```bash
# Use local signaling server with debug logging
export REMOSHELL_SIGNALING_URL="ws://localhost:8787"
export REMOSHELL_LOG_LEVEL="debug"
remoshell start
```

### Production Deployment

```toml
# /etc/remoshell/config.toml
[daemon]
log_level = "warn"

[security]
require_approval = true
approval_timeout = 60

[file]
allowed_paths = ["/home", "/tmp"]
max_size = 52428800  # 50MB
```

### Docker/Container

```dockerfile
ENV REMOSHELL_SIGNALING_URL=wss://signal.example.com
ENV REMOSHELL_LOG_LEVEL=info
```

### systemd Service

```ini
# /etc/systemd/system/remoshell.service
[Service]
Environment="REMOSHELL_SIGNALING_URL=wss://signal.example.com"
Environment="REMOSHELL_LOG_LEVEL=info"
ExecStart=/usr/bin/remoshell --config /etc/remoshell/config.toml start --systemd
```

## Troubleshooting

### Configuration Not Loading

1. Check file exists: `ls ~/.config/remoshell/config.toml`
2. Validate TOML syntax: `cat ~/.config/remoshell/config.toml | toml-validator`
3. Check file permissions: `ls -la ~/.config/remoshell/`

### Validation Errors

If you see validation errors on startup:

```
Error: max_sessions must be between 1 and 1000, got 0
```

Check your config file for the invalid value and correct it.

### Environment Variable Not Working

1. Verify the variable is set: `echo $REMOSHELL_SIGNALING_URL`
2. Ensure it's exported: `export REMOSHELL_SIGNALING_URL=...`
3. Check spelling (case-sensitive)
4. Note: Empty environment variables are ignored (treated as unset)
