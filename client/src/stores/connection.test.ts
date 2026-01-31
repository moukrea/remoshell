import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createConnectionStore,
  getConnectionStore,
  resetConnectionStore,
  type ConnectionStore,
} from './connection';

describe('Connection Store', () => {
  let store: ConnectionStore;

  beforeEach(() => {
    resetConnectionStore();
    store = createConnectionStore();
  });

  afterEach(() => {
    resetConnectionStore();
  });

  describe('Initial State', () => {
    it('should have correct initial state', () => {
      expect(store.state.signalingStatus).toBe('disconnected');
      expect(store.state.peers).toEqual({});
      expect(store.state.activePeerId).toBeNull();
      expect(store.state.signalingUrl).toBeNull();
      expect(store.state.reconnectAttempts).toBe(0);
    });
  });

  describe('Signaling Connection', () => {
    it('should update status to connecting when connectSignaling is called', () => {
      store.connectSignaling('wss://example.com/signaling');

      expect(store.state.signalingStatus).toBe('connecting');
      expect(store.state.signalingUrl).toBe('wss://example.com/signaling');
    });

    it('should emit signaling:connecting event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectSignaling('wss://example.com/signaling');

      expect(subscriber).toHaveBeenCalledWith({ type: 'signaling:connecting' });
    });

    it('should update status to connected when signalingConnected is called', () => {
      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();

      expect(store.state.signalingStatus).toBe('connected');
    });

    it('should reset reconnect attempts on successful connection', () => {
      // Simulate failed connection
      store.connectSignaling('wss://example.com/signaling');
      store.disconnectSignaling('Connection failed');
      expect(store.state.reconnectAttempts).toBe(1);

      // Reconnect successfully
      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();

      expect(store.state.reconnectAttempts).toBe(0);
    });

    it('should emit signaling:connected event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();

      expect(subscriber).toHaveBeenCalledWith({ type: 'signaling:connected' });
    });

    it('should update status to disconnected when disconnectSignaling is called', () => {
      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();
      store.disconnectSignaling();

      expect(store.state.signalingStatus).toBe('disconnected');
    });

    it('should emit signaling:disconnected event on clean disconnect', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();
      subscriber.mockClear();
      store.disconnectSignaling();

      expect(subscriber).toHaveBeenCalledWith({
        type: 'signaling:disconnected',
        error: undefined,
      });
    });

    it('should emit signaling:error event when disconnected with error', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();
      subscriber.mockClear();
      store.disconnectSignaling('Connection lost');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'signaling:error',
        error: 'Connection lost',
      });
      expect(store.state.lastSignalingError).toBe('Connection lost');
    });

    it('should increment reconnect attempts on error disconnect', () => {
      store.connectSignaling('wss://example.com/signaling');
      store.signalingConnected();
      store.disconnectSignaling('Error 1');
      expect(store.state.reconnectAttempts).toBe(1);

      store.connectSignaling('wss://example.com/signaling');
      store.disconnectSignaling('Error 2');
      expect(store.state.reconnectAttempts).toBe(2);
    });
  });

  describe('Peer Connections', () => {
    it('should add a peer when connectToPeer is called', () => {
      store.connectToPeer('peer-1', 'Test Peer');

      expect(store.state.peers['peer-1']).toBeDefined();
      expect(store.state.peers['peer-1'].id).toBe('peer-1');
      expect(store.state.peers['peer-1'].status).toBe('connecting');
      expect(store.state.peers['peer-1'].label).toBe('Test Peer');
      expect(store.state.peers['peer-1'].reconnectAttempts).toBe(0);
    });

    it('should emit peer:connecting event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:connecting',
        peerId: 'peer-1',
      });
    });

    it('should update peer status when peerConnected is called', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');

      expect(store.state.peers['peer-1'].status).toBe('connected');
      expect(store.state.peers['peer-1'].connectedAt).toBeDefined();
    });

    it('should emit peer:connected event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:connected',
        peerId: 'peer-1',
      });
    });

    it('should update peer status when disconnectPeer is called', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1');

      expect(store.state.peers['peer-1'].status).toBe('disconnected');
      expect(store.state.peers['peer-1'].disconnectedAt).toBeDefined();
    });

    it('should emit peer:disconnected event on clean disconnect', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      subscriber.mockClear();
      store.disconnectPeer('peer-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:disconnected',
        peerId: 'peer-1',
        error: undefined,
      });
    });

    it('should set peer status to failed and emit peer:error on error disconnect', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      subscriber.mockClear();
      store.disconnectPeer('peer-1', 'Connection failed');

      expect(store.state.peers['peer-1'].status).toBe('failed');
      expect(store.state.peers['peer-1'].lastError).toBe('Connection failed');
      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:error',
        peerId: 'peer-1',
        error: 'Connection failed',
      });
    });

    it('should increment peer reconnect attempts on error', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1', 'Error 1');

      expect(store.state.peers['peer-1'].reconnectAttempts).toBe(1);

      // Second error without successful reconnection in between
      store.disconnectPeer('peer-1', 'Error 2');

      expect(store.state.peers['peer-1'].reconnectAttempts).toBe(2);
    });

    it('should reset peer reconnect attempts on successful reconnection', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1', 'Error');
      expect(store.state.peers['peer-1'].reconnectAttempts).toBe(1);

      store.peerConnected('peer-1');
      expect(store.state.peers['peer-1'].reconnectAttempts).toBe(0);
    });

    it('should remove peer from store when removePeer is called', () => {
      store.connectToPeer('peer-1');
      store.removePeer('peer-1');

      expect(store.state.peers['peer-1']).toBeUndefined();
    });

    it('should track multiple peers', () => {
      store.connectToPeer('peer-1', 'Peer 1');
      store.connectToPeer('peer-2', 'Peer 2');
      store.connectToPeer('peer-3', 'Peer 3');

      expect(Object.keys(store.state.peers)).toHaveLength(3);
      expect(store.state.peers['peer-1'].label).toBe('Peer 1');
      expect(store.state.peers['peer-2'].label).toBe('Peer 2');
      expect(store.state.peers['peer-3'].label).toBe('Peer 3');
    });
  });

  describe('Active Peer', () => {
    it('should set active peer', () => {
      store.connectToPeer('peer-1');
      store.setActivePeer('peer-1');

      expect(store.state.activePeerId).toBe('peer-1');
    });

    it('should emit peer:active event when active peer changes', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      subscriber.mockClear();
      store.setActivePeer('peer-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:active',
        peerId: 'peer-1',
      });
    });

    it('should clear active peer when set to null', () => {
      store.connectToPeer('peer-1');
      store.setActivePeer('peer-1');
      store.setActivePeer(null);

      expect(store.state.activePeerId).toBeNull();
    });

    it('should emit peer:active with undefined peerId when cleared', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      store.setActivePeer('peer-1');
      subscriber.mockClear();
      store.setActivePeer(null);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:active',
        peerId: undefined,
      });
    });

    it('should not set active peer if peer does not exist', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      store.setActivePeer('non-existent');

      expect(store.state.activePeerId).toBeNull();
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should clear active peer when disconnecting active peer', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.setActivePeer('peer-1');
      store.disconnectPeer('peer-1');

      expect(store.state.activePeerId).toBeNull();
    });

    it('should clear active peer when removing active peer', () => {
      store.connectToPeer('peer-1');
      store.setActivePeer('peer-1');
      store.removePeer('peer-1');

      expect(store.state.activePeerId).toBeNull();
    });

    it('should return active peer via getActivePeer', () => {
      store.connectToPeer('peer-1', 'Active Peer');
      store.setActivePeer('peer-1');

      const activePeer = store.getActivePeer();
      expect(activePeer).not.toBeNull();
      expect(activePeer?.id).toBe('peer-1');
      expect(activePeer?.label).toBe('Active Peer');
    });

    it('should return null from getActivePeer when no active peer', () => {
      expect(store.getActivePeer()).toBeNull();
    });
  });

  describe('Send Data', () => {
    it('should return true when sending to connected peer', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');

      const result = store.sendData('peer-1', { message: 'hello' });

      expect(result).toBe(true);
    });

    it('should emit peer:data event when sending data', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      subscriber.mockClear();

      const data = { message: 'hello' };
      store.sendData('peer-1', data);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'peer:data',
        peerId: 'peer-1',
        data,
      });
    });

    it('should return false when sending to non-existent peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = store.sendData('non-existent', { message: 'hello' });

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should return false when sending to disconnected peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1');

      const result = store.sendData('peer-1', { message: 'hello' });

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should return false when sending to connecting peer', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      store.connectToPeer('peer-1');

      const result = store.sendData('peer-1', { message: 'hello' });

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.connectSignaling('wss://example.com');

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = store.subscribe(subscriber);

      store.connectSignaling('wss://example.com');
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      store.signalingConnected();

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should support multiple subscribers', () => {
      const subscriber1 = vi.fn();
      const subscriber2 = vi.fn();

      store.subscribe(subscriber1);
      store.subscribe(subscriber2);

      store.connectSignaling('wss://example.com');

      expect(subscriber1).toHaveBeenCalledTimes(1);
      expect(subscriber2).toHaveBeenCalledTimes(1);
    });

    it('should handle errors in subscribers gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      store.subscribe(errorSubscriber);
      store.subscribe(goodSubscriber);

      // Should not throw
      expect(() => store.connectSignaling('wss://example.com')).not.toThrow();

      // Both subscribers should have been called
      expect(errorSubscriber).toHaveBeenCalledTimes(1);
      expect(goodSubscriber).toHaveBeenCalledTimes(1);
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Helper Functions', () => {
    it('should return connected peers via getConnectedPeers', () => {
      store.connectToPeer('peer-1');
      store.connectToPeer('peer-2');
      store.connectToPeer('peer-3');
      store.peerConnected('peer-1');
      store.peerConnected('peer-3');

      const connectedPeers = store.getConnectedPeers();

      expect(connectedPeers).toHaveLength(2);
      expect(connectedPeers.map(p => p.id)).toContain('peer-1');
      expect(connectedPeers.map(p => p.id)).toContain('peer-3');
      expect(connectedPeers.map(p => p.id)).not.toContain('peer-2');
    });

    it('should check canReconnectSignaling correctly', () => {
      expect(store.canReconnectSignaling(3)).toBe(true);

      store.connectSignaling('wss://example.com');
      store.disconnectSignaling('Error 1');
      expect(store.canReconnectSignaling(3)).toBe(true);

      store.connectSignaling('wss://example.com');
      store.disconnectSignaling('Error 2');
      expect(store.canReconnectSignaling(3)).toBe(true);

      store.connectSignaling('wss://example.com');
      store.disconnectSignaling('Error 3');
      expect(store.canReconnectSignaling(3)).toBe(false);
    });

    it('should check canReconnectPeer correctly', () => {
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      expect(store.canReconnectPeer('peer-1', 2)).toBe(true);

      store.disconnectPeer('peer-1', 'Error 1');
      expect(store.canReconnectPeer('peer-1', 2)).toBe(true);

      // Second error without successful reconnection - should now be at max attempts
      store.disconnectPeer('peer-1', 'Error 2');
      expect(store.canReconnectPeer('peer-1', 2)).toBe(false);
    });

    it('should return false from canReconnectPeer for non-existent peer', () => {
      expect(store.canReconnectPeer('non-existent', 3)).toBe(false);
    });
  });

  describe('Reset', () => {
    it('should reset store to initial state', () => {
      store.connectSignaling('wss://example.com');
      store.signalingConnected();
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.setActivePeer('peer-1');

      store.reset();

      expect(store.state.signalingStatus).toBe('disconnected');
      expect(store.state.peers).toEqual({});
      expect(store.state.activePeerId).toBeNull();
      expect(store.state.signalingUrl).toBeNull();
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getConnectionStore', () => {
      const store1 = getConnectionStore();
      const store2 = getConnectionStore();

      expect(store1).toBe(store2);
    });

    it('should create new instance after resetConnectionStore', () => {
      const store1 = getConnectionStore();
      resetConnectionStore();
      const store2 = getConnectionStore();

      expect(store1).not.toBe(store2);
    });
  });

  describe('Reactive Updates', () => {
    it('should update peers reactively', () => {
      const initialPeerCount = Object.keys(store.state.peers).length;
      expect(initialPeerCount).toBe(0);

      store.connectToPeer('peer-1');
      const afterAddCount = Object.keys(store.state.peers).length;
      expect(afterAddCount).toBe(1);

      store.connectToPeer('peer-2');
      const afterSecondAdd = Object.keys(store.state.peers).length;
      expect(afterSecondAdd).toBe(2);

      store.removePeer('peer-1');
      const afterRemove = Object.keys(store.state.peers).length;
      expect(afterRemove).toBe(1);
    });

    it('should maintain peer state consistency during rapid updates', () => {
      // Simulate rapid state changes
      store.connectToPeer('peer-1');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1', 'Error');
      store.peerConnected('peer-1');
      store.disconnectPeer('peer-1');
      store.peerConnected('peer-1');

      // Final state should be connected
      expect(store.state.peers['peer-1'].status).toBe('connected');
    });
  });
});
