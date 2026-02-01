/**
 * Terminal Data Flow Integration Tests
 *
 * These tests verify the complete terminal data flow from session creation
 * through data transmission and reception.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMockSignalingClient, createMockWebRTCManager, type MockSignalingClient, type MockWebRTCManager } from '../lib/orchestration/__tests__/mocks';
import { resetConnectionStore } from '../stores/connection';
import { resetSessionStore, getSessionStore } from '../stores/sessions';
import { resetConfig } from '../config';
import {
  encodeEnvelope,
  createEnvelope,
  Msg,
} from '../lib/protocol';

// Mock the module imports
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

// Import after mocking
import { ConnectionOrchestrator, resetOrchestrator } from '../lib/orchestration/ConnectionOrchestrator';

describe('Terminal Data Flow Integration', () => {
  let orchestrator: ConnectionOrchestrator;

  beforeEach(async () => {
    vi.useFakeTimers();

    // Create fresh mocks for each test
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();

    // Reset all singletons
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    resetConfig();

    vi.clearAllMocks();

    // Initialize orchestrator and connect
    orchestrator = new ConnectionOrchestrator();
    await orchestrator.initialize();

    // Simulate connection
    mockSignaling._simulateConnect('local-peer-id', []);
    mockSignaling._simulatePeerJoined('remote-peer');
    mockWebRTC._simulateConnect('remote-peer');
  });

  afterEach(() => {
    orchestrator.destroy();
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe('Session Creation', () => {
    it('should create a terminal session and map to peer', () => {
      const sessionStore = getSessionStore();

      // Create a session for the connected peer
      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
        cols: 80,
        rows: 24,
      });

      expect(sessionId).toBeDefined();
      const session = sessionStore.getSession(sessionId);
      expect(session).toBeDefined();
      expect(session?.peerId).toBe('remote-peer');
      expect(session?.status).toBe('connecting');
      expect(session?.cols).toBe(80);
      expect(session?.rows).toBe(24);
    });

    it('should set session as active when first session created', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'First Terminal',
      });

      expect(sessionStore.state.activeSessionId).toBe(sessionId);
    });

    it('should not change active session when additional sessions created', () => {
      const sessionStore = getSessionStore();

      const firstSessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'First Terminal',
      });

      sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Second Terminal',
      });

      expect(sessionStore.state.activeSessionId).toBe(firstSessionId);
    });
  });

  describe('Session Status Updates', () => {
    it('should update session status to connected', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      expect(sessionStore.getSession(sessionId)?.status).toBe('connecting');

      sessionStore.setSessionStatus(sessionId, 'connected');

      expect(sessionStore.getSession(sessionId)?.status).toBe('connected');
    });

    it('should update session status with error', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.setSessionStatus(sessionId, 'error', 'Connection failed');

      const session = sessionStore.getSession(sessionId);
      expect(session?.status).toBe('error');
      expect(session?.lastError).toBe('Connection failed');
    });
  });

  describe('Terminal Input', () => {
    it('should send input through orchestrator', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      // Map session to peer in orchestrator
      orchestrator.setSessionPeer(sessionId, 'remote-peer');

      // Update status to connected so input can be sent
      sessionStore.setSessionStatus(sessionId, 'connected');

      // Send input
      const result = sessionStore.sendInput(sessionId, 'echo hello\n');

      expect(result).toBe(true);
    });

    it('should not send input when session is not connected', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      // Session is still in 'connecting' status
      const result = sessionStore.sendInput(sessionId, 'echo hello\n');

      expect(result).toBe(false);
    });

    it('should not send input when session is paused', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.setSessionStatus(sessionId, 'connected');
      sessionStore.pauseSession(sessionId);

      const result = sessionStore.sendInput(sessionId, 'echo hello\n');

      expect(result).toBe(false);
    });

    it('should send input after resuming paused session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.setSessionStatus(sessionId, 'connected');
      sessionStore.pauseSession(sessionId);
      sessionStore.resumeSession(sessionId);

      const result = sessionStore.sendInput(sessionId, 'echo hello\n');

      expect(result).toBe(true);
    });
  });

  describe('Terminal Output', () => {
    it('should write output to session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.setSessionStatus(sessionId, 'connected');

      // Advance time a bit so lastActivityAt will differ from createdAt
      vi.advanceTimersByTime(100);

      // Write output
      sessionStore.writeOutput(sessionId, 'Hello, World!\n');

      // Verify lastActivityAt was updated
      const session = sessionStore.getSession(sessionId);
      expect(session?.lastActivityAt).toBeGreaterThan(session?.createdAt ?? 0);
    });

    it('should emit output event when output is written', () => {
      const sessionStore = getSessionStore();

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.writeOutput(sessionId, 'Test output');

      // Find the output event
      const outputEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:output'
      );

      expect(outputEvent).toBeDefined();
      expect(outputEvent?.[0].sessionId).toBe(sessionId);
      expect(outputEvent?.[0].data).toEqual({ output: 'Test output' });
    });

    it('should handle output to non-existent session gracefully', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const sessionStore = getSessionStore();

      // Should not throw
      sessionStore.writeOutput('non-existent-session', 'Some output');

      expect(consoleWarn).toHaveBeenCalled();
      consoleWarn.mockRestore();
    });
  });

  describe('Terminal Resize', () => {
    it('should resize terminal session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
        cols: 80,
        rows: 24,
      });

      const result = sessionStore.resizeTerminal(sessionId, 120, 40);

      expect(result).toBe(true);
      const session = sessionStore.getSession(sessionId);
      expect(session?.cols).toBe(120);
      expect(session?.rows).toBe(40);
    });

    it('should emit resize event', () => {
      const sessionStore = getSessionStore();

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.resizeTerminal(sessionId, 100, 50);

      const resizeEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:resize'
      );

      expect(resizeEvent).toBeDefined();
      expect(resizeEvent?.[0].data).toEqual({ cols: 100, rows: 50 });
    });
  });

  describe('Session Close', () => {
    it('should close terminal session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      expect(sessionStore.getSession(sessionId)).toBeDefined();

      const result = sessionStore.closeSession(sessionId);

      expect(result).toBe(true);
      expect(sessionStore.getSession(sessionId)).toBeNull();
    });

    it('should emit close event', () => {
      const sessionStore = getSessionStore();

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.closeSession(sessionId);

      const closeEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:closed'
      );

      expect(closeEvent).toBeDefined();
      expect(closeEvent?.[0].sessionId).toBe(sessionId);
    });

    it('should switch active session when active session is closed', () => {
      const sessionStore = getSessionStore();

      const firstSessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'First Terminal',
      });

      const secondSessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Second Terminal',
      });

      expect(sessionStore.state.activeSessionId).toBe(firstSessionId);

      sessionStore.closeSession(firstSessionId);

      expect(sessionStore.state.activeSessionId).toBe(secondSessionId);
    });

    it('should set active session to null when last session is closed', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Only Terminal',
      });

      sessionStore.closeSession(sessionId);

      expect(sessionStore.state.activeSessionId).toBeNull();
    });
  });

  describe('Peer Disconnect', () => {
    it('should close all sessions when peer disconnects', () => {
      const sessionStore = getSessionStore();

      const session1 = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 1',
      });

      const session2 = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 2',
      });

      expect(sessionStore.getSessionsByPeer('remote-peer').length).toBe(2);

      sessionStore.closeSessionsByPeer('remote-peer');

      expect(sessionStore.getSession(session1)).toBeNull();
      expect(sessionStore.getSession(session2)).toBeNull();
      expect(sessionStore.getSessionsByPeer('remote-peer').length).toBe(0);
    });

    it('should only close sessions for disconnecting peer', () => {
      const sessionStore = getSessionStore();

      // Add another peer
      mockSignaling._simulatePeerJoined('other-peer');
      mockWebRTC._simulateConnect('other-peer');

      sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 1',
      });

      const otherSession = sessionStore.createSession({
        peerId: 'other-peer',
        title: 'Other Terminal',
      });

      sessionStore.closeSessionsByPeer('remote-peer');

      // Other peer's session should still exist
      expect(sessionStore.getSession(otherSession)).toBeDefined();
    });
  });

  describe('Protocol Message Flow', () => {
    it('should handle incoming SessionData (stdout) from remote peer', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      orchestrator.setSessionPeer(sessionId, 'remote-peer');
      sessionStore.setSessionStatus(sessionId, 'connected');

      // Create a SessionData message as if from daemon
      const outputText = 'Hello from daemon!\n';
      const message = Msg.SessionData({
        session_id: sessionId,
        stream: 'Stdout',
        data: new TextEncoder().encode(outputText),
      });

      const envelope = createEnvelope(1, message);
      const encoded = encodeEnvelope(envelope);

      // Subscribe to session events
      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      // Simulate receiving data on terminal channel
      mockWebRTC._simulateData('remote-peer', encoded, 'terminal');

      // The orchestrator should have written output to the session
      const outputEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:output'
      );

      expect(outputEvent).toBeDefined();
      expect(outputEvent?.[0].data).toEqual({ output: outputText });
    });

    it('should handle incoming SessionData (stderr) from remote peer', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      orchestrator.setSessionPeer(sessionId, 'remote-peer');
      sessionStore.setSessionStatus(sessionId, 'connected');

      // Create a SessionData message with stderr
      const errorText = 'Error: something went wrong\n';
      const message = Msg.SessionData({
        session_id: sessionId,
        stream: 'Stderr',
        data: new TextEncoder().encode(errorText),
      });

      const envelope = createEnvelope(1, message);
      const encoded = encodeEnvelope(envelope);

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      mockWebRTC._simulateData('remote-peer', encoded, 'terminal');

      const outputEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:output'
      );

      expect(outputEvent).toBeDefined();
      expect(outputEvent?.[0].data).toEqual({ output: errorText });
    });

    it('should handle incoming SessionClosed from remote peer', () => {
      const consoleLog = vi.spyOn(console, 'log').mockImplementation(() => {});
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      orchestrator.setSessionPeer(sessionId, 'remote-peer');
      sessionStore.setSessionStatus(sessionId, 'connected');

      // Create a SessionClosed message
      const message = Msg.SessionClosed({
        session_id: sessionId,
        exit_code: 0,
        signal: null,
        reason: 'Process exited',
      });

      const envelope = createEnvelope(1, message);
      const encoded = encodeEnvelope(envelope);

      mockWebRTC._simulateData('remote-peer', encoded, 'terminal');

      // Session should be marked as disconnected
      const session = sessionStore.getSession(sessionId);
      expect(session?.status).toBe('disconnected');

      consoleLog.mockRestore();
    });

    it('should ignore stdin data from remote (daemon should not send stdin)', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      orchestrator.setSessionPeer(sessionId, 'remote-peer');
      sessionStore.setSessionStatus(sessionId, 'connected');

      // Create a SessionData message with stdin (which should be ignored)
      const message = Msg.SessionData({
        session_id: sessionId,
        stream: 'Stdin',
        data: new TextEncoder().encode('ignored input'),
      });

      const envelope = createEnvelope(1, message);
      const encoded = encodeEnvelope(envelope);

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      mockWebRTC._simulateData('remote-peer', encoded, 'terminal');

      // Should NOT write output for stdin
      const outputEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:output'
      );

      expect(outputEvent).toBeUndefined();
    });
  });

  describe('Flow Control', () => {
    it('should pause session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      const result = sessionStore.pauseSession(sessionId);

      expect(result).toBe(true);
      expect(sessionStore.getSession(sessionId)?.flowControl).toBe('paused');
    });

    it('should resume session', () => {
      const sessionStore = getSessionStore();

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.pauseSession(sessionId);
      const result = sessionStore.resumeSession(sessionId);

      expect(result).toBe(true);
      expect(sessionStore.getSession(sessionId)?.flowControl).toBe('running');
    });

    it('should emit pause/resume events', () => {
      const sessionStore = getSessionStore();

      const eventHandler = vi.fn();
      sessionStore.subscribe(eventHandler);

      const sessionId = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Test Terminal',
      });

      sessionStore.pauseSession(sessionId);
      sessionStore.resumeSession(sessionId);

      const pauseEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:flow:paused'
      );
      const resumeEvent = eventHandler.mock.calls.find(
        (call) => call[0].type === 'session:flow:resumed'
      );

      expect(pauseEvent).toBeDefined();
      expect(resumeEvent).toBeDefined();
    });
  });

  describe('Tab Management', () => {
    it('should reorder sessions', () => {
      const sessionStore = getSessionStore();

      const session1 = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 1',
      });

      const session2 = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 2',
      });

      const session3 = sessionStore.createSession({
        peerId: 'remote-peer',
        title: 'Terminal 3',
      });

      expect(sessionStore.state.sessionOrder).toEqual([session1, session2, session3]);

      sessionStore.reorderSessions(0, 2);

      expect(sessionStore.state.sessionOrder).toEqual([session2, session3, session1]);
    });

    it('should get sessions in order', () => {
      const sessionStore = getSessionStore();

      sessionStore.createSession({ peerId: 'remote-peer', title: 'Terminal 1' });
      sessionStore.createSession({ peerId: 'remote-peer', title: 'Terminal 2' });
      sessionStore.createSession({ peerId: 'remote-peer', title: 'Terminal 3' });

      const orderedSessions = sessionStore.getSessionsInOrder();

      expect(orderedSessions.length).toBe(3);
      expect(orderedSessions[0].title).toBe('Terminal 1');
      expect(orderedSessions[1].title).toBe('Terminal 2');
      expect(orderedSessions[2].title).toBe('Terminal 3');
    });
  });

  describe('Error Handling', () => {
    it('should handle malformed terminal data gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      // Send garbage data on terminal channel
      const garbageData = new Uint8Array([0xff, 0xfe, 0x00, 0x01]);
      mockWebRTC._simulateData('remote-peer', garbageData, 'terminal');

      // Should not throw, just log error
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });

    it('should handle terminal data for unknown session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      // Create a message for non-existent session
      const message = Msg.SessionData({
        session_id: 'non-existent-session',
        stream: 'Stdout',
        data: new TextEncoder().encode('orphaned output'),
      });

      const envelope = createEnvelope(1, message);
      const encoded = encodeEnvelope(envelope);

      // Should not throw
      mockWebRTC._simulateData('remote-peer', encoded, 'terminal');

      consoleWarn.mockRestore();
    });
  });
});
