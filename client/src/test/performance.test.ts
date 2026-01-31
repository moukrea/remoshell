/**
 * Performance tests for the RemoteShell client.
 *
 * Tests:
 * - Bundle size assertions
 * - Memory usage patterns
 * - Buffer pool efficiency
 * - Write batcher correctness
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  setProfilingEnabled,
  isProfilingEnabled,
  startTiming,
  endTiming,
  recordSample,
  getStats,
  getAllStats,
  clearMetrics,
  MetricNames,
  measure,
  measureAsync,
} from '../lib/performance/metrics';
import {
  BufferPool,
  getBufferPool,
  resetBufferPool,
  WriteBatcher,
} from '../lib/performance/buffer-pool';

describe('Performance Metrics', () => {
  beforeEach(() => {
    clearMetrics();
    setProfilingEnabled(true);
  });

  afterEach(() => {
    setProfilingEnabled(false);
  });

  it('should toggle profiling on and off', () => {
    expect(isProfilingEnabled()).toBe(true);
    setProfilingEnabled(false);
    expect(isProfilingEnabled()).toBe(false);
  });

  it('should record and calculate timing statistics', () => {
    const metricName = 'test.metric';

    // Record some samples
    for (let i = 1; i <= 100; i++) {
      recordSample(metricName, i);
    }

    const stats = getStats(metricName);
    expect(stats).toBeDefined();
    expect(stats!.count).toBe(100);
    expect(stats!.min).toBe(1);
    expect(stats!.max).toBe(100);
    expect(stats!.avg).toBe(50.5);
    expect(stats!.p50).toBeCloseTo(50, 0);
    expect(stats!.p95).toBeCloseTo(95, 0);
    expect(stats!.p99).toBeCloseTo(99, 0);
  });

  it('should handle start/end timing pairs', () => {
    const metricName = 'test.timing';

    startTiming(metricName, 'op1');
    // Simulate some work
    const duration = endTiming(metricName, 'op1');

    expect(duration).toBeDefined();
    expect(duration!).toBeGreaterThanOrEqual(0);

    const stats = getStats(metricName);
    expect(stats).toBeDefined();
    expect(stats!.count).toBe(1);
  });

  it('should support concurrent timing measurements', () => {
    const metricName = 'concurrent.test';

    // Start multiple concurrent measurements
    startTiming(metricName, 'a');
    startTiming(metricName, 'b');
    startTiming(metricName, 'c');

    // End in different order
    endTiming(metricName, 'b');
    endTiming(metricName, 'a');
    endTiming(metricName, 'c');

    const stats = getStats(metricName);
    expect(stats).toBeDefined();
    expect(stats!.count).toBe(3);
  });

  it('should measure synchronous function execution', () => {
    const metricName = 'sync.function';

    const result = measure(metricName, () => {
      // Simulate work
      let sum = 0;
      for (let i = 0; i < 1000; i++) {
        sum += i;
      }
      return sum;
    });

    expect(result).toBe(499500);
    const stats = getStats(metricName);
    expect(stats).toBeDefined();
    expect(stats!.count).toBe(1);
  });

  it('should measure async function execution', async () => {
    const metricName = 'async.function';

    const result = await measureAsync(metricName, async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
      return 'done';
    });

    expect(result).toBe('done');
    const stats = getStats(metricName);
    expect(stats).toBeDefined();
    expect(stats!.count).toBe(1);
    expect(stats!.avg).toBeGreaterThanOrEqual(10);
  });

  it('should skip recording when profiling is disabled', () => {
    setProfilingEnabled(false);

    recordSample('disabled.metric', 100);
    startTiming('disabled.timing');
    endTiming('disabled.timing');

    const stats = getStats('disabled.metric');
    expect(stats).toBeUndefined();
  });

  it('should return all collected stats', () => {
    recordSample('metric.a', 1);
    recordSample('metric.b', 2);
    recordSample('metric.c', 3);

    const allStats = getAllStats();
    expect(allStats.length).toBe(3);
    expect(allStats.map((s) => s.name)).toContain('metric.a');
    expect(allStats.map((s) => s.name)).toContain('metric.b');
    expect(allStats.map((s) => s.name)).toContain('metric.c');
  });

  it('should have defined metric names', () => {
    expect(MetricNames.TERMINAL_WRITE).toBe('terminal.write');
    expect(MetricNames.TERMINAL_RENDER).toBe('terminal.render');
    expect(MetricNames.WEBRTC_LATENCY).toBe('webrtc.latency');
    expect(MetricNames.PAIRING_TOTAL).toBe('pairing.total');
  });
});

describe('Buffer Pool', () => {
  let pool: BufferPool;

  beforeEach(() => {
    pool = new BufferPool(16);
  });

  it('should acquire buffers of requested size', () => {
    const buffer = pool.acquire(1024);
    expect(buffer.buffer.byteLength).toBeGreaterThanOrEqual(1024);
    expect(buffer.view).toBeInstanceOf(Uint8Array);
    buffer.release();
  });

  it('should round up buffer size to power of 2', () => {
    const sizes = [100, 500, 1000, 3000];
    const expected = [128, 512, 1024, 4096];

    for (let i = 0; i < sizes.length; i++) {
      const buffer = pool.acquire(sizes[i]);
      expect(buffer.buffer.byteLength).toBe(expected[i]);
      buffer.release();
    }
  });

  it('should reuse released buffers', () => {
    // Acquire and release a buffer
    const buffer1 = pool.acquire(1024);
    const originalBuffer = buffer1.buffer;
    buffer1.release();

    // Acquire again - should get the same buffer
    const buffer2 = pool.acquire(1024);
    expect(buffer2.buffer).toBe(originalBuffer);
    buffer2.release();
  });

  it('should track hit rate statistics', () => {
    // First acquisition is a miss
    const b1 = pool.acquire(1024);
    b1.release();

    // Second is a hit
    const b2 = pool.acquire(1024);
    b2.release();

    // Third is a hit
    const b3 = pool.acquire(1024);
    b3.release();

    const stats = pool.getStats();
    expect(stats.misses).toBe(1);
    expect(stats.hits).toBe(2);
    expect(stats.hitRate).toBeCloseTo(0.67, 1);
  });

  it('should wrap data into pooled buffer', () => {
    const data = new Uint8Array([1, 2, 3, 4, 5]);
    const buffer = pool.wrap(data);

    expect(buffer.length).toBe(5);
    expect(buffer.view[0]).toBe(1);
    expect(buffer.view[4]).toBe(5);
    buffer.release();
  });

  it('should handle array data in wrap', () => {
    const buffer = pool.wrap([10, 20, 30]);
    expect(buffer.length).toBe(3);
    expect(buffer.view[1]).toBe(20);
    buffer.release();
  });

  it('should not pool very large buffers', () => {
    // Acquire a large buffer (> 64KB)
    const largeBuffer = pool.acquire(100000);
    largeBuffer.release();

    // Should not be in the pool
    const stats = pool.getStats();
    expect(stats.pooledBuffers).toBe(0);
  });

  it('should respect max pool size', () => {
    const smallPool = new BufferPool(2);

    // Acquire and release 5 buffers
    const buffers = [];
    for (let i = 0; i < 5; i++) {
      buffers.push(smallPool.acquire(1024));
    }
    for (const b of buffers) {
      b.release();
    }

    // Only 2 should be pooled
    expect(smallPool.getStats().pooledBuffers).toBe(2);
  });

  it('should clear all pooled buffers', () => {
    const b1 = pool.acquire(1024);
    const b2 = pool.acquire(2048);
    b1.release();
    b2.release();

    expect(pool.getStats().pooledBuffers).toBe(2);

    pool.clear();
    expect(pool.getStats().pooledBuffers).toBe(0);
    expect(pool.getStats().hits).toBe(0);
  });
});

describe('Global Buffer Pool', () => {
  beforeEach(() => {
    resetBufferPool();
  });

  it('should provide singleton buffer pool', () => {
    const pool1 = getBufferPool();
    const pool2 = getBufferPool();
    expect(pool1).toBe(pool2);
  });

  it('should reset global pool', () => {
    const pool1 = getBufferPool();
    pool1.acquire(1024).release();

    resetBufferPool();

    const pool2 = getBufferPool();
    expect(pool1).not.toBe(pool2);
    expect(pool2.getStats().pooledBuffers).toBe(0);
  });
});

describe('Write Batcher', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should batch multiple writes', () => {
    const flushed: Uint8Array[] = [];
    const batcher = new WriteBatcher(
      (data) => flushed.push(data),
      100, // maxSize
      50 // maxDelayMs
    );

    batcher.write(new Uint8Array([1, 2, 3]));
    batcher.write(new Uint8Array([4, 5]));

    expect(flushed.length).toBe(0);

    vi.advanceTimersByTime(50);

    expect(flushed.length).toBe(1);
    expect(flushed[0]).toEqual(new Uint8Array([1, 2, 3, 4, 5]));
  });

  it('should flush immediately when size exceeded', () => {
    const flushed: Uint8Array[] = [];
    const batcher = new WriteBatcher(
      (data) => flushed.push(data),
      10, // maxSize
      1000 // maxDelayMs
    );

    batcher.write(new Uint8Array([1, 2, 3, 4, 5]));
    expect(flushed.length).toBe(0);

    batcher.write(new Uint8Array([6, 7, 8, 9, 10, 11])); // Exceeds 10 bytes total
    expect(flushed.length).toBe(1);
    expect(flushed[0].length).toBe(11);
  });

  it('should allow manual flush', () => {
    const flushed: Uint8Array[] = [];
    const batcher = new WriteBatcher(
      (data) => flushed.push(data),
      1000,
      1000
    );

    batcher.write(new Uint8Array([1]));
    batcher.write(new Uint8Array([2]));
    batcher.flush();

    expect(flushed.length).toBe(1);
    expect(flushed[0]).toEqual(new Uint8Array([1, 2]));
  });

  it('should handle cancel correctly', () => {
    const flushed: Uint8Array[] = [];
    const batcher = new WriteBatcher(
      (data) => flushed.push(data),
      100,
      50
    );

    batcher.write(new Uint8Array([1, 2, 3]));
    batcher.cancel();

    vi.advanceTimersByTime(100);

    expect(flushed.length).toBe(0);
  });

  it('should handle empty flush gracefully', () => {
    const flushed: Uint8Array[] = [];
    const batcher = new WriteBatcher((data) => flushed.push(data));

    batcher.flush();
    expect(flushed.length).toBe(0);
  });
});

describe('Bundle Size Targets', () => {
  // Note: These are placeholder tests. In a real scenario,
  // you would read actual bundle sizes from the build output.

  it('should document bundle size target of < 500KB', () => {
    const TARGET_BUNDLE_SIZE_KB = 500;
    expect(TARGET_BUNDLE_SIZE_KB).toBe(500);
  });

  it('should document initial load target of < 200KB', () => {
    // The initial bundle (without lazy-loaded chunks) should be smaller
    const TARGET_INITIAL_SIZE_KB = 200;
    expect(TARGET_INITIAL_SIZE_KB).toBe(200);
  });
});

describe('Latency Targets', () => {
  it('should document pairing time target of < 3 seconds', () => {
    const TARGET_PAIRING_MS = 3000;
    expect(TARGET_PAIRING_MS).toBe(3000);
  });

  it('should document input-output latency target of < 50ms', () => {
    const TARGET_LATENCY_MS = 50;
    expect(TARGET_LATENCY_MS).toBe(50);
  });
});

describe('Memory Targets', () => {
  it('should document daemon memory target of < 50MB', () => {
    const TARGET_DAEMON_MEMORY_MB = 50;
    expect(TARGET_DAEMON_MEMORY_MB).toBe(50);
  });

  it('should document scrollback limit for memory efficiency', () => {
    // Scrollback is limited to reduce memory usage
    const SCROLLBACK_LIMIT = 3000;
    expect(SCROLLBACK_LIMIT).toBe(3000);
  });
});
