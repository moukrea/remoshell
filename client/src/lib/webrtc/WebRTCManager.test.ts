import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  WebRTCManager,
  getWebRTCManager,
  resetWebRTCManager,
  DEFAULT_ICE_SERVERS,
  type SignalData,
} from './WebRTCManager';

// Mock simple-peer
vi.mock('simple-peer', () => {
  return {
    default: vi.fn().mockImplementation((opts) => {
      return createMockPeer(opts);
    }),
  };
});

/**
 * Create a mock peer instance
 */
function createMockPeer(opts: { initiator: boolean }) {
  const eventHandlers: Record<string, ((...args: unknown[]) => void)[]> = {};

  const mockPeer = {
    initiator: opts.initiator,
    destroyed: false,
    _pc: createMockRTCPeerConnection(),

    on: vi.fn((event: string, handler: (...args: unknown[]) => void) => {
      if (!eventHandlers[event]) {
        eventHandlers[event] = [];
      }
      eventHandlers[event].push(handler);
      return mockPeer;
    }),

    emit: (event: string, ...args: unknown[]) => {
      const handlers = eventHandlers[event];
      if (handlers) {
        handlers.forEach(handler => handler(...args));
      }
    },

    signal: vi.fn((data: SignalData) => {
      // Simulate signal processing
      if (data.type === 'answer' && !opts.initiator) {
        // Invalid - responder shouldn't receive answer
        throw new Error('Invalid signal');
      }
    }),

    send: vi.fn((_data: Uint8Array) => {
      if (mockPeer.destroyed) {
        throw new Error('Peer is destroyed');
      }
    }),

    destroy: vi.fn(() => {
      mockPeer.destroyed = true;
      mockPeer.emit('close');
    }),
  };

  return mockPeer;
}

/**
 * Create a mock RTCPeerConnection
 */
function createMockRTCPeerConnection() {
  const dataChannels: Map<string, MockDataChannel> = new Map();

  return {
    ondatachannel: null as ((event: { channel: MockDataChannel }) => void) | null,

    createDataChannel: vi.fn((label: string, options?: RTCDataChannelInit) => {
      const channel = createMockDataChannel(label, options);
      dataChannels.set(label, channel);
      return channel;
    }),

    getDataChannels: () => dataChannels,
  };
}

interface MockDataChannel {
  label: string;
  readyState: RTCDataChannelState;
  ordered: boolean;
  maxRetransmits?: number;
  onmessage: ((event: MessageEvent) => void) | null;
  onerror: ((event: Event) => void) | null;
  onclose: (() => void) | null;
  send: Mock;
  close: Mock;
}

/**
 * Create a mock RTCDataChannel
 */
function createMockDataChannel(label: string, options?: RTCDataChannelInit): MockDataChannel {
  return {
    label,
    readyState: 'open' as RTCDataChannelState,
    ordered: options?.ordered ?? true,
    maxRetransmits: options?.maxRetransmits,
    onmessage: null,
    onerror: null,
    onclose: null,
    send: vi.fn(),
    close: vi.fn(),
  };
}

/**
 * Helper to get mock peer from manager
 */
function getMockPeer(manager: WebRTCManager, peerId: string) {
  // Access private peers map for testing
  const peers = (manager as unknown as { peers: Map<string, { peer: ReturnType<typeof createMockPeer> }> }).peers;
  return peers.get(peerId)?.peer;
}

describe('WebRTCManager', () => {
  let manager: WebRTCManager;

  beforeEach(() => {
    resetWebRTCManager();
    manager = new WebRTCManager();
  });

  afterEach(() => {
    manager.destroyAll();
    resetWebRTCManager();
  });

  describe('Constructor and ICE Configuration', () => {
    it('should use default ICE servers when none provided', () => {
      const mgr = new WebRTCManager();
      expect(mgr.getIceServers()).toEqual(DEFAULT_ICE_SERVERS);
    });

    it('should use custom ICE servers when provided', () => {
      const customServers: RTCIceServer[] = [
        { urls: 'stun:stun.example.com:19302' },
      ];
      const mgr = new WebRTCManager(customServers);
      expect(mgr.getIceServers()).toEqual(customServers);
    });

    it('should update ICE servers with setIceServers', () => {
      const newServers: RTCIceServer[] = [
        { urls: 'stun:stun.new.com:19302' },
      ];
      manager.setIceServers(newServers);
      expect(manager.getIceServers()).toEqual(newServers);
    });

    it('should return a copy of ICE servers', () => {
      const servers = manager.getIceServers();
      servers.push({ urls: 'stun:added.com' });
      expect(manager.getIceServers()).not.toContainEqual({ urls: 'stun:added.com' });
    });
  });

  describe('Connection Creation', () => {
    it('should create an initiator connection', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });

      expect(manager.hasPeer('peer-1')).toBe(true);
      expect(manager.getConnectionState('peer-1')).toBe('connecting');
    });

    it('should create a responder connection', () => {
      manager.createConnection({ peerId: 'peer-2', initiator: false });

      expect(manager.hasPeer('peer-2')).toBe(true);
      expect(manager.getConnectionState('peer-2')).toBe('connecting');
    });

    it('should replace existing connection with same peerId', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const firstPeer = getMockPeer(manager, 'peer-1');

      manager.createConnection({ peerId: 'peer-1', initiator: false });
      const secondPeer = getMockPeer(manager, 'peer-1');

      expect(firstPeer).not.toBe(secondPeer);
      expect(firstPeer?.destroy).toHaveBeenCalled();
    });

    it('should emit state_change event on connection creation', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'state_change',
        peerId: 'peer-1',
        state: 'connecting',
      });
    });
  });

  describe('Signal Handling', () => {
    it('should emit signal event when peer generates signal', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      const signalData: SignalData = { type: 'offer', sdp: 'test-sdp' };
      mockPeer?.emit('signal', signalData);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'signal',
        peerId: 'peer-1',
        data: signalData,
      });
    });

    it('should pass signal data to peer', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      const signalData: SignalData = { type: 'offer', sdp: 'test-sdp' };
      manager.signal('peer-1', signalData);

      expect(mockPeer?.signal).toHaveBeenCalledWith(signalData);
    });

    it('should warn when signaling non-existent peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      manager.signal('non-existent', { type: 'offer', sdp: 'test' });

      expect(consoleWarn).toHaveBeenCalledWith('Cannot signal: peer non-existent not found');
      consoleWarn.mockRestore();
    });

    it('should emit error event when signal fails', () => {
      const subscriber = vi.fn();
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      manager.subscribe(subscriber);
      manager.createConnection({ peerId: 'peer-1', initiator: false });
      const mockPeer = getMockPeer(manager, 'peer-1');

      // Make signal throw an error
      (mockPeer?.signal as Mock).mockImplementation(() => {
        throw new Error('Signal error');
      });

      manager.signal('peer-1', { type: 'offer', sdp: 'test' });

      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'error',
          peerId: 'peer-1',
        })
      );

      consoleError.mockRestore();
    });
  });

  describe('Connection Events', () => {
    it('should emit connect event when peer connects', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('connect');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'connect',
        peerId: 'peer-1',
      });
    });

    it('should update state to connected on connect', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('connect');

      expect(manager.getConnectionState('peer-1')).toBe('connected');
      expect(manager.isConnected('peer-1')).toBe(true);
    });

    it('should emit close event when peer closes', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('close');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'close',
        peerId: 'peer-1',
      });
    });

    it('should emit error event when peer errors', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      const error = new Error('Connection failed');
      mockPeer?.emit('error', error);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'error',
        peerId: 'peer-1',
        error,
      });
      expect(manager.getConnectionState('peer-1')).toBe('failed');
    });

    it('should emit data event when peer receives data', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      const data = new Uint8Array([1, 2, 3]);
      mockPeer?.emit('data', data);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'data',
        peerId: 'peer-1',
        data,
        channel: 'control',
      });
    });
  });

  describe('ICE State Changes', () => {
    it('should update state on ICE state changes', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      // Simulate ICE state changes
      mockPeer?.emit('iceStateChange', 'connected');
      expect(manager.getConnectionState('peer-1')).toBe('connected');

      mockPeer?.emit('iceStateChange', 'disconnected');
      expect(manager.getConnectionState('peer-1')).toBe('disconnected');

      mockPeer?.emit('iceStateChange', 'failed');
      expect(manager.getConnectionState('peer-1')).toBe('failed');
    });
  });

  describe('Data Sending', () => {
    beforeEach(() => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');
      mockPeer?.emit('connect');
    });

    it('should send data on control channel', () => {
      const data = new Uint8Array([1, 2, 3]);
      const result = manager.sendData('peer-1', data, 'control');

      expect(result).toBe(true);
      const mockPeer = getMockPeer(manager, 'peer-1');
      expect(mockPeer?.send).toHaveBeenCalledWith(data);
    });

    it('should send string data as Uint8Array', () => {
      const result = manager.sendData('peer-1', 'hello', 'control');

      expect(result).toBe(true);
      const mockPeer = getMockPeer(manager, 'peer-1');
      // Verify send was called with Uint8Array containing 'hello'
      expect(mockPeer?.send).toHaveBeenCalled();
      const callArg = (mockPeer?.send as Mock).mock.calls[0][0];
      // Check it's a Uint8Array-like object (cross-realm safe)
      expect(callArg.constructor.name).toBe('Uint8Array');
      expect(new TextDecoder().decode(callArg)).toBe('hello');
    });

    it('should return false for non-existent peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = manager.sendData('non-existent', new Uint8Array([1]), 'control');

      expect(result).toBe(false);
      consoleWarn.mockRestore();
    });

    it('should return false for disconnected peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const mockPeer = getMockPeer(manager, 'peer-1');
      mockPeer?.emit('iceStateChange', 'disconnected');

      const result = manager.sendData('peer-1', new Uint8Array([1]), 'control');

      expect(result).toBe(false);
      consoleWarn.mockRestore();
    });
  });

  describe('Data Channels', () => {
    it('should create terminal and files channels for initiator on connect', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('connect');

      const pc = mockPeer?._pc;
      expect(pc?.createDataChannel).toHaveBeenCalledWith('terminal', {
        ordered: false,
        maxRetransmits: 0,
      });
      expect(pc?.createDataChannel).toHaveBeenCalledWith('files', {
        ordered: true,
      });
    });

    it('should not create additional channels for responder', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: false });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('connect');

      const pc = mockPeer?._pc;
      // Responder doesn't create channels, it receives them
      expect(pc?.createDataChannel).not.toHaveBeenCalled();
    });
  });

  describe('Channel Availability', () => {
    it('should report control channel as available when connected', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');
      mockPeer?.emit('connect');

      expect(manager.isChannelAvailable('peer-1', 'control')).toBe(true);
    });

    it('should report channel as unavailable when disconnected', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });

      expect(manager.isChannelAvailable('peer-1', 'control')).toBe(false);
    });

    it('should report channel as unavailable for non-existent peer', () => {
      expect(manager.isChannelAvailable('non-existent', 'control')).toBe(false);
    });
  });

  describe('Connection Cleanup', () => {
    it('should destroy connection and clean up', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      manager.destroyConnection('peer-1');

      expect(mockPeer?.destroy).toHaveBeenCalled();
      expect(manager.hasPeer('peer-1')).toBe(false);
    });

    it('should destroy all connections', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      manager.createConnection({ peerId: 'peer-2', initiator: false });

      manager.destroyAll();

      expect(manager.hasPeer('peer-1')).toBe(false);
      expect(manager.hasPeer('peer-2')).toBe(false);
      expect(manager.getConnectionCount()).toBe(0);
    });

    it('should clean up peer on close event', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      const mockPeer = getMockPeer(manager, 'peer-1');

      mockPeer?.emit('close');

      expect(manager.hasPeer('peer-1')).toBe(false);
    });
  });

  describe('Connection Queries', () => {
    it('should return connection count', () => {
      expect(manager.getConnectionCount()).toBe(0);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      expect(manager.getConnectionCount()).toBe(1);

      manager.createConnection({ peerId: 'peer-2', initiator: false });
      expect(manager.getConnectionCount()).toBe(2);
    });

    it('should return connected peers', () => {
      manager.createConnection({ peerId: 'peer-1', initiator: true });
      manager.createConnection({ peerId: 'peer-2', initiator: false });

      const mockPeer1 = getMockPeer(manager, 'peer-1');
      mockPeer1?.emit('connect');

      const connectedPeers = manager.getConnectedPeers();
      expect(connectedPeers).toEqual(['peer-1']);
    });

    it('should return null state for non-existent peer', () => {
      expect(manager.getConnectionState('non-existent')).toBeNull();
    });

    it('should return false for isConnected on non-existent peer', () => {
      expect(manager.isConnected('non-existent')).toBe(false);
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });

      expect(subscriber).toHaveBeenCalled();
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = manager.subscribe(subscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      manager.createConnection({ peerId: 'peer-2', initiator: true });

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should handle subscriber errors gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      manager.subscribe(errorSubscriber);
      manager.subscribe(goodSubscriber);

      manager.createConnection({ peerId: 'peer-1', initiator: true });

      expect(errorSubscriber).toHaveBeenCalled();
      expect(goodSubscriber).toHaveBeenCalled();
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getWebRTCManager', () => {
      const instance1 = getWebRTCManager();
      const instance2 = getWebRTCManager();

      expect(instance1).toBe(instance2);
    });

    it('should create new instance after resetWebRTCManager', () => {
      const instance1 = getWebRTCManager();
      resetWebRTCManager();
      const instance2 = getWebRTCManager();

      expect(instance1).not.toBe(instance2);
    });
  });
});
