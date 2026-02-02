# Versioning Guide

This project uses two distinct version numbers that serve different purposes.

## Release Version

The release version (e.g., `1.2.3`) is the user-facing version that identifies
a specific release of the software. It follows semantic versioning and is
synchronized across all components.

### Release Version Locations
- `Cargo.toml` (workspace)
- `client/package.json`
- `signaling/package.json`
- `client/src-tauri/Cargo.toml`
- `client/src-tauri/tauri.conf.json`

### Updating Release Version
Use `scripts/bump-version.sh` to update all locations:
```bash
./scripts/bump-version.sh 1.2.3
```

## Protocol Version

The protocol version (`PROTOCOL_VERSION` in `crates/protocol/src/messages.rs`)
identifies the wire protocol used for client-daemon communication.

### When to Increment Protocol Version
- Breaking changes to message format
- New required fields in messages
- Removed or renamed message types
- Changed semantics of existing messages

### When NOT to Increment Protocol Version
- New optional fields with defaults
- Bug fixes
- Release version bumps
- UI or documentation changes

### Backward Compatibility
When protocol version changes, older clients cannot communicate with newer
daemons (and vice versa). Plan protocol changes carefully and consider
migration strategies.

## Version Compatibility Matrix

| Client Version | Daemon Version | Compatible? |
|---------------|----------------|-------------|
| Same          | Same           | Yes         |
| Different     | Same Protocol  | Yes         |
| Any           | Different Proto| No          |
