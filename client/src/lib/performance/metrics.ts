/**
 * Performance metrics collection and measurement utilities.
 *
 * Provides timing utilities for profiling terminal rendering,
 * data flow latency, and other performance-critical paths.
 */

/** Performance timing entry */
export interface TimingEntry {
  name: string;
  startTime: number;
  endTime?: number;
  duration?: number;
  metadata?: Record<string, unknown>;
}

/** Aggregate statistics for a metric */
export interface MetricStats {
  name: string;
  count: number;
  min: number;
  max: number;
  avg: number;
  p50: number;
  p95: number;
  p99: number;
}

/** Buffer for collecting timing samples */
const timingSamples: Map<string, number[]> = new Map();

/** Active timing entries (in-progress measurements) */
const activeTimings: Map<string, TimingEntry> = new Map();

/** Maximum samples to retain per metric */
const MAX_SAMPLES = 1000;

/** Whether performance profiling is enabled */
let profilingEnabled = false;

/**
 * Enable or disable performance profiling.
 * When disabled, timing calls become no-ops for minimal overhead.
 */
export function setProfilingEnabled(enabled: boolean): void {
  profilingEnabled = enabled;
  if (!enabled) {
    timingSamples.clear();
    activeTimings.clear();
  }
}

/**
 * Check if profiling is enabled.
 */
export function isProfilingEnabled(): boolean {
  return profilingEnabled;
}

/**
 * Start a timing measurement.
 *
 * @param name - Metric name for this timing
 * @param id - Optional unique ID for concurrent measurements
 * @param metadata - Optional metadata to attach
 */
export function startTiming(
  name: string,
  id?: string,
  metadata?: Record<string, unknown>
): void {
  if (!profilingEnabled) return;

  const key = id ? `${name}:${id}` : name;
  activeTimings.set(key, {
    name,
    startTime: performance.now(),
    metadata,
  });
}

/**
 * End a timing measurement and record the sample.
 *
 * @param name - Metric name for this timing
 * @param id - Optional unique ID (must match startTiming)
 * @returns Duration in milliseconds, or undefined if not found
 */
export function endTiming(name: string, id?: string): number | undefined {
  if (!profilingEnabled) return undefined;

  const key = id ? `${name}:${id}` : name;
  const entry = activeTimings.get(key);
  if (!entry) return undefined;

  const endTime = performance.now();
  const duration = endTime - entry.startTime;

  entry.endTime = endTime;
  entry.duration = duration;

  // Record the sample
  recordSample(name, duration);

  activeTimings.delete(key);
  return duration;
}

/**
 * Record a timing sample directly (for external measurements).
 *
 * @param name - Metric name
 * @param durationMs - Duration in milliseconds
 */
export function recordSample(name: string, durationMs: number): void {
  if (!profilingEnabled) return;

  let samples = timingSamples.get(name);
  if (!samples) {
    samples = [];
    timingSamples.set(name, samples);
  }

  samples.push(durationMs);

  // Trim old samples if over limit
  if (samples.length > MAX_SAMPLES) {
    samples.splice(0, samples.length - MAX_SAMPLES);
  }
}

/**
 * Calculate statistics for a metric.
 *
 * @param name - Metric name
 * @returns Statistics or undefined if no samples
 */
export function getStats(name: string): MetricStats | undefined {
  const samples = timingSamples.get(name);
  if (!samples || samples.length === 0) return undefined;

  const sorted = [...samples].sort((a, b) => a - b);
  const count = sorted.length;
  const sum = sorted.reduce((acc, v) => acc + v, 0);

  return {
    name,
    count,
    min: sorted[0],
    max: sorted[count - 1],
    avg: sum / count,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
  };
}

/**
 * Calculate percentile from sorted array.
 */
function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(index, sorted.length - 1))];
}

/**
 * Get all collected statistics.
 */
export function getAllStats(): MetricStats[] {
  const stats: MetricStats[] = [];
  for (const name of timingSamples.keys()) {
    const s = getStats(name);
    if (s) stats.push(s);
  }
  return stats;
}

/**
 * Clear all collected samples.
 */
export function clearMetrics(): void {
  timingSamples.clear();
}

/**
 * Format stats as a readable string.
 */
export function formatStats(stats: MetricStats): string {
  return (
    `${stats.name}: count=${stats.count}, ` +
    `avg=${stats.avg.toFixed(2)}ms, ` +
    `p50=${stats.p50.toFixed(2)}ms, ` +
    `p95=${stats.p95.toFixed(2)}ms, ` +
    `p99=${stats.p99.toFixed(2)}ms`
  );
}

/**
 * Log all collected metrics to console.
 */
export function logMetrics(): void {
  const allStats = getAllStats();
  if (allStats.length === 0) {
    console.log('[Performance] No metrics collected');
    return;
  }

  console.group('[Performance] Metrics Summary');
  for (const stats of allStats) {
    console.log(formatStats(stats));
  }
  console.groupEnd();
}

// Performance metric names for consistency
export const MetricNames = {
  // Terminal rendering
  TERMINAL_WRITE: 'terminal.write',
  TERMINAL_RENDER: 'terminal.render',
  TERMINAL_RESIZE: 'terminal.resize',

  // Data flow
  DATA_RECEIVE: 'data.receive',
  DATA_DECODE: 'data.decode',
  DATA_SEND: 'data.send',
  DATA_ENCODE: 'data.encode',

  // WebRTC
  WEBRTC_CONNECT: 'webrtc.connect',
  WEBRTC_LATENCY: 'webrtc.latency',

  // Pairing
  PAIRING_TOTAL: 'pairing.total',
  PAIRING_QR_SCAN: 'pairing.qr_scan',
  PAIRING_SIGNALING: 'pairing.signaling',
} as const;

/**
 * Measure the execution time of a function.
 *
 * @param name - Metric name
 * @param fn - Function to measure
 * @returns Result of the function
 */
export function measure<T>(name: string, fn: () => T): T {
  if (!profilingEnabled) return fn();

  const start = performance.now();
  try {
    return fn();
  } finally {
    const duration = performance.now() - start;
    recordSample(name, duration);
  }
}

/**
 * Measure the execution time of an async function.
 *
 * @param name - Metric name
 * @param fn - Async function to measure
 * @returns Result of the function
 */
export async function measureAsync<T>(
  name: string,
  fn: () => Promise<T>
): Promise<T> {
  if (!profilingEnabled) return fn();

  const start = performance.now();
  try {
    return await fn();
  } finally {
    const duration = performance.now() - start;
    recordSample(name, duration);
  }
}
