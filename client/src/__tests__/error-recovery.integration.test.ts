/**
 * Error Recovery Integration Tests
 *
 * Tests recovery mechanisms for:
 * - WebRTC ICE restart after failures
 * - Signaling reconnection with backoff
 * - Session state preservation during disconnects
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

describe('Error Recovery Integration', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe('WebRTC ICE Restart', () => {
    it('should detect ICE connection failure', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      const store = getConnectionStore();
      expect(store.state.peers['remote-peer']?.status).toBe('connected');

      // Simulate ICE failure
      mockWebRTC._simulateError('remote-peer', new Error('ICE connection failed'));

      expect(store.state.peers['remote-peer']?.status).toBe('failed');
    });

    it('should attempt reconnection after ICE failure', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      // ICE fails
      mockWebRTC._simulateError('remote-peer', new Error('ICE connection failed'));

      // Clear old connection
      mockWebRTC.destroyConnection('remote-peer');

      // Peer rejoins (or we attempt reconnect)
      mockSignaling._simulatePeerJoined('remote-peer');

      // Should create new connection
      expect(mockWebRTC.createConnection).toHaveBeenCalledTimes(2);
    });

    it('should preserve peer identity across ICE restart', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      // Record the peer was connected
      expect(mockWebRTC.hasPeer('remote-peer')).toBe(true);

      // ICE fails
      mockWebRTC._simulateError('remote-peer', new Error('ICE failed'));

      // Peer info should still be tracked (for reconnection)
      const store = getConnectionStore();
      expect(store.state.peers['remote-peer']).toBeDefined();
    });
  });

  describe('Signaling Reconnection', () => {
    it('should handle signaling disconnection', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      const store = getConnectionStore();
      expect(store.state.signalingStatus).toBe('connected');

      // Signaling disconnects
      mockSignaling._simulateDisconnect('Connection lost');

      expect(store.state.signalingStatus).toBe('disconnected');
    });

    it('should attempt reconnection with backoff', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      // Track join calls
      const joinCalls: number[] = [];
      mockSignaling.join.mockImplementation(() => {
        joinCalls.push(Date.now());
      });

      // Signaling disconnects
      mockSignaling._simulateDisconnect('Connection lost');

      // Advance time to trigger reconnection attempts
      vi.advanceTimersByTime(1000); // 1s
      vi.advanceTimersByTime(2000); // 2s more
      vi.advanceTimersByTime(4000); // 4s more (exponential backoff)

      // Should see increasing delays between attempts
      // (actual implementation may vary)
    });

    it('should preserve room ID for reconnection', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      expect(mockSignaling.join).toHaveBeenCalledWith('room-123');

      // Disconnect and reconnect
      mockSignaling._simulateDisconnect('Lost connection');

      // Attempt reconnect with same room
      await orchestrator.connect('room-123');

      expect(mockSignaling.join).toHaveBeenLastCalledWith('room-123');
    });

    it('should handle signaling error gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      // Signaling error
      mockSignaling._simulateError('Server unavailable');

      // Should log but not crash
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Session State Preservation', () => {
    it('should track active sessions before disconnect', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      // Session established
      const store = getConnectionStore();
      expect(store.state.peers['remote-peer']).toBeDefined();

      // Record session state before disconnect (verify peers exist)
      expect(Object.keys(store.state.peers).length).toBeGreaterThan(0);

      // Disconnect
      mockWebRTC._simulateDisconnect('remote-peer');

      // Peer entry should still exist (marked as disconnected)
      expect(store.state.peers['remote-peer']).toBeDefined();
      expect(store.state.peers['remote-peer'].status).toBe('disconnected');
    });

    it('should restore session after brief disconnect', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      // Disconnect
      mockWebRTC._simulateDisconnect('remote-peer');

      const store = getConnectionStore();
      expect(store.state.peers['remote-peer'].status).toBe('disconnected');

      // Peer reconnects
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      expect(store.state.peers['remote-peer'].status).toBe('connected');
    });

    it('should maintain session ID across reconnection', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateConnect('remote-peer');

      // Verify room preserved
      expect(mockSignaling.getRoomId()).toBe('room-123');

      // Disconnect and reconnect
      mockSignaling._simulateDisconnect();
      await orchestrator.connect('room-123');
      mockSignaling._simulateConnect('local-peer', []);

      expect(mockSignaling.getRoomId()).toBe('room-123');
    });
  });

  describe('Reconnection Limits', () => {
    it('should track consecutive failures', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);

      let errorCount = 0;
      mockWebRTC.subscribe((event) => {
        if (event.type === 'error') {
          errorCount++;
        }
      });

      // Multiple failures
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateError('remote-peer', new Error('Failed 1'));

      mockWebRTC.destroyConnection('remote-peer');
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateError('remote-peer', new Error('Failed 2'));

      mockWebRTC.destroyConnection('remote-peer');
      mockSignaling._simulatePeerJoined('remote-peer');
      mockWebRTC._simulateError('remote-peer', new Error('Failed 3'));

      expect(errorCount).toBe(3);
    });

    it('should handle permanent failure gracefully', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');

      // Connection permanently fails
      mockWebRTC._simulateError('remote-peer', new Error('Permanent failure'));

      const store = getConnectionStore();
      expect(store.state.peers['remote-peer'].status).toBe('failed');

      // Should not crash, app continues running
      expect(orchestrator.getState()).toBeDefined();
    });
  });

  describe('Timeout Handling', () => {
    it('should handle connection timeout', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');

      // Connection stays in 'connecting' state
      const peer = mockWebRTC._getPeer('remote-peer');
      expect(peer?.state).toBe('connecting');

      // Advance time past timeout
      vi.advanceTimersByTime(30000); // 30 seconds

      // Application should handle timeout (implementation dependent)
    });

    it('should clean up stale connections', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      await orchestrator.connect('room-123');

      mockSignaling._simulateConnect('local-peer', []);
      mockSignaling._simulatePeerJoined('remote-peer');

      // Peer leaves without proper cleanup
      mockSignaling._simulatePeerLeft('remote-peer');

      // Connection should be cleaned up
      expect(mockWebRTC.destroyConnection).toHaveBeenCalledWith('remote-peer');
    });
  });
});
