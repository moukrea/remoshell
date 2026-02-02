/**
 * Connection Orchestrator - Wires together SignalingClient, WebRTCManager, and ConnectionStore
 * This is the critical piece that coordinates peer discovery and connection establishment.
 */

import { getSignalingClient, SignalingClient, type AnySignalingEvent } from '../signaling/SignalingClient';
import { getWebRTCManager, WebRTCManager, type WebRTCEvent, type SignalData } from '../webrtc/WebRTCManager';
import { getConnectionStore, type ConnectionStore } from '../../stores/connection';
import { getSessionStore, type SessionStore, type SessionEvent } from '../../stores/sessions';
import { getFileStore, type FileStore, type FileEvent, type FileEntry as StoreFileEntry } from '../../stores/files';
import { getConfig } from '../../config';
import {
  encodeEnvelope,
  decodeEnvelope,
  createEnvelope,
  Msg,
  type SessionData,
  type FileEntry as ProtocolFileEntry,
} from '../protocol';

/**
 * Data received callback type
 */
export type DataReceivedHandler = (peerId: string, data: Uint8Array, channel: string) => void;

/**
 * Upload progress callback type
 */
export type UploadProgressCallback = (progress: { bytesSent: number; totalBytes: number }) => void;

/**
 * Chunk size for file uploads (64KB)
 */
const UPLOAD_CHUNK_SIZE = 64 * 1024;

/**
 * Connection orchestrator state
 */
export type OrchestratorState = 'idle' | 'initialized' | 'connected' | 'disconnected';

/**
 * ConnectionOrchestrator coordinates all connection-related components
 * to establish and manage peer-to-peer connections.
 */
export class ConnectionOrchestrator {
  private signaling: SignalingClient | null = null;
  private webrtc: WebRTCManager | null = null;
  private store: ConnectionStore | null = null;
  private sessionStore: SessionStore | null = null;
  private fileStore: FileStore | null = null;
  private initialized = false;
  private initializing = false;
  private state: OrchestratorState = 'idle';
  private sessionPeerMap: Map<string, string> = new Map(); // sessionId -> peerId
  private dataHandlers: Set<DataReceivedHandler> = new Set();
  private signalingUnsubscribe: (() => void) | null = null;
  private webrtcUnsubscribe: (() => void) | null = null;
  private sessionUnsubscribe: (() => void) | null = null;
  private fileUnsubscribe: (() => void) | null = null;
  private messageSequence = 0;

  /**
   * Initialize the orchestrator by wiring all components together
   */
  async initialize(): Promise<void> {
    if (this.initialized || this.initializing) {
      return;
    }
    this.initializing = true;

    try {
      const config = getConfig();

      // Initialize components
      this.signaling = getSignalingClient({ serverUrl: config.signalingUrl });
      this.webrtc = getWebRTCManager();
      this.store = getConnectionStore();
      this.sessionStore = getSessionStore();

      // Update ICE servers from config
      this.webrtc.setIceServers(config.iceServers);

      // Wire up signaling events
      this.signalingUnsubscribe = this.signaling.subscribe(this.handleSignalingEvent);

      // Wire up WebRTC events
      this.webrtcUnsubscribe = this.webrtc.subscribe(this.handleWebRTCEvent);

      // Wire up session store events
      this.sessionUnsubscribe = this.sessionStore.subscribe(this.handleSessionEvent);

      // Wire up file store events
      this.fileStore = getFileStore();
      this.fileUnsubscribe = this.fileStore.subscribe(this.handleFileEvent);

      this.initialized = true;
      this.state = 'initialized';
    } finally {
      this.initializing = false;
    }
  }

  /**
   * Connect to a signaling room and begin peer discovery
   */
  async connect(roomId: string): Promise<void> {
    if (!this.initialized || !this.signaling || !this.store) {
      throw new Error('Orchestrator not initialized. Call initialize() first.');
    }

    const config = getConfig();
    this.store.connectSignaling(config.signalingUrl);
    this.signaling.join(roomId);
  }

  /**
   * Disconnect from signaling and all peers
   */
  async disconnect(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    // Leave signaling room
    this.signaling?.leave();

    // Disconnect all WebRTC peers
    this.webrtc?.destroyAll();

    // Update store state
    this.store?.disconnectSignaling();

    // Clear session-peer mapping
    this.sessionPeerMap.clear();

    this.state = 'disconnected';
  }

  /**
   * Check if orchestrator is initialized
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Get current orchestrator state
   */
  getState(): OrchestratorState {
    return this.state;
  }

  /**
   * Get peer ID for a given session ID
   */
  getSessionPeer(sessionId: string): string | undefined {
    return this.sessionPeerMap.get(sessionId);
  }

  /**
   * Map a session ID to a peer ID
   */
  setSessionPeer(sessionId: string, peerId: string): void {
    this.sessionPeerMap.set(sessionId, peerId);
  }

  /**
   * Remove session-peer mapping
   */
  removeSessionPeer(sessionId: string): void {
    this.sessionPeerMap.delete(sessionId);
  }

  /**
   * Get peer ID for a session, or undefined
   */
  getPeerForSession(sessionId: string): string | undefined {
    return this.sessionPeerMap.get(sessionId);
  }

  /**
   * Register a handler for incoming data channel messages
   */
  onData(handler: DataReceivedHandler): () => void {
    this.dataHandlers.add(handler);
    return () => {
      this.dataHandlers.delete(handler);
    };
  }

  /**
   * Send data to a peer on a specific channel
   */
  sendData(peerId: string, data: Uint8Array | string, channel: 'control' | 'terminal' | 'files' = 'control'): boolean {
    if (!this.webrtc) {
      return false;
    }
    return this.webrtc.sendData(peerId, data, channel);
  }

  /**
   * Handle signaling events and route to appropriate handlers
   */
  private handleSignalingEvent = (event: AnySignalingEvent): void => {
    switch (event.type) {
      case 'connected':
        this.handleSignalingConnected(event.peerId, event.existingPeers);
        break;
      case 'disconnected':
        this.handleSignalingDisconnected(event.reason);
        break;
      case 'peer_joined':
        this.handlePeerJoined(event.peerId);
        break;
      case 'peer_left':
        this.handlePeerLeft(event.peerId);
        break;
      case 'offer':
        this.handleOffer(event.peerId, event.offer);
        break;
      case 'answer':
        this.handleAnswer(event.peerId, event.answer);
        break;
      case 'ice':
        this.handleIceCandidate(event.peerId, event.candidate);
        break;
      case 'error':
        this.handleSignalingError(event.message);
        break;
    }
  };

  /**
   * Handle WebRTC events and route to appropriate handlers
   */
  private handleWebRTCEvent = (event: WebRTCEvent): void => {
    switch (event.type) {
      case 'signal':
        this.handleLocalSignal(event.peerId, event.data as SignalData);
        break;
      case 'connect':
        this.handlePeerConnected(event.peerId);
        break;
      case 'close':
        this.handlePeerDisconnected(event.peerId);
        break;
      case 'data':
        this.handlePeerData(event.peerId, event.data as Uint8Array, event.channel ?? 'control');
        break;
      case 'error':
        this.handlePeerError(event.peerId, event.error);
        break;
    }
  };

  /**
   * Handle successful connection to signaling server
   */
  private handleSignalingConnected = (_localPeerId: string, existingPeers: string[]): void => {
    this.store?.signalingConnected();
    this.state = 'connected';

    // Create connections to existing peers in the room
    // We are the initiator since we joined after them
    for (const peerId of existingPeers) {
      this.store?.connectToPeer(peerId);
      this.webrtc?.createConnection({
        peerId,
        initiator: true,
      });
    }
  };

  /**
   * Handle disconnection from signaling server
   */
  private handleSignalingDisconnected = (reason?: string): void => {
    this.store?.disconnectSignaling(reason);
    this.state = 'disconnected';
  };

  /**
   * Handle a new peer joining the room
   */
  private handlePeerJoined = (peerId: string): void => {
    // Add peer to store and create connection as initiator
    this.store?.connectToPeer(peerId);
    this.webrtc?.createConnection({
      peerId,
      initiator: true,
    });
  };

  /**
   * Handle a peer leaving the room
   */
  private handlePeerLeft = (peerId: string): void => {
    // Clean up WebRTC connection
    this.webrtc?.destroyConnection(peerId);
    // Update store
    this.store?.disconnectPeer(peerId);
    this.store?.removePeer(peerId);

    // Clean up session mapping
    for (const [sessionId, mappedPeerId] of this.sessionPeerMap.entries()) {
      if (mappedPeerId === peerId) {
        this.sessionPeerMap.delete(sessionId);
      }
    }
  };

  /**
   * Handle receiving an offer from a remote peer
   */
  private handleOffer = (peerId: string, offer: RTCSessionDescriptionInit): void => {
    // If we don't have a connection to this peer, create one as responder
    if (!this.webrtc?.hasPeer(peerId)) {
      this.store?.connectToPeer(peerId);
      this.webrtc?.createConnection({
        peerId,
        initiator: false,
      });
    }

    // Signal the offer to the peer connection
    this.webrtc?.signal(peerId, offer as SignalData);
  };

  /**
   * Handle receiving an answer from a remote peer
   */
  private handleAnswer = (peerId: string, answer: RTCSessionDescriptionInit): void => {
    this.webrtc?.signal(peerId, answer as SignalData);
  };

  /**
   * Handle receiving an ICE candidate from a remote peer
   */
  private handleIceCandidate = (peerId: string, candidate: RTCIceCandidateInit): void => {
    this.webrtc?.signal(peerId, { candidate } as SignalData);
  };

  /**
   * Handle signaling error
   */
  private handleSignalingError = (message: string): void => {
    console.error('Signaling error:', message);
    // Store is already updated by disconnectSignaling with error
  };

  /**
   * Handle local signal data that needs to be sent to remote peer
   */
  private handleLocalSignal = (_peerId: string, signalData: SignalData): void => {
    if (!this.signaling) {
      return;
    }

    // Determine signal type and send via signaling server
    if ('type' in signalData) {
      if (signalData.type === 'offer') {
        this.signaling.sendOffer(signalData as RTCSessionDescriptionInit);
      } else if (signalData.type === 'answer') {
        this.signaling.sendAnswer(signalData as RTCSessionDescriptionInit);
      }
    } else if ('candidate' in signalData) {
      this.signaling.sendIceCandidate(signalData as RTCIceCandidateInit);
    }
  };

  /**
   * Handle peer WebRTC connection established
   */
  private handlePeerConnected = (peerId: string): void => {
    this.store?.peerConnected(peerId);
  };

  /**
   * Handle peer WebRTC connection closed
   */
  private handlePeerDisconnected = (peerId: string): void => {
    this.store?.disconnectPeer(peerId);
  };

  /**
   * Handle data received from a peer
   */
  private handlePeerData = (peerId: string, data: Uint8Array, channel: string): void => {
    // Handle terminal channel data
    if (channel === 'terminal') {
      try {
        const envelope = decodeEnvelope(data);
        const message = envelope.payload;

        if (message.type === 'SessionData') {
          const sessionData = message.data as SessionData;
          // Only process stdout/stderr (output from daemon)
          if (sessionData.stream === 'Stdout' || sessionData.stream === 'Stderr') {
            const text = new TextDecoder().decode(sessionData.data);
            this.sessionStore?.writeOutput(sessionData.session_id, text);
          }
        }
        // SessionClosed would also come through here
        if (message.type === 'SessionClosed') {
          const closedData = message.data as { session_id: string; reason?: string | null };
          console.log('[Orchestrator] Session closed:', closedData.session_id, closedData.reason);
          this.sessionStore?.setSessionStatus(closedData.session_id, 'disconnected', closedData.reason ?? undefined);
        }
      } catch (error) {
        console.error('[Orchestrator] Error processing terminal data:', error);
      }
    }

    // Handle files channel data
    if (channel === 'files') {
      try {
        const envelope = decodeEnvelope(data);
        const message = envelope.payload;

        if (message.type === 'FileListResponse') {
          const responseData = message.data as { path: string; entries: ProtocolFileEntry[] };
          // Convert protocol entries to store entries
          const storeEntries: StoreFileEntry[] = responseData.entries.map((entry) => ({
            name: entry.name,
            path: `${responseData.path}/${entry.name}`.replace(/\/+/g, '/'),
            type: this.mapEntryType(entry.entry_type),
            size: entry.size,
            modifiedAt: entry.modified * 1000, // Convert to milliseconds
            permissions: this.parsePermissions(entry.mode),
            isHidden: entry.name.startsWith('.'),
          }));
          this.fileStore?.setEntries(storeEntries);
        }

        if (message.type === 'FileDownloadChunk') {
          const chunkData = message.data as {
            path: string;
            offset: number;
            total_size: number;
            data: Uint8Array;
            is_last: boolean;
          };
          console.log('[Orchestrator] Received file chunk:', chunkData.path, chunkData.offset, chunkData.is_last);

          // Pass chunk to file store for assembly and progress tracking
          this.fileStore?.receiveChunk(
            chunkData.path,
            chunkData.offset,
            chunkData.data,
            chunkData.total_size,
            chunkData.is_last
          );

          // Request next chunk if not the last one
          if (!chunkData.is_last) {
            const nextOffset = chunkData.offset + chunkData.data.length;
            this.sendFileDownloadRequest(chunkData.path, nextOffset);
          }
        }

        if (message.type === 'Error') {
          const errorData = message.data as { code: string; message: string; context: string | null };
          console.error('[Orchestrator] File operation error:', errorData.message, 'context:', errorData.context);
          this.fileStore?.setError(errorData.message);
          // If this is a file operation error with a path context, fail the transfer
          if (errorData.context) {
            this.fileStore?.failDownloadByPath(errorData.context, errorData.message);
          }
        }
      } catch (error) {
        console.error('[Orchestrator] Error processing file data:', error);
      }
    }

    // Notify all registered data handlers
    for (const handler of this.dataHandlers) {
      try {
        handler(peerId, data, channel);
      } catch (error) {
        console.error('Error in data handler:', error);
      }
    }
  };

  /**
   * Handle peer error
   */
  private handlePeerError = (peerId: string, error?: Error): void => {
    const errorMessage = error?.message ?? 'Unknown peer error';
    this.store?.disconnectPeer(peerId, errorMessage);
  };

  /**
   * Map protocol entry type to store file type
   */
  private mapEntryType(entryType: string): 'file' | 'directory' | 'symlink' | 'unknown' {
    switch (entryType) {
      case 'File':
        return 'file';
      case 'Directory':
        return 'directory';
      case 'Symlink':
        return 'symlink';
      default:
        return 'unknown';
    }
  }

  /**
   * Parse Unix permissions mode to FilePermissions object
   */
  private parsePermissions(mode: number): { read: boolean; write: boolean; execute: boolean } {
    // Parse owner permissions from Unix mode (bits 8-6)
    return {
      read: (mode & 0o400) !== 0,
      write: (mode & 0o200) !== 0,
      execute: (mode & 0o100) !== 0,
    };
  }

  /**
   * Handle session store events (input, resize, created, closed)
   */
  private handleSessionEvent = (event: SessionEvent): void => {
    // Track session-to-peer mapping when session is created
    if (event.type === 'session:created' && event.sessionId) {
      const session = this.sessionStore?.getSession(event.sessionId);
      if (session) {
        this.setSessionPeer(event.sessionId, session.peerId);
        console.log('[Orchestrator] Mapped session to peer:', event.sessionId, '->', session.peerId);
      }
    }

    // Clean up mapping when session is closed
    if (event.type === 'session:closed' && event.sessionId) {
      this.removeSessionPeer(event.sessionId);
      console.log('[Orchestrator] Removed session mapping:', event.sessionId);
    }

    if (event.type === 'session:input' && event.sessionId) {
      const data = event.data as { input: string } | undefined;
      if (data?.input) {
        this.sendTerminalInput(event.sessionId, data.input);
      }
    }
    if (event.type === 'session:resize' && event.sessionId) {
      const data = event.data as { cols: number; rows: number } | undefined;
      if (data) {
        this.sendTerminalResize(event.sessionId, data.cols, data.rows);
      }
    }
  };

  /**
   * Handle file store events (navigate, refresh, download, upload)
   */
  private handleFileEvent = (event: FileEvent): void => {
    switch (event.type) {
      case 'files:navigate':
      case 'files:refresh':
        if (event.path) {
          this.sendFileListRequest(event.path);
        }
        break;
      case 'files:download':
        if (event.path && event.transferId) {
          // Start download by requesting the first chunk
          this.sendFileDownloadRequest(event.path, 0);
        }
        break;
      case 'files:upload':
        if (event.path && event.transferId && event.data) {
          const uploadData = event.data as { file: File };
          if (uploadData.file) {
            this.sendFileUploadStart(event.path, uploadData.file.size, 0o644);
          }
        }
        break;
    }
  };

  /**
   * Get the first connected peer (for file operations when no specific peer is needed)
   */
  private getFirstConnectedPeer(): string | undefined {
    // Get first peer from session-peer map
    for (const [, peerId] of this.sessionPeerMap.entries()) {
      return peerId;
    }
    return undefined;
  }

  /**
   * Send terminal input to a peer
   */
  private sendTerminalInput(sessionId: string, data: string): void {
    const peerId = this.getSessionPeer(sessionId);
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send input: no peer for session', sessionId);
      return;
    }

    const message = Msg.SessionData({
      session_id: sessionId,
      stream: 'Stdin',
      data: new TextEncoder().encode(data),
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'terminal');
  }

  /**
   * Send terminal resize to a peer
   */
  private sendTerminalResize(sessionId: string, cols: number, rows: number): void {
    const peerId = this.getSessionPeer(sessionId);
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send resize: no peer for session', sessionId);
      return;
    }

    const message = Msg.SessionResize({
      session_id: sessionId,
      cols,
      rows,
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'terminal');
  }

  /**
   * Send a file list request to a peer
   */
  private sendFileListRequest(path: string): void {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send file list request: no connected peer');
      this.fileStore?.setError('No connected peer');
      return;
    }

    const message = Msg.FileListRequest({
      path,
      include_hidden: this.fileStore?.state.showHidden ?? false,
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'files');
  }

  /**
   * Send a file download request to a peer
   */
  private sendFileDownloadRequest(path: string, offset: number): void {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send file download request: no connected peer');
      return;
    }

    const message = Msg.FileDownloadRequest({
      path,
      offset,
      chunk_size: 65536, // 64KB chunks
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'files');
  }

  /**
   * Send a file upload start request to a peer
   */
  private sendFileUploadStart(path: string, size: number, mode: number): void {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send file upload start: no connected peer');
      return;
    }

    const message = Msg.FileUploadStart({
      path,
      size,
      mode,
      overwrite: true,
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'files');
  }

  /**
   * Send a file upload chunk to a peer.
   * This method is public to allow external code to send upload chunks
   * as they are read from files.
   */
  sendFileUploadChunk(path: string, offset: number, data: Uint8Array): void {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send file upload chunk: no connected peer');
      return;
    }

    const message = Msg.FileUploadChunk({
      path,
      offset,
      data,
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'files');
  }

  /**
   * Send a file upload complete message to a peer.
   */
  private sendFileUploadComplete(path: string, checksum: Uint8Array): void {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      console.warn('[Orchestrator] Cannot send file upload complete: no connected peer');
      return;
    }

    const message = Msg.FileUploadComplete({
      path,
      checksum,
    });

    const envelope = createEnvelope(++this.messageSequence, message);
    const encoded = encodeEnvelope(envelope);

    this.webrtc.sendData(peerId, encoded, 'files');
  }

  /**
   * Upload a file to the remote daemon with chunked transfer and checksum verification.
   *
   * @param file - The File object to upload
   * @param destPath - Destination path on the remote system
   * @param onProgress - Optional callback for progress updates
   */
  async uploadFile(
    file: File,
    destPath: string,
    onProgress?: UploadProgressCallback
  ): Promise<void> {
    const peerId = this.getFirstConnectedPeer();
    if (!peerId || !this.webrtc) {
      throw new Error('No connected peer');
    }

    console.log('[Orchestrator] Starting upload:', destPath, 'size:', file.size);

    // Send upload start message
    this.sendFileUploadStart(destPath, file.size, 0o644);

    // Read file in chunks and send, while computing checksum
    let offset = 0;
    const hashChunks: Uint8Array[] = [];

    while (offset < file.size) {
      const end = Math.min(offset + UPLOAD_CHUNK_SIZE, file.size);
      const blob = file.slice(offset, end);
      const arrayBuffer = await blob.arrayBuffer();
      const chunk = new Uint8Array(arrayBuffer);

      // Store chunk for hash calculation
      hashChunks.push(chunk);

      // Send chunk to peer
      this.sendFileUploadChunk(destPath, offset, chunk);

      offset = end;

      // Report progress
      onProgress?.({ bytesSent: offset, totalBytes: file.size });

      // Small delay for flow control to prevent overwhelming the data channel
      if (offset < file.size) {
        await new Promise(resolve => setTimeout(resolve, 1));
      }
    }

    // Calculate SHA-256 checksum of the entire file
    const checksum = await this.calculateChecksum(hashChunks);

    // Send upload complete with checksum
    this.sendFileUploadComplete(destPath, checksum);

    console.log('[Orchestrator] Upload complete:', destPath);
  }

  /**
   * Calculate SHA-256 checksum from an array of chunks using Web Crypto API.
   */
  private async calculateChecksum(chunks: Uint8Array[]): Promise<Uint8Array> {
    // Calculate total size
    const totalSize = chunks.reduce((acc, chunk) => acc + chunk.length, 0);

    // Combine all chunks into a single buffer
    const combined = new Uint8Array(totalSize);
    let offset = 0;
    for (const chunk of chunks) {
      combined.set(chunk, offset);
      offset += chunk.length;
    }

    // Calculate SHA-256 hash using Web Crypto API
    const hashBuffer = await crypto.subtle.digest('SHA-256', combined);
    return new Uint8Array(hashBuffer);
  }

  /**
   * Clean up and destroy the orchestrator
   */
  destroy(): void {
    // Unsubscribe from events
    this.signalingUnsubscribe?.();
    this.webrtcUnsubscribe?.();
    this.sessionUnsubscribe?.();
    this.fileUnsubscribe?.();

    // Disconnect everything
    this.disconnect();

    // Clear handlers
    this.dataHandlers.clear();

    // Reset state
    this.signaling = null;
    this.webrtc = null;
    this.store = null;
    this.sessionStore = null;
    this.fileStore = null;
    this.initialized = false;
    this.state = 'idle';
    this.messageSequence = 0;
  }
}

/**
 * Singleton instance of ConnectionOrchestrator
 */
let orchestratorInstance: ConnectionOrchestrator | null = null;

/**
 * Get or create the singleton ConnectionOrchestrator instance
 */
export function getOrchestrator(): ConnectionOrchestrator {
  if (!orchestratorInstance) {
    orchestratorInstance = new ConnectionOrchestrator();
  }
  return orchestratorInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetOrchestrator(): void {
  if (orchestratorInstance) {
    orchestratorInstance.destroy();
  }
  orchestratorInstance = null;
}
