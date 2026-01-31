/**
 * Protocol message definitions for RemoShell.
 *
 * This module defines all RPC message types used for communication between
 * the daemon and clients. All messages are serialized using MessagePack.
 *
 * These types match the Rust protocol definitions in crates/protocol/src/messages.rs
 */

/** Current protocol version. */
export const PROTOCOL_VERSION = 1;

// ============================================================================
// Envelope
// ============================================================================

/**
 * Envelope wrapper for all protocol messages.
 *
 * The envelope provides versioning and sequence numbers for message ordering
 * and compatibility checking.
 */
export interface Envelope {
  /** Protocol version for compatibility checking. */
  version: number;
  /** Sequence number for message ordering and acknowledgment. */
  sequence: number;
  /** The actual message payload. */
  payload: Message;
}

/**
 * Create a new envelope with the current protocol version.
 */
export function createEnvelope(sequence: number, payload: Message): Envelope {
  return {
    version: PROTOCOL_VERSION,
    sequence,
    payload,
  };
}

// ============================================================================
// Message Union Type
// ============================================================================

/**
 * Top-level message type - discriminated union of all message types.
 * Uses tagged format with { type: "...", data: {...} } matching Rust serde.
 */
export type Message =
  // Session messages
  | { type: 'SessionCreate'; data: SessionCreate }
  | { type: 'SessionCreated'; data: SessionCreated }
  | { type: 'SessionAttach'; data: SessionAttach }
  | { type: 'SessionDetach'; data: SessionDetach }
  | { type: 'SessionKill'; data: SessionKill }
  | { type: 'SessionResize'; data: SessionResize }
  | { type: 'SessionData'; data: SessionData }
  | { type: 'SessionClosed'; data: SessionClosed }
  // File messages
  | { type: 'FileListRequest'; data: FileListRequest }
  | { type: 'FileListResponse'; data: FileListResponse }
  | { type: 'FileDownloadRequest'; data: FileDownloadRequest }
  | { type: 'FileDownloadChunk'; data: FileDownloadChunk }
  | { type: 'FileUploadStart'; data: FileUploadStart }
  | { type: 'FileUploadChunk'; data: FileUploadChunk }
  | { type: 'FileUploadComplete'; data: FileUploadComplete }
  // Device messages
  | { type: 'DeviceInfo'; data: DeviceInfo }
  | { type: 'DeviceApprovalRequest'; data: DeviceApprovalRequest }
  | { type: 'DeviceApproved'; data: DeviceApproved }
  | { type: 'DeviceRejected'; data: DeviceRejected }
  // Control messages
  | { type: 'Ping'; data: Ping }
  | { type: 'Pong'; data: Pong }
  | { type: 'Error'; data: ErrorMessage }
  | { type: 'Capabilities'; data: Capabilities };

// ============================================================================
// Message Type Helpers
// ============================================================================

/** All possible message type strings */
export type MessageType = Message['type'];

/** Helper to create typed messages */
export const Msg = {
  SessionCreate: (data: SessionCreate): Message => ({ type: 'SessionCreate', data }),
  SessionCreated: (data: SessionCreated): Message => ({ type: 'SessionCreated', data }),
  SessionAttach: (data: SessionAttach): Message => ({ type: 'SessionAttach', data }),
  SessionDetach: (data: SessionDetach): Message => ({ type: 'SessionDetach', data }),
  SessionKill: (data: SessionKill): Message => ({ type: 'SessionKill', data }),
  SessionResize: (data: SessionResize): Message => ({ type: 'SessionResize', data }),
  SessionData: (data: SessionData): Message => ({ type: 'SessionData', data }),
  SessionClosed: (data: SessionClosed): Message => ({ type: 'SessionClosed', data }),
  FileListRequest: (data: FileListRequest): Message => ({ type: 'FileListRequest', data }),
  FileListResponse: (data: FileListResponse): Message => ({ type: 'FileListResponse', data }),
  FileDownloadRequest: (data: FileDownloadRequest): Message => ({ type: 'FileDownloadRequest', data }),
  FileDownloadChunk: (data: FileDownloadChunk): Message => ({ type: 'FileDownloadChunk', data }),
  FileUploadStart: (data: FileUploadStart): Message => ({ type: 'FileUploadStart', data }),
  FileUploadChunk: (data: FileUploadChunk): Message => ({ type: 'FileUploadChunk', data }),
  FileUploadComplete: (data: FileUploadComplete): Message => ({ type: 'FileUploadComplete', data }),
  DeviceInfo: (data: DeviceInfo): Message => ({ type: 'DeviceInfo', data }),
  DeviceApprovalRequest: (data: DeviceApprovalRequest): Message => ({ type: 'DeviceApprovalRequest', data }),
  DeviceApproved: (data: DeviceApproved): Message => ({ type: 'DeviceApproved', data }),
  DeviceRejected: (data: DeviceRejected): Message => ({ type: 'DeviceRejected', data }),
  Ping: (data: Ping): Message => ({ type: 'Ping', data }),
  Pong: (data: Pong): Message => ({ type: 'Pong', data }),
  Error: (data: ErrorMessage): Message => ({ type: 'Error', data }),
  Capabilities: (data: Capabilities): Message => ({ type: 'Capabilities', data }),
} as const;

// ============================================================================
// Session Messages
// ============================================================================

/** Request to create a new shell session. */
export interface SessionCreate {
  /** Requested terminal columns. */
  cols: number;
  /** Requested terminal rows. */
  rows: number;
  /** Optional shell command to run (default: user's shell). */
  shell: string | null;
  /** Environment variables to set. */
  env: Array<[string, string]>;
  /** Working directory for the session. */
  cwd: string | null;
}

/** Default SessionCreate values */
export function defaultSessionCreate(): SessionCreate {
  return {
    cols: 80,
    rows: 24,
    shell: null,
    env: [],
    cwd: null,
  };
}

/** Response confirming session creation. */
export interface SessionCreated {
  /** Unique session identifier. */
  session_id: string;
  /** Process ID of the shell. */
  pid: number;
}

/** Request to attach to an existing session. */
export interface SessionAttach {
  /** Session ID to attach to. */
  session_id: string;
}

/** Request to detach from a session. */
export interface SessionDetach {
  /** Session ID to detach from. */
  session_id: string;
}

/** Request to kill a session. */
export interface SessionKill {
  /** Session ID to kill. */
  session_id: string;
  /** Optional signal to send (default: SIGTERM). */
  signal: number | null;
}

/** Terminal resize notification. */
export interface SessionResize {
  /** Session ID to resize. */
  session_id: string;
  /** New terminal columns. */
  cols: number;
  /** New terminal rows. */
  rows: number;
}

/** Session data (input or output). */
export interface SessionData {
  /** Session ID this data belongs to. */
  session_id: string;
  /** The data stream type. */
  stream: DataStream;
  /** The actual data bytes. */
  data: Uint8Array;
}

/** Data stream type. */
export type DataStream = 'Stdin' | 'Stdout' | 'Stderr';

/** Session closed notification. */
export interface SessionClosed {
  /** Session ID that was closed. */
  session_id: string;
  /** Exit code if the process exited normally. */
  exit_code: number | null;
  /** Signal number if the process was killed by a signal. */
  signal: number | null;
  /** Human-readable reason for closure. */
  reason: string | null;
}

// ============================================================================
// File Messages
// ============================================================================

/** Request to list files in a directory. */
export interface FileListRequest {
  /** Path to list. */
  path: string;
  /** Include hidden files. */
  include_hidden: boolean;
}

/** Response with directory listing. */
export interface FileListResponse {
  /** Path that was listed. */
  path: string;
  /** List of entries in the directory. */
  entries: FileEntry[];
}

/** A single file or directory entry. */
export interface FileEntry {
  /** Entry name (not full path). */
  name: string;
  /** Entry type. */
  entry_type: FileEntryType;
  /** Size in bytes (0 for directories). */
  size: number;
  /** Unix permissions mode. */
  mode: number;
  /** Last modified timestamp (Unix epoch seconds). */
  modified: number;
}

/** Type of file entry. */
export type FileEntryType = 'File' | 'Directory' | 'Symlink' | 'Other';

/** Request to download a file. */
export interface FileDownloadRequest {
  /** Path to download. */
  path: string;
  /** Starting offset (for resuming). */
  offset: number;
  /** Maximum chunk size. */
  chunk_size: number;
}

/** Chunk of downloaded file data. */
export interface FileDownloadChunk {
  /** Path being downloaded. */
  path: string;
  /** Offset of this chunk. */
  offset: number;
  /** Total file size. */
  total_size: number;
  /** The chunk data. */
  data: Uint8Array;
  /** Whether this is the last chunk. */
  is_last: boolean;
}

/** Start a file upload. */
export interface FileUploadStart {
  /** Destination path. */
  path: string;
  /** Total file size. */
  size: number;
  /** Unix permissions mode. */
  mode: number;
  /** Whether to overwrite if exists. */
  overwrite: boolean;
}

/** Chunk of uploaded file data. */
export interface FileUploadChunk {
  /** Destination path. */
  path: string;
  /** Offset of this chunk. */
  offset: number;
  /** The chunk data. */
  data: Uint8Array;
}

/** Complete a file upload. */
export interface FileUploadComplete {
  /** Destination path. */
  path: string;
  /** SHA-256 hash of the complete file for verification. */
  checksum: Uint8Array;
}

// ============================================================================
// Device Messages
// ============================================================================

/** Device information announcement. */
export interface DeviceInfo {
  /** Unique device identifier (public key fingerprint). */
  device_id: string;
  /** Human-readable device name. */
  name: string;
  /** Operating system. */
  os: string;
  /** OS version. */
  os_version: string;
  /** Device architecture. */
  arch: string;
  /** Protocol version supported. */
  protocol_version: number;
}

/** Request approval to connect. */
export interface DeviceApprovalRequest {
  /** Device ID requesting approval. */
  device_id: string;
  /** Device name. */
  name: string;
  /** Public key for verification. */
  public_key: Uint8Array;
  /** Human-readable reason for connection. */
  reason: string | null;
}

/** Device connection approved. */
export interface DeviceApproved {
  /** Device ID that was approved. */
  device_id: string;
  /** Optional trust expiration (Unix timestamp). */
  expires_at: number | null;
  /** Allowed capabilities for this device. */
  allowed_capabilities: string[];
}

/** Device connection rejected. */
export interface DeviceRejected {
  /** Device ID that was rejected. */
  device_id: string;
  /** Reason for rejection. */
  reason: string;
  /** Whether the device should retry later. */
  retry_allowed: boolean;
}

// ============================================================================
// Control Messages
// ============================================================================

/** Ping for keepalive and latency measurement. */
export interface Ping {
  /** Timestamp when ping was sent (for latency calculation). */
  timestamp: number;
  /** Optional payload for echo. */
  payload: Uint8Array;
}

/** Pong response to ping. */
export interface Pong {
  /** Original timestamp from ping. */
  timestamp: number;
  /** Echo of the original payload. */
  payload: Uint8Array;
}

/** Error message. */
export interface ErrorMessage {
  /** Error code for programmatic handling. */
  code: ErrorCode;
  /** Human-readable error message. */
  message: string;
  /** Optional context (e.g., session_id, path). */
  context: string | null;
  /** Whether the error is recoverable. */
  recoverable: boolean;
}

/** Error codes for common error conditions. */
export type ErrorCode =
  | 'Unknown'
  | 'Unauthorized'
  | 'NotFound'
  | 'InvalidRequest'
  | 'InternalError'
  | 'Timeout'
  | 'RateLimited'
  | 'AlreadyExists'
  | 'PermissionDenied'
  | 'VersionMismatch';

/** Capabilities announcement. */
export interface Capabilities {
  /** Supported protocol versions. */
  protocol_versions: number[];
  /** Supported features. */
  features: string[];
  /** Maximum message size supported. */
  max_message_size: number;
  /** Maximum concurrent sessions supported. */
  max_sessions: number;
  /** Supported compression algorithms. */
  compression: string[];
}

/** Default Capabilities values */
export function defaultCapabilities(): Capabilities {
  return {
    protocol_versions: [PROTOCOL_VERSION],
    features: ['shell', 'file-transfer', 'device-trust'],
    max_message_size: 1024 * 1024, // 1MB
    max_sessions: 16,
    compression: ['lz4'],
  };
}
