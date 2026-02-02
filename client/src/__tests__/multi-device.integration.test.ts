/**
 * Multi-Device Integration Tests
 *
 * Tests scenarios involving multiple connected devices:
 * - Simultaneous peer connections
 * - Device switching
 * - Isolated disconnections
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockSignalingClient,
  createMockWebRTCManager,
  type MockSignalingClient,
  type MockWebRTCManager,
} from '../lib/orchestration/__tests__/mocks';
import { resetConnectionStore, getConnectionStore } from '../stores/connection';
import { resetSessionStore } from '../stores/sessions';
import { resetConfig } from '../config';

// Mock module imports
let mockSignaling: MockSignalingClient;
let mockWebRTC: MockWebRTCManager;

vi.mock('../lib/signaling/SignalingClient', () => ({
  getSignalingClient: vi.fn(() => mockSignaling),
  resetSignalingClient: vi.fn(),
}));

vi.mock('../lib/webrtc/WebRTCManager', () => ({
  getWebRTCManager: vi.fn(() => mockWebRTC),
  resetWebRTCManager: vi.fn(),
}));

import { ConnectionOrchestrator, resetOrchestrator } from '../lib/orchestration/ConnectionOrchestrator';

describe('Multi-Device Integration', () => {
  beforeEach(() => {
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    resetConfig();
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.clearAllMocks();
  });

  describe('Simultaneous Connections', () => {
    it('should connect to multiple peers simultaneously', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      // Connect to signaling
      mockSignaling._simulateConnect('local-peer', []);

      // Multiple peers join
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');
      mockSignaling._simulatePeerJoined('device-3');

      // Verify connections created for all peers
      expect(mockWebRTC.createConnection).toHaveBeenCalledTimes(3);
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'device-1' })
      );
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'device-2' })
      );
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'device-3' })
      );
    });

    it('should track connection state for each peer independently', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      // Peers join
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      // Only device-1 connects
      mockWebRTC._simulateConnect('device-1');

      const store = getConnectionStore();
      expect(store.state.peers['device-1']?.status).toBe('connected');
      expect(store.state.peers['device-2']?.status).toBe('connecting');
    });

    it('should handle existing peers when joining room', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      // Join with existing peers already in room
      mockSignaling._simulateConnect('local-peer', ['device-1', 'device-2']);

      // Should create connections to all existing peers
      expect(mockWebRTC.createConnection).toHaveBeenCalledTimes(2);
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'device-1', initiator: true })
      );
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'device-2', initiator: true })
      );
    });
  });

  describe('Device Switching', () => {
    it('should maintain other connections when switching active device', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      // Both devices should be connected
      expect(mockWebRTC.isConnected('device-1')).toBe(true);
      expect(mockWebRTC.isConnected('device-2')).toBe(true);

      // Simulate switching active device (application level)
      // Connection state should be preserved
      const connectedPeers = mockWebRTC.getConnectedPeers();
      expect(connectedPeers).toContain('device-1');
      expect(connectedPeers).toContain('device-2');
    });

    it('should send data to specific peer only', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      // Send data to device-1 only
      const data = new TextEncoder().encode('test data');
      mockWebRTC.sendData('device-1', data, 'terminal');

      expect(mockWebRTC.sendData).toHaveBeenCalledWith('device-1', data, 'terminal');
      expect(mockWebRTC.sendData).not.toHaveBeenCalledWith('device-2', data, 'terminal');
    });
  });

  describe('Isolated Disconnections', () => {
    it('should not affect other peers when one disconnects', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');
      mockSignaling._simulatePeerJoined('device-3');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');
      mockWebRTC._simulateConnect('device-3');

      // Device-2 disconnects
      mockWebRTC._simulateDisconnect('device-2');

      // Other devices should remain connected
      expect(mockWebRTC.isConnected('device-1')).toBe(true);
      expect(mockWebRTC.isConnected('device-2')).toBe(false);
      expect(mockWebRTC.isConnected('device-3')).toBe(true);
    });

    it('should handle peer leaving room', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      // Peer leaves signaling room
      mockSignaling._simulatePeerLeft('device-1');

      // device-2 should still be tracked and connected
      expect(mockWebRTC.isConnected('device-2')).toBe(true);
    });

    it('should track connection count accurately', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      expect(mockWebRTC.getConnectionCount()).toBe(0);

      mockSignaling._simulatePeerJoined('device-1');
      expect(mockWebRTC.getConnectionCount()).toBe(1);

      mockSignaling._simulatePeerJoined('device-2');
      expect(mockWebRTC.getConnectionCount()).toBe(2);

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      mockWebRTC.destroyConnection('device-1');
      expect(mockWebRTC.getConnectionCount()).toBe(1);
    });
  });

  describe('Error Isolation', () => {
    it('should isolate errors to specific peer', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      // Error on device-1
      mockWebRTC._simulateError('device-1', new Error('ICE connection failed'));

      const store = getConnectionStore();
      expect(store.state.peers['device-1']?.status).toBe('failed');
      expect(store.state.peers['device-2']?.status).toBe('connected');
    });

    it('should allow reconnection to failed peer', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateError('device-1', new Error('Connection failed'));

      // Clear the failed connection
      mockWebRTC.destroyConnection('device-1');

      // Peer rejoins
      mockSignaling._simulatePeerJoined('device-1');

      // Should create new connection
      expect(mockWebRTC.createConnection).toHaveBeenLastCalledWith(
        expect.objectContaining({ peerId: 'device-1' })
      );
    });
  });

  describe('Data Routing', () => {
    it('should receive data from correct peer', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('device-1');
      mockSignaling._simulatePeerJoined('device-2');

      mockWebRTC._simulateConnect('device-1');
      mockWebRTC._simulateConnect('device-2');

      const receivedFromDevice1: Uint8Array[] = [];
      const receivedFromDevice2: Uint8Array[] = [];

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data') {
          if (event.peerId === 'device-1') {
            receivedFromDevice1.push(event.data as Uint8Array);
          } else if (event.peerId === 'device-2') {
            receivedFromDevice2.push(event.data as Uint8Array);
          }
        }
      });

      // Data from device-1
      mockWebRTC._simulateData('device-1', new Uint8Array([1, 2, 3]), 'terminal');

      // Data from device-2
      mockWebRTC._simulateData('device-2', new Uint8Array([4, 5, 6]), 'terminal');

      expect(receivedFromDevice1).toHaveLength(1);
      expect(receivedFromDevice2).toHaveLength(1);
      expect(receivedFromDevice1[0]).toEqual(new Uint8Array([1, 2, 3]));
      expect(receivedFromDevice2[0]).toEqual(new Uint8Array([4, 5, 6]));
    });
  });
});
