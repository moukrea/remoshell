/**
 * Signaling client for WebRTC peer connection negotiation
 * Connects to the Cloudflare signaling server and handles message relay
 */

/**
 * Message types sent from the signaling server
 */
export type ServerMessageType =
  | 'join'
  | 'peer-joined'
  | 'peer-left'
  | 'offer'
  | 'answer'
  | 'ice'
  | 'error';

/**
 * Message types sent to the signaling server
 */
export type ClientMessageType = 'offer' | 'answer' | 'ice';

/**
 * Base signaling message structure
 */
export interface SignalingMessage {
  type: ServerMessageType;
  peerId?: string;
  data?: unknown;
}

/**
 * Join message received when connecting
 */
export interface JoinMessage extends SignalingMessage {
  type: 'join';
  peerId: string;
  data: { peers: string[] };
}

/**
 * Peer joined message
 */
export interface PeerJoinedMessage extends SignalingMessage {
  type: 'peer-joined';
  peerId: string;
}

/**
 * Peer left message
 */
export interface PeerLeftMessage extends SignalingMessage {
  type: 'peer-left';
  peerId: string;
}

/**
 * WebRTC offer message
 */
export interface OfferMessage extends SignalingMessage {
  type: 'offer';
  peerId: string;
  data: RTCSessionDescriptionInit;
}

/**
 * WebRTC answer message
 */
export interface AnswerMessage extends SignalingMessage {
  type: 'answer';
  peerId: string;
  data: RTCSessionDescriptionInit;
}

/**
 * ICE candidate message
 */
export interface IceCandidateMessage extends SignalingMessage {
  type: 'ice';
  peerId: string;
  data: RTCIceCandidateInit;
}

/**
 * Error message from server
 */
export interface ErrorMessage extends SignalingMessage {
  type: 'error';
  data: { message: string };
}

/**
 * Signaling client connection state
 */
export type SignalingConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting';

/**
 * Event types emitted by SignalingClient
 */
export type SignalingEventType =
  | 'connected'
  | 'disconnected'
  | 'peer_joined'
  | 'peer_left'
  | 'offer'
  | 'answer'
  | 'ice'
  | 'error'
  | 'state_change';

/**
 * Base event payload
 */
export interface SignalingEvent {
  type: SignalingEventType;
}

/**
 * Connected event - fired when successfully joined a room
 */
export interface ConnectedEvent extends SignalingEvent {
  type: 'connected';
  peerId: string;
  existingPeers: string[];
}

/**
 * Disconnected event
 */
export interface DisconnectedEvent extends SignalingEvent {
  type: 'disconnected';
  reason?: string;
}

/**
 * Peer joined event
 */
export interface PeerJoinedEvent extends SignalingEvent {
  type: 'peer_joined';
  peerId: string;
}

/**
 * Peer left event
 */
export interface PeerLeftEvent extends SignalingEvent {
  type: 'peer_left';
  peerId: string;
}

/**
 * Offer received event
 */
export interface OfferEvent extends SignalingEvent {
  type: 'offer';
  peerId: string;
  offer: RTCSessionDescriptionInit;
}

/**
 * Answer received event
 */
export interface AnswerEvent extends SignalingEvent {
  type: 'answer';
  peerId: string;
  answer: RTCSessionDescriptionInit;
}

/**
 * ICE candidate received event
 */
export interface IceEvent extends SignalingEvent {
  type: 'ice';
  peerId: string;
  candidate: RTCIceCandidateInit;
}

/**
 * Error event
 */
export interface SignalingErrorEvent extends SignalingEvent {
  type: 'error';
  error: Error;
  message: string;
}

/**
 * State change event
 */
export interface StateChangeEvent extends SignalingEvent {
  type: 'state_change';
  state: SignalingConnectionState;
  previousState: SignalingConnectionState;
}

/**
 * Union of all signaling events
 */
export type AnySignalingEvent =
  | ConnectedEvent
  | DisconnectedEvent
  | PeerJoinedEvent
  | PeerLeftEvent
  | OfferEvent
  | AnswerEvent
  | IceEvent
  | SignalingErrorEvent
  | StateChangeEvent;

/**
 * Event subscriber callback
 */
export type SignalingEventSubscriber = (event: AnySignalingEvent) => void;

/**
 * Configuration for SignalingClient
 */
export interface SignalingClientConfig {
  /** Base URL of the signaling server */
  serverUrl: string;
  /** Maximum reconnection attempts (default: 5) */
  maxReconnectAttempts?: number;
  /** Base delay for reconnection backoff in ms (default: 1000) */
  reconnectBaseDelay?: number;
  /** Maximum delay for reconnection backoff in ms (default: 30000) */
  reconnectMaxDelay?: number;
  /** Connection timeout in ms (default: 10000) */
  connectionTimeout?: number;
}

/**
 * Default configuration values
 */
const DEFAULT_CONFIG = {
  maxReconnectAttempts: 5,
  reconnectBaseDelay: 1000,
  reconnectMaxDelay: 30000,
  connectionTimeout: 10000,
};

/**
 * SignalingClient manages WebSocket connection to the signaling server
 * for WebRTC peer connection negotiation
 */
export class SignalingClient {
  private config: Required<SignalingClientConfig>;
  private ws: WebSocket | null = null;
  private state: SignalingConnectionState = 'disconnected';
  private subscribers: Set<SignalingEventSubscriber> = new Set();
  private roomId: string | null = null;
  private peerId: string | null = null;
  private reconnectAttempts = 0;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  private connectionTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  constructor(config: SignalingClientConfig) {
    this.config = {
      ...DEFAULT_CONFIG,
      ...config,
    };
  }

  /**
   * Subscribe to signaling events
   */
  subscribe(callback: SignalingEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Emit an event to all subscribers
   */
  private emit(event: AnySignalingEvent): void {
    this.subscribers.forEach((subscriber) => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in signaling event subscriber:', error);
      }
    });
  }

  /**
   * Update connection state and emit state change event
   */
  private setState(newState: SignalingConnectionState): void {
    if (this.state === newState) return;

    const previousState = this.state;
    this.state = newState;

    this.emit({
      type: 'state_change',
      state: newState,
      previousState,
    });
  }

  /**
   * Get current connection state
   */
  getState(): SignalingConnectionState {
    return this.state;
  }

  /**
   * Get the local peer ID (assigned by server after joining)
   */
  getPeerId(): string | null {
    return this.peerId;
  }

  /**
   * Get the current room ID
   */
  getRoomId(): string | null {
    return this.roomId;
  }

  /**
   * Check if connected to the signaling server
   */
  isConnected(): boolean {
    return this.state === 'connected';
  }

  /**
   * Join a room by establishing WebSocket connection
   */
  join(roomId: string): void {
    if (this.state === 'connected' || this.state === 'connecting') {
      console.warn('Already connected or connecting to signaling server');
      return;
    }

    this.roomId = roomId;
    this.intentionalClose = false;
    this.reconnectAttempts = 0;
    this.connect();
  }

  /**
   * Establish WebSocket connection
   */
  private connect(): void {
    if (!this.roomId) {
      console.error('Cannot connect without room ID');
      return;
    }

    this.setState(this.reconnectAttempts > 0 ? 'reconnecting' : 'connecting');

    // Build WebSocket URL
    const baseUrl = this.config.serverUrl.replace(/^http/, 'ws');
    const url = `${baseUrl}/room/${this.roomId}`;

    try {
      this.ws = new WebSocket(url);

      // Set connection timeout
      this.connectionTimer = setTimeout(() => {
        if (this.ws && this.ws.readyState === WebSocket.CONNECTING) {
          console.warn(`[SignalingClient] Connection timeout after ${this.config.connectionTimeout}ms`);
          this.connectionTimer = null;
          this.ws.close();
          this.scheduleReconnect();
        }
      }, this.config.connectionTimeout);

      this.setupWebSocketHandlers();
    } catch (error) {
      if (this.connectionTimer) {
        clearTimeout(this.connectionTimer);
        this.connectionTimer = null;
      }
      this.handleConnectionError(error);
    }
  }

  /**
   * Setup WebSocket event handlers
   */
  private setupWebSocketHandlers(): void {
    if (!this.ws) return;

    this.ws.onopen = () => {
      // Clear connection timeout on successful connection
      if (this.connectionTimer) {
        clearTimeout(this.connectionTimer);
        this.connectionTimer = null;
      }
      // Connection established, waiting for join message from server
    };

    this.ws.onmessage = (event: MessageEvent) => {
      this.handleMessage(event.data);
    };

    this.ws.onclose = (event: CloseEvent) => {
      // Clear connection timeout if still pending
      if (this.connectionTimer) {
        clearTimeout(this.connectionTimer);
        this.connectionTimer = null;
      }
      this.handleClose(event);
    };

    this.ws.onerror = () => {
      // Error handling is done in onclose
    };
  }

  /**
   * Handle incoming WebSocket messages
   */
  private handleMessage(data: string): void {
    let message: SignalingMessage;
    try {
      message = JSON.parse(data);
    } catch {
      console.error('Failed to parse signaling message:', data);
      return;
    }

    switch (message.type) {
      case 'join':
        this.handleJoinMessage(message as JoinMessage);
        break;
      case 'peer-joined':
        this.handlePeerJoined(message as PeerJoinedMessage);
        break;
      case 'peer-left':
        this.handlePeerLeft(message as PeerLeftMessage);
        break;
      case 'offer':
        this.handleOffer(message as OfferMessage);
        break;
      case 'answer':
        this.handleAnswer(message as AnswerMessage);
        break;
      case 'ice':
        this.handleIceCandidate(message as IceCandidateMessage);
        break;
      case 'error':
        this.handleServerError(message as ErrorMessage);
        break;
      default:
        console.warn('Unknown signaling message type:', message.type);
    }
  }

  /**
   * Handle join message (received after successful connection)
   */
  private handleJoinMessage(message: JoinMessage): void {
    this.peerId = message.peerId;
    this.reconnectAttempts = 0;
    this.setState('connected');

    this.emit({
      type: 'connected',
      peerId: message.peerId,
      existingPeers: message.data.peers,
    });
  }

  /**
   * Handle peer joined message
   */
  private handlePeerJoined(message: PeerJoinedMessage): void {
    this.emit({
      type: 'peer_joined',
      peerId: message.peerId,
    });
  }

  /**
   * Handle peer left message
   */
  private handlePeerLeft(message: PeerLeftMessage): void {
    this.emit({
      type: 'peer_left',
      peerId: message.peerId,
    });
  }

  /**
   * Handle offer message
   */
  private handleOffer(message: OfferMessage): void {
    this.emit({
      type: 'offer',
      peerId: message.peerId,
      offer: message.data,
    });
  }

  /**
   * Handle answer message
   */
  private handleAnswer(message: AnswerMessage): void {
    this.emit({
      type: 'answer',
      peerId: message.peerId,
      answer: message.data,
    });
  }

  /**
   * Handle ICE candidate message
   */
  private handleIceCandidate(message: IceCandidateMessage): void {
    this.emit({
      type: 'ice',
      peerId: message.peerId,
      candidate: message.data,
    });
  }

  /**
   * Handle server error message
   */
  private handleServerError(message: ErrorMessage): void {
    const errorMessage = message.data?.message ?? 'Unknown server error';
    this.emit({
      type: 'error',
      error: new Error(errorMessage),
      message: errorMessage,
    });
  }

  /**
   * Handle WebSocket close event
   */
  private handleClose(event: CloseEvent): void {
    this.ws = null;

    if (this.intentionalClose) {
      this.setState('disconnected');
      this.emit({
        type: 'disconnected',
        reason: 'Intentional disconnect',
      });
      return;
    }

    // Attempt reconnection if not intentionally closed
    if (this.reconnectAttempts < this.config.maxReconnectAttempts) {
      this.scheduleReconnect();
    } else {
      this.setState('disconnected');
      this.emit({
        type: 'disconnected',
        reason: event.reason || `Connection closed (code: ${event.code})`,
      });
      this.emit({
        type: 'error',
        error: new Error('Max reconnection attempts reached'),
        message: 'Max reconnection attempts reached',
      });
    }
  }

  /**
   * Handle connection error
   */
  private handleConnectionError(error: unknown): void {
    const errorMessage = error instanceof Error ? error.message : 'Connection failed';
    this.emit({
      type: 'error',
      error: error instanceof Error ? error : new Error(errorMessage),
      message: errorMessage,
    });

    if (this.reconnectAttempts < this.config.maxReconnectAttempts) {
      this.scheduleReconnect();
    } else {
      this.setState('disconnected');
    }
  }

  /**
   * Schedule a reconnection attempt with exponential backoff
   */
  private scheduleReconnect(): void {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
    }

    this.reconnectAttempts++;

    // Calculate delay with exponential backoff and jitter
    const exponentialDelay = this.config.reconnectBaseDelay * Math.pow(2, this.reconnectAttempts - 1);
    const jitter = Math.random() * 0.3 * exponentialDelay; // Add 0-30% jitter
    const delay = Math.min(exponentialDelay + jitter, this.config.reconnectMaxDelay);

    this.setState('reconnecting');

    this.reconnectTimeout = setTimeout(() => {
      this.reconnectTimeout = null;
      this.connect();
    }, delay);
  }

  /**
   * Leave the current room and disconnect
   */
  leave(): void {
    this.intentionalClose = true;
    this.cleanup();
    this.setState('disconnected');
  }

  /**
   * Clean up resources
   */
  private cleanup(): void {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    if (this.connectionTimer) {
      clearTimeout(this.connectionTimer);
      this.connectionTimer = null;
    }

    if (this.ws) {
      try {
        this.ws.close();
      } catch {
        // Ignore close errors
      }
      this.ws = null;
    }

    this.roomId = null;
    this.peerId = null;
    this.reconnectAttempts = 0;
  }

  /**
   * Send an offer to peers (broadcast via signaling server)
   */
  sendOffer(offer: RTCSessionDescriptionInit): boolean {
    return this.sendMessage({
      type: 'offer',
      data: offer,
    });
  }

  /**
   * Send an answer to peers (broadcast via signaling server)
   */
  sendAnswer(answer: RTCSessionDescriptionInit): boolean {
    return this.sendMessage({
      type: 'answer',
      data: answer,
    });
  }

  /**
   * Send an ICE candidate to peers (broadcast via signaling server)
   */
  sendIceCandidate(candidate: RTCIceCandidateInit): boolean {
    return this.sendMessage({
      type: 'ice',
      data: candidate,
    });
  }

  /**
   * Send a message through the WebSocket
   */
  private sendMessage(message: { type: ClientMessageType; data: unknown }): boolean {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.warn('Cannot send message: WebSocket not connected');
      return false;
    }

    try {
      this.ws.send(JSON.stringify(message));
      return true;
    } catch (error) {
      console.error('Failed to send signaling message:', error);
      return false;
    }
  }

  /**
   * Destroy the client and clean up all resources
   */
  destroy(): void {
    this.intentionalClose = true;
    this.cleanup();
    this.subscribers.clear();
  }
}

/**
 * Singleton instance of SignalingClient
 */
let signalingClientInstance: SignalingClient | null = null;

/**
 * Get or create a singleton SignalingClient instance
 */
export function getSignalingClient(config?: SignalingClientConfig): SignalingClient {
  if (!signalingClientInstance) {
    if (!config) {
      throw new Error('SignalingClient not initialized. Provide config for first call.');
    }
    signalingClientInstance = new SignalingClient(config);
  }
  return signalingClientInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetSignalingClient(): void {
  if (signalingClientInstance) {
    signalingClientInstance.destroy();
  }
  signalingClientInstance = null;
}
