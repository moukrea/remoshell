/**
 * Buffer pool for efficient memory reuse in hot paths.
 *
 * Reduces garbage collection pressure by recycling ArrayBuffer instances
 * instead of allocating new ones for each message.
 */

/** Default pool size */
const DEFAULT_POOL_SIZE = 32;

/** Default buffer size (4KB - typical terminal chunk) */
const DEFAULT_BUFFER_SIZE = 4096;

/** Maximum buffer size to pool (larger buffers are not reused) */
const MAX_POOLED_SIZE = 65536; // 64KB

/**
 * A pooled buffer wrapper that tracks usage.
 */
export interface PooledBuffer {
  /** The underlying ArrayBuffer */
  buffer: ArrayBuffer;
  /** Get a Uint8Array view of the buffer */
  view: Uint8Array;
  /** Release the buffer back to the pool */
  release: () => void;
  /** The actual data length (may be less than buffer size) */
  length: number;
}

/**
 * Internal pooled buffer implementation.
 */
class PooledBufferImpl implements PooledBuffer {
  readonly buffer: ArrayBuffer;
  readonly view: Uint8Array;
  private readonly pool: BufferPool;
  private _length: number = 0;
  private released: boolean = false;

  constructor(buffer: ArrayBuffer, pool: BufferPool) {
    this.buffer = buffer;
    this.view = new Uint8Array(buffer);
    this.pool = pool;
  }

  get length(): number {
    return this._length;
  }

  set length(value: number) {
    this._length = Math.min(value, this.buffer.byteLength);
  }

  release(): void {
    if (this.released) return;
    this.released = true;
    this._length = 0;
    this.pool.returnBuffer(this);
  }

  reset(): void {
    this.released = false;
    this._length = 0;
  }
}

/**
 * Buffer pool for efficient memory reuse.
 *
 * Maintains separate pools for different buffer sizes.
 */
export class BufferPool {
  private pools: Map<number, PooledBufferImpl[]> = new Map();
  private readonly maxPoolSize: number;

  /** Stats for monitoring */
  private stats = {
    hits: 0,
    misses: 0,
    allocations: 0,
    returns: 0,
  };

  constructor(maxPoolSize: number = DEFAULT_POOL_SIZE) {
    this.maxPoolSize = maxPoolSize;
  }

  /**
   * Acquire a buffer of at least the specified size.
   *
   * @param minSize - Minimum buffer size needed
   * @returns A pooled buffer that should be released when done
   */
  acquire(minSize: number = DEFAULT_BUFFER_SIZE): PooledBuffer {
    // Round up to power of 2 for better reuse
    const size = this.roundUpToPowerOf2(Math.max(minSize, 64));

    // Check if we have a pooled buffer
    const pool = this.pools.get(size);
    if (pool && pool.length > 0) {
      const buffer = pool.pop()!;
      buffer.reset();
      this.stats.hits++;
      return buffer;
    }

    // Allocate new buffer
    this.stats.misses++;
    this.stats.allocations++;
    const arrayBuffer = new ArrayBuffer(size);
    return new PooledBufferImpl(arrayBuffer, this);
  }

  /**
   * Return a buffer to the pool.
   */
  returnBuffer(buffer: PooledBufferImpl): void {
    const size = buffer.buffer.byteLength;

    // Don't pool very large buffers
    if (size > MAX_POOLED_SIZE) {
      return;
    }

    let pool = this.pools.get(size);
    if (!pool) {
      pool = [];
      this.pools.set(size, pool);
    }

    // Only pool if under limit
    if (pool.length < this.maxPoolSize) {
      this.stats.returns++;
      pool.push(buffer);
    }
  }

  /**
   * Get a Uint8Array from the pool, copying data into it.
   *
   * @param data - Data to copy
   * @returns Pooled buffer containing the data
   */
  wrap(data: Uint8Array | ArrayBuffer | number[]): PooledBuffer {
    const source =
      data instanceof ArrayBuffer
        ? new Uint8Array(data)
        : data instanceof Uint8Array
        ? data
        : new Uint8Array(data);

    const buffer = this.acquire(source.length);
    buffer.view.set(source);
    (buffer as PooledBufferImpl).length = source.length;
    return buffer;
  }

  /**
   * Round up to the nearest power of 2.
   */
  private roundUpToPowerOf2(n: number): number {
    n--;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    return n + 1;
  }

  /**
   * Get pool statistics.
   */
  getStats(): { hits: number; misses: number; hitRate: number; pooledBuffers: number } {
    const total = this.stats.hits + this.stats.misses;
    let pooledBuffers = 0;
    this.pools.forEach((pool) => {
      pooledBuffers += pool.length;
    });

    return {
      hits: this.stats.hits,
      misses: this.stats.misses,
      hitRate: total > 0 ? this.stats.hits / total : 0,
      pooledBuffers,
    };
  }

  /**
   * Clear all pooled buffers.
   */
  clear(): void {
    this.pools.clear();
    this.stats = { hits: 0, misses: 0, allocations: 0, returns: 0 };
  }
}

/** Global buffer pool instance */
let globalPool: BufferPool | null = null;

/**
 * Get the global buffer pool.
 */
export function getBufferPool(): BufferPool {
  if (!globalPool) {
    globalPool = new BufferPool();
  }
  return globalPool;
}

/**
 * Reset the global buffer pool.
 */
export function resetBufferPool(): void {
  if (globalPool) {
    globalPool.clear();
  }
  globalPool = null;
}

/**
 * Write batcher for coalescing multiple small writes.
 *
 * Reduces the number of write operations by batching data within a time window.
 */
export class WriteBatcher {
  private buffer: Uint8Array[] = [];
  private totalSize: number = 0;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly flushCallback: (data: Uint8Array) => void;
  private readonly maxSize: number;
  private readonly maxDelayMs: number;

  constructor(
    flushCallback: (data: Uint8Array) => void,
    maxSize: number = 16384, // 16KB
    maxDelayMs: number = 16 // ~1 frame at 60fps
  ) {
    this.flushCallback = flushCallback;
    this.maxSize = maxSize;
    this.maxDelayMs = maxDelayMs;
  }

  /**
   * Write data to the batcher.
   * Data will be flushed when the buffer is full or after the delay.
   */
  write(data: Uint8Array): void {
    this.buffer.push(data);
    this.totalSize += data.length;

    // Flush immediately if over size limit
    if (this.totalSize >= this.maxSize) {
      this.flush();
      return;
    }

    // Schedule delayed flush if not already scheduled
    if (!this.flushTimer) {
      this.flushTimer = setTimeout(() => {
        this.flush();
      }, this.maxDelayMs);
    }
  }

  /**
   * Force flush all buffered data immediately.
   */
  flush(): void {
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }

    if (this.buffer.length === 0) {
      return;
    }

    // Combine all buffers
    const combined = new Uint8Array(this.totalSize);
    let offset = 0;
    for (const buf of this.buffer) {
      combined.set(buf, offset);
      offset += buf.length;
    }

    // Clear state before callback (in case callback throws)
    this.buffer = [];
    this.totalSize = 0;

    // Invoke callback with combined data
    this.flushCallback(combined);
  }

  /**
   * Cancel any pending flush.
   */
  cancel(): void {
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
    this.buffer = [];
    this.totalSize = 0;
  }
}
