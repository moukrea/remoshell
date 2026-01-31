/**
 * Tests for protocol serialization.
 *
 * These tests verify that messages can be encoded and decoded correctly,
 * matching the Rust MessagePack serialization format.
 */

import { describe, it, expect } from 'vitest';
import {
  encodeEnvelope,
  decodeEnvelope,
  encodeMessage,
  decodeMessage,
  isValidMessage,
  isValidEnvelope,
  getChannelSerializer,
} from './serialization';
import {
  createEnvelope,
  defaultSessionCreate,
  defaultCapabilities,
  Msg,
  PROTOCOL_VERSION,
  type Message,
} from './messages';

/**
 * Deep equality check that properly compares Uint8Array instances.
 * Vitest's toEqual has issues with cross-realm Uint8Array comparison.
 */
function deepEqual(a: unknown, b: unknown): boolean {
  // Handle Uint8Array specifically
  if (a instanceof Uint8Array && b instanceof Uint8Array) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) return false;
    }
    return true;
  }

  // Handle arrays
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!deepEqual(a[i], b[i])) return false;
    }
    return true;
  }

  // Handle objects
  if (typeof a === 'object' && a !== null && typeof b === 'object' && b !== null) {
    const keysA = Object.keys(a as Record<string, unknown>);
    const keysB = Object.keys(b as Record<string, unknown>);
    if (keysA.length !== keysB.length) return false;
    for (const key of keysA) {
      if (!deepEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])) {
        return false;
      }
    }
    return true;
  }

  // Handle primitives
  return a === b;
}

/**
 * Helper to test roundtrip serialization of an envelope.
 */
function roundtripEnvelope(msg: Message): void {
  const envelope = createEnvelope(42, msg);
  const bytes = encodeEnvelope(envelope);
  const decoded = decodeEnvelope(bytes);
  expect(deepEqual(decoded, envelope)).toBe(true);
}

/**
 * Helper to test roundtrip serialization of a standalone message.
 */
function roundtripMessage(msg: Message): void {
  const bytes = encodeMessage(msg);
  const decoded = decodeMessage(bytes);
  expect(deepEqual(decoded, msg)).toBe(true);
}

describe('Envelope Serialization', () => {
  it('should preserve protocol version', () => {
    const envelope = createEnvelope(1, Msg.Ping({ timestamp: 12345, payload: new Uint8Array() }));
    expect(envelope.version).toBe(PROTOCOL_VERSION);

    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);
    expect(decoded.version).toBe(PROTOCOL_VERSION);
  });

  it('should preserve sequence number', () => {
    const envelope = createEnvelope(999, Msg.Ping({ timestamp: 0, payload: new Uint8Array() }));
    expect(envelope.sequence).toBe(999);

    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);
    expect(decoded.sequence).toBe(999);
  });

  it('should produce compact binary output', () => {
    const envelope = createEnvelope(
      1,
      Msg.Ping({ timestamp: Date.now(), payload: new Uint8Array() })
    );
    const bytes = encodeEnvelope(envelope);
    // Ping should be very compact
    expect(bytes.length).toBeLessThan(100);
  });
});

describe('Session Message Roundtrip', () => {
  it('should roundtrip SessionCreate with all fields', () => {
    roundtripEnvelope(
      Msg.SessionCreate({
        cols: 120,
        rows: 40,
        shell: '/bin/bash',
        env: [
          ['TERM', 'xterm-256color'],
          ['LANG', 'en_US.UTF-8'],
        ],
        cwd: '/home/user',
      })
    );
  });

  it('should roundtrip SessionCreate with defaults', () => {
    roundtripEnvelope(Msg.SessionCreate(defaultSessionCreate()));
  });

  it('should roundtrip SessionCreated', () => {
    roundtripEnvelope(
      Msg.SessionCreated({
        session_id: 'sess-abc123',
        pid: 12345,
      })
    );
  });

  it('should roundtrip SessionAttach', () => {
    roundtripEnvelope(
      Msg.SessionAttach({
        session_id: 'sess-xyz789',
      })
    );
  });

  it('should roundtrip SessionDetach', () => {
    roundtripEnvelope(
      Msg.SessionDetach({
        session_id: 'sess-abc123',
      })
    );
  });

  it('should roundtrip SessionKill', () => {
    roundtripEnvelope(
      Msg.SessionKill({
        session_id: 'sess-abc123',
        signal: 9,
      })
    );
  });

  it('should roundtrip SessionResize', () => {
    roundtripEnvelope(
      Msg.SessionResize({
        session_id: 'sess-abc123',
        cols: 200,
        rows: 50,
      })
    );
  });

  it('should roundtrip SessionData with Stdin', () => {
    roundtripEnvelope(
      Msg.SessionData({
        session_id: 'sess-abc123',
        stream: 'Stdin',
        data: new TextEncoder().encode('ls -la\n'),
      })
    );
  });

  it('should roundtrip SessionData with Stdout', () => {
    roundtripEnvelope(
      Msg.SessionData({
        session_id: 'sess-abc123',
        stream: 'Stdout',
        data: new TextEncoder().encode('total 42\ndrwxr-xr-x  2 user user 4096 Jan  1 12:00 .\n'),
      })
    );
  });

  it('should roundtrip SessionData with Stderr', () => {
    roundtripEnvelope(
      Msg.SessionData({
        session_id: 'sess-abc123',
        stream: 'Stderr',
        data: new TextEncoder().encode('Error: file not found\n'),
      })
    );
  });

  it('should roundtrip SessionClosed with exit code', () => {
    roundtripEnvelope(
      Msg.SessionClosed({
        session_id: 'sess-abc123',
        exit_code: 0,
        signal: null,
        reason: 'Process exited normally',
      })
    );
  });

  it('should roundtrip SessionClosed with signal', () => {
    roundtripEnvelope(
      Msg.SessionClosed({
        session_id: 'sess-abc123',
        exit_code: null,
        signal: 9,
        reason: 'Killed by SIGKILL',
      })
    );
  });
});

describe('File Message Roundtrip', () => {
  it('should roundtrip FileListRequest', () => {
    roundtripEnvelope(
      Msg.FileListRequest({
        path: '/home/user/documents',
        include_hidden: true,
      })
    );
  });

  it('should roundtrip FileListResponse', () => {
    roundtripEnvelope(
      Msg.FileListResponse({
        path: '/home/user',
        entries: [
          {
            name: 'file.txt',
            entry_type: 'File',
            size: 1024,
            mode: 0o644,
            modified: 1704067200,
          },
          {
            name: 'docs',
            entry_type: 'Directory',
            size: 0,
            mode: 0o755,
            modified: 1704067200,
          },
          {
            name: 'link',
            entry_type: 'Symlink',
            size: 0,
            mode: 0o777,
            modified: 1704067200,
          },
        ],
      })
    );
  });

  it('should roundtrip FileDownloadRequest', () => {
    roundtripEnvelope(
      Msg.FileDownloadRequest({
        path: '/home/user/large-file.bin',
        offset: 1024,
        chunk_size: 65536,
      })
    );
  });

  it('should roundtrip FileDownloadChunk', () => {
    roundtripEnvelope(
      Msg.FileDownloadChunk({
        path: '/home/user/file.txt',
        offset: 0,
        total_size: 100,
        data: new TextEncoder().encode('Hello, World!'),
        is_last: false,
      })
    );
  });

  it('should roundtrip FileUploadStart', () => {
    roundtripEnvelope(
      Msg.FileUploadStart({
        path: '/home/user/upload.bin',
        size: 1048576,
        mode: 0o644,
        overwrite: true,
      })
    );
  });

  it('should roundtrip FileUploadChunk', () => {
    roundtripEnvelope(
      Msg.FileUploadChunk({
        path: '/home/user/upload.bin',
        offset: 65536,
        data: new Uint8Array(1024).fill(0),
      })
    );
  });

  it('should roundtrip FileUploadComplete', () => {
    roundtripEnvelope(
      Msg.FileUploadComplete({
        path: '/home/user/upload.bin',
        checksum: new Uint8Array([0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90]),
      })
    );
  });
});

describe('Device Message Roundtrip', () => {
  it('should roundtrip DeviceInfo', () => {
    roundtripEnvelope(
      Msg.DeviceInfo({
        device_id: 'abc123def456',
        name: 'My Laptop',
        os: 'Linux',
        os_version: '6.1.0',
        arch: 'x86_64',
        protocol_version: PROTOCOL_VERSION,
      })
    );
  });

  it('should roundtrip DeviceApprovalRequest', () => {
    roundtripEnvelope(
      Msg.DeviceApprovalRequest({
        device_id: 'new-device-123',
        name: 'Work Computer',
        public_key: new Uint8Array(32).fill(0x04),
        reason: 'Need to access project files',
      })
    );
  });

  it('should roundtrip DeviceApproved', () => {
    roundtripEnvelope(
      Msg.DeviceApproved({
        device_id: 'new-device-123',
        expires_at: 1735689600,
        allowed_capabilities: ['shell', 'file-read'],
      })
    );
  });

  it('should roundtrip DeviceRejected', () => {
    roundtripEnvelope(
      Msg.DeviceRejected({
        device_id: 'suspicious-device',
        reason: 'Device not recognized',
        retry_allowed: false,
      })
    );
  });
});

describe('Control Message Roundtrip', () => {
  it('should roundtrip Ping', () => {
    roundtripEnvelope(
      Msg.Ping({
        timestamp: 1704067200000,
        payload: new TextEncoder().encode('ping!'),
      })
    );
  });

  it('should roundtrip Pong', () => {
    roundtripEnvelope(
      Msg.Pong({
        timestamp: 1704067200000,
        payload: new TextEncoder().encode('ping!'),
      })
    );
  });

  it('should roundtrip Error', () => {
    roundtripEnvelope(
      Msg.Error({
        code: 'NotFound',
        message: 'Session not found',
        context: 'sess-unknown',
        recoverable: false,
      })
    );
  });

  it('should roundtrip Capabilities', () => {
    roundtripEnvelope(Msg.Capabilities(defaultCapabilities()));
  });

  it('should roundtrip custom Capabilities', () => {
    roundtripEnvelope(
      Msg.Capabilities({
        protocol_versions: [1, 2],
        features: ['shell', 'files', 'tunnels'],
        max_message_size: 2 * 1024 * 1024,
        max_sessions: 32,
        compression: ['lz4', 'zstd'],
      })
    );
  });
});

describe('Error Code Roundtrip', () => {
  const errorCodes = [
    'Unknown',
    'Unauthorized',
    'NotFound',
    'InvalidRequest',
    'InternalError',
    'Timeout',
    'RateLimited',
    'AlreadyExists',
    'PermissionDenied',
    'VersionMismatch',
  ] as const;

  for (const code of errorCodes) {
    it(`should roundtrip Error with code ${code}`, () => {
      roundtripEnvelope(
        Msg.Error({
          code,
          message: `Test error: ${code}`,
          context: null,
          recoverable: true,
        })
      );
    });
  }
});

describe('Standalone Message Serialization', () => {
  it('should roundtrip message without envelope', () => {
    roundtripMessage(
      Msg.Ping({
        timestamp: 12345,
        payload: new Uint8Array(),
      })
    );
  });

  it('should roundtrip complex message without envelope', () => {
    roundtripMessage(
      Msg.SessionCreate({
        cols: 80,
        rows: 24,
        shell: '/bin/bash',
        env: [['TERM', 'xterm-256color']],
        cwd: '/home/user',
      })
    );
  });
});

describe('Edge Cases', () => {
  it('should handle empty data', () => {
    roundtripEnvelope(
      Msg.SessionData({
        session_id: 's',
        stream: 'Stdin',
        data: new Uint8Array(0),
      })
    );
  });

  it('should handle large data', () => {
    // Test with 64KB of data
    roundtripEnvelope(
      Msg.SessionData({
        session_id: 'sess-large',
        stream: 'Stdout',
        data: new Uint8Array(65536).fill(0xab),
      })
    );
  });

  it('should handle unicode strings', () => {
    roundtripEnvelope(
      Msg.SessionCreate({
        cols: 80,
        rows: 24,
        shell: '/bin/bash',
        env: [
          ['LANG', 'ja_JP.UTF-8'],
          ['GREETING', 'Hello!'],
        ],
        cwd: '/home/user/documents',
      })
    );
  });

  it('should handle special characters in path', () => {
    roundtripEnvelope(
      Msg.FileListRequest({
        path: '/home/user/My Documents/file (1).txt',
        include_hidden: false,
      })
    );
  });

  it('should handle binary data in checksum', () => {
    // Create checksum with all byte values
    const checksum = new Uint8Array(256);
    for (let i = 0; i < 256; i++) {
      checksum[i] = i;
    }
    roundtripEnvelope(
      Msg.FileUploadComplete({
        path: '/tmp/test',
        checksum,
      })
    );
  });

  it('should handle null optional fields', () => {
    roundtripEnvelope(
      Msg.SessionCreate({
        cols: 80,
        rows: 24,
        shell: null,
        env: [],
        cwd: null,
      })
    );
  });

  it('should handle empty arrays', () => {
    roundtripEnvelope(
      Msg.FileListResponse({
        path: '/empty',
        entries: [],
      })
    );
  });
});

describe('Binary Size', () => {
  it('should produce compact typical messages', () => {
    const envelope = createEnvelope(
      1,
      Msg.SessionData({
        session_id: 'sess-12345678',
        stream: 'Stdout',
        data: new TextEncoder().encode('Hello, World!\n'),
      })
    );
    const bytes = encodeEnvelope(envelope);
    // Typical messages should be well under 1KB
    expect(bytes.length).toBeLessThan(1024);
  });

  it('should produce compact ping messages', () => {
    const envelope = createEnvelope(
      1,
      Msg.Ping({
        timestamp: Number.MAX_SAFE_INTEGER,
        payload: new Uint8Array(),
      })
    );
    const bytes = encodeEnvelope(envelope);
    // Ping should be very compact
    expect(bytes.length).toBeLessThan(100);
  });

  it('should produce compact session create messages', () => {
    const envelope = createEnvelope(1, Msg.SessionCreate(defaultSessionCreate()));
    const bytes = encodeEnvelope(envelope);
    // Default session create should be reasonably compact
    expect(bytes.length).toBeLessThan(150);
  });
});

describe('Validation', () => {
  it('should validate valid messages', () => {
    const msg = Msg.Ping({ timestamp: 123, payload: new Uint8Array() });
    expect(isValidMessage(msg)).toBe(true);
  });

  it('should reject invalid message objects', () => {
    expect(isValidMessage(null)).toBe(false);
    expect(isValidMessage(undefined)).toBe(false);
    expect(isValidMessage(123)).toBe(false);
    expect(isValidMessage('string')).toBe(false);
    expect(isValidMessage({})).toBe(false);
    expect(isValidMessage({ type: 'Unknown' })).toBe(false);
    expect(isValidMessage({ data: {} })).toBe(false);
  });

  it('should reject unknown message types', () => {
    expect(isValidMessage({ type: 'InvalidType', data: {} })).toBe(false);
  });

  it('should validate valid envelopes', () => {
    const envelope = createEnvelope(1, Msg.Ping({ timestamp: 123, payload: new Uint8Array() }));
    expect(isValidEnvelope(envelope)).toBe(true);
  });

  it('should reject invalid envelopes', () => {
    expect(isValidEnvelope(null)).toBe(false);
    expect(isValidEnvelope({})).toBe(false);
    expect(isValidEnvelope({ version: 1 })).toBe(false);
    expect(isValidEnvelope({ version: 1, sequence: 1 })).toBe(false);
    expect(isValidEnvelope({ version: '1', sequence: 1, payload: {} })).toBe(false);
  });
});

describe('Channel Serializers', () => {
  it('should get control channel serializer', () => {
    const serializer = getChannelSerializer('control');
    expect(serializer).toBeDefined();
    expect(typeof serializer.encode).toBe('function');
    expect(typeof serializer.decode).toBe('function');
  });

  it('should get terminal channel serializer', () => {
    const serializer = getChannelSerializer('terminal');
    expect(serializer).toBeDefined();
    expect(typeof serializer.encode).toBe('function');
    expect(typeof serializer.decode).toBe('function');
  });

  it('should get files channel serializer', () => {
    const serializer = getChannelSerializer('files');
    expect(serializer).toBeDefined();
    expect(typeof serializer.encode).toBe('function');
    expect(typeof serializer.decode).toBe('function');
  });

  it('should work with control channel serializer', () => {
    const serializer = getChannelSerializer('control');
    const envelope = createEnvelope(1, Msg.Ping({ timestamp: 123, payload: new Uint8Array() }));

    const bytes = serializer.encode(envelope);
    const decoded = serializer.decode(bytes);

    expect(deepEqual(decoded, envelope)).toBe(true);
  });

  it('should work with terminal channel serializer', () => {
    const serializer = getChannelSerializer('terminal');
    const envelope = createEnvelope(
      1,
      Msg.SessionData({
        session_id: 'sess-1',
        stream: 'Stdout',
        data: new TextEncoder().encode('output'),
      })
    );

    const bytes = serializer.encode(envelope);
    const decoded = serializer.decode(bytes);

    expect(deepEqual(decoded, envelope)).toBe(true);
  });

  it('should work with files channel serializer', () => {
    const serializer = getChannelSerializer('files');
    const envelope = createEnvelope(
      1,
      Msg.FileDownloadChunk({
        path: '/test',
        offset: 0,
        total_size: 100,
        data: new Uint8Array([1, 2, 3]),
        is_last: true,
      })
    );

    const bytes = serializer.encode(envelope);
    const decoded = serializer.decode(bytes);

    expect(deepEqual(decoded, envelope)).toBe(true);
  });
});
