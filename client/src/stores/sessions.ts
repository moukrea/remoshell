import { createStore, produce } from 'solid-js/store';
import { createSignal, batch } from 'solid-js';

/**
 * Flow control state for a terminal session
 */
export type FlowControlState = 'running' | 'paused';

/**
 * Terminal session status
 */
export type SessionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

/**
 * Represents a terminal session
 */
export interface TerminalSession {
  id: string;
  peerId: string;
  status: SessionStatus;
  title: string;
  flowControl: FlowControlState;
  cols: number;
  rows: number;
  createdAt: number;
  lastActivityAt: number;
  lastError?: string;
}

/**
 * Session store state
 */
export interface SessionState {
  sessions: Record<string, TerminalSession>;
  activeSessionId: string | null;
  sessionOrder: string[];
}

/**
 * Event types emitted by the session store
 */
export type SessionEventType =
  | 'session:created'
  | 'session:closed'
  | 'session:active'
  | 'session:input'
  | 'session:resize'
  | 'session:flow:paused'
  | 'session:flow:resumed'
  | 'session:error';

/**
 * Event payload types
 */
export interface SessionEvent {
  type: SessionEventType;
  sessionId?: string;
  data?: unknown;
  error?: string;
}

/**
 * Event subscriber callback type
 */
export type SessionEventSubscriber = (event: SessionEvent) => void;

/**
 * Options for creating a new session
 */
export interface CreateSessionOptions {
  peerId: string;
  title?: string;
  cols?: number;
  rows?: number;
}

/**
 * Input data to send to a session
 */
export interface SessionInput {
  sessionId: string;
  data: string;
}

/**
 * Resize data for a session
 */
export interface SessionResize {
  sessionId: string;
  cols: number;
  rows: number;
}

/**
 * Create initial state for the session store
 */
function createInitialState(): SessionState {
  return {
    sessions: {},
    activeSessionId: null,
    sessionOrder: [],
  };
}

/**
 * Default terminal dimensions
 */
const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;

/**
 * Generate a unique session ID
 */
function generateSessionId(): string {
  return `session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * Creates a session store for managing terminal sessions
 */
export function createSessionStore() {
  const [state, setState] = createStore<SessionState>(createInitialState());
  const [subscribers] = createSignal<Set<SessionEventSubscriber>>(new Set());

  /**
   * Emit an event to all subscribers
   */
  const emit = (event: SessionEvent): void => {
    subscribers().forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in session event subscriber:', error);
      }
    });
  };

  /**
   * Subscribe to session events
   */
  const subscribe = (callback: SessionEventSubscriber): (() => void) => {
    subscribers().add(callback);
    return () => {
      subscribers().delete(callback);
    };
  };

  /**
   * Create a new terminal session
   */
  const createSession = (options: CreateSessionOptions): string => {
    const sessionId = generateSessionId();
    const now = Date.now();

    const session: TerminalSession = {
      id: sessionId,
      peerId: options.peerId,
      status: 'connecting',
      title: options.title ?? `Terminal ${Object.keys(state.sessions).length + 1}`,
      flowControl: 'running',
      cols: options.cols ?? DEFAULT_COLS,
      rows: options.rows ?? DEFAULT_ROWS,
      createdAt: now,
      lastActivityAt: now,
    };

    batch(() => {
      setState(
        produce((s) => {
          s.sessions[sessionId] = session;
          s.sessionOrder.push(sessionId);
        })
      );
      // Set as active session if it's the first one
      if (state.activeSessionId === null) {
        setState('activeSessionId', sessionId);
      }
    });

    emit({ type: 'session:created', sessionId });
    return sessionId;
  };

  /**
   * Close and remove a terminal session
   */
  const closeSession = (sessionId: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot close session: session ${sessionId} not found`);
      return false;
    }

    const orderIndex = state.sessionOrder.indexOf(sessionId);

    batch(() => {
      setState(
        produce((s) => {
          delete s.sessions[sessionId];
          const idx = s.sessionOrder.indexOf(sessionId);
          if (idx !== -1) {
            s.sessionOrder.splice(idx, 1);
          }
        })
      );

      // If we closed the active session, switch to another one
      if (state.activeSessionId === sessionId) {
        // Try to select the next session, or the previous one if at the end
        const newActiveIndex = Math.min(orderIndex, state.sessionOrder.length - 1);
        const newActiveId = newActiveIndex >= 0 ? state.sessionOrder[newActiveIndex] : null;
        setState('activeSessionId', newActiveId);
      }
    });

    emit({ type: 'session:closed', sessionId });
    return true;
  };

  /**
   * Set the active session
   */
  const setActiveSession = (sessionId: string | null): boolean => {
    if (sessionId !== null && !state.sessions[sessionId]) {
      console.warn(`Cannot set active session: session ${sessionId} not found`);
      return false;
    }

    setState('activeSessionId', sessionId);
    emit({ type: 'session:active', sessionId: sessionId ?? undefined });
    return true;
  };

  /**
   * Send input to a session
   */
  const sendInput = (sessionId: string, data: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot send input: session ${sessionId} not found`);
      return false;
    }

    if (session.status !== 'connected') {
      console.warn(`Cannot send input: session ${sessionId} is not connected`);
      return false;
    }

    if (session.flowControl === 'paused') {
      console.warn(`Cannot send input: session ${sessionId} is paused`);
      return false;
    }

    // Update last activity timestamp
    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.lastActivityAt = Date.now();
        }
      })
    );

    emit({
      type: 'session:input',
      sessionId,
      data: { input: data } as unknown,
    });
    return true;
  };

  /**
   * Resize a terminal session
   */
  const resizeTerminal = (sessionId: string, cols: number, rows: number): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot resize terminal: session ${sessionId} not found`);
      return false;
    }

    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.cols = cols;
          sess.rows = rows;
          sess.lastActivityAt = Date.now();
        }
      })
    );

    emit({
      type: 'session:resize',
      sessionId,
      data: { cols, rows } as unknown,
    });
    return true;
  };

  /**
   * Pause flow control for a session (XOFF)
   */
  const pauseSession = (sessionId: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot pause session: session ${sessionId} not found`);
      return false;
    }

    if (session.flowControl === 'paused') {
      return true; // Already paused
    }

    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.flowControl = 'paused';
        }
      })
    );

    emit({ type: 'session:flow:paused', sessionId });
    return true;
  };

  /**
   * Resume flow control for a session (XON)
   */
  const resumeSession = (sessionId: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot resume session: session ${sessionId} not found`);
      return false;
    }

    if (session.flowControl === 'running') {
      return true; // Already running
    }

    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.flowControl = 'running';
        }
      })
    );

    emit({ type: 'session:flow:resumed', sessionId });
    return true;
  };

  /**
   * Update session status
   */
  const setSessionStatus = (sessionId: string, status: SessionStatus, error?: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot update status: session ${sessionId} not found`);
      return false;
    }

    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.status = status;
          sess.lastActivityAt = Date.now();
          if (error) {
            sess.lastError = error;
          }
        }
      })
    );

    if (status === 'error' && error) {
      emit({ type: 'session:error', sessionId, error });
    }
    return true;
  };

  /**
   * Update session title
   */
  const setSessionTitle = (sessionId: string, title: string): boolean => {
    const session = state.sessions[sessionId];
    if (!session) {
      console.warn(`Cannot update title: session ${sessionId} not found`);
      return false;
    }

    setState(
      produce((s) => {
        const sess = s.sessions[sessionId];
        if (sess) {
          sess.title = title;
        }
      })
    );
    return true;
  };

  /**
   * Get the current active session
   */
  const getActiveSession = (): TerminalSession | null => {
    const sessionId = state.activeSessionId;
    return sessionId ? state.sessions[sessionId] ?? null : null;
  };

  /**
   * Get a session by ID
   */
  const getSession = (sessionId: string): TerminalSession | null => {
    return state.sessions[sessionId] ?? null;
  };

  /**
   * Get all sessions for a peer
   */
  const getSessionsByPeer = (peerId: string): TerminalSession[] => {
    return Object.values(state.sessions).filter(session => session.peerId === peerId);
  };

  /**
   * Get sessions in tab order
   */
  const getSessionsInOrder = (): TerminalSession[] => {
    return state.sessionOrder
      .map(id => state.sessions[id])
      .filter((session): session is TerminalSession => session !== undefined);
  };

  /**
   * Reorder sessions (for tab drag and drop)
   */
  const reorderSessions = (fromIndex: number, toIndex: number): boolean => {
    if (fromIndex < 0 || fromIndex >= state.sessionOrder.length ||
        toIndex < 0 || toIndex >= state.sessionOrder.length) {
      console.warn('Cannot reorder sessions: invalid indices');
      return false;
    }

    setState(
      produce((s) => {
        const [removed] = s.sessionOrder.splice(fromIndex, 1);
        s.sessionOrder.splice(toIndex, 0, removed);
      })
    );
    return true;
  };

  /**
   * Close all sessions for a peer
   */
  const closeSessionsByPeer = (peerId: string): void => {
    const sessionsToClose = Object.values(state.sessions)
      .filter(session => session.peerId === peerId)
      .map(session => session.id);

    sessionsToClose.forEach(sessionId => closeSession(sessionId));
  };

  /**
   * Reset the store to initial state
   */
  const reset = (): void => {
    setState(
      produce((s) => {
        for (const key of Object.keys(s.sessions)) {
          delete s.sessions[key];
        }
        s.activeSessionId = null;
        s.sessionOrder.length = 0;
      })
    );
  };

  return {
    // State (readonly)
    state,

    // Session actions
    createSession,
    closeSession,
    setActiveSession,
    sendInput,
    resizeTerminal,

    // Flow control actions
    pauseSession,
    resumeSession,

    // Status and title actions
    setSessionStatus,
    setSessionTitle,

    // Tab management
    reorderSessions,
    closeSessionsByPeer,

    // Getters
    getActiveSession,
    getSession,
    getSessionsByPeer,
    getSessionsInOrder,

    // Event subscriptions
    subscribe,

    // Utility
    reset,
  };
}

/**
 * Type for the session store instance
 */
export type SessionStore = ReturnType<typeof createSessionStore>;

/**
 * Singleton instance of the session store
 */
let sessionStoreInstance: SessionStore | null = null;

/**
 * Get or create the singleton session store instance
 */
export function getSessionStore(): SessionStore {
  if (!sessionStoreInstance) {
    sessionStoreInstance = createSessionStore();
  }
  return sessionStoreInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetSessionStore(): void {
  if (sessionStoreInstance) {
    sessionStoreInstance.reset();
  }
  sessionStoreInstance = null;
}
