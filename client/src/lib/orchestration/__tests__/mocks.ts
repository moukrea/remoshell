/**
 * Mock implementations for integration testing
 * These mocks provide controllable implementations of SignalingClient and WebRTCManager
 * for testing the connection orchestration flow without real network dependencies.
 */

import { vi } from 'vitest';
import type {
  SignalingConnectionState,
  AnySignalingEvent,
  SignalingEventSubscriber,
} from '../../signaling/SignalingClient';
import type {
  WebRTCEvent,
  WebRTCEventSubscriber,
  ConnectionState,
  ChannelType,
  CreateConnectionOptions,
} from '../../webrtc/WebRTCManager';

/**
 * Mock SignalingClient for testing
 * Provides controllable signaling behavior with event emission capabilities
 */
export function createMockSignalingClient() {
  let state: SignalingConnectionState = 'disconnected';
  let peerId: string | null = null;
  let roomId: string | null = null;
  const subscribers = new Set<SignalingEventSubscriber>();

  const mock = {
    // Core methods
    join: vi.fn((newRoomId: string) => {
      roomId = newRoomId;
      state = 'connecting';
    }),
    leave: vi.fn(() => {
      state = 'disconnected';
      roomId = null;
      peerId = null;
    }),
    subscribe: vi.fn((callback: SignalingEventSubscriber) => {
      subscribers.add(callback);
      return () => {
        subscribers.delete(callback);
      };
    }),
    destroy: vi.fn(() => {
      subscribers.clear();
      state = 'disconnected';
    }),

    // Signal sending methods
    sendOffer: vi.fn().mockReturnValue(true),
    sendAnswer: vi.fn().mockReturnValue(true),
    sendIceCandidate: vi.fn().mockReturnValue(true),

    // State getters
    getState: vi.fn(() => state),
    getPeerId: vi.fn(() => peerId),
    getRoomId: vi.fn(() => roomId),
    isConnected: vi.fn(() => state === 'connected'),

    // Test helpers - emit events to subscribers
    _emit: (event: AnySignalingEvent) => {
      subscribers.forEach((cb) => {
        try {
          cb(event);
        } catch (error) {
          console.error('Error in signaling subscriber:', error);
        }
      });
    },

    _setState: (newState: SignalingConnectionState) => {
      state = newState;
    },

    _setPeerId: (newPeerId: string) => {
      peerId = newPeerId;
    },

    _getSubscriberCount: () => subscribers.size,

    // Simulate connection flow
    _simulateConnect: (localPeerId: string, existingPeers: string[] = []) => {
      peerId = localPeerId;
      state = 'connected';
      mock._emit({
        type: 'connected',
        peerId: localPeerId,
        existingPeers,
      });
    },

    _simulateDisconnect: (reason?: string) => {
      state = 'disconnected';
      peerId = null;
      mock._emit({
        type: 'disconnected',
        reason,
      });
    },

    _simulatePeerJoined: (joinedPeerId: string) => {
      mock._emit({
        type: 'peer_joined',
        peerId: joinedPeerId,
      });
    },

    _simulatePeerLeft: (leftPeerId: string) => {
      mock._emit({
        type: 'peer_left',
        peerId: leftPeerId,
      });
    },

    _simulateOffer: (fromPeerId: string, offer: RTCSessionDescriptionInit) => {
      mock._emit({
        type: 'offer',
        peerId: fromPeerId,
        offer,
      });
    },

    _simulateAnswer: (fromPeerId: string, answer: RTCSessionDescriptionInit) => {
      mock._emit({
        type: 'answer',
        peerId: fromPeerId,
        answer,
      });
    },

    _simulateIceCandidate: (fromPeerId: string, candidate: RTCIceCandidateInit) => {
      mock._emit({
        type: 'ice',
        peerId: fromPeerId,
        candidate,
      });
    },

    _simulateError: (message: string) => {
      mock._emit({
        type: 'error',
        error: new Error(message),
        message,
      });
    },
  };

  return mock;
}

/**
 * Type for the mock SignalingClient
 */
export type MockSignalingClient = ReturnType<typeof createMockSignalingClient>;

/**
 * Mock WebRTCManager for testing
 * Provides controllable WebRTC behavior with peer connection simulation
 */
export function createMockWebRTCManager() {
  const peers = new Map<string, {
    state: ConnectionState;
    initiator: boolean;
    channels: Set<ChannelType>;
  }>();
  const subscribers = new Set<WebRTCEventSubscriber>();

  const mock = {
    // Core methods
    createConnection: vi.fn((options: CreateConnectionOptions) => {
      const { peerId, initiator } = options;
      peers.set(peerId, {
        state: 'connecting',
        initiator,
        channels: new Set(['control']),
      });
      mock._emit({
        type: 'state_change',
        peerId,
        state: 'connecting',
      });
    }),

    signal: vi.fn((_peerId: string, _data: unknown) => {
      // Process signal data - no-op in mock
    }),

    sendData: vi.fn((peerId: string, _data: Uint8Array | string, _channel: ChannelType = 'control') => {
      const peer = peers.get(peerId);
      if (!peer || peer.state !== 'connected') {
        return false;
      }
      return true;
    }),

    destroyConnection: vi.fn((peerId: string) => {
      peers.delete(peerId);
    }),

    destroyAll: vi.fn(() => {
      peers.clear();
    }),

    subscribe: vi.fn((callback: WebRTCEventSubscriber) => {
      subscribers.add(callback);
      return () => {
        subscribers.delete(callback);
      };
    }),

    // State getters
    getConnectionState: vi.fn((peerId: string) => {
      return peers.get(peerId)?.state ?? null;
    }),

    isConnected: vi.fn((peerId: string) => {
      return peers.get(peerId)?.state === 'connected';
    }),

    getConnectedPeers: vi.fn(() => {
      const connected: string[] = [];
      peers.forEach((peer, id) => {
        if (peer.state === 'connected') {
          connected.push(id);
        }
      });
      return connected;
    }),

    hasPeer: vi.fn((peerId: string) => {
      return peers.has(peerId);
    }),

    isChannelAvailable: vi.fn((peerId: string, channel: ChannelType) => {
      const peer = peers.get(peerId);
      return peer?.state === 'connected' && peer.channels.has(channel);
    }),

    getConnectionCount: vi.fn(() => peers.size),

    setIceServers: vi.fn(),
    getIceServers: vi.fn().mockReturnValue([]),

    // Test helpers
    _emit: (event: WebRTCEvent) => {
      subscribers.forEach((cb) => {
        try {
          cb(event);
        } catch (error) {
          console.error('Error in WebRTC subscriber:', error);
        }
      });
    },

    _getSubscriberCount: () => subscribers.size,

    _getPeer: (peerId: string) => peers.get(peerId),

    // Simulate peer events
    _simulateSignal: (peerId: string, signalData: unknown) => {
      mock._emit({
        type: 'signal',
        peerId,
        data: signalData,
      });
    },

    _simulateConnect: (peerId: string) => {
      const peer = peers.get(peerId);
      if (peer) {
        peer.state = 'connected';
        peer.channels.add('terminal');
        peer.channels.add('files');
      }
      mock._emit({
        type: 'connect',
        peerId,
      });
      mock._emit({
        type: 'state_change',
        peerId,
        state: 'connected',
      });
    },

    _simulateDisconnect: (peerId: string) => {
      const peer = peers.get(peerId);
      if (peer) {
        peer.state = 'disconnected';
      }
      mock._emit({
        type: 'close',
        peerId,
      });
      mock._emit({
        type: 'state_change',
        peerId,
        state: 'disconnected',
      });
    },

    _simulateData: (peerId: string, data: Uint8Array, channel: ChannelType = 'control' as ChannelType) => {
      mock._emit({
        type: 'data',
        peerId,
        data,
        channel: channel,
      });
    },

    _simulateError: (peerId: string, error: Error) => {
      const peer = peers.get(peerId);
      if (peer) {
        peer.state = 'failed';
      }
      mock._emit({
        type: 'error',
        peerId,
        error,
      });
      mock._emit({
        type: 'state_change',
        peerId,
        state: 'failed',
      });
    },
  };

  return mock;
}

/**
 * Type for the mock WebRTCManager
 */
export type MockWebRTCManager = ReturnType<typeof createMockWebRTCManager>;

/**
 * Create test utilities for common integration test setups
 */
export function createTestUtilities() {
  const mockSignaling = createMockSignalingClient();
  const mockWebRTC = createMockWebRTCManager();

  return {
    mockSignaling,
    mockWebRTC,

    /**
     * Simulate a full connection flow with a peer
     */
    simulateFullConnection: async (peerId: string) => {
      // 1. Connect to signaling
      mockSignaling._simulateConnect('local-peer', []);

      // 2. Peer joins
      mockSignaling._simulatePeerJoined(peerId);

      // 3. WebRTC connection established
      mockWebRTC._simulateConnect(peerId);

      return peerId;
    },

    /**
     * Simulate peer disconnect
     */
    simulatePeerDisconnect: async (peerId: string) => {
      mockWebRTC._simulateDisconnect(peerId);
      mockSignaling._simulatePeerLeft(peerId);
    },

    /**
     * Simulate terminal data flow
     */
    simulateTerminalData: (peerId: string, data: string) => {
      const encoded = new TextEncoder().encode(data);
      mockWebRTC._simulateData(peerId, encoded, 'terminal');
    },

    /**
     * Reset all mocks
     */
    reset: () => {
      vi.clearAllMocks();
      mockSignaling.destroy();
      mockWebRTC.destroyAll();
    },
  };
}

/**
 * Type for test utilities
 */
export type TestUtilities = ReturnType<typeof createTestUtilities>;
