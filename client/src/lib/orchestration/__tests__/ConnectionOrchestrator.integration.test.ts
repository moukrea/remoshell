/**
 * ConnectionOrchestrator Integration Tests
 *
 * These tests verify the complete connection flow works end-to-end
 * with mocked signaling and WebRTC components.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMockSignalingClient, createMockWebRTCManager, type MockSignalingClient, type MockWebRTCManager } from './mocks';
import { resetConnectionStore, getConnectionStore } from '../../../stores/connection';
import { resetSessionStore } from '../../../stores/sessions';
import { resetConfig } from '../../../config';

// We need to mock the module imports before importing the orchestrator
let mockSignaling: MockSignalingClient;
let mockWebRTC: MockWebRTCManager;

vi.mock('../../signaling/SignalingClient', () => ({
  getSignalingClient: vi.fn(() => mockSignaling),
  resetSignalingClient: vi.fn(),
}));

vi.mock('../../webrtc/WebRTCManager', () => ({
  getWebRTCManager: vi.fn(() => mockWebRTC),
  resetWebRTCManager: vi.fn(),
}));

// Import after mocking
import { ConnectionOrchestrator, resetOrchestrator } from '../ConnectionOrchestrator';
import { getSignalingClient } from '../../signaling/SignalingClient';
import { getWebRTCManager } from '../../webrtc/WebRTCManager';

describe('ConnectionOrchestrator Integration', () => {
  beforeEach(() => {
    vi.useFakeTimers();

    // Create fresh mocks for each test
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();

    // Reset all singletons
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    resetConfig();

    // Clear mock call history
    vi.clearAllMocks();
  });

  afterEach(() => {
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe('Initialization', () => {
    it('should initialize and wire all components', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      // Verify signaling client was obtained
      expect(getSignalingClient).toHaveBeenCalled();

      // Verify WebRTC manager was obtained
      expect(getWebRTCManager).toHaveBeenCalled();

      // Verify orchestrator is initialized
      expect(orchestrator.isInitialized()).toBe(true);
      expect(orchestrator.getState()).toBe('initialized');
    });

    it('should subscribe to signaling events', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      // Verify subscription was made
      expect(mockSignaling.subscribe).toHaveBeenCalled();
      expect(mockSignaling._getSubscriberCount()).toBeGreaterThan(0);
    });

    it('should subscribe to WebRTC events', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      // Verify subscription was made
      expect(mockWebRTC.subscribe).toHaveBeenCalled();
      expect(mockWebRTC._getSubscriberCount()).toBeGreaterThan(0);
    });

    it('should not reinitialize if already initialized', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      const subscriberCount = mockSignaling._getSubscriberCount();

      // Call initialize again
      await orchestrator.initialize();

      // Should not add more subscribers
      expect(mockSignaling._getSubscriberCount()).toBe(subscriberCount);
    });
  });

  describe('Signaling Connection', () => {
    it('should update connection store on signaling connected', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      const store = getConnectionStore();

      // Simulate signaling connected event
      mockSignaling._simulateConnect('my-peer-id', []);

      expect(store.state.signalingStatus).toBe('connected');
      expect(orchestrator.getState()).toBe('connected');
    });

    it('should update connection store on signaling disconnected', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      const store = getConnectionStore();

      // First connect
      mockSignaling._simulateConnect('my-peer-id', []);
      expect(store.state.signalingStatus).toBe('connected');

      // Then disconnect
      mockSignaling._simulateDisconnect('Connection lost');

      expect(store.state.signalingStatus).toBe('disconnected');
      expect(orchestrator.getState()).toBe('disconnected');
    });

    it('should connect to existing peers in room', async () => {
      const orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();

      const store = getConnectionStore();

      // Simulate joining room with existing peers
      mockSignaling._simulateConnect('my-peer-id', ['peer-1', 'peer-2']);

      // Should create connections to both existing peers
      expect(mockWebRTC.createConnection).toHaveBeenCalledTimes(2);
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'peer-1', initiator: true })
      );
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({ peerId: 'peer-2', initiator: true })
      );

      // Peers should be in store
      expect(store.state.peers['peer-1']).toBeDefined();
      expect(store.state.peers['peer-2']).toBeDefined();
    });
  });

  describe('Peer Events', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      mockSignaling._simulateConnect('my-peer-id', []);
    });

    it('should create WebRTC connection when peer joins', async () => {
      // Simulate peer joining
      mockSignaling._simulatePeerJoined('new-peer');

      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({
          peerId: 'new-peer',
          initiator: true,
        })
      );
    });

    it('should update store when peer joins', async () => {
      const store = getConnectionStore();

      mockSignaling._simulatePeerJoined('new-peer');

      expect(store.state.peers['new-peer']).toBeDefined();
      expect(store.state.peers['new-peer'].status).toBe('connecting');
    });

    it('should cleanup when peer leaves', async () => {
      const store = getConnectionStore();

      // First add peer
      mockSignaling._simulatePeerJoined('leaving-peer');
      expect(store.state.peers['leaving-peer']).toBeDefined();

      // Then peer leaves
      mockSignaling._simulatePeerLeft('leaving-peer');

      expect(mockWebRTC.destroyConnection).toHaveBeenCalledWith('leaving-peer');
    });

    it('should update store when WebRTC peer connects', async () => {
      const store = getConnectionStore();

      // Peer joins and creates WebRTC connection
      mockSignaling._simulatePeerJoined('connecting-peer');

      // WebRTC connection established
      mockWebRTC._simulateConnect('connecting-peer');

      expect(store.state.peers['connecting-peer'].status).toBe('connected');
    });

    it('should update store when WebRTC peer disconnects', async () => {
      const store = getConnectionStore();

      // Setup connected peer
      mockSignaling._simulatePeerJoined('disconnecting-peer');
      mockWebRTC._simulateConnect('disconnecting-peer');
      expect(store.state.peers['disconnecting-peer'].status).toBe('connected');

      // Peer disconnects
      mockWebRTC._simulateDisconnect('disconnecting-peer');

      expect(store.state.peers['disconnecting-peer'].status).toBe('disconnected');
    });

    it('should handle peer error', async () => {
      const store = getConnectionStore();

      // Setup peer
      mockSignaling._simulatePeerJoined('error-peer');

      // Error occurs
      mockWebRTC._simulateError('error-peer', new Error('Connection failed'));

      expect(store.state.peers['error-peer'].status).toBe('failed');
    });
  });

  describe('WebRTC Signaling Relay', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      mockSignaling._simulateConnect('my-peer-id', []);
    });

    it('should relay local offer to signaling server', async () => {
      // Peer joins triggering offer creation
      mockSignaling._simulatePeerJoined('remote-peer');

      // Simulate WebRTC generating an offer
      const offer: RTCSessionDescriptionInit = { type: 'offer', sdp: 'test-offer-sdp' };
      mockWebRTC._simulateSignal('remote-peer', offer);

      expect(mockSignaling.sendOffer).toHaveBeenCalledWith(offer);
    });

    it('should relay local answer to signaling server', async () => {
      // Receive offer from remote peer
      const offer: RTCSessionDescriptionInit = { type: 'offer', sdp: 'remote-offer-sdp' };
      mockSignaling._simulateOffer('remote-peer', offer);

      // Simulate WebRTC generating an answer
      const answer: RTCSessionDescriptionInit = { type: 'answer', sdp: 'test-answer-sdp' };
      mockWebRTC._simulateSignal('remote-peer', answer);

      expect(mockSignaling.sendAnswer).toHaveBeenCalledWith(answer);
    });

    it('should relay local ICE candidate to signaling server', async () => {
      mockSignaling._simulatePeerJoined('remote-peer');

      // Simulate WebRTC generating ICE candidate
      // Note: WebRTC signal data for ICE comes wrapped as { candidate: RTCIceCandidateInit }
      const candidate = {
        candidate: 'test-ice-candidate',
        sdpMLineIndex: 0,
      };
      mockWebRTC._simulateSignal('remote-peer', { candidate });

      // The orchestrator passes the wrapper object { candidate: ... } to sendIceCandidate
      // This matches the current behavior in ConnectionOrchestrator.handleLocalSignal
      expect(mockSignaling.sendIceCandidate).toHaveBeenCalledWith({ candidate });
    });

    it('should handle incoming offer by creating responder connection', async () => {
      const offer: RTCSessionDescriptionInit = { type: 'offer', sdp: 'remote-offer-sdp' };
      mockSignaling._simulateOffer('remote-peer', offer);

      // Should create connection as responder
      expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
        expect.objectContaining({
          peerId: 'remote-peer',
          initiator: false,
        })
      );

      // Should signal the offer to WebRTC
      expect(mockWebRTC.signal).toHaveBeenCalledWith('remote-peer', offer);
    });

    it('should handle incoming answer', async () => {
      // First create outgoing connection
      mockSignaling._simulatePeerJoined('remote-peer');

      // Receive answer
      const answer: RTCSessionDescriptionInit = { type: 'answer', sdp: 'remote-answer-sdp' };
      mockSignaling._simulateAnswer('remote-peer', answer);

      expect(mockWebRTC.signal).toHaveBeenCalledWith('remote-peer', answer);
    });

    it('should handle incoming ICE candidate', async () => {
      // First create connection
      mockSignaling._simulatePeerJoined('remote-peer');

      // Receive ICE candidate
      const candidate = {
        candidate: 'remote-ice-candidate',
        sdpMLineIndex: 0,
      };
      mockSignaling._simulateIceCandidate('remote-peer', candidate);

      expect(mockWebRTC.signal).toHaveBeenCalledWith('remote-peer', { candidate });
    });
  });

  describe('Data Handling', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      mockSignaling._simulateConnect('my-peer-id', []);
      mockSignaling._simulatePeerJoined('data-peer');
      mockWebRTC._simulateConnect('data-peer');
    });

    it('should register and call data handlers', async () => {
      const handler = vi.fn();
      orchestrator.onData(handler);

      const testData = new Uint8Array([1, 2, 3, 4]);
      mockWebRTC._simulateData('data-peer', testData, 'control');

      expect(handler).toHaveBeenCalledWith('data-peer', testData, 'control');
    });

    it('should unregister data handler', async () => {
      const handler = vi.fn();
      const unsubscribe = orchestrator.onData(handler);

      unsubscribe();

      const testData = new Uint8Array([1, 2, 3, 4]);
      mockWebRTC._simulateData('data-peer', testData, 'control');

      expect(handler).not.toHaveBeenCalled();
    });

    it('should handle multiple data handlers', async () => {
      const handler1 = vi.fn();
      const handler2 = vi.fn();

      orchestrator.onData(handler1);
      orchestrator.onData(handler2);

      const testData = new Uint8Array([1, 2, 3, 4]);
      mockWebRTC._simulateData('data-peer', testData, 'control');

      expect(handler1).toHaveBeenCalled();
      expect(handler2).toHaveBeenCalled();
    });

    it('should send data through WebRTC', async () => {
      const testData = new Uint8Array([5, 6, 7, 8]);
      const result = orchestrator.sendData('data-peer', testData, 'terminal');

      expect(mockWebRTC.sendData).toHaveBeenCalledWith('data-peer', testData, 'terminal');
      expect(result).toBe(true);
    });

    it('should return false when sending to non-connected peer', async () => {
      const testData = new Uint8Array([5, 6, 7, 8]);
      const result = orchestrator.sendData('non-existent-peer', testData);

      expect(result).toBe(false);
    });
  });

  describe('Session-Peer Mapping', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
    });

    it('should map session to peer', () => {
      orchestrator.setSessionPeer('session-1', 'peer-1');
      expect(orchestrator.getSessionPeer('session-1')).toBe('peer-1');
    });

    it('should remove session mapping', () => {
      orchestrator.setSessionPeer('session-1', 'peer-1');
      orchestrator.removeSessionPeer('session-1');
      expect(orchestrator.getSessionPeer('session-1')).toBeUndefined();
    });

    it('should clear session mappings when peer leaves', async () => {
      mockSignaling._simulateConnect('my-peer-id', []);
      mockSignaling._simulatePeerJoined('mapped-peer');

      orchestrator.setSessionPeer('session-1', 'mapped-peer');
      orchestrator.setSessionPeer('session-2', 'mapped-peer');

      // Peer leaves
      mockSignaling._simulatePeerLeft('mapped-peer');

      expect(orchestrator.getSessionPeer('session-1')).toBeUndefined();
      expect(orchestrator.getSessionPeer('session-2')).toBeUndefined();
    });
  });

  describe('Disconnect and Cleanup', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
      mockSignaling._simulateConnect('my-peer-id', []);
      mockSignaling._simulatePeerJoined('peer-1');
      mockWebRTC._simulateConnect('peer-1');
    });

    it('should disconnect all peers on orchestrator disconnect', async () => {
      await orchestrator.disconnect();

      expect(mockWebRTC.destroyAll).toHaveBeenCalled();
      expect(mockSignaling.leave).toHaveBeenCalled();
    });

    it('should update state on disconnect', async () => {
      await orchestrator.disconnect();

      expect(orchestrator.getState()).toBe('disconnected');
    });

    it('should clear session mappings on disconnect', async () => {
      orchestrator.setSessionPeer('session-1', 'peer-1');

      await orchestrator.disconnect();

      expect(orchestrator.getSessionPeer('session-1')).toBeUndefined();
    });

    it('should cleanup on destroy', () => {
      orchestrator.setSessionPeer('session-1', 'peer-1');

      orchestrator.destroy();

      expect(orchestrator.isInitialized()).toBe(false);
      expect(orchestrator.getState()).toBe('idle');
      expect(orchestrator.getSessionPeer('session-1')).toBeUndefined();
    });
  });

  describe('Error Handling', () => {
    let orchestrator: ConnectionOrchestrator;

    beforeEach(async () => {
      orchestrator = new ConnectionOrchestrator();
      await orchestrator.initialize();
    });

    it('should throw if connect called before initialize', async () => {
      const freshOrchestrator = new ConnectionOrchestrator();

      await expect(freshOrchestrator.connect('test-room')).rejects.toThrow(
        'Orchestrator not initialized'
      );
    });

    it('should handle signaling error gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      mockSignaling._simulateConnect('my-peer-id', []);
      mockSignaling._simulateError('Connection failed');

      // Should not throw, error is logged
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });

    it('should handle data handler errors gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      mockSignaling._simulateConnect('my-peer-id', []);
      mockSignaling._simulatePeerJoined('data-peer');
      mockWebRTC._simulateConnect('data-peer');

      // Add handler that throws
      const errorHandler = vi.fn(() => {
        throw new Error('Handler error');
      });
      const goodHandler = vi.fn();

      orchestrator.onData(errorHandler);
      orchestrator.onData(goodHandler);

      // Send data
      mockWebRTC._simulateData('data-peer', new Uint8Array([1, 2, 3]), 'control');

      // Both should be called, error should be caught
      expect(errorHandler).toHaveBeenCalled();
      expect(goodHandler).toHaveBeenCalled();
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });
});
