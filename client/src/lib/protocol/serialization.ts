/**
 * MessagePack serialization for RemoShell protocol.
 *
 * This module provides encode/decode functions that match the Rust MessagePack
 * serialization format using the @msgpack/msgpack library.
 *
 * IMPORTANT: Rust's rmp_serde uses compact array format by default, not maps.
 * This means structs are serialized as arrays with fields in declaration order,
 * and tagged enums use [type_name, data_tuple] format.
 */

import { encode as msgpackEncode, decode as msgpackDecode, ExtensionCodec } from '@msgpack/msgpack';
import type {
  Envelope,
  Message,
  SessionCreate,
  SessionCreated,
  SessionAttach,
  SessionDetach,
  SessionKill,
  SessionResize,
  SessionData,
  DataStream,
  SessionClosed,
  FileListRequest,
  FileListResponse,
  FileEntryType,
  FileDownloadRequest,
  FileDownloadChunk,
  FileUploadStart,
  FileUploadChunk,
  FileUploadComplete,
  DeviceInfo,
  DeviceApprovalRequest,
  DeviceApproved,
  DeviceRejected,
  Ping,
  Pong,
  ErrorMessage,
  ErrorCode,
  Capabilities,
} from './messages';

// ============================================================================
// Extension Codec for Binary Data
// ============================================================================

const extensionCodec = new ExtensionCodec();

// ============================================================================
// Serialization Options
// ============================================================================

const ENCODE_OPTIONS = {
  extensionCodec,
  forceIntegerToFloat: false,
  ignoreUndefined: true,
};

const DECODE_OPTIONS = {
  extensionCodec,
};

// ============================================================================
// Envelope Serialization (Rust format: [version, sequence, payload])
// ============================================================================

/**
 * Serialize an envelope to MessagePack bytes.
 * Rust format: [version, sequence, [type, data_tuple]]
 */
export function encodeEnvelope(envelope: Envelope): Uint8Array {
  const serializable = [
    envelope.version,
    envelope.sequence,
    serializeMessage(envelope.payload),
  ];
  return msgpackEncode(serializable, ENCODE_OPTIONS);
}

/**
 * Deserialize an envelope from MessagePack bytes.
 */
export function decodeEnvelope(bytes: Uint8Array): Envelope {
  const decoded = msgpackDecode(bytes, DECODE_OPTIONS) as [number, number, [string, unknown[]]];

  return {
    version: decoded[0],
    sequence: decoded[1],
    payload: deserializeMessage(decoded[2]),
  };
}

// ============================================================================
// Message Serialization
// Rust tagged enum format: [type_name, data_tuple]
// ============================================================================

/**
 * Serialize a message to Rust's tagged enum format: [type_name, data_tuple]
 */
function serializeMessage(message: Message): [string, unknown[]] {
  return [message.type, serializeMessageData(message.type, message.data)];
}

/**
 * Serialize message data to array format matching Rust struct field order.
 */
function serializeMessageData(type: string, data: unknown): unknown[] {
  switch (type) {
    // Session messages
    case 'SessionCreate': {
      const d = data as SessionCreate;
      // Rust order: cols, rows, shell, env, cwd
      return [d.cols, d.rows, d.shell, d.env, d.cwd];
    }
    case 'SessionCreated': {
      const d = data as SessionCreated;
      // Rust order: session_id, pid
      return [d.session_id, d.pid];
    }
    case 'SessionAttach': {
      const d = data as SessionAttach;
      return [d.session_id];
    }
    case 'SessionDetach': {
      const d = data as SessionDetach;
      return [d.session_id];
    }
    case 'SessionKill': {
      const d = data as SessionKill;
      return [d.session_id, d.signal];
    }
    case 'SessionResize': {
      const d = data as SessionResize;
      return [d.session_id, d.cols, d.rows];
    }
    case 'SessionData': {
      const d = data as SessionData;
      // Rust order: session_id, stream, data
      return [d.session_id, d.stream, d.data];
    }
    case 'SessionClosed': {
      const d = data as SessionClosed;
      return [d.session_id, d.exit_code, d.signal, d.reason];
    }

    // File messages
    case 'FileListRequest': {
      const d = data as FileListRequest;
      return [d.path, d.include_hidden];
    }
    case 'FileListResponse': {
      const d = data as FileListResponse;
      const entries = d.entries.map((e) => [e.name, e.entry_type, e.size, e.mode, e.modified]);
      return [d.path, entries];
    }
    case 'FileDownloadRequest': {
      const d = data as FileDownloadRequest;
      return [d.path, d.offset, d.chunk_size];
    }
    case 'FileDownloadChunk': {
      const d = data as FileDownloadChunk;
      return [d.path, d.offset, d.total_size, d.data, d.is_last];
    }
    case 'FileUploadStart': {
      const d = data as FileUploadStart;
      return [d.path, d.size, d.mode, d.overwrite];
    }
    case 'FileUploadChunk': {
      const d = data as FileUploadChunk;
      return [d.path, d.offset, d.data];
    }
    case 'FileUploadComplete': {
      const d = data as FileUploadComplete;
      return [d.path, d.checksum];
    }

    // Device messages
    case 'DeviceInfo': {
      const d = data as DeviceInfo;
      return [d.device_id, d.name, d.os, d.os_version, d.arch, d.protocol_version];
    }
    case 'DeviceApprovalRequest': {
      const d = data as DeviceApprovalRequest;
      return [d.device_id, d.name, d.public_key, d.reason];
    }
    case 'DeviceApproved': {
      const d = data as DeviceApproved;
      return [d.device_id, d.expires_at, d.allowed_capabilities];
    }
    case 'DeviceRejected': {
      const d = data as DeviceRejected;
      return [d.device_id, d.reason, d.retry_allowed];
    }

    // Control messages
    case 'Ping': {
      const d = data as Ping;
      return [d.timestamp, d.payload];
    }
    case 'Pong': {
      const d = data as Pong;
      return [d.timestamp, d.payload];
    }
    case 'Error': {
      const d = data as ErrorMessage;
      return [d.code, d.message, d.context, d.recoverable];
    }
    case 'Capabilities': {
      const d = data as Capabilities;
      return [d.protocol_versions, d.features, d.max_message_size, d.max_sessions, d.compression];
    }

    default:
      throw new Error(`Unknown message type: ${type}`);
  }
}

/**
 * Deserialize a message from Rust's tagged enum format.
 */
function deserializeMessage(decoded: [string, unknown[]]): Message {
  const [type, dataArray] = decoded;
  const data = deserializeMessageData(type, dataArray);
  return { type, data } as Message;
}

/**
 * Deserialize message data from array format.
 */
function deserializeMessageData(type: string, arr: unknown[]): unknown {
  switch (type) {
    // Session messages
    case 'SessionCreate':
      return {
        cols: arr[0] as number,
        rows: arr[1] as number,
        shell: arr[2] as string | null,
        env: arr[3] as Array<[string, string]>,
        cwd: arr[4] as string | null,
      } satisfies SessionCreate;

    case 'SessionCreated':
      return {
        session_id: arr[0] as string,
        pid: arr[1] as number,
      } satisfies SessionCreated;

    case 'SessionAttach':
      return {
        session_id: arr[0] as string,
      } satisfies SessionAttach;

    case 'SessionDetach':
      return {
        session_id: arr[0] as string,
      } satisfies SessionDetach;

    case 'SessionKill':
      return {
        session_id: arr[0] as string,
        signal: arr[1] as number | null,
      } satisfies SessionKill;

    case 'SessionResize':
      return {
        session_id: arr[0] as string,
        cols: arr[1] as number,
        rows: arr[2] as number,
      } satisfies SessionResize;

    case 'SessionData':
      return {
        session_id: arr[0] as string,
        stream: arr[1] as DataStream,
        data: ensureUint8Array(arr[2]),
      } satisfies SessionData;

    case 'SessionClosed':
      return {
        session_id: arr[0] as string,
        exit_code: arr[1] as number | null,
        signal: arr[2] as number | null,
        reason: arr[3] as string | null,
      } satisfies SessionClosed;

    // File messages
    case 'FileListRequest':
      return {
        path: arr[0] as string,
        include_hidden: arr[1] as boolean,
      } satisfies FileListRequest;

    case 'FileListResponse': {
      const entries = (arr[1] as unknown[][]).map((e) => ({
        name: e[0] as string,
        entry_type: e[1] as FileEntryType,
        size: e[2] as number,
        mode: e[3] as number,
        modified: e[4] as number,
      }));
      return {
        path: arr[0] as string,
        entries,
      } satisfies FileListResponse;
    }

    case 'FileDownloadRequest':
      return {
        path: arr[0] as string,
        offset: arr[1] as number,
        chunk_size: arr[2] as number,
      } satisfies FileDownloadRequest;

    case 'FileDownloadChunk':
      return {
        path: arr[0] as string,
        offset: arr[1] as number,
        total_size: arr[2] as number,
        data: ensureUint8Array(arr[3]),
        is_last: arr[4] as boolean,
      } satisfies FileDownloadChunk;

    case 'FileUploadStart':
      return {
        path: arr[0] as string,
        size: arr[1] as number,
        mode: arr[2] as number,
        overwrite: arr[3] as boolean,
      } satisfies FileUploadStart;

    case 'FileUploadChunk':
      return {
        path: arr[0] as string,
        offset: arr[1] as number,
        data: ensureUint8Array(arr[2]),
      } satisfies FileUploadChunk;

    case 'FileUploadComplete':
      return {
        path: arr[0] as string,
        checksum: ensureUint8Array(arr[1]),
      } satisfies FileUploadComplete;

    // Device messages
    case 'DeviceInfo':
      return {
        device_id: arr[0] as string,
        name: arr[1] as string,
        os: arr[2] as string,
        os_version: arr[3] as string,
        arch: arr[4] as string,
        protocol_version: arr[5] as number,
      } satisfies DeviceInfo;

    case 'DeviceApprovalRequest':
      return {
        device_id: arr[0] as string,
        name: arr[1] as string,
        public_key: ensureUint8Array(arr[2]),
        reason: arr[3] as string | null,
      } satisfies DeviceApprovalRequest;

    case 'DeviceApproved':
      return {
        device_id: arr[0] as string,
        expires_at: arr[1] as number | null,
        allowed_capabilities: arr[2] as string[],
      } satisfies DeviceApproved;

    case 'DeviceRejected':
      return {
        device_id: arr[0] as string,
        reason: arr[1] as string,
        retry_allowed: arr[2] as boolean,
      } satisfies DeviceRejected;

    // Control messages
    case 'Ping':
      return {
        timestamp: arr[0] as number,
        payload: ensureUint8Array(arr[1]),
      } satisfies Ping;

    case 'Pong':
      return {
        timestamp: arr[0] as number,
        payload: ensureUint8Array(arr[1]),
      } satisfies Pong;

    case 'Error':
      return {
        code: arr[0] as ErrorCode,
        message: arr[1] as string,
        context: arr[2] as string | null,
        recoverable: arr[3] as boolean,
      } satisfies ErrorMessage;

    case 'Capabilities':
      return {
        protocol_versions: arr[0] as number[],
        features: arr[1] as string[],
        max_message_size: arr[2] as number,
        max_sessions: arr[3] as number,
        compression: arr[4] as string[],
      } satisfies Capabilities;

    default:
      throw new Error(`Unknown message type: ${type}`);
  }
}

/**
 * Ensure a value is a Uint8Array.
 */
function ensureUint8Array(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) {
    return new Uint8Array(value);
  }
  return new Uint8Array(0);
}

// ============================================================================
// Standalone Message Serialization
// ============================================================================

/**
 * Serialize a message to MessagePack bytes (without envelope).
 */
export function encodeMessage(message: Message): Uint8Array {
  return msgpackEncode(serializeMessage(message), ENCODE_OPTIONS);
}

/**
 * Deserialize a message from MessagePack bytes (without envelope).
 */
export function decodeMessage(bytes: Uint8Array): Message {
  const decoded = msgpackDecode(bytes, DECODE_OPTIONS) as [string, unknown[]];
  return deserializeMessage(decoded);
}

// ============================================================================
// Channel-Specific Serializers
// ============================================================================

/**
 * Channel types matching the WebRTC data channels.
 */
export type ChannelType = 'control' | 'terminal' | 'files';

/**
 * Serializer for the control channel.
 */
export const controlChannelSerializer = {
  encode: encodeEnvelope,
  decode: decodeEnvelope,
};

/**
 * Serializer for the terminal channel.
 */
export const terminalChannelSerializer = {
  encode: encodeEnvelope,
  decode: decodeEnvelope,
};

/**
 * Serializer for the files channel.
 */
export const filesChannelSerializer = {
  encode: encodeEnvelope,
  decode: decodeEnvelope,
};

/**
 * Get the appropriate serializer for a channel type.
 */
export function getChannelSerializer(channel: ChannelType) {
  switch (channel) {
    case 'control':
      return controlChannelSerializer;
    case 'terminal':
      return terminalChannelSerializer;
    case 'files':
      return filesChannelSerializer;
  }
}

// ============================================================================
// Validation Helpers
// ============================================================================

const VALID_MESSAGE_TYPES = [
  'SessionCreate',
  'SessionCreated',
  'SessionAttach',
  'SessionDetach',
  'SessionKill',
  'SessionResize',
  'SessionData',
  'SessionClosed',
  'FileListRequest',
  'FileListResponse',
  'FileDownloadRequest',
  'FileDownloadChunk',
  'FileUploadStart',
  'FileUploadChunk',
  'FileUploadComplete',
  'DeviceInfo',
  'DeviceApprovalRequest',
  'DeviceApproved',
  'DeviceRejected',
  'Ping',
  'Pong',
  'Error',
  'Capabilities',
];

/**
 * Check if a decoded value is a valid Message.
 */
export function isValidMessage(value: unknown): value is Message {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const obj = value as Record<string, unknown>;
  if (typeof obj.type !== 'string' || !('data' in obj)) {
    return false;
  }
  return VALID_MESSAGE_TYPES.includes(obj.type as string);
}

/**
 * Check if a decoded value is a valid Envelope.
 */
export function isValidEnvelope(value: unknown): value is Envelope {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.version === 'number' &&
    typeof obj.sequence === 'number' &&
    isValidMessage(obj.payload)
  );
}
