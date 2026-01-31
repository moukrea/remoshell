/**
 * Latency measurement utilities for WebRTC data channels.
 *
 * Provides ping/pong latency measurement to track end-to-end
 * data channel performance.
 */

import { recordSample, MetricNames } from './metrics';

/** Latency measurement result */
export interface LatencyResult {
  /** Round-trip time in milliseconds */
  rtt: number;
  /** One-way latency estimate (RTT / 2) */
  latency: number;
  /** Timestamp when measurement was taken */
  timestamp: number;
}

/** Latency statistics */
export interface LatencyStats {
  /** Number of measurements */
  count: number;
  /** Minimum RTT observed */
  minRtt: number;
  /** Maximum RTT observed */
  maxRtt: number;
  /** Average RTT */
  avgRtt: number;
  /** Most recent RTT */
  lastRtt: number;
}

/**
 * Manages latency measurements for a connection.
 */
export class LatencyMeasurer {
  private pendingPings: Map<number, number> = new Map();
  private results: LatencyResult[] = [];
  private readonly maxResults: number;
  private pingIdCounter = 0;

  constructor(maxResults: number = 100) {
    this.maxResults = maxResults;
  }

  /**
   * Start a latency measurement (send ping).
   *
   * @returns Ping ID to include in the ping message
   */
  startMeasurement(): { pingId: number; timestamp: number } {
    const pingId = ++this.pingIdCounter;
    const timestamp = performance.now();
    this.pendingPings.set(pingId, timestamp);

    // Clean up old pending pings (> 30 seconds)
    const cutoff = timestamp - 30000;
    for (const [id, time] of this.pendingPings) {
      if (time < cutoff) {
        this.pendingPings.delete(id);
      }
    }

    return { pingId, timestamp };
  }

  /**
   * Complete a latency measurement (received pong).
   *
   * @param pingId - The ping ID from the pong response
   * @returns The latency result, or null if ping not found
   */
  completeMeasurement(pingId: number): LatencyResult | null {
    const startTime = this.pendingPings.get(pingId);
    if (startTime === undefined) {
      return null;
    }

    this.pendingPings.delete(pingId);

    const endTime = performance.now();
    const rtt = endTime - startTime;
    const latency = rtt / 2;

    const result: LatencyResult = {
      rtt,
      latency,
      timestamp: Date.now(),
    };

    // Record to metrics
    recordSample(MetricNames.WEBRTC_LATENCY, rtt);

    // Store result
    this.results.push(result);
    if (this.results.length > this.maxResults) {
      this.results.shift();
    }

    return result;
  }

  /**
   * Get latency statistics.
   */
  getStats(): LatencyStats | null {
    if (this.results.length === 0) {
      return null;
    }

    const rtts = this.results.map((r) => r.rtt);
    const sum = rtts.reduce((a, b) => a + b, 0);

    return {
      count: rtts.length,
      minRtt: Math.min(...rtts),
      maxRtt: Math.max(...rtts),
      avgRtt: sum / rtts.length,
      lastRtt: rtts[rtts.length - 1],
    };
  }

  /**
   * Get the most recent results.
   */
  getRecentResults(count: number = 10): LatencyResult[] {
    return this.results.slice(-count);
  }

  /**
   * Check if latency is within target.
   *
   * @param targetMs - Target latency in milliseconds
   * @returns True if average RTT is within target
   */
  isWithinTarget(targetMs: number = 50): boolean {
    const stats = this.getStats();
    if (!stats) return true; // No data yet
    return stats.avgRtt <= targetMs * 2; // RTT is round-trip, target is one-way
  }

  /**
   * Clear all measurements.
   */
  clear(): void {
    this.pendingPings.clear();
    this.results = [];
  }
}

/**
 * End-to-end timing tracker for terminal data flow.
 *
 * Tracks the time from sending input to receiving corresponding output.
 */
export class EndToEndTimer {
  private pending: Map<string, number> = new Map();
  private results: number[] = [];
  private readonly maxResults: number;

  constructor(maxResults: number = 100) {
    this.maxResults = maxResults;
  }

  /**
   * Mark the start of an operation (e.g., sending input).
   *
   * @param id - Unique identifier for this operation
   */
  start(id: string): void {
    this.pending.set(id, performance.now());

    // Clean up old entries (> 10 seconds)
    const cutoff = performance.now() - 10000;
    for (const [key, time] of this.pending) {
      if (time < cutoff) {
        this.pending.delete(key);
      }
    }
  }

  /**
   * Mark the end of an operation.
   *
   * @param id - The identifier from start()
   * @returns Duration in milliseconds, or null if not found
   */
  end(id: string): number | null {
    const startTime = this.pending.get(id);
    if (startTime === undefined) {
      return null;
    }

    this.pending.delete(id);
    const duration = performance.now() - startTime;

    this.results.push(duration);
    if (this.results.length > this.maxResults) {
      this.results.shift();
    }

    return duration;
  }

  /**
   * Get average end-to-end time.
   */
  getAverage(): number | null {
    if (this.results.length === 0) return null;
    return this.results.reduce((a, b) => a + b, 0) / this.results.length;
  }

  /**
   * Get percentile value.
   */
  getPercentile(p: number): number | null {
    if (this.results.length === 0) return null;
    const sorted = [...this.results].sort((a, b) => a - b);
    const index = Math.ceil((p / 100) * sorted.length) - 1;
    return sorted[Math.max(0, index)];
  }

  /**
   * Clear all measurements.
   */
  clear(): void {
    this.pending.clear();
    this.results = [];
  }
}

/**
 * Create a ping message for latency measurement.
 */
export function createPingPayload(pingId: number): Uint8Array {
  // Simple format: 4-byte ping ID + 8-byte timestamp
  const buffer = new ArrayBuffer(12);
  const view = new DataView(buffer);
  view.setUint32(0, pingId, false); // Big-endian
  view.setFloat64(4, performance.now(), false);
  return new Uint8Array(buffer);
}

/**
 * Parse a ping/pong payload.
 */
export function parsePingPayload(data: Uint8Array): { pingId: number; timestamp: number } | null {
  if (data.length < 12) return null;
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  return {
    pingId: view.getUint32(0, false),
    timestamp: view.getFloat64(4, false),
  };
}
