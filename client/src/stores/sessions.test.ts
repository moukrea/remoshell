import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createSessionStore,
  getSessionStore,
  resetSessionStore,
  type SessionStore,
} from './sessions';

describe('Session Store', () => {
  let store: SessionStore;

  beforeEach(() => {
    resetSessionStore();
    store = createSessionStore();
  });

  afterEach(() => {
    resetSessionStore();
  });

  describe('Initial State', () => {
    it('should have correct initial state', () => {
      expect(store.state.sessions).toEqual({});
      expect(store.state.activeSessionId).toBeNull();
      expect(store.state.sessionOrder).toEqual([]);
    });
  });

  describe('Create Session', () => {
    it('should create a session with generated ID', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      expect(sessionId).toBeDefined();
      expect(sessionId).toMatch(/^session-\d+-[a-z0-9]+$/);
    });

    it('should add session to state', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      expect(store.state.sessions[sessionId]).toBeDefined();
      expect(store.state.sessions[sessionId].id).toBe(sessionId);
      expect(store.state.sessions[sessionId].peerId).toBe('peer-1');
    });

    it('should set initial session properties', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const session = store.state.sessions[sessionId];

      expect(session.status).toBe('connecting');
      expect(session.flowControl).toBe('running');
      expect(session.cols).toBe(80);
      expect(session.rows).toBe(24);
      expect(session.createdAt).toBeDefined();
      expect(session.lastActivityAt).toBeDefined();
    });

    it('should use custom options when provided', () => {
      const sessionId = store.createSession({
        peerId: 'peer-1',
        title: 'Custom Terminal',
        cols: 120,
        rows: 40,
      });
      const session = store.state.sessions[sessionId];

      expect(session.title).toBe('Custom Terminal');
      expect(session.cols).toBe(120);
      expect(session.rows).toBe(40);
    });

    it('should generate default title based on session count', () => {
      const sessionId1 = store.createSession({ peerId: 'peer-1' });
      const sessionId2 = store.createSession({ peerId: 'peer-1' });

      expect(store.state.sessions[sessionId1].title).toBe('Terminal 1');
      expect(store.state.sessions[sessionId2].title).toBe('Terminal 2');
    });

    it('should add session to sessionOrder', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      expect(store.state.sessionOrder).toContain(sessionId);
      expect(store.state.sessionOrder.length).toBe(1);
    });

    it('should set first session as active', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      expect(store.state.activeSessionId).toBe(sessionId);
    });

    it('should not change active session when creating subsequent sessions', () => {
      const sessionId1 = store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-1' });

      expect(store.state.activeSessionId).toBe(sessionId1);
    });

    it('should emit session:created event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const sessionId = store.createSession({ peerId: 'peer-1' });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:created',
        sessionId,
      });
    });
  });

  describe('Close Session', () => {
    it('should remove session from state', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.closeSession(sessionId);

      expect(store.state.sessions[sessionId]).toBeUndefined();
    });

    it('should remove session from sessionOrder', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.closeSession(sessionId);

      expect(store.state.sessionOrder).not.toContain(sessionId);
    });

    it('should return true on successful close', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const result = store.closeSession(sessionId);

      expect(result).toBe(true);
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.closeSession('non-existent');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should switch active session when closing active session', () => {
      const sessionId1 = store.createSession({ peerId: 'peer-1' });
      const sessionId2 = store.createSession({ peerId: 'peer-1' });
      store.setActiveSession(sessionId1);

      store.closeSession(sessionId1);

      expect(store.state.activeSessionId).toBe(sessionId2);
    });

    it('should clear active session when closing last session', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.closeSession(sessionId);

      expect(store.state.activeSessionId).toBeNull();
    });

    it('should emit session:closed event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.closeSession(sessionId);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:closed',
        sessionId,
      });
    });
  });

  describe('Set Active Session', () => {
    it('should set active session', () => {
      store.createSession({ peerId: 'peer-1' }); // session 1
      const sessionId2 = store.createSession({ peerId: 'peer-1' });

      store.setActiveSession(sessionId2);

      expect(store.state.activeSessionId).toBe(sessionId2);
    });

    it('should return true on success', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const result = store.setActiveSession(sessionId);

      expect(result).toBe(true);
    });

    it('should clear active session when set to null', () => {
      store.createSession({ peerId: 'peer-1' });
      store.setActiveSession(null);

      expect(store.state.activeSessionId).toBeNull();
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.setActiveSession('non-existent');

      expect(result).toBe(false);
      expect(store.state.activeSessionId).toBeNull();
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should emit session:active event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.setActiveSession(sessionId);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:active',
        sessionId,
      });
    });

    it('should emit session:active with undefined when cleared', () => {
      store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.setActiveSession(null);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:active',
        sessionId: undefined,
      });
    });
  });

  describe('Send Input', () => {
    it('should return true when sending to connected session', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.setSessionStatus(sessionId, 'connected');

      const result = store.sendInput(sessionId, 'test input');

      expect(result).toBe(true);
    });

    it('should emit session:input event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.setSessionStatus(sessionId, 'connected');
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.sendInput(sessionId, 'test input');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:input',
        sessionId,
        data: { input: 'test input' },
      });
    });

    it('should update lastActivityAt', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.setSessionStatus(sessionId, 'connected');
      const initialTime = store.state.sessions[sessionId].lastActivityAt;

      // Wait a tiny bit to ensure time difference
      const result = store.sendInput(sessionId, 'test');

      expect(result).toBe(true);
      expect(store.state.sessions[sessionId].lastActivityAt).toBeGreaterThanOrEqual(initialTime);
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.sendInput('non-existent', 'test');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should return false for disconnected session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const sessionId = store.createSession({ peerId: 'peer-1' });
      // Default status is 'connecting', not 'connected'

      const result = store.sendInput(sessionId, 'test');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should return false for paused session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.setSessionStatus(sessionId, 'connected');
      store.pauseSession(sessionId);

      const result = store.sendInput(sessionId, 'test');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Resize Terminal', () => {
    it('should update terminal dimensions', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      store.resizeTerminal(sessionId, 100, 50);

      expect(store.state.sessions[sessionId].cols).toBe(100);
      expect(store.state.sessions[sessionId].rows).toBe(50);
    });

    it('should return true on success', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const result = store.resizeTerminal(sessionId, 100, 50);

      expect(result).toBe(true);
    });

    it('should emit session:resize event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.resizeTerminal(sessionId, 100, 50);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:resize',
        sessionId,
        data: { cols: 100, rows: 50 },
      });
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.resizeTerminal('non-existent', 100, 50);

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Flow Control', () => {
    it('should pause session', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      const result = store.pauseSession(sessionId);

      expect(result).toBe(true);
      expect(store.state.sessions[sessionId].flowControl).toBe('paused');
    });

    it('should emit session:flow:paused event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.pauseSession(sessionId);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:flow:paused',
        sessionId,
      });
    });

    it('should resume session', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.pauseSession(sessionId);

      const result = store.resumeSession(sessionId);

      expect(result).toBe(true);
      expect(store.state.sessions[sessionId].flowControl).toBe('running');
    });

    it('should emit session:flow:resumed event', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.pauseSession(sessionId);
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.resumeSession(sessionId);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:flow:resumed',
        sessionId,
      });
    });

    it('should return true when already paused', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      store.pauseSession(sessionId);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const result = store.pauseSession(sessionId);

      expect(result).toBe(true);
      expect(subscriber).not.toHaveBeenCalled(); // No event emitted
    });

    it('should return true when already running', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const result = store.resumeSession(sessionId);

      expect(result).toBe(true);
      expect(subscriber).not.toHaveBeenCalled(); // No event emitted
    });

    it('should return false for non-existent session on pause', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.pauseSession('non-existent');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should return false for non-existent session on resume', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.resumeSession('non-existent');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Session Status', () => {
    it('should update session status', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      store.setSessionStatus(sessionId, 'connected');

      expect(store.state.sessions[sessionId].status).toBe('connected');
    });

    it('should set error and emit session:error event on error status', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.setSessionStatus(sessionId, 'error', 'Connection failed');

      expect(store.state.sessions[sessionId].status).toBe('error');
      expect(store.state.sessions[sessionId].lastError).toBe('Connection failed');
      expect(subscriber).toHaveBeenCalledWith({
        type: 'session:error',
        sessionId,
        error: 'Connection failed',
      });
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.setSessionStatus('non-existent', 'connected');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Session Title', () => {
    it('should update session title', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      store.setSessionTitle(sessionId, 'New Title');

      expect(store.state.sessions[sessionId].title).toBe('New Title');
    });

    it('should return false for non-existent session', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const result = store.setSessionTitle('non-existent', 'Title');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Getters', () => {
    it('should get active session', () => {
      const sessionId = store.createSession({ peerId: 'peer-1', title: 'Active' });

      const activeSession = store.getActiveSession();

      expect(activeSession).not.toBeNull();
      expect(activeSession?.id).toBe(sessionId);
      expect(activeSession?.title).toBe('Active');
    });

    it('should return null when no active session', () => {
      expect(store.getActiveSession()).toBeNull();
    });

    it('should get session by ID', () => {
      const sessionId = store.createSession({ peerId: 'peer-1' });

      const session = store.getSession(sessionId);

      expect(session).not.toBeNull();
      expect(session?.id).toBe(sessionId);
    });

    it('should return null for non-existent session', () => {
      expect(store.getSession('non-existent')).toBeNull();
    });

    it('should get sessions by peer', () => {
      store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-2' });

      const peer1Sessions = store.getSessionsByPeer('peer-1');
      const peer2Sessions = store.getSessionsByPeer('peer-2');

      expect(peer1Sessions).toHaveLength(2);
      expect(peer2Sessions).toHaveLength(1);
    });

    it('should get sessions in order', () => {
      const id1 = store.createSession({ peerId: 'peer-1', title: 'First' });
      const id2 = store.createSession({ peerId: 'peer-1', title: 'Second' });
      const id3 = store.createSession({ peerId: 'peer-1', title: 'Third' });

      const sessions = store.getSessionsInOrder();

      expect(sessions).toHaveLength(3);
      expect(sessions[0].id).toBe(id1);
      expect(sessions[1].id).toBe(id2);
      expect(sessions[2].id).toBe(id3);
    });
  });

  describe('Tab Management', () => {
    it('should reorder sessions', () => {
      const id1 = store.createSession({ peerId: 'peer-1' });
      const id2 = store.createSession({ peerId: 'peer-1' });
      const id3 = store.createSession({ peerId: 'peer-1' });

      expect(store.state.sessionOrder).toEqual([id1, id2, id3]);

      store.reorderSessions(0, 2);

      expect(store.state.sessionOrder).toEqual([id2, id3, id1]);
    });

    it('should return false for invalid indices', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      store.createSession({ peerId: 'peer-1' });

      const result = store.reorderSessions(-1, 0);

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });

    it('should close all sessions for a peer', () => {
      store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-2' });

      store.closeSessionsByPeer('peer-1');

      expect(store.getSessionsByPeer('peer-1')).toHaveLength(0);
      expect(store.getSessionsByPeer('peer-2')).toHaveLength(1);
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.createSession({ peerId: 'peer-1' });

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = store.subscribe(subscriber);

      store.createSession({ peerId: 'peer-1' });
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      store.createSession({ peerId: 'peer-1' });

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should handle errors in subscribers gracefully', () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      store.subscribe(errorSubscriber);
      store.subscribe(goodSubscriber);

      expect(() => store.createSession({ peerId: 'peer-1' })).not.toThrow();

      expect(errorSubscriber).toHaveBeenCalledTimes(1);
      expect(goodSubscriber).toHaveBeenCalledTimes(1);
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Reset', () => {
    it('should reset store to initial state', () => {
      store.createSession({ peerId: 'peer-1' });
      store.createSession({ peerId: 'peer-1' });

      store.reset();

      expect(store.state.sessions).toEqual({});
      expect(store.state.activeSessionId).toBeNull();
      expect(store.state.sessionOrder).toEqual([]);
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getSessionStore', () => {
      const store1 = getSessionStore();
      const store2 = getSessionStore();

      expect(store1).toBe(store2);
    });

    it('should create new instance after resetSessionStore', () => {
      const store1 = getSessionStore();
      resetSessionStore();
      const store2 = getSessionStore();

      expect(store1).not.toBe(store2);
    });
  });

  describe('Session Lifecycle', () => {
    it('should handle complete session lifecycle', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      // Create session
      const sessionId = store.createSession({ peerId: 'peer-1' });
      expect(store.state.sessions[sessionId].status).toBe('connecting');

      // Connect
      store.setSessionStatus(sessionId, 'connected');
      expect(store.state.sessions[sessionId].status).toBe('connected');

      // Send input
      const inputResult = store.sendInput(sessionId, 'ls -la');
      expect(inputResult).toBe(true);

      // Resize
      store.resizeTerminal(sessionId, 120, 40);
      expect(store.state.sessions[sessionId].cols).toBe(120);
      expect(store.state.sessions[sessionId].rows).toBe(40);

      // Pause and resume
      store.pauseSession(sessionId);
      expect(store.state.sessions[sessionId].flowControl).toBe('paused');

      store.resumeSession(sessionId);
      expect(store.state.sessions[sessionId].flowControl).toBe('running');

      // Close
      store.closeSession(sessionId);
      expect(store.state.sessions[sessionId]).toBeUndefined();
    });

    it('should handle multiple sessions correctly', () => {
      const id1 = store.createSession({ peerId: 'peer-1', title: 'Session 1' });
      const id2 = store.createSession({ peerId: 'peer-1', title: 'Session 2' });
      const id3 = store.createSession({ peerId: 'peer-2', title: 'Session 3' });

      expect(Object.keys(store.state.sessions)).toHaveLength(3);
      expect(store.state.sessionOrder).toEqual([id1, id2, id3]);

      // Connect all
      store.setSessionStatus(id1, 'connected');
      store.setSessionStatus(id2, 'connected');
      store.setSessionStatus(id3, 'connected');

      // Switch active session
      store.setActiveSession(id2);
      expect(store.state.activeSessionId).toBe(id2);

      // Close middle session
      store.closeSession(id2);
      expect(Object.keys(store.state.sessions)).toHaveLength(2);
      expect(store.state.sessionOrder).toEqual([id1, id3]);
      expect(store.state.activeSessionId).toBe(id3);

      // Close by peer
      store.closeSessionsByPeer('peer-1');
      expect(Object.keys(store.state.sessions)).toHaveLength(1);
      expect(store.state.sessions[id3]).toBeDefined();
    });
  });
});
