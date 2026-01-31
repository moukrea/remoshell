import { createStore, produce } from 'solid-js/store';
import { createSignal, batch } from 'solid-js';

/**
 * Notification types
 */
export type NotificationType = 'info' | 'success' | 'warning' | 'error';

/**
 * Action button for notifications
 */
export interface NotificationAction {
  label: string;
  onClick: () => void;
}

/**
 * Represents a single notification
 */
export interface Notification {
  id: string;
  type: NotificationType;
  title: string;
  message?: string;
  actions?: NotificationAction[];
  duration: number;
  createdAt: number;
  dismissing?: boolean;
}

/**
 * Notification store state
 */
export interface NotificationState {
  notifications: Record<string, Notification>;
  order: string[];
}

/**
 * Event types emitted by the notification store
 */
export type NotificationEventType =
  | 'notification:show'
  | 'notification:dismiss'
  | 'notification:dismissed';

/**
 * Event payload types
 */
export interface NotificationEvent {
  type: NotificationEventType;
  notificationId: string;
}

/**
 * Event subscriber callback type
 */
export type NotificationEventSubscriber = (event: NotificationEvent) => void;

/**
 * Options for showing a notification
 */
export interface ShowNotificationOptions {
  type?: NotificationType;
  title: string;
  message?: string;
  actions?: NotificationAction[];
  duration?: number;
}

/**
 * Default auto-dismiss duration in milliseconds
 */
export const DEFAULT_NOTIFICATION_DURATION = 5000;


/**
 * Creates a notification store for managing toast notifications
 */
export function createNotificationStore() {
  const [state, setState] = createStore<NotificationState>({
    notifications: {},
    order: [],
  });
  const [subscribers] = createSignal<Set<NotificationEventSubscriber>>(new Set());
  const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

  /**
   * Counter for generating unique IDs (scoped to this store instance)
   */
  let notificationIdCounter = 0;

  /**
   * Generate a unique notification ID
   */
  const generateNotificationId = (): string => {
    notificationIdCounter += 1;
    return `notification-${Date.now()}-${notificationIdCounter}`;
  };

  /**
   * Emit an event to all subscribers
   */
  const emit = (event: NotificationEvent): void => {
    subscribers().forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in notification event subscriber:', error);
      }
    });
  };

  /**
   * Subscribe to notification events
   */
  const subscribe = (callback: NotificationEventSubscriber): (() => void) => {
    subscribers().add(callback);
    return () => {
      subscribers().delete(callback);
    };
  };

  /**
   * Clear a dismiss timer for a notification
   */
  const clearDismissTimer = (id: string): void => {
    const timer = dismissTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      dismissTimers.delete(id);
    }
  };

  /**
   * Set up auto-dismiss timer for a notification
   */
  const setupDismissTimer = (id: string, duration: number): void => {
    if (duration <= 0) return;

    clearDismissTimer(id);
    const timer = setTimeout(() => {
      dismiss(id);
    }, duration);
    dismissTimers.set(id, timer);
  };

  /**
   * Show a notification
   */
  const show = (options: ShowNotificationOptions): string => {
    const id = generateNotificationId();
    const notification: Notification = {
      id,
      type: options.type ?? 'info',
      title: options.title,
      message: options.message,
      actions: options.actions,
      duration: options.duration ?? DEFAULT_NOTIFICATION_DURATION,
      createdAt: Date.now(),
    };

    batch(() => {
      setState(
        produce((s) => {
          s.notifications[id] = notification;
          s.order.push(id);
        })
      );
    });

    emit({ type: 'notification:show', notificationId: id });

    // Set up auto-dismiss timer
    setupDismissTimer(id, notification.duration);

    return id;
  };

  /**
   * Dismiss a notification (starts exit animation)
   */
  const dismiss = (id: string): boolean => {
    const notification = state.notifications[id];
    if (!notification) {
      return false;
    }

    // Already dismissing
    if (notification.dismissing) {
      return true;
    }

    clearDismissTimer(id);

    setState(
      produce((s) => {
        const n = s.notifications[id];
        if (n) {
          n.dismissing = true;
        }
      })
    );

    emit({ type: 'notification:dismiss', notificationId: id });

    return true;
  };

  /**
   * Remove a notification from state (called after animation completes)
   */
  const remove = (id: string): boolean => {
    const notification = state.notifications[id];
    if (!notification) {
      return false;
    }

    clearDismissTimer(id);

    setState(
      produce((s) => {
        delete s.notifications[id];
        const index = s.order.indexOf(id);
        if (index !== -1) {
          s.order.splice(index, 1);
        }
      })
    );

    emit({ type: 'notification:dismissed', notificationId: id });

    return true;
  };

  /**
   * Dismiss all notifications
   */
  const dismissAll = (): void => {
    const ids = [...state.order];
    ids.forEach(id => dismiss(id));
  };

  /**
   * Get all notifications in display order
   */
  const getNotifications = (): Notification[] => {
    return state.order.map(id => state.notifications[id]).filter(Boolean);
  };

  /**
   * Get a notification by ID
   */
  const getNotification = (id: string): Notification | null => {
    return state.notifications[id] ?? null;
  };

  /**
   * Pause auto-dismiss timer (e.g., when hovering)
   */
  const pauseTimer = (id: string): void => {
    clearDismissTimer(id);
  };

  /**
   * Resume auto-dismiss timer (e.g., when mouse leaves)
   */
  const resumeTimer = (id: string): void => {
    const notification = state.notifications[id];
    if (notification && !notification.dismissing) {
      // Calculate remaining time based on how long notification has been shown
      const elapsed = Date.now() - notification.createdAt;
      const remaining = Math.max(notification.duration - elapsed, 1000);
      setupDismissTimer(id, remaining);
    }
  };

  /**
   * Reset the store to initial state
   */
  const reset = (): void => {
    // Clear all timers
    dismissTimers.forEach((timer) => clearTimeout(timer));
    dismissTimers.clear();

    setState(
      produce((s) => {
        // Clear all notifications
        for (const key of Object.keys(s.notifications)) {
          delete s.notifications[key];
        }
        s.order = [];
      })
    );
  };

  // Convenience methods for different notification types
  const info = (title: string, message?: string, options?: Omit<ShowNotificationOptions, 'type' | 'title' | 'message'>): string => {
    return show({ ...options, type: 'info', title, message });
  };

  const success = (title: string, message?: string, options?: Omit<ShowNotificationOptions, 'type' | 'title' | 'message'>): string => {
    return show({ ...options, type: 'success', title, message });
  };

  const warning = (title: string, message?: string, options?: Omit<ShowNotificationOptions, 'type' | 'title' | 'message'>): string => {
    return show({ ...options, type: 'warning', title, message });
  };

  const error = (title: string, message?: string, options?: Omit<ShowNotificationOptions, 'type' | 'title' | 'message'>): string => {
    return show({ ...options, type: 'error', title, message });
  };

  return {
    // State (readonly)
    state,

    // Main actions
    show,
    dismiss,
    remove,
    dismissAll,

    // Timer control
    pauseTimer,
    resumeTimer,

    // Getters
    getNotifications,
    getNotification,

    // Convenience methods
    info,
    success,
    warning,
    error,

    // Event subscriptions
    subscribe,

    // Utility
    reset,
  };
}

/**
 * Type for the notification store instance
 */
export type NotificationStore = ReturnType<typeof createNotificationStore>;

/**
 * Singleton instance of the notification store
 */
let notificationStoreInstance: NotificationStore | null = null;

/**
 * Get or create the singleton notification store instance
 */
export function getNotificationStore(): NotificationStore {
  if (!notificationStoreInstance) {
    notificationStoreInstance = createNotificationStore();
  }
  return notificationStoreInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetNotificationStore(): void {
  if (notificationStoreInstance) {
    notificationStoreInstance.reset();
  }
  notificationStoreInstance = null;
}
