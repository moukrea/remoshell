/**
 * App Lifecycle Module
 *
 * This module exports the AppLifecycle manager for handling
 * mobile app foreground/background transitions.
 */

export {
  AppLifecycle,
  getAppLifecycle,
  resetAppLifecycle,
  initializeAppLifecycle,
  DEFAULT_KEEP_ALIVE_INTERVAL,
  DEFAULT_MAX_QUEUED_NOTIFICATIONS,
  type LifecycleState,
  type LifecycleEventType,
  type LifecycleEvent,
  type LifecycleEventSubscriber,
  type QueuedNotification,
  type LifecycleConfig,
} from './AppLifecycle';
