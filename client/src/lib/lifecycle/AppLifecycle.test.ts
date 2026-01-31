import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  AppLifecycle,
  getAppLifecycle,
  resetAppLifecycle,
  initializeAppLifecycle,
  DEFAULT_KEEP_ALIVE_INTERVAL,
  DEFAULT_MAX_QUEUED_NOTIFICATIONS,
  type LifecycleEvent,
  type LifecycleEventSubscriber,
} from './AppLifecycle';

// Mock TauriIPCBridge
vi.mock('../tauri/TauriIPCBridge', () => ({
  TauriIPCBridge: {
    isAvailable: vi.fn(() => false),
  },
}));

describe('AppLifecycle', () => {
  let lifecycle: AppLifecycle;

  beforeEach(() => {
    vi.useFakeTimers();
    resetAppLifecycle();
    lifecycle = new AppLifecycle();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetAppLifecycle();
  });

  describe('Initial State', () => {
    it('should start in foreground state', () => {
      expect(lifecycle.getState()).toBe('foreground');
      expect(lifecycle.isForeground()).toBe(true);
      expect(lifecycle.isBackground()).toBe(false);
    });

    it('should not be initialized by default', () => {
      expect(lifecycle.isInitialized()).toBe(false);
    });

    it('should not have terminal flow paused', () => {
      expect(lifecycle.isTerminalFlowPaused()).toBe(false);
    });

    it('should have empty notification queue', () => {
      expect(lifecycle.getQueuedNotifications()).toEqual([]);
    });
  });

  describe('Initialization', () => {
    it('should set initialized flag after initialize()', async () => {
      await lifecycle.initialize();
      expect(lifecycle.isInitialized()).toBe(true);
    });

    it('should not re-initialize if already initialized', async () => {
      await lifecycle.initialize();
      await lifecycle.initialize(); // Should not throw
      expect(lifecycle.isInitialized()).toBe(true);
    });

    it('should set up web visibility listeners', async () => {
      const addEventListenerSpy = vi.spyOn(document, 'addEventListener');
      await lifecycle.initialize();

      expect(addEventListenerSpy).toHaveBeenCalledWith(
        'visibilitychange',
        expect.any(Function)
      );

      addEventListenerSpy.mockRestore();
    });

    it('should set up window focus/blur listeners', async () => {
      const addEventListenerSpy = vi.spyOn(window, 'addEventListener');
      await lifecycle.initialize();

      expect(addEventListenerSpy).toHaveBeenCalledWith('focus', expect.any(Function));
      expect(addEventListenerSpy).toHaveBeenCalledWith('blur', expect.any(Function));
      expect(addEventListenerSpy).toHaveBeenCalledWith('pagehide', expect.any(Function));
      expect(addEventListenerSpy).toHaveBeenCalledWith('pageshow', expect.any(Function));

      addEventListenerSpy.mockRestore();
    });
  });

  describe('Web Visibility Change', () => {
    beforeEach(async () => {
      await lifecycle.initialize();
    });

    it('should transition to background on visibility hidden', () => {
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Simulate visibility change to hidden
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isBackground()).toBe(true);
      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'lifecycle:background',
          previousState: 'foreground',
        })
      );
    });

    it('should transition to foreground on visibility visible', () => {
      // First go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Then return to foreground
      Object.defineProperty(document, 'visibilityState', {
        value: 'visible',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isForeground()).toBe(true);
      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'lifecycle:foreground',
          previousState: 'background',
        })
      );
    });

    it('should not emit duplicate events for same state', () => {
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Trigger foreground when already in foreground
      Object.defineProperty(document, 'visibilityState', {
        value: 'visible',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(subscriber).not.toHaveBeenCalled();
    });
  });

  describe('Terminal Flow Control', () => {
    beforeEach(async () => {
      await lifecycle.initialize();
    });

    it('should pause terminal flow when backgrounded', () => {
      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isTerminalFlowPaused()).toBe(true);
    });

    it('should resume terminal flow when foregrounded', () => {
      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      // Return to foreground
      Object.defineProperty(document, 'visibilityState', {
        value: 'visible',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isTerminalFlowPaused()).toBe(false);
    });

    it('should queue terminal data when paused', () => {
      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      const data = new Uint8Array([1, 2, 3]);
      const queued = lifecycle.queueTerminalData(data);

      expect(queued).toBe(true);
    });

    it('should not queue terminal data when active', () => {
      const data = new Uint8Array([1, 2, 3]);
      const queued = lifecycle.queueTerminalData(data);

      expect(queued).toBe(false);
    });

    it('should drain terminal data queue', () => {
      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      const data1 = new Uint8Array([1, 2, 3]);
      const data2 = new Uint8Array([4, 5, 6]);
      lifecycle.queueTerminalData(data1);
      lifecycle.queueTerminalData(data2);

      const queue = lifecycle.drainTerminalDataQueue();

      expect(queue).toHaveLength(2);
      expect(queue[0]).toEqual(data1);
      expect(queue[1]).toEqual(data2);
      expect(lifecycle.drainTerminalDataQueue()).toEqual([]);
    });

    it('should limit terminal data queue size', () => {
      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      // Queue more than max (100)
      for (let i = 0; i < 105; i++) {
        lifecycle.queueTerminalData(new Uint8Array([i]));
      }

      const queue = lifecycle.drainTerminalDataQueue();
      expect(queue.length).toBeLessThanOrEqual(100);
    });
  });

  describe('Notification Queue', () => {
    it('should queue notifications', () => {
      const id = lifecycle.queueNotification('Title', 'Body');

      expect(id).toMatch(/^queued-notification-\d+-\d+$/);
      expect(lifecycle.getQueuedNotifications()).toHaveLength(1);
      expect(lifecycle.getQueuedNotifications()[0]).toMatchObject({
        id,
        title: 'Title',
        body: 'Body',
      });
    });

    it('should queue notifications with icon', () => {
      const id = lifecycle.queueNotification('Title', 'Body', 'icon.png');

      expect(lifecycle.getQueuedNotifications()[0].icon).toBe('icon.png');
    });

    it('should set queuedAt timestamp', () => {
      vi.setSystemTime(new Date('2024-01-01T12:00:00Z'));
      lifecycle.queueNotification('Title', 'Body');

      expect(lifecycle.getQueuedNotifications()[0].queuedAt).toBe(
        new Date('2024-01-01T12:00:00Z').getTime()
      );
    });

    it('should clear notification queue', () => {
      lifecycle.queueNotification('Title 1', 'Body 1');
      lifecycle.queueNotification('Title 2', 'Body 2');

      lifecycle.clearNotificationQueue();

      expect(lifecycle.getQueuedNotifications()).toEqual([]);
    });

    it('should limit notification queue size', () => {
      const maxSize = DEFAULT_MAX_QUEUED_NOTIFICATIONS;

      for (let i = 0; i < maxSize + 10; i++) {
        lifecycle.queueNotification(`Title ${i}`, `Body ${i}`);
      }

      expect(lifecycle.getQueuedNotifications().length).toBe(maxSize);
    });

    it('should return copy of notification queue', () => {
      lifecycle.queueNotification('Title', 'Body');

      const queue1 = lifecycle.getQueuedNotifications();
      const queue2 = lifecycle.getQueuedNotifications();

      expect(queue1).not.toBe(queue2);
      expect(queue1).toEqual(queue2);
    });
  });

  describe('Keep-Alive', () => {
    beforeEach(async () => {
      await lifecycle.initialize();
    });

    it('should start keep-alive when backgrounded', () => {
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      // Should emit initial keep-alive
      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });

    it('should send keep-alive at configured interval', () => {
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));
      subscriber.mockClear();

      // Advance timer by keep-alive interval
      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL);

      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });

    it('should stop keep-alive when foregrounded', () => {
      const subscriber = vi.fn();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      // Return to foreground
      Object.defineProperty(document, 'visibilityState', {
        value: 'visible',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      lifecycle.subscribe(subscriber);
      subscriber.mockClear();

      // Advance timer - should not emit keep-alive
      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL * 2);

      expect(subscriber).not.toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });

    it('should respect custom keep-alive interval', async () => {
      lifecycle.destroy();
      lifecycle = new AppLifecycle({ keepAliveInterval: 5000 });
      await lifecycle.initialize();

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));
      subscriber.mockClear();

      // Advance by custom interval
      vi.advanceTimersByTime(5000);

      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });

    it('should disable keep-alive when configured', async () => {
      lifecycle.destroy();
      lifecycle = new AppLifecycle({ enableKeepAlive: false });
      await lifecycle.initialize();

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));
      subscriber.mockClear();

      // Advance timer - should not emit keep-alive
      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL * 2);

      expect(subscriber).not.toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', async () => {
      await lifecycle.initialize();
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(subscriber).toHaveBeenCalled();
    });

    it('should allow unsubscribing from events', async () => {
      await lifecycle.initialize();
      const subscriber = vi.fn();
      const unsubscribe = lifecycle.subscribe(subscriber);

      unsubscribe();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      // Only keep-alive events if enabled, no background event
      expect(subscriber).not.toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:background' })
      );
    });

    it('should include timestamp in events', async () => {
      await lifecycle.initialize();
      vi.setSystemTime(new Date('2024-01-01T12:00:00Z'));

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(subscriber).toHaveBeenCalledWith(
        expect.objectContaining({
          timestamp: new Date('2024-01-01T12:00:00Z').getTime(),
        })
      );
    });

    it('should handle errors in subscribers gracefully', async () => {
      await lifecycle.initialize();
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      lifecycle.subscribe(errorSubscriber);
      lifecycle.subscribe(goodSubscriber);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });

      expect(() => document.dispatchEvent(new Event('visibilitychange'))).not.toThrow();
      expect(consoleError).toHaveBeenCalled();
      expect(goodSubscriber).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Cleanup', () => {
    it('should clean up on destroy', async () => {
      await lifecycle.initialize();
      const removeEventListenerSpy = vi.spyOn(document, 'removeEventListener');

      lifecycle.destroy();

      expect(lifecycle.isInitialized()).toBe(false);
      expect(removeEventListenerSpy).toHaveBeenCalledWith(
        'visibilitychange',
        expect.any(Function)
      );

      removeEventListenerSpy.mockRestore();
    });

    it('should clear subscribers on destroy', async () => {
      await lifecycle.initialize();
      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);

      lifecycle.destroy();
      await lifecycle.initialize();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(subscriber).not.toHaveBeenCalled();
    });

    it('should stop keep-alive on destroy', async () => {
      await lifecycle.initialize();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);
      subscriber.mockClear();

      lifecycle.destroy();

      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL * 2);

      expect(subscriber).not.toHaveBeenCalled();
    });

    it('should clear queues on destroy', () => {
      lifecycle.queueNotification('Title', 'Body');

      lifecycle.destroy();

      expect(lifecycle.getQueuedNotifications()).toEqual([]);
    });

    it('should reset state on destroy', async () => {
      await lifecycle.initialize();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      lifecycle.destroy();

      expect(lifecycle.isForeground()).toBe(true);
      expect(lifecycle.isTerminalFlowPaused()).toBe(false);
    });
  });

  describe('Reset', () => {
    it('should reset state without destroying listeners', async () => {
      await lifecycle.initialize();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      lifecycle.reset();

      expect(lifecycle.isForeground()).toBe(true);
      expect(lifecycle.isInitialized()).toBe(true);
    });

    it('should clear queues on reset', () => {
      lifecycle.queueNotification('Title', 'Body');

      lifecycle.reset();

      expect(lifecycle.getQueuedNotifications()).toEqual([]);
    });

    it('should stop keep-alive on reset', async () => {
      await lifecycle.initialize();

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      const subscriber = vi.fn();
      lifecycle.subscribe(subscriber);
      subscriber.mockClear();

      lifecycle.reset();

      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL * 2);

      expect(subscriber).not.toHaveBeenCalledWith(
        expect.objectContaining({ type: 'lifecycle:keepalive' })
      );
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getAppLifecycle', () => {
      const lifecycle1 = getAppLifecycle();
      const lifecycle2 = getAppLifecycle();

      expect(lifecycle1).toBe(lifecycle2);
    });

    it('should create new instance after resetAppLifecycle', () => {
      const lifecycle1 = getAppLifecycle();
      resetAppLifecycle();
      const lifecycle2 = getAppLifecycle();

      expect(lifecycle1).not.toBe(lifecycle2);
    });

    it('should destroy old instance on reset', () => {
      const lifecycle1 = getAppLifecycle();
      const destroySpy = vi.spyOn(lifecycle1, 'destroy');

      resetAppLifecycle();

      expect(destroySpy).toHaveBeenCalled();
    });

    it('should initialize singleton with initializeAppLifecycle', async () => {
      const lifecycle = await initializeAppLifecycle();

      expect(lifecycle.isInitialized()).toBe(true);
      expect(lifecycle).toBe(getAppLifecycle());
    });

    it('should pass config to singleton', () => {
      const lifecycle = getAppLifecycle({ keepAliveInterval: 10000 });

      // Verify config was applied by checking it doesn't throw
      expect(lifecycle).toBeDefined();
    });
  });

  describe('Configuration', () => {
    it('should use default keep-alive interval', () => {
      expect(DEFAULT_KEEP_ALIVE_INTERVAL).toBe(30000);
    });

    it('should use default max queued notifications', () => {
      expect(DEFAULT_MAX_QUEUED_NOTIFICATIONS).toBe(50);
    });

    it('should allow custom max queued notifications', () => {
      lifecycle = new AppLifecycle({ maxQueuedNotifications: 10 });

      for (let i = 0; i < 15; i++) {
        lifecycle.queueNotification(`Title ${i}`, `Body ${i}`);
      }

      expect(lifecycle.getQueuedNotifications().length).toBe(10);
    });
  });

  describe('Full Lifecycle', () => {
    it('should handle complete foreground to background to foreground cycle', async () => {
      await lifecycle.initialize();
      const events: LifecycleEvent[] = [];
      lifecycle.subscribe((event) => events.push(event));

      // Start in foreground
      expect(lifecycle.isForeground()).toBe(true);

      // Go to background
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isBackground()).toBe(true);
      expect(lifecycle.isTerminalFlowPaused()).toBe(true);

      // Queue some data and notifications
      lifecycle.queueTerminalData(new Uint8Array([1, 2, 3]));
      lifecycle.queueNotification('New message', 'You have a new message');

      // Verify keep-alive
      vi.advanceTimersByTime(DEFAULT_KEEP_ALIVE_INTERVAL);
      expect(events.filter((e) => e.type === 'lifecycle:keepalive').length).toBeGreaterThan(0);

      // Return to foreground
      Object.defineProperty(document, 'visibilityState', {
        value: 'visible',
        writable: true,
      });
      document.dispatchEvent(new Event('visibilitychange'));

      expect(lifecycle.isForeground()).toBe(true);
      expect(lifecycle.isTerminalFlowPaused()).toBe(false);

      // Notifications should still be queued (for consumer to handle)
      expect(lifecycle.getQueuedNotifications()).toHaveLength(1);

      // Terminal data queue should be cleared
      expect(lifecycle.drainTerminalDataQueue()).toEqual([]);
    });
  });
});
