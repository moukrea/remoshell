# Performance Guide

This document describes the performance characteristics, optimization strategies, and benchmarking procedures for RemoteShell.

## Performance Targets

| Metric | Target | Description |
|--------|--------|-------------|
| Pairing Time | < 3 seconds | Time from QR scan to connected state |
| Input-Output Latency | < 50ms | Keystroke to visible output round-trip |
| Bundle Size | < 500KB | Initial JavaScript bundle (gzipped) |
| Daemon Memory | < 50MB | Idle memory consumption |

## Architecture Optimizations

### Client-Side (SolidJS + xterm.js)

#### Lazy Loading

Heavy components are loaded on-demand to reduce initial bundle size:

```typescript
// Terminal loaded when first accessed
const XTermWrapper = lazy(() => import('./components/terminal/XTermWrapper'));

// File browser loaded when files tab opened
const FileBrowser = lazy(() => import('./components/files/FileBrowser'));
```

#### Bundle Splitting

The Vite configuration splits the bundle into logical chunks:

- `vendor-solid`: Core SolidJS framework
- `vendor-xterm`: Terminal rendering (xterm.js + WebGL)
- `vendor-webrtc`: Peer connection (simple-peer)
- `vendor-qr`: QR code scanning (jsqr)
- `vendor-msgpack`: Message serialization

#### Terminal Performance

xterm.js is configured for optimal performance:

- **WebGL Rendering**: GPU-accelerated text rendering
- **Limited Scrollback**: 3000 lines (reduces memory)
- **Fast Scroll**: Alt+scroll for quick navigation
- **Flow Control**: Backpressure prevents buffer overflow

#### Performance Metrics

The client includes built-in performance profiling:

```typescript
import { setProfilingEnabled, logMetrics } from './lib/performance';

// Enable during development
setProfilingEnabled(true);

// View collected metrics
logMetrics();
```

Tracked metrics:
- `terminal.write`: Time to write data
- `terminal.render`: Render completion time
- `webrtc.latency`: Data channel round-trip

### Daemon-Side (Rust)

#### Memory Efficiency

- **Buffer Pooling**: Reuse buffers for PTY I/O
- **Bounded Channels**: Prevent unbounded memory growth
- **Backpressure**: Drop messages for slow clients
- **Stream Processing**: Process data in chunks

#### PTY Optimization

```rust
// Optimal buffer size for PTY reads
const READ_BUFFER_SIZE: usize = 4096;

// Client channel capacity
const DEFAULT_CHANNEL_CAPACITY: usize = 256;
```

#### Async Runtime

- Uses Tokio with `spawn_blocking` for PTY I/O
- Multi-threaded executor for parallelism
- Efficient task scheduling

## Benchmarking

### Client Benchmarks

Build with size analysis:

```bash
cd client
npm run build -- --analyze
```

Check bundle sizes:

```bash
ls -lh dist/assets/*.js
```

### Daemon Benchmarks

Run criterion benchmarks:

```bash
cd crates/daemon
cargo bench
```

Benchmark categories:
- `message_serialization`: Framing overhead
- `buffer_operations`: Allocation patterns
- `data_copy`: Memory throughput
- `channel_throughput`: Inter-task communication

### End-to-End Latency

Measure input latency:

1. Enable profiling in client
2. Type characters and observe metrics
3. Target: p99 < 50ms

```typescript
// In browser console:
import { getStats } from './lib/performance';
console.log(getStats('terminal.write'));
```

## Memory Profiling

### Client

Use browser DevTools Memory tab:
- Take heap snapshots
- Watch for leaks (growing retained size)
- Target: < 50MB for terminal sessions

### Daemon

Use Rust memory profilers:

```bash
# With heaptrack
heaptrack ./target/release/remoshell-daemon start

# View report
heaptrack_gui heaptrack.*.zst
```

Profile allocations:

```bash
# Valgrind massif
valgrind --tool=massif ./target/release/remoshell-daemon start
ms_print massif.out.*
```

## Optimization Checklist

### Before Release

- [ ] Bundle size < 500KB gzipped
- [ ] No console errors in production build
- [ ] All lazy routes load within 500ms
- [ ] Terminal renders at 60fps
- [ ] Memory stable during extended use

### Continuous Monitoring

- [ ] Run benchmarks on CI
- [ ] Track bundle size regression
- [ ] Performance test with slow networks
- [ ] Memory leak detection in tests

## Known Limitations

1. **WebGL Fallback**: Some devices fall back to canvas (slower)
2. **Large Files**: File transfers > 100MB may cause memory pressure
3. **Many Sessions**: > 10 concurrent sessions increases memory usage

## Troubleshooting

### Slow Terminal

1. Check WebGL is enabled: `terminal.options.rendererType`
2. Reduce scrollback if memory-constrained
3. Verify flow control is working (no dropped frames)

### High Latency

1. Check network conditions (ping to signaling server)
2. Verify TURN relay isn't required (direct connection preferred)
3. Monitor data channel buffer levels

### Memory Growth

1. Close unused terminal sessions
2. Clear scrollback periodically
3. Check for event listener leaks
