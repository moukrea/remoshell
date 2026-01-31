import { createStore, produce } from 'solid-js/store';
import { createSignal, batch } from 'solid-js';

/**
 * Signaling server connection status
 */
export type SignalingStatus = 'disconnected' | 'connecting' | 'connected';

/**
 * Peer connection status
 */
export type PeerConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'failed';

/**
 * Represents a WebRTC peer connection
 */
export interface PeerConnection {
  id: string;
  status: PeerConnectionStatus;
  label?: string;
  createdAt: number;
  connectedAt?: number;
  disconnectedAt?: number;
  reconnectAttempts: number;
  lastError?: string;
}

/**
 * Connection store state
 */
export interface ConnectionState {
  signalingStatus: SignalingStatus;
  peers: Record<string, PeerConnection>;
  activePeerId: string | null;
  signalingUrl: string | null;
  lastSignalingError?: string;
  reconnectAttempts: number;
}

/**
 * Event types emitted by the connection store
 */
export type ConnectionEventType =
  | 'signaling:connecting'
  | 'signaling:connected'
  | 'signaling:disconnected'
  | 'signaling:error'
  | 'peer:connecting'
  | 'peer:connected'
  | 'peer:disconnected'
  | 'peer:error'
  | 'peer:data'
  | 'peer:active';

/**
 * Event payload types
 */
export interface ConnectionEvent {
  type: ConnectionEventType;
  peerId?: string;
  data?: unknown;
  error?: string;
}

/**
 * Event subscriber callback type
 */
export type ConnectionEventSubscriber = (event: ConnectionEvent) => void;

/**
 * Initial state for the connection store
 */
const initialState: ConnectionState = {
  signalingStatus: 'disconnected',
  peers: {},
  activePeerId: null,
  signalingUrl: null,
  lastSignalingError: undefined,
  reconnectAttempts: 0,
};

/**
 * Creates a connection store for managing WebRTC peer connections
 */
export function createConnectionStore() {
  const [state, setState] = createStore<ConnectionState>({ ...initialState });
  const [subscribers] = createSignal<Set<ConnectionEventSubscriber>>(new Set());

  /**
   * Emit an event to all subscribers
   */
  const emit = (event: ConnectionEvent): void => {
    subscribers().forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in connection event subscriber:', error);
      }
    });
  };

  /**
   * Subscribe to connection events
   */
  const subscribe = (callback: ConnectionEventSubscriber): (() => void) => {
    subscribers().add(callback);
    return () => {
      subscribers().delete(callback);
    };
  };

  /**
   * Connect to the signaling server
   */
  const connectSignaling = (url: string): void => {
    batch(() => {
      setState('signalingStatus', 'connecting');
      setState('signalingUrl', url);
      setState('lastSignalingError', undefined);
    });
    emit({ type: 'signaling:connecting' });
  };

  /**
   * Mark signaling as connected
   */
  const signalingConnected = (): void => {
    batch(() => {
      setState('signalingStatus', 'connected');
      setState('reconnectAttempts', 0);
    });
    emit({ type: 'signaling:connected' });
  };

  /**
   * Disconnect from the signaling server
   */
  const disconnectSignaling = (error?: string): void => {
    batch(() => {
      setState('signalingStatus', 'disconnected');
      if (error) {
        setState('lastSignalingError', error);
        setState('reconnectAttempts', state.reconnectAttempts + 1);
      }
    });
    emit({
      type: error ? 'signaling:error' : 'signaling:disconnected',
      error
    });
  };

  /**
   * Connect to a peer
   */
  const connectToPeer = (peerId: string, label?: string): void => {
    setState(
      produce((s) => {
        s.peers[peerId] = {
          id: peerId,
          status: 'connecting',
          label,
          createdAt: Date.now(),
          reconnectAttempts: 0,
        };
      })
    );
    emit({ type: 'peer:connecting', peerId });
  };

  /**
   * Mark a peer as connected
   */
  const peerConnected = (peerId: string): void => {
    setState(
      produce((s) => {
        const peer = s.peers[peerId];
        if (peer) {
          peer.status = 'connected';
          peer.connectedAt = Date.now();
          peer.reconnectAttempts = 0;
          peer.lastError = undefined;
        }
      })
    );
    emit({ type: 'peer:connected', peerId });
  };

  /**
   * Disconnect a peer
   */
  const disconnectPeer = (peerId: string, error?: string): void => {
    setState(
      produce((s) => {
        const peer = s.peers[peerId];
        if (peer) {
          peer.status = error ? 'failed' : 'disconnected';
          peer.disconnectedAt = Date.now();
          if (error) {
            peer.lastError = error;
            peer.reconnectAttempts += 1;
          }
        }
        // Clear active peer if disconnected
        if (s.activePeerId === peerId) {
          s.activePeerId = null;
        }
      })
    );
    emit({
      type: error ? 'peer:error' : 'peer:disconnected',
      peerId,
      error
    });
  };

  /**
   * Remove a peer from the store
   */
  const removePeer = (peerId: string): void => {
    setState(
      produce((s) => {
        delete s.peers[peerId];
        if (s.activePeerId === peerId) {
          s.activePeerId = null;
        }
      })
    );
  };

  /**
   * Set the active peer
   */
  const setActivePeer = (peerId: string | null): void => {
    if (peerId !== null && !state.peers[peerId]) {
      console.warn(`Cannot set active peer: peer ${peerId} not found`);
      return;
    }
    setState('activePeerId', peerId);
    emit({ type: 'peer:active', peerId: peerId ?? undefined });
  };

  /**
   * Send data to a peer (placeholder - actual implementation depends on WebRTC)
   * Returns true if the data was queued for sending
   */
  const sendData = (peerId: string, data: unknown): boolean => {
    const peer = state.peers[peerId];
    if (!peer) {
      console.warn(`Cannot send data: peer ${peerId} not found`);
      return false;
    }
    if (peer.status !== 'connected') {
      console.warn(`Cannot send data: peer ${peerId} is not connected`);
      return false;
    }
    // Emit data event for external handling
    emit({ type: 'peer:data', peerId, data });
    return true;
  };

  /**
   * Get the current active peer
   */
  const getActivePeer = (): PeerConnection | null => {
    const peerId = state.activePeerId;
    return peerId ? state.peers[peerId] ?? null : null;
  };

  /**
   * Get all connected peers
   */
  const getConnectedPeers = (): PeerConnection[] => {
    return Object.values(state.peers).filter(peer => peer.status === 'connected');
  };

  /**
   * Check if we can reconnect based on attempt count
   */
  const canReconnectSignaling = (maxAttempts: number = 5): boolean => {
    return state.reconnectAttempts < maxAttempts;
  };

  /**
   * Check if a peer can reconnect based on attempt count
   */
  const canReconnectPeer = (peerId: string, maxAttempts: number = 3): boolean => {
    const peer = state.peers[peerId];
    return peer ? peer.reconnectAttempts < maxAttempts : false;
  };

  /**
   * Reset the store to initial state
   */
  const reset = (): void => {
    setState(
      produce((s) => {
        s.signalingStatus = 'disconnected';
        // Clear all peers
        for (const key of Object.keys(s.peers)) {
          delete s.peers[key];
        }
        s.activePeerId = null;
        s.signalingUrl = null;
        s.lastSignalingError = undefined;
        s.reconnectAttempts = 0;
      })
    );
  };

  return {
    // State (readonly)
    state,

    // Signaling actions
    connectSignaling,
    signalingConnected,
    disconnectSignaling,

    // Peer actions
    connectToPeer,
    peerConnected,
    disconnectPeer,
    removePeer,
    setActivePeer,
    sendData,

    // Getters
    getActivePeer,
    getConnectedPeers,
    canReconnectSignaling,
    canReconnectPeer,

    // Event subscriptions
    subscribe,

    // Utility
    reset,
  };
}

/**
 * Type for the connection store instance
 */
export type ConnectionStore = ReturnType<typeof createConnectionStore>;

/**
 * Singleton instance of the connection store
 */
let connectionStoreInstance: ConnectionStore | null = null;

/**
 * Get or create the singleton connection store instance
 */
export function getConnectionStore(): ConnectionStore {
  if (!connectionStoreInstance) {
    connectionStoreInstance = createConnectionStore();
  }
  return connectionStoreInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetConnectionStore(): void {
  if (connectionStoreInstance) {
    connectionStoreInstance.reset();
  }
  connectionStoreInstance = null;
}
