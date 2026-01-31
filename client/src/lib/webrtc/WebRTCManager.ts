import Peer from 'simple-peer';

/**
 * Data channel types for WebRTC connections
 */
export type ChannelType = 'control' | 'terminal' | 'files';

/**
 * Connection state for a peer
 */
export type ConnectionState =
  | 'new'
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'failed'
  | 'closed';

/**
 * Signal data exchanged for WebRTC negotiation
 */
export type SignalData = Peer.SignalData;

/**
 * Options for creating a new peer connection
 */
export interface CreateConnectionOptions {
  peerId: string;
  initiator: boolean;
  iceServers?: RTCIceServer[];
}

/**
 * Data channel configuration
 */
interface DataChannelConfig {
  ordered: boolean;
  maxRetransmits?: number;
}

/**
 * Event types emitted by WebRTCManager
 */
export type WebRTCEventType =
  | 'signal'
  | 'connect'
  | 'data'
  | 'close'
  | 'error'
  | 'state_change';

/**
 * Event payload for WebRTC events
 */
export interface WebRTCEvent {
  type: WebRTCEventType;
  peerId: string;
  data?: unknown;
  channel?: ChannelType;
  state?: ConnectionState;
  error?: Error;
}

/**
 * Event subscriber callback type
 */
export type WebRTCEventSubscriber = (event: WebRTCEvent) => void;

/**
 * Default STUN servers for ICE configuration
 */
export const DEFAULT_ICE_SERVERS: RTCIceServer[] = [
  { urls: 'stun:stun.l.google.com:19302' },
  { urls: 'stun:stun1.l.google.com:19302' },
  { urls: 'stun:stun2.l.google.com:19302' },
];

/**
 * Data channel configurations
 */
const CHANNEL_CONFIGS: Record<ChannelType, DataChannelConfig> = {
  control: { ordered: true }, // Reliable and ordered for control messages
  terminal: { ordered: false, maxRetransmits: 0 }, // Unordered for low latency terminal data
  files: { ordered: true }, // Reliable and ordered for file transfers
};

/**
 * Internal representation of a peer connection
 */
interface PeerConnectionEntry {
  peer: Peer.Instance;
  state: ConnectionState;
  channels: Map<ChannelType, RTCDataChannel>;
  initiator: boolean;
}

/**
 * WebRTC Manager for handling peer connections using simple-peer
 */
export class WebRTCManager {
  private peers: Map<string, PeerConnectionEntry> = new Map();
  private subscribers: Set<WebRTCEventSubscriber> = new Set();
  private iceServers: RTCIceServer[];

  constructor(iceServers: RTCIceServer[] = DEFAULT_ICE_SERVERS) {
    this.iceServers = iceServers;
  }

  /**
   * Subscribe to WebRTC events
   */
  subscribe(callback: WebRTCEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Emit an event to all subscribers
   */
  private emit(event: WebRTCEvent): void {
    this.subscribers.forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in WebRTC event subscriber:', error);
      }
    });
  }

  /**
   * Update ICE servers configuration
   */
  setIceServers(iceServers: RTCIceServer[]): void {
    this.iceServers = iceServers;
  }

  /**
   * Get current ICE servers configuration
   */
  getIceServers(): RTCIceServer[] {
    return [...this.iceServers];
  }

  /**
   * Create a new peer connection
   */
  createConnection(options: CreateConnectionOptions): void {
    const { peerId, initiator, iceServers } = options;

    // Clean up existing connection if any
    if (this.peers.has(peerId)) {
      this.destroyConnection(peerId);
    }

    const config: Peer.Options = {
      initiator,
      trickle: true,
      config: {
        iceServers: iceServers ?? this.iceServers,
      },
    };

    // For initiator, we can configure data channels
    // For responder, channels are created when the peer sets them up
    if (initiator) {
      config.channelConfig = CHANNEL_CONFIGS.control;
    }

    const peer = new Peer(config);

    const entry: PeerConnectionEntry = {
      peer,
      state: 'new',
      channels: new Map(),
      initiator,
    };

    this.peers.set(peerId, entry);
    this.setupPeerEventHandlers(peerId, peer);
    this.updateConnectionState(peerId, 'connecting');
  }

  /**
   * Setup event handlers for a peer connection
   */
  private setupPeerEventHandlers(peerId: string, peer: Peer.Instance): void {
    peer.on('signal', (data: SignalData) => {
      this.emit({
        type: 'signal',
        peerId,
        data,
      });
    });

    peer.on('connect', () => {
      this.updateConnectionState(peerId, 'connected');
      this.emit({
        type: 'connect',
        peerId,
      });

      // Create additional data channels for initiator
      const entry = this.peers.get(peerId);
      if (entry?.initiator) {
        this.createDataChannels(peerId);
      }
    });

    peer.on('data', (data: Uint8Array) => {
      // Default channel data (control channel)
      this.emit({
        type: 'data',
        peerId,
        data,
        channel: 'control',
      });
    });

    peer.on('close', () => {
      this.updateConnectionState(peerId, 'closed');
      this.emit({
        type: 'close',
        peerId,
      });
      this.cleanupPeer(peerId);
    });

    peer.on('error', (err: Error) => {
      this.updateConnectionState(peerId, 'failed');
      this.emit({
        type: 'error',
        peerId,
        error: err,
      });
    });

    // Handle ICE connection state changes
    peer.on('iceStateChange', (iceConnectionState: RTCIceConnectionState) => {
      const entry = this.peers.get(peerId);
      if (!entry) return;

      let newState: ConnectionState | null = null;
      switch (iceConnectionState) {
        case 'connected':
        case 'completed':
          newState = 'connected';
          break;
        case 'disconnected':
          newState = 'disconnected';
          break;
        case 'failed':
          newState = 'failed';
          break;
        case 'closed':
          newState = 'closed';
          break;
      }

      if (newState && newState !== entry.state) {
        this.updateConnectionState(peerId, newState);
      }
    });
  }

  /**
   * Create additional data channels for terminal and files
   */
  private createDataChannels(peerId: string): void {
    const entry = this.peers.get(peerId);
    if (!entry) return;

    // Access the underlying RTCPeerConnection
    // Note: simple-peer exposes this as a private property
    const peerConnection = (entry.peer as unknown as { _pc: RTCPeerConnection })._pc;
    if (!peerConnection) return;

    // Create terminal channel (unordered for low latency)
    try {
      const terminalChannel = peerConnection.createDataChannel('terminal', {
        ordered: CHANNEL_CONFIGS.terminal.ordered,
        maxRetransmits: CHANNEL_CONFIGS.terminal.maxRetransmits,
      });
      this.setupDataChannelHandlers(peerId, 'terminal', terminalChannel);
      entry.channels.set('terminal', terminalChannel);
    } catch (error) {
      console.error('Failed to create terminal channel:', error);
    }

    // Create files channel (ordered and reliable)
    try {
      const filesChannel = peerConnection.createDataChannel('files', {
        ordered: CHANNEL_CONFIGS.files.ordered,
      });
      this.setupDataChannelHandlers(peerId, 'files', filesChannel);
      entry.channels.set('files', filesChannel);
    } catch (error) {
      console.error('Failed to create files channel:', error);
    }

    // Setup handlers for incoming data channels (for responder)
    peerConnection.ondatachannel = (event: RTCDataChannelEvent) => {
      const channel = event.channel;
      const channelType = channel.label as ChannelType;
      if (channelType === 'terminal' || channelType === 'files') {
        this.setupDataChannelHandlers(peerId, channelType, channel);
        entry.channels.set(channelType, channel);
      }
    };
  }

  /**
   * Setup event handlers for a data channel
   */
  private setupDataChannelHandlers(
    peerId: string,
    channelType: ChannelType,
    channel: RTCDataChannel
  ): void {
    channel.onmessage = (event: MessageEvent) => {
      let data: Uint8Array;
      if (event.data instanceof ArrayBuffer) {
        data = new Uint8Array(event.data);
      } else if (event.data instanceof Uint8Array) {
        data = event.data;
      } else {
        // Handle string data
        data = new TextEncoder().encode(event.data);
      }

      this.emit({
        type: 'data',
        peerId,
        data,
        channel: channelType,
      });
    };

    channel.onerror = (event: Event) => {
      console.error(`Data channel ${channelType} error:`, event);
    };

    channel.onclose = () => {
      const entry = this.peers.get(peerId);
      if (entry) {
        entry.channels.delete(channelType);
      }
    };
  }

  /**
   * Handle incoming signal data from remote peer
   */
  signal(peerId: string, data: SignalData): void {
    const entry = this.peers.get(peerId);
    if (!entry) {
      console.warn(`Cannot signal: peer ${peerId} not found`);
      return;
    }

    try {
      entry.peer.signal(data);
    } catch (error) {
      console.error(`Error signaling peer ${peerId}:`, error);
      this.emit({
        type: 'error',
        peerId,
        error: error instanceof Error ? error : new Error(String(error)),
      });
    }
  }

  /**
   * Send data on a specific channel
   */
  sendData(peerId: string, data: Uint8Array | string, channel: ChannelType = 'control'): boolean {
    const entry = this.peers.get(peerId);
    if (!entry) {
      console.warn(`Cannot send data: peer ${peerId} not found`);
      return false;
    }

    if (entry.state !== 'connected') {
      console.warn(`Cannot send data: peer ${peerId} is not connected (state: ${entry.state})`);
      return false;
    }

    const dataToSend = typeof data === 'string' ? new TextEncoder().encode(data) : data;

    try {
      if (channel === 'control') {
        // Use simple-peer's built-in send for control channel
        entry.peer.send(dataToSend);
        return true;
      }

      const dataChannel = entry.channels.get(channel);
      if (!dataChannel) {
        console.warn(`Data channel ${channel} not available for peer ${peerId}`);
        return false;
      }

      if (dataChannel.readyState !== 'open') {
        console.warn(`Data channel ${channel} is not open for peer ${peerId}`);
        return false;
      }

      // Create ArrayBuffer from Uint8Array for RTCDataChannel.send()
      const buffer = new ArrayBuffer(dataToSend.byteLength);
      new Uint8Array(buffer).set(dataToSend);
      dataChannel.send(buffer);
      return true;
    } catch (error) {
      console.error(`Error sending data on channel ${channel}:`, error);
      return false;
    }
  }

  /**
   * Update connection state and emit state change event
   */
  private updateConnectionState(peerId: string, state: ConnectionState): void {
    const entry = this.peers.get(peerId);
    if (!entry) return;

    const previousState = entry.state;
    if (previousState === state) return;

    entry.state = state;
    this.emit({
      type: 'state_change',
      peerId,
      state,
    });
  }

  /**
   * Get the current connection state for a peer
   */
  getConnectionState(peerId: string): ConnectionState | null {
    const entry = this.peers.get(peerId);
    return entry?.state ?? null;
  }

  /**
   * Check if a peer is connected
   */
  isConnected(peerId: string): boolean {
    return this.getConnectionState(peerId) === 'connected';
  }

  /**
   * Get all connected peer IDs
   */
  getConnectedPeers(): string[] {
    const connected: string[] = [];
    this.peers.forEach((entry, peerId) => {
      if (entry.state === 'connected') {
        connected.push(peerId);
      }
    });
    return connected;
  }

  /**
   * Check if a specific channel is available for a peer
   */
  isChannelAvailable(peerId: string, channel: ChannelType): boolean {
    const entry = this.peers.get(peerId);
    if (!entry || entry.state !== 'connected') return false;

    if (channel === 'control') {
      return true; // Control channel is always available when connected
    }

    const dataChannel = entry.channels.get(channel);
    return dataChannel?.readyState === 'open';
  }

  /**
   * Clean up a peer connection
   */
  private cleanupPeer(peerId: string): void {
    const entry = this.peers.get(peerId);
    if (!entry) return;

    // Close all data channels
    entry.channels.forEach((channel) => {
      try {
        channel.close();
      } catch {
        // Ignore close errors
      }
    });
    entry.channels.clear();

    this.peers.delete(peerId);
  }

  /**
   * Destroy a peer connection
   */
  destroyConnection(peerId: string): void {
    const entry = this.peers.get(peerId);
    if (!entry) return;

    try {
      entry.peer.destroy();
    } catch {
      // Ignore destroy errors
    }

    this.cleanupPeer(peerId);
  }

  /**
   * Destroy all peer connections and clean up
   */
  destroyAll(): void {
    const peerIds = Array.from(this.peers.keys());
    peerIds.forEach(peerId => {
      this.destroyConnection(peerId);
    });
  }

  /**
   * Get the number of active connections
   */
  getConnectionCount(): number {
    return this.peers.size;
  }

  /**
   * Check if a peer exists (connected or connecting)
   */
  hasPeer(peerId: string): boolean {
    return this.peers.has(peerId);
  }
}

/**
 * Singleton instance of WebRTCManager
 */
let webRTCManagerInstance: WebRTCManager | null = null;

/**
 * Get or create the singleton WebRTCManager instance
 */
export function getWebRTCManager(): WebRTCManager {
  if (!webRTCManagerInstance) {
    webRTCManagerInstance = new WebRTCManager();
  }
  return webRTCManagerInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetWebRTCManager(): void {
  if (webRTCManagerInstance) {
    webRTCManagerInstance.destroyAll();
  }
  webRTCManagerInstance = null;
}
