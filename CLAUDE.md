# RemoShell Project Memory

## Build Commands

### Frontend (client/)
```bash
npm ci                 # Install dependencies (clean)
npm run dev            # Dev server (localhost:3000)
npm run build          # Production build
npm run typecheck      # TypeScript validation
npm test               # Run tests
```

### Rust (workspace root)
```bash
cargo build            # Build all crates
cargo test --workspace # Run all tests
cargo fmt --all        # Format code
cargo clippy --all-targets -- -D warnings  # Lint (strict)
```

### Tauri (client/)
```bash
npm run tauri build    # Build desktop app
npm run tauri android build  # Build Android APK
npm run tauri android init   # Initialize Android project
```

## CI/CD Patterns

### Workflow Orchestration
- **Pattern**: CI runs first, Release uses `workflow_run` trigger to wait for CI
- **Why**: Prevents releasing broken code when CI fails
- **Files**: `.github/workflows/ci.yml`, `.github/workflows/release.yml`

### GitHub Actions Secrets in Conditions
- **WRONG**: `if: ${{ secrets.MY_SECRET != '' }}` - Causes workflow parsing failures
- **WRONG**: `if: ${{ env.MY_SECRET != '' }}` - env not available at condition eval time
- **RIGHT**: Set job-level env: `env: HAS_SECRET: ${{ secrets.MY_SECRET != '' }}`
  Then use: `if: ${{ env.HAS_SECRET == 'true' }}`

### GitHub Pages Deployment
- **Pattern**: Single deployment in Release workflow only
- **Why**: Prevents race conditions from parallel deployments
- **Deleted**: `deploy-web.yml` (was duplicate)

## Android Build

### Signing
- Uses `apksigner` directly (not third-party actions)
- Keystore stored as base64 in `ANDROID_SIGNING_KEY` secret
- Required secrets: `ANDROID_SIGNING_KEY`, `ANDROID_KEY_ALIAS`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_PASSWORD`

### Icons
- Source: `client/src-tauri/icons/android/`
- Copied during build to `gen/android/app/src/main/res/mipmap-*/`
- Supports: mdpi, hdpi, xhdpi, xxhdpi, xxxhdpi, anydpi-v26 (adaptive)

### Safe Areas
- Use `viewport-fit=cover` in HTML meta
- Use `100dvh` instead of `100vh` for dynamic viewport
- Apply `env(safe-area-inset-*)` CSS for status bar handling

## Linux Desktop Build

### Required Dependencies
```bash
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```
**Note**: `libgtk-3-dev` is often forgotten but required for Tauri.

## Code Style

### Rust
- Format: `cargo fmt --all` (uses `rustfmt.toml`)
- Lint: `cargo clippy -- -D warnings` (deny all warnings)
- Edition: 2021
- Auto-format hook: `.claude/hooks/format-rust.sh`

### TypeScript
- Strict mode enabled
- SolidJS (not React) for frontend
- Run `npm run typecheck` before commits

## Versioning

- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`
- Breaking changes: `feat!:` or `BREAKING CHANGE:` in body
- Auto-bump: major for breaking, minor for feat, patch for fix
- Script: `scripts/bump-version.sh`
