# Local Development Guide

This guide covers setting up and running RemoteShell for local development.

## Prerequisites

### Required Tools

- **Rust** (1.75+): Install via [rustup](https://rustup.rs/)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **Node.js** (18+): Install via [nvm](https://github.com/nvm-sh/nvm) or [official installer](https://nodejs.org/)
  ```bash
  nvm install 18
  nvm use 18
  ```

- **pnpm**: Package manager for the frontend
  ```bash
  npm install -g pnpm
  ```

- **wrangler**: Cloudflare Workers CLI (for signaling server)
  ```bash
  npm install -g wrangler
  ```

### Optional Tools

- **Docker & Docker Compose**: For containerized development
- **cargo-watch**: For automatic Rust rebuilds
  ```bash
  cargo install cargo-watch
  ```

## Initial Setup

### 1. Clone the Repository

```bash
git clone https://github.com/your-org/remoshell.git
cd remoshell
```

### 2. Configure Environment

```bash
# Copy environment template
cp .env.local.example .env.local

# Copy daemon config template
cp config/local.toml.example config/local.toml
```

Edit `.env.local` and `config/local.toml` as needed for your environment.

### 3. Install Dependencies

```bash
# Install Rust dependencies (will compile on first build)
cargo check

# Install frontend dependencies
cd client && pnpm install && cd ..

# Install signaling dependencies
cd signaling && pnpm install && cd ..
```

## Running Without Docker

### Start Services Individually

**Terminal 1 - Signaling Server:**
```bash
cd signaling
pnpm dev
# Runs on ws://localhost:8787
```

**Terminal 2 - Daemon:**
```bash
cargo run -p remoshell-daemon
# Runs on http://localhost:8080
```

**Terminal 3 - Frontend:**
```bash
cd client
pnpm dev
# Runs on http://localhost:5173
```

### Using Make Commands

```bash
# Start all services (requires multiple terminals or tmux)
make dev

# Start individual services
make dev-signaling
make dev-daemon
make dev-frontend

# Run with hot reload
make watch-daemon
```

## Running With Docker

### Quick Start

```bash
# Start all services
docker compose up

# Start in detached mode
docker compose up -d

# View logs
docker compose logs -f

# Stop all services
docker compose down
```

### Individual Services

```bash
# Start only signaling server
docker compose up signaling

# Start signaling and daemon
docker compose up signaling daemon

# Start with TURN server (for WebRTC relay testing)
docker compose --profile turn up
```

### Using Docker Environment

```bash
# Copy Docker environment template
cp .env.docker.example .env.docker

# Use Docker environment file
docker compose --env-file .env.docker up
```

### Rebuilding Containers

```bash
# Rebuild all containers
docker compose build

# Rebuild specific service
docker compose build daemon

# Rebuild without cache
docker compose build --no-cache
```

## Development Workflow

### Code Changes

1. **Rust code (daemon/crates)**: Changes require rebuild
   - With `cargo-watch`: Automatic rebuild
   - Without: Run `cargo build` then restart

2. **Frontend (client)**: Vite hot-reloads automatically

3. **Signaling (workers)**: Wrangler hot-reloads automatically

### Running Tests

```bash
# Run all tests
make test

# Run Rust tests only
cargo test

# Run frontend tests
cd client && pnpm test

# Run with coverage
make test-coverage
```

### Linting and Formatting

```bash
# Check all
make lint

# Format code
make fmt

# Rust only
cargo fmt
cargo clippy

# Frontend only
cd client && pnpm lint
```

## Troubleshooting

### Port Already in Use

```bash
# Find process using port 8787
lsof -i :8787

# Kill process
kill -9 <PID>

# Or use different ports in .env.local
```

### Signaling Connection Failed

1. Verify signaling server is running: `curl http://localhost:8787/health`
2. Check WebSocket URL in environment matches running server
3. Check firewall/network settings

### Docker Issues

```bash
# Reset all containers and volumes
docker compose down -v

# Clear Docker build cache
docker builder prune

# View container logs
docker compose logs <service-name>

# Enter container for debugging
docker compose exec daemon /bin/bash
```

### Rust Build Failures

```bash
# Clean build artifacts
cargo clean

# Update dependencies
cargo update

# Check for missing system dependencies
# On Ubuntu/Debian:
sudo apt-get install build-essential pkg-config libssl-dev
```

### WebRTC Connection Issues

1. Check ICE server configuration in `config/local.toml`
2. For NAT traversal issues, enable TURN server:
   ```bash
   docker compose --profile turn up
   ```
3. Check browser console for WebRTC errors

### Hot Reload Not Working

**Rust (cargo-watch):**
```bash
# Install cargo-watch
cargo install cargo-watch

# Run with watch
cargo watch -x 'run -p remoshell-daemon'
```

**Frontend (Vite):**
- Ensure you're accessing via `localhost`, not `127.0.0.1`
- Check Vite configuration in `client/vite.config.ts`

**Docker:**
- Verify volume mounts are correct in `docker-compose.yml`
- Check file permissions on mounted volumes

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Rust log level (trace/debug/info/warn/error) |
| `SIGNALING_URL` | `ws://localhost:8787` | Signaling server WebSocket URL |
| `DAEMON_PORT` | `8080` | Daemon HTTP server port |
| `DEV_SERVER_PORT` | `5173` | Vite dev server port |
| `VITE_SIGNALING_URL` | `ws://localhost:8787` | Frontend signaling URL |
| `TURN_USER` | - | TURN server username |
| `TURN_PASS` | - | TURN server password |

## Next Steps

- Read [ARCHITECTURE.md](./ARCHITECTURE.md) for system design overview
- Review [PROTOCOL.md](./PROTOCOL.md) for WebRTC protocol details
- Check [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution guidelines
