/**
 * Performance measurement and optimization utilities.
 *
 * @module performance
 */

export {
  // Types
  type TimingEntry,
  type MetricStats,

  // Core functions
  setProfilingEnabled,
  isProfilingEnabled,
  startTiming,
  endTiming,
  recordSample,
  measure,
  measureAsync,

  // Statistics
  getStats,
  getAllStats,
  clearMetrics,
  formatStats,
  logMetrics,

  // Metric names
  MetricNames,
} from './metrics';

export {
  // Buffer pool
  type PooledBuffer,
  BufferPool,
  getBufferPool,
  resetBufferPool,

  // Write batching
  WriteBatcher,
} from './buffer-pool';

export {
  // Latency measurement
  type LatencyResult,
  type LatencyStats,
  LatencyMeasurer,
  EndToEndTimer,
  createPingPayload,
  parsePingPayload,
} from './latency';
