# Contributing to RemoShell

Thank you for your interest in contributing to RemoShell! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- **Rust**: Version 1.85 or later (stable toolchain)
- **Node.js**: Version 20 or later
- **npm**: Comes with Node.js
- **System dependencies** (Linux):
  - `libssl-dev` for TLS support
  - `pkg-config` for native library detection

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/remoshell/remoshell.git
cd remoshell

# Build all Rust crates
cargo build

# Install and build frontend
cd client
npm ci
npm run build
```

### Running Tests

```bash
# Rust tests
cargo test

# Frontend tests
cd client
npm test

# Signaling server tests
cd signaling
npm test
```

## Code Style

### Rust

We follow standard Rust formatting and linting:

```bash
# Format code
cargo fmt

# Run clippy with all warnings
cargo clippy -- -D warnings
```

Key style points:
- Use `rustfmt` defaults
- Write doc comments for public items (`///` for items, `//!` for modules)
- Prefer `Result<T, E>` over panics
- Use `thiserror` for error types
- Keep functions focused and reasonably sized

### TypeScript/JavaScript

We use Prettier and ESLint:

```bash
cd client
npm run lint
npm run format
```

Key style points:
- Use TypeScript for all new code
- Write JSDoc comments for exported functions
- Prefer functional components in SolidJS
- Use descriptive variable names

### Commit Messages

Follow conventional commit format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Build process or auxiliary tool changes

Examples:
```
feat(daemon): add session reconnection support
fix(protocol): handle edge case in frame decoding
docs(readme): update installation instructions
```

## Testing Requirements

### Before Submitting a PR

1. **All tests pass**:
   ```bash
   cargo test
   cd client && npm test
   ```

2. **Code is formatted**:
   ```bash
   cargo fmt --check
   cd client && npm run lint
   ```

3. **No clippy warnings**:
   ```bash
   cargo clippy -- -D warnings
   ```

4. **Documentation is updated**: If you add new public APIs, include doc comments.

### Writing Tests

- **Unit tests**: Place in the same file as the code, in a `#[cfg(test)]` module
- **Integration tests**: Place in `tests/` directory
- **Frontend tests**: Colocate with components (`.test.ts` files)

## Pull Request Process

1. **Fork the repository** and create a feature branch:
   ```bash
   git checkout -b feature/my-new-feature
   ```

2. **Make your changes** with clear, atomic commits

3. **Update documentation** if your changes affect public APIs or user-facing behavior

4. **Push your branch** and open a PR:
   ```bash
   git push origin feature/my-new-feature
   ```

5. **Fill out the PR template** with:
   - Summary of changes
   - Related issue numbers
   - Testing performed
   - Screenshots (for UI changes)

6. **Address review feedback** promptly

### PR Checklist

- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] Changelog entry added (for significant changes)
- [ ] All CI checks pass
- [ ] Self-review completed

## Project Structure

Understanding the codebase:

```
remoshell/
├── crates/
│   ├── protocol/        # Core protocol definitions
│   │   ├── src/
│   │   │   ├── crypto.rs    # Ed25519 identity, signing
│   │   │   ├── framing.rs   # Frame codec with LZ4 compression
│   │   │   ├── messages.rs  # MessagePack message types
│   │   │   └── noise.rs     # Noise XX handshake
│   │   └── Cargo.toml
│   ├── daemon/          # Background service
│   │   ├── src/
│   │   │   ├── session/     # PTY session management
│   │   │   ├── network/     # WebRTC/QUIC handlers
│   │   │   ├── devices/     # Trust store
│   │   │   ├── files/       # File browser/transfer
│   │   │   └── ui/          # TUI, QR code, systemd
│   │   └── Cargo.toml
│   └── tauri-client/    # Tauri bindings
│       ├── src/
│       │   ├── commands/    # IPC command handlers
│       │   ├── quic/        # QUIC client manager
│       │   └── storage/     # SQLite + keychain
│       └── Cargo.toml
├── client/              # SolidJS frontend
│   ├── src/
│   │   ├── lib/             # Core libraries
│   │   │   ├── protocol/    # Message serialization
│   │   │   ├── signaling/   # WebSocket client
│   │   │   └── webrtc/      # WebRTC manager
│   │   ├── stores/          # SolidJS state stores
│   │   └── components/      # UI components
│   └── src-tauri/       # Tauri configuration
└── signaling/           # Cloudflare Worker
    └── src/
        ├── index.ts     # Worker entry point
        └── room.ts      # Room management
```

## Issue Templates

### Bug Report

When reporting bugs, include:

1. **Environment**: OS, Rust version, Node.js version
2. **Steps to reproduce**: Minimal reproduction steps
3. **Expected behavior**: What should happen
4. **Actual behavior**: What actually happens
5. **Logs/Screenshots**: Any relevant error messages or screenshots

### Feature Request

When proposing features, include:

1. **Use case**: Why is this feature needed?
2. **Proposed solution**: How should it work?
3. **Alternatives considered**: Other approaches you've thought about
4. **Additional context**: Mockups, examples from other tools, etc.

## Security Issues

**Do not open public issues for security vulnerabilities.**

Instead, please report security issues privately by emailing the maintainers. Include:

1. Description of the vulnerability
2. Steps to reproduce
3. Potential impact
4. Suggested fix (if any)

## Getting Help

- **Discussions**: Use GitHub Discussions for questions and ideas
- **Issues**: Use GitHub Issues for bugs and feature requests
- **Code review**: Request reviews from maintainers for complex changes

## License

By contributing to RemoShell, you agree that your contributions will be licensed under the MIT OR Apache-2.0 license.
