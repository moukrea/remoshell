/**
 * App Lifecycle Manager
 *
 * This module handles mobile app lifecycle transitions (foreground/background).
 * It works across both Tauri and web platforms.
 *
 * Features:
 * - Detects foreground/background state changes
 * - Pauses terminal data flow when backgrounded
 * - Queues notifications to show when foregrounded
 * - Sends keep-alive pings to maintain connection
 * - Emits events for state changes
 */

import { TauriIPCBridge } from '../tauri/TauriIPCBridge';

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * Application lifecycle state
 */
export type LifecycleState = 'foreground' | 'background';

/**
 * Lifecycle event types
 */
export type LifecycleEventType =
  | 'lifecycle:foreground'
  | 'lifecycle:background'
  | 'lifecycle:keepalive';

/**
 * Lifecycle event payload
 */
export interface LifecycleEvent {
  type: LifecycleEventType;
  timestamp: number;
  previousState?: LifecycleState;
}

/**
 * Event subscriber callback type
 */
export type LifecycleEventSubscriber = (event: LifecycleEvent) => void;

/**
 * Queued notification to show when app returns to foreground
 */
export interface QueuedNotification {
  id: string;
  title: string;
  body: string;
  icon?: string;
  queuedAt: number;
}

/**
 * Configuration options for the lifecycle manager
 */
export interface LifecycleConfig {
  /** Interval for keep-alive pings in milliseconds (default: 30000) */
  keepAliveInterval?: number;
  /** Whether to enable keep-alive pings (default: true) */
  enableKeepAlive?: boolean;
  /** Maximum number of notifications to queue (default: 50) */
  maxQueuedNotifications?: number;
}

/**
 * Default configuration values
 */
export const DEFAULT_KEEP_ALIVE_INTERVAL = 30000;
export const DEFAULT_MAX_QUEUED_NOTIFICATIONS = 50;

// ============================================================================
// App Lifecycle Manager
// ============================================================================

/**
 * AppLifecycle provides lifecycle management for mobile app transitions.
 * It detects foreground/background state changes and manages resources accordingly.
 */
export class AppLifecycle {
  private state: LifecycleState = 'foreground';
  private subscribers: Set<LifecycleEventSubscriber> = new Set();
  private notificationQueue: QueuedNotification[] = [];
  private keepAliveTimer: ReturnType<typeof setInterval> | null = null;
  private initialized = false;
  private initializing = false;
  private config: Required<LifecycleConfig>;

  // Tauri event cleanup functions
  private tauriUnlistenFns: (() => void)[] = [];

  // Web event cleanup functions
  private webCleanupFns: (() => void)[] = [];

  // Terminal data flow control
  private terminalFlowPaused = false;
  private terminalDataQueue: Uint8Array[] = [];

  // Notification ID counter
  private notificationIdCounter = 0;

  constructor(config: LifecycleConfig = {}) {
    this.config = {
      keepAliveInterval: config.keepAliveInterval ?? DEFAULT_KEEP_ALIVE_INTERVAL,
      enableKeepAlive: config.enableKeepAlive ?? true,
      maxQueuedNotifications: config.maxQueuedNotifications ?? DEFAULT_MAX_QUEUED_NOTIFICATIONS,
    };
  }

  // ==========================================================================
  // Initialization
  // ==========================================================================

  /**
   * Initialize the lifecycle manager.
   * This sets up event listeners for both Tauri and web platforms.
   */
  async initialize(): Promise<void> {
    if (this.initialized || this.initializing) {
      return;
    }

    this.initializing = true;

    try {
      // Set up platform-specific listeners
      if (TauriIPCBridge.isAvailable()) {
        await this.initializeTauriListeners();
      }

      // Always set up web listeners as fallback
      this.initializeWebListeners();

      this.initialized = true;
    } finally {
      this.initializing = false;
    }
  }

  /**
   * Set up Tauri-specific event listeners for window focus/blur.
   */
  private async initializeTauriListeners(): Promise<void> {
    try {
      const tauri = window.__TAURI__;
      if (!tauri) {
        return;
      }

      // Listen for window focus event
      const unlistenFocus = await tauri.event.listen<void>('tauri://focus', () => {
        this.handleForeground();
      });
      this.tauriUnlistenFns.push(unlistenFocus);

      // Listen for window blur event
      const unlistenBlur = await tauri.event.listen<void>('tauri://blur', () => {
        this.handleBackground();
      });
      this.tauriUnlistenFns.push(unlistenBlur);
    } catch (error) {
      console.error('Failed to initialize Tauri lifecycle listeners:', error);
    }
  }

  /**
   * Set up web-specific event listeners for visibility change.
   */
  private initializeWebListeners(): void {
    if (typeof document === 'undefined') {
      return;
    }

    // Handle visibility change
    const handleVisibilityChange = (): void => {
      if (document.visibilityState === 'hidden') {
        this.handleBackground();
      } else if (document.visibilityState === 'visible') {
        this.handleForeground();
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    this.webCleanupFns.push(() => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    });

    // Handle page hide/show events (for mobile browsers)
    const handlePageHide = (): void => {
      this.handleBackground();
    };

    const handlePageShow = (): void => {
      this.handleForeground();
    };

    window.addEventListener('pagehide', handlePageHide);
    window.addEventListener('pageshow', handlePageShow);
    this.webCleanupFns.push(() => {
      window.removeEventListener('pagehide', handlePageHide);
      window.removeEventListener('pageshow', handlePageShow);
    });

    // Handle window focus/blur for desktop browsers
    const handleWindowFocus = (): void => {
      this.handleForeground();
    };

    const handleWindowBlur = (): void => {
      // Only treat blur as background if also hidden
      // This prevents backgrounding when switching tabs but staying visible
      if (document.visibilityState === 'hidden') {
        this.handleBackground();
      }
    };

    window.addEventListener('focus', handleWindowFocus);
    window.addEventListener('blur', handleWindowBlur);
    this.webCleanupFns.push(() => {
      window.removeEventListener('focus', handleWindowFocus);
      window.removeEventListener('blur', handleWindowBlur);
    });
  }

  // ==========================================================================
  // State Transitions
  // ==========================================================================

  /**
   * Handle transition to foreground state.
   */
  private handleForeground(): void {
    if (this.state === 'foreground') {
      return;
    }

    const previousState = this.state;
    this.state = 'foreground';

    // Stop keep-alive pings
    this.stopKeepAlive();

    // Resume terminal data flow
    this.resumeTerminalFlow();

    // Process queued notifications
    this.processNotificationQueue();

    // Emit event
    this.emit({
      type: 'lifecycle:foreground',
      timestamp: Date.now(),
      previousState,
    });
  }

  /**
   * Handle transition to background state.
   */
  private handleBackground(): void {
    if (this.state === 'background') {
      return;
    }

    const previousState = this.state;
    this.state = 'background';

    // Pause terminal data flow
    this.pauseTerminalFlow();

    // Start keep-alive pings
    if (this.config.enableKeepAlive) {
      this.startKeepAlive();
    }

    // Emit event
    this.emit({
      type: 'lifecycle:background',
      timestamp: Date.now(),
      previousState,
    });
  }

  // ==========================================================================
  // Terminal Flow Control
  // ==========================================================================

  /**
   * Check if terminal data flow is paused.
   */
  isTerminalFlowPaused(): boolean {
    return this.terminalFlowPaused;
  }

  /**
   * Pause terminal data flow.
   * Data will be queued instead of rendered.
   */
  private pauseTerminalFlow(): void {
    this.terminalFlowPaused = true;
  }

  /**
   * Resume terminal data flow.
   * Processes any queued data.
   */
  private resumeTerminalFlow(): void {
    this.terminalFlowPaused = false;
    // Clear the queue - consumers should re-fetch state if needed
    this.terminalDataQueue = [];
  }

  /**
   * Queue terminal data when paused.
   * Returns true if data was queued, false if flow is active.
   */
  queueTerminalData(data: Uint8Array): boolean {
    if (!this.terminalFlowPaused) {
      return false;
    }

    // Limit queue size to prevent memory issues
    const MAX_QUEUE_SIZE = 100;
    if (this.terminalDataQueue.length >= MAX_QUEUE_SIZE) {
      // Remove oldest entries
      this.terminalDataQueue.shift();
    }

    this.terminalDataQueue.push(data);
    return true;
  }

  /**
   * Get and clear queued terminal data.
   */
  drainTerminalDataQueue(): Uint8Array[] {
    const queue = this.terminalDataQueue;
    this.terminalDataQueue = [];
    return queue;
  }

  // ==========================================================================
  // Notification Queue
  // ==========================================================================

  /**
   * Queue a notification to show when app returns to foreground.
   * Returns the notification ID.
   */
  queueNotification(title: string, body: string, icon?: string): string {
    this.notificationIdCounter += 1;
    const id = `queued-notification-${Date.now()}-${this.notificationIdCounter}`;

    const notification: QueuedNotification = {
      id,
      title,
      body,
      icon,
      queuedAt: Date.now(),
    };

    this.notificationQueue.push(notification);

    // Limit queue size
    while (this.notificationQueue.length > this.config.maxQueuedNotifications) {
      this.notificationQueue.shift();
    }

    return id;
  }

  /**
   * Get all queued notifications.
   */
  getQueuedNotifications(): QueuedNotification[] {
    return [...this.notificationQueue];
  }

  /**
   * Clear the notification queue.
   */
  clearNotificationQueue(): void {
    this.notificationQueue = [];
  }

  /**
   * Process queued notifications when returning to foreground.
   * This is called automatically on foreground transition.
   */
  private processNotificationQueue(): void {
    // Notifications are kept in queue for subscribers to handle
    // They should call getQueuedNotifications() and clearNotificationQueue()
  }

  // ==========================================================================
  // Keep-Alive
  // ==========================================================================

  /**
   * Start sending keep-alive pings.
   */
  private startKeepAlive(): void {
    if (this.keepAliveTimer) {
      return;
    }

    this.keepAliveTimer = setInterval(() => {
      this.sendKeepAlive();
    }, this.config.keepAliveInterval);

    // Send initial ping immediately
    this.sendKeepAlive();
  }

  /**
   * Stop sending keep-alive pings.
   */
  private stopKeepAlive(): void {
    if (this.keepAliveTimer) {
      clearInterval(this.keepAliveTimer);
      this.keepAliveTimer = null;
    }
  }

  /**
   * Send a keep-alive ping.
   */
  private sendKeepAlive(): void {
    this.emit({
      type: 'lifecycle:keepalive',
      timestamp: Date.now(),
    });
  }

  // ==========================================================================
  // Event System
  // ==========================================================================

  /**
   * Subscribe to lifecycle events.
   * Returns an unsubscribe function.
   */
  subscribe(callback: LifecycleEventSubscriber): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Emit an event to all subscribers.
   */
  private emit(event: LifecycleEvent): void {
    this.subscribers.forEach((subscriber) => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in lifecycle event subscriber:', error);
      }
    });
  }

  // ==========================================================================
  // State Access
  // ==========================================================================

  /**
   * Get the current lifecycle state.
   */
  getState(): LifecycleState {
    return this.state;
  }

  /**
   * Check if the app is in foreground.
   */
  isForeground(): boolean {
    return this.state === 'foreground';
  }

  /**
   * Check if the app is in background.
   */
  isBackground(): boolean {
    return this.state === 'background';
  }

  /**
   * Check if the lifecycle manager is initialized.
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  // ==========================================================================
  // Cleanup
  // ==========================================================================

  /**
   * Destroy the lifecycle manager and clean up all resources.
   */
  destroy(): void {
    // Stop keep-alive
    this.stopKeepAlive();

    // Clean up Tauri listeners
    this.tauriUnlistenFns.forEach((unlisten) => unlisten());
    this.tauriUnlistenFns = [];

    // Clean up web listeners
    this.webCleanupFns.forEach((cleanup) => cleanup());
    this.webCleanupFns = [];

    // Clear subscribers
    this.subscribers.clear();

    // Clear queues
    this.notificationQueue = [];
    this.terminalDataQueue = [];

    // Reset state
    this.state = 'foreground';
    this.terminalFlowPaused = false;
    this.initialized = false;
  }

  /**
   * Reset the lifecycle manager to initial state.
   * Unlike destroy(), this keeps the listeners active.
   */
  reset(): void {
    // Stop keep-alive
    this.stopKeepAlive();

    // Clear queues
    this.notificationQueue = [];
    this.terminalDataQueue = [];

    // Reset state
    this.state = 'foreground';
    this.terminalFlowPaused = false;
  }
}

// ============================================================================
// Singleton Instance
// ============================================================================

/**
 * Singleton instance of AppLifecycle.
 */
let appLifecycleInstance: AppLifecycle | null = null;

/**
 * Get or create the singleton AppLifecycle instance.
 */
export function getAppLifecycle(config?: LifecycleConfig): AppLifecycle {
  if (!appLifecycleInstance) {
    appLifecycleInstance = new AppLifecycle(config);
  }
  return appLifecycleInstance;
}

/**
 * Initialize the singleton AppLifecycle instance.
 * This is a convenience function that gets/creates the instance and initializes it.
 */
export async function initializeAppLifecycle(config?: LifecycleConfig): Promise<AppLifecycle> {
  const lifecycle = getAppLifecycle(config);
  await lifecycle.initialize();
  return lifecycle;
}

/**
 * Reset the singleton instance (useful for testing).
 */
export function resetAppLifecycle(): void {
  if (appLifecycleInstance) {
    appLifecycleInstance.destroy();
  }
  appLifecycleInstance = null;
}
