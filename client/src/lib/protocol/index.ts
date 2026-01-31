/**
 * Protocol module for RemoShell client.
 *
 * This module exports message types and serialization utilities for
 * communication with the RemoShell daemon using MessagePack.
 */

// Re-export all message types
export {
  PROTOCOL_VERSION,
  createEnvelope,
  defaultSessionCreate,
  defaultCapabilities,
  Msg,
  type Envelope,
  type Message,
  type MessageType,
  // Session messages
  type SessionCreate,
  type SessionCreated,
  type SessionAttach,
  type SessionDetach,
  type SessionKill,
  type SessionResize,
  type SessionData,
  type DataStream,
  type SessionClosed,
  // File messages
  type FileListRequest,
  type FileListResponse,
  type FileEntry,
  type FileEntryType,
  type FileDownloadRequest,
  type FileDownloadChunk,
  type FileUploadStart,
  type FileUploadChunk,
  type FileUploadComplete,
  // Device messages
  type DeviceInfo,
  type DeviceApprovalRequest,
  type DeviceApproved,
  type DeviceRejected,
  // Control messages
  type Ping,
  type Pong,
  type ErrorMessage,
  type ErrorCode,
  type Capabilities,
} from './messages';

// Re-export serialization utilities
export {
  encodeEnvelope,
  decodeEnvelope,
  encodeMessage,
  decodeMessage,
  controlChannelSerializer,
  terminalChannelSerializer,
  filesChannelSerializer,
  getChannelSerializer,
  isValidMessage,
  isValidEnvelope,
  type ChannelType,
} from './serialization';
