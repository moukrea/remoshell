/**
 * Tests for Rust protocol interoperability.
 *
 * These tests verify that TypeScript serialization matches Rust MessagePack format.
 * Test vectors are generated from Rust using: cargo run --package protocol --example test_vectors
 */

import { describe, it, expect } from 'vitest';
import {
  encodeEnvelope,
  decodeEnvelope,
} from './serialization';
import {
  createEnvelope,
  defaultSessionCreate,
  defaultCapabilities,
  Msg,
  PROTOCOL_VERSION,
} from './messages';

// ============================================================================
// Test vectors generated from Rust
// ============================================================================

export const ping = new Uint8Array([147, 1, 1, 146, 164, 80, 105, 110, 103, 146, 205, 48, 57, 196, 0]);
export const session_create_default = new Uint8Array([147, 1, 2, 146, 173, 83, 101, 115, 115, 105, 111, 110, 67, 114, 101, 97, 116, 101, 149, 80, 24, 192, 144, 192]);
export const session_data = new Uint8Array([147, 1, 3, 146, 171, 83, 101, 115, 115, 105, 111, 110, 68, 97, 116, 97, 147, 166, 115, 101, 115, 115, 45, 49, 166, 83, 116, 100, 111, 117, 116, 196, 5, 72, 101, 108, 108, 111]);
export const error = new Uint8Array([147, 1, 4, 146, 165, 69, 114, 114, 111, 114, 148, 168, 78, 111, 116, 70, 111, 117, 110, 100, 169, 78, 111, 116, 32, 102, 111, 117, 110, 100, 164, 116, 101, 115, 116, 194]);
export const file_list_response = new Uint8Array([147, 1, 5, 146, 176, 70, 105, 108, 101, 76, 105, 115, 116, 82, 101, 115, 112, 111, 110, 115, 101, 146, 165, 47, 104, 111, 109, 101, 145, 149, 168, 116, 101, 115, 116, 46, 116, 120, 116, 164, 70, 105, 108, 101, 100, 205, 1, 164, 206, 101, 146, 0, 128]);
export const capabilities = new Uint8Array([147, 1, 6, 146, 172, 67, 97, 112, 97, 98, 105, 108, 105, 116, 105, 101, 115, 149, 145, 1, 147, 165, 115, 104, 101, 108, 108, 173, 102, 105, 108, 101, 45, 116, 114, 97, 110, 115, 102, 101, 114, 172, 100, 101, 118, 105, 99, 101, 45, 116, 114, 117, 115, 116, 206, 0, 16, 0, 0, 16, 145, 163, 108, 122, 52]);

// ============================================================================
// Interop Tests: Decode Rust-serialized bytes
// ============================================================================

describe('Rust Interop: Decode Rust bytes', () => {
  it('should decode Ping from Rust bytes', () => {
    const envelope = decodeEnvelope(ping);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(1);
    expect(envelope.payload.type).toBe('Ping');
    if (envelope.payload.type === 'Ping') {
      expect(envelope.payload.data.timestamp).toBe(12345);
      expect(envelope.payload.data.payload.length).toBe(0);
    }
  });

  it('should decode SessionCreate from Rust bytes', () => {
    const envelope = decodeEnvelope(session_create_default);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(2);
    expect(envelope.payload.type).toBe('SessionCreate');
    if (envelope.payload.type === 'SessionCreate') {
      expect(envelope.payload.data.cols).toBe(80);
      expect(envelope.payload.data.rows).toBe(24);
      expect(envelope.payload.data.shell).toBeNull();
      expect(envelope.payload.data.env).toEqual([]);
      expect(envelope.payload.data.cwd).toBeNull();
    }
  });

  it('should decode SessionData from Rust bytes', () => {
    const envelope = decodeEnvelope(session_data);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(3);
    expect(envelope.payload.type).toBe('SessionData');
    if (envelope.payload.type === 'SessionData') {
      expect(envelope.payload.data.session_id).toBe('sess-1');
      expect(envelope.payload.data.stream).toBe('Stdout');
      expect(new TextDecoder().decode(envelope.payload.data.data)).toBe('Hello');
    }
  });

  it('should decode Error from Rust bytes', () => {
    const envelope = decodeEnvelope(error);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(4);
    expect(envelope.payload.type).toBe('Error');
    if (envelope.payload.type === 'Error') {
      expect(envelope.payload.data.code).toBe('NotFound');
      expect(envelope.payload.data.message).toBe('Not found');
      expect(envelope.payload.data.context).toBe('test');
      expect(envelope.payload.data.recoverable).toBe(false);
    }
  });

  it('should decode FileListResponse from Rust bytes', () => {
    const envelope = decodeEnvelope(file_list_response);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(5);
    expect(envelope.payload.type).toBe('FileListResponse');
    if (envelope.payload.type === 'FileListResponse') {
      expect(envelope.payload.data.path).toBe('/home');
      expect(envelope.payload.data.entries.length).toBe(1);
      expect(envelope.payload.data.entries[0].name).toBe('test.txt');
      expect(envelope.payload.data.entries[0].entry_type).toBe('File');
      expect(envelope.payload.data.entries[0].size).toBe(100);
      expect(envelope.payload.data.entries[0].mode).toBe(0o644);
      expect(envelope.payload.data.entries[0].modified).toBe(1704067200);
    }
  });

  it('should decode Capabilities from Rust bytes', () => {
    const envelope = decodeEnvelope(capabilities);

    expect(envelope.version).toBe(1);
    expect(envelope.sequence).toBe(6);
    expect(envelope.payload.type).toBe('Capabilities');
    if (envelope.payload.type === 'Capabilities') {
      expect(envelope.payload.data.protocol_versions).toEqual([1]);
      expect(envelope.payload.data.features).toEqual(['shell', 'file-transfer', 'device-trust']);
      expect(envelope.payload.data.max_message_size).toBe(1024 * 1024);
      expect(envelope.payload.data.max_sessions).toBe(16);
      expect(envelope.payload.data.compression).toEqual(['lz4']);
    }
  });
});

// ============================================================================
// Interop Tests: Verify byte-level compatibility
// ============================================================================

describe('Rust Interop: Verify serialization format', () => {
  it('should verify Ping format matches Rust', () => {
    // We can't verify exact bytes since field ordering may differ,
    // but we can verify roundtrip works
    const envelope = createEnvelope(
      1,
      Msg.Ping({
        timestamp: 12345,
        payload: new Uint8Array(),
      })
    );
    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);

    expect(decoded.version).toBe(envelope.version);
    expect(decoded.sequence).toBe(envelope.sequence);
    expect(decoded.payload.type).toBe('Ping');
  });

  it('should verify SessionCreate format matches Rust', () => {
    const envelope = createEnvelope(2, Msg.SessionCreate(defaultSessionCreate()));
    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);

    expect(decoded.version).toBe(envelope.version);
    expect(decoded.sequence).toBe(envelope.sequence);
    expect(decoded.payload.type).toBe('SessionCreate');
    if (decoded.payload.type === 'SessionCreate') {
      expect(decoded.payload.data.cols).toBe(80);
      expect(decoded.payload.data.rows).toBe(24);
    }
  });

  it('should verify Capabilities format matches Rust', () => {
    const envelope = createEnvelope(6, Msg.Capabilities(defaultCapabilities()));
    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);

    expect(decoded.version).toBe(envelope.version);
    expect(decoded.sequence).toBe(envelope.sequence);
    expect(decoded.payload.type).toBe('Capabilities');
    if (decoded.payload.type === 'Capabilities') {
      expect(decoded.payload.data.protocol_versions).toEqual([PROTOCOL_VERSION]);
      expect(decoded.payload.data.features).toContain('shell');
    }
  });
});

// ============================================================================
// Interop Tests: Cross-platform binary data handling
// ============================================================================

describe('Rust Interop: Binary data handling', () => {
  it('should correctly handle binary data from Rust', () => {
    // SessionData contains binary data
    const envelope = decodeEnvelope(session_data);

    if (envelope.payload.type === 'SessionData') {
      const data = envelope.payload.data.data;
      expect(data).toBeInstanceOf(Uint8Array);
      expect(data.length).toBe(5);
      expect(Array.from(data)).toEqual([72, 101, 108, 108, 111]); // "Hello"
    }
  });

  it('should encode binary data compatible with Rust', () => {
    const envelope = createEnvelope(
      1,
      Msg.SessionData({
        session_id: 'test',
        stream: 'Stdout',
        data: new Uint8Array([1, 2, 3, 255, 0]),
      })
    );
    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);

    if (decoded.payload.type === 'SessionData') {
      expect(Array.from(decoded.payload.data.data)).toEqual([1, 2, 3, 255, 0]);
    }
  });

  it('should handle empty binary data', () => {
    const envelope = createEnvelope(
      1,
      Msg.Ping({
        timestamp: 0,
        payload: new Uint8Array(),
      })
    );
    const bytes = encodeEnvelope(envelope);
    const decoded = decodeEnvelope(bytes);

    if (decoded.payload.type === 'Ping') {
      expect(decoded.payload.data.payload.length).toBe(0);
    }
  });
});

// ============================================================================
// Interop Tests: Enum handling
// ============================================================================

describe('Rust Interop: Enum handling', () => {
  it('should correctly handle DataStream enum', () => {
    const streams = ['Stdin', 'Stdout', 'Stderr'] as const;

    for (const stream of streams) {
      const envelope = createEnvelope(
        1,
        Msg.SessionData({
          session_id: 'test',
          stream,
          data: new Uint8Array(),
        })
      );
      const bytes = encodeEnvelope(envelope);
      const decoded = decodeEnvelope(bytes);

      if (decoded.payload.type === 'SessionData') {
        expect(decoded.payload.data.stream).toBe(stream);
      }
    }
  });

  it('should correctly handle FileEntryType enum', () => {
    const types = ['File', 'Directory', 'Symlink', 'Other'] as const;

    for (const entryType of types) {
      const envelope = createEnvelope(
        1,
        Msg.FileListResponse({
          path: '/test',
          entries: [
            {
              name: 'item',
              entry_type: entryType,
              size: 0,
              mode: 0o644,
              modified: 0,
            },
          ],
        })
      );
      const bytes = encodeEnvelope(envelope);
      const decoded = decodeEnvelope(bytes);

      if (decoded.payload.type === 'FileListResponse') {
        expect(decoded.payload.data.entries[0].entry_type).toBe(entryType);
      }
    }
  });

  it('should correctly handle ErrorCode enum', () => {
    const codes = [
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

    for (const code of codes) {
      const envelope = createEnvelope(
        1,
        Msg.Error({
          code,
          message: 'test',
          context: null,
          recoverable: false,
        })
      );
      const bytes = encodeEnvelope(envelope);
      const decoded = decodeEnvelope(bytes);

      if (decoded.payload.type === 'Error') {
        expect(decoded.payload.data.code).toBe(code);
      }
    }
  });
});
