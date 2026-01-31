import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createNotificationStore,
  getNotificationStore,
  resetNotificationStore,
  DEFAULT_NOTIFICATION_DURATION,
  type NotificationStore,
} from './notifications';

describe('Notification Store', () => {
  let store: NotificationStore;

  beforeEach(() => {
    vi.useFakeTimers();
    resetNotificationStore();
    store = createNotificationStore();
  });

  afterEach(() => {
    vi.useRealTimers();
    resetNotificationStore();
  });

  describe('Initial State', () => {
    it('should have correct initial state', () => {
      expect(store.state.notifications).toEqual({});
      expect(store.state.order).toEqual([]);
    });
  });

  describe('Show Notification', () => {
    it('should show a notification with generated ID', () => {
      const id = store.show({ title: 'Test' });

      expect(id).toBeDefined();
      expect(id).toMatch(/^notification-\d+-\d+$/);
    });

    it('should add notification to state', () => {
      const id = store.show({ title: 'Test' });

      expect(store.state.notifications[id]).toBeDefined();
      expect(store.state.notifications[id].title).toBe('Test');
    });

    it('should set default type to info', () => {
      const id = store.show({ title: 'Test' });

      expect(store.state.notifications[id].type).toBe('info');
    });

    it('should respect provided type', () => {
      const id = store.show({ title: 'Test', type: 'error' });

      expect(store.state.notifications[id].type).toBe('error');
    });

    it('should set default duration', () => {
      const id = store.show({ title: 'Test' });

      expect(store.state.notifications[id].duration).toBe(DEFAULT_NOTIFICATION_DURATION);
    });

    it('should respect custom duration', () => {
      const id = store.show({ title: 'Test', duration: 10000 });

      expect(store.state.notifications[id].duration).toBe(10000);
    });

    it('should add notification to order', () => {
      const id = store.show({ title: 'Test' });

      expect(store.state.order).toContain(id);
    });

    it('should include message when provided', () => {
      const id = store.show({ title: 'Test', message: 'Test message' });

      expect(store.state.notifications[id].message).toBe('Test message');
    });

    it('should include actions when provided', () => {
      const action = { label: 'Undo', onClick: vi.fn() };
      const id = store.show({ title: 'Test', actions: [action] });

      expect(store.state.notifications[id].actions).toHaveLength(1);
      expect(store.state.notifications[id].actions?.[0].label).toBe('Undo');
    });

    it('should emit notification:show event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const id = store.show({ title: 'Test' });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:show',
        notificationId: id,
      });
    });

    it('should set createdAt timestamp', () => {
      vi.setSystemTime(new Date('2024-01-01T12:00:00Z'));
      const id = store.show({ title: 'Test' });

      expect(store.state.notifications[id].createdAt).toBe(new Date('2024-01-01T12:00:00Z').getTime());
    });
  });

  describe('Convenience Methods', () => {
    it('should show info notification', () => {
      const id = store.info('Info title', 'Info message');

      expect(store.state.notifications[id].type).toBe('info');
      expect(store.state.notifications[id].title).toBe('Info title');
      expect(store.state.notifications[id].message).toBe('Info message');
    });

    it('should show success notification', () => {
      const id = store.success('Success title');

      expect(store.state.notifications[id].type).toBe('success');
      expect(store.state.notifications[id].title).toBe('Success title');
    });

    it('should show warning notification', () => {
      const id = store.warning('Warning title');

      expect(store.state.notifications[id].type).toBe('warning');
      expect(store.state.notifications[id].title).toBe('Warning title');
    });

    it('should show error notification', () => {
      const id = store.error('Error title');

      expect(store.state.notifications[id].type).toBe('error');
      expect(store.state.notifications[id].title).toBe('Error title');
    });

    it('should accept additional options in convenience methods', () => {
      const id = store.info('Title', 'Message', { duration: 10000 });

      expect(store.state.notifications[id].duration).toBe(10000);
    });
  });

  describe('Dismiss Notification', () => {
    it('should mark notification as dismissing', () => {
      const id = store.show({ title: 'Test' });

      store.dismiss(id);

      expect(store.state.notifications[id].dismissing).toBe(true);
    });

    it('should return true when dismissing existing notification', () => {
      const id = store.show({ title: 'Test' });

      const result = store.dismiss(id);

      expect(result).toBe(true);
    });

    it('should return false for non-existent notification', () => {
      const result = store.dismiss('non-existent');

      expect(result).toBe(false);
    });

    it('should emit notification:dismiss event', () => {
      const id = store.show({ title: 'Test' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.dismiss(id);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:dismiss',
        notificationId: id,
      });
    });

    it('should not emit event if already dismissing', () => {
      const id = store.show({ title: 'Test' });
      store.dismiss(id);
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      const result = store.dismiss(id);

      expect(result).toBe(true);
      expect(subscriber).not.toHaveBeenCalled();
    });
  });

  describe('Remove Notification', () => {
    it('should remove notification from state', () => {
      const id = store.show({ title: 'Test' });

      store.remove(id);

      expect(store.state.notifications[id]).toBeUndefined();
    });

    it('should remove from order', () => {
      const id = store.show({ title: 'Test' });

      store.remove(id);

      expect(store.state.order).not.toContain(id);
    });

    it('should return true when removing existing notification', () => {
      const id = store.show({ title: 'Test' });

      const result = store.remove(id);

      expect(result).toBe(true);
    });

    it('should return false for non-existent notification', () => {
      const result = store.remove('non-existent');

      expect(result).toBe(false);
    });

    it('should emit notification:dismissed event', () => {
      const id = store.show({ title: 'Test' });
      const subscriber = vi.fn();
      store.subscribe(subscriber);
      subscriber.mockClear();

      store.remove(id);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:dismissed',
        notificationId: id,
      });
    });
  });

  describe('Auto-dismiss Timer', () => {
    it('should auto-dismiss after duration', () => {
      const id = store.show({ title: 'Test', duration: 3000 });

      expect(store.state.notifications[id]).toBeDefined();
      expect(store.state.notifications[id].dismissing).toBeUndefined();

      vi.advanceTimersByTime(3000);

      expect(store.state.notifications[id].dismissing).toBe(true);
    });

    it('should use default duration when not specified', () => {
      const id = store.show({ title: 'Test' });

      vi.advanceTimersByTime(DEFAULT_NOTIFICATION_DURATION - 1);
      expect(store.state.notifications[id].dismissing).toBeUndefined();

      vi.advanceTimersByTime(1);
      expect(store.state.notifications[id].dismissing).toBe(true);
    });

    it('should not auto-dismiss when duration is 0', () => {
      const id = store.show({ title: 'Test', duration: 0 });

      vi.advanceTimersByTime(10000);

      expect(store.state.notifications[id].dismissing).toBeUndefined();
    });

    it('should clear timer when manually dismissed', () => {
      const id = store.show({ title: 'Test', duration: 5000 });

      // Manually dismiss before timer
      store.dismiss(id);
      expect(store.state.notifications[id].dismissing).toBe(true);

      // Advance timer - should not cause issues
      vi.advanceTimersByTime(5000);
      expect(store.state.notifications[id]).toBeDefined(); // Still there until removed
    });
  });

  describe('Pause and Resume Timer', () => {
    it('should pause auto-dismiss timer', () => {
      const id = store.show({ title: 'Test', duration: 5000 });

      vi.advanceTimersByTime(2000);
      store.pauseTimer(id);

      vi.advanceTimersByTime(10000);

      expect(store.state.notifications[id].dismissing).toBeUndefined();
    });

    it('should resume timer with remaining time', () => {
      vi.setSystemTime(new Date('2024-01-01T12:00:00Z'));
      const id = store.show({ title: 'Test', duration: 5000 });

      vi.advanceTimersByTime(2000);
      store.pauseTimer(id);

      vi.advanceTimersByTime(1000);
      vi.setSystemTime(new Date('2024-01-01T12:00:03Z'));
      store.resumeTimer(id);

      // Should have about 2 seconds remaining (5000 - 3000 elapsed)
      vi.advanceTimersByTime(1999);
      expect(store.state.notifications[id].dismissing).toBeUndefined();

      vi.advanceTimersByTime(1);
      expect(store.state.notifications[id].dismissing).toBe(true);
    });

    it('should not resume if notification is dismissing', () => {
      const id = store.show({ title: 'Test', duration: 5000 });

      store.dismiss(id);
      store.resumeTimer(id);

      // Should not throw or create new timer
      vi.advanceTimersByTime(10000);
    });
  });

  describe('Dismiss All', () => {
    it('should dismiss all notifications', () => {
      const id1 = store.show({ title: 'Test 1' });
      const id2 = store.show({ title: 'Test 2' });
      const id3 = store.show({ title: 'Test 3' });

      store.dismissAll();

      expect(store.state.notifications[id1].dismissing).toBe(true);
      expect(store.state.notifications[id2].dismissing).toBe(true);
      expect(store.state.notifications[id3].dismissing).toBe(true);
    });
  });

  describe('Getters', () => {
    it('should get all notifications in order', () => {
      const id1 = store.show({ title: 'First' });
      const id2 = store.show({ title: 'Second' });
      const id3 = store.show({ title: 'Third' });

      const notifications = store.getNotifications();

      expect(notifications).toHaveLength(3);
      expect(notifications[0].id).toBe(id1);
      expect(notifications[1].id).toBe(id2);
      expect(notifications[2].id).toBe(id3);
    });

    it('should get notification by ID', () => {
      const id = store.show({ title: 'Test' });

      const notification = store.getNotification(id);

      expect(notification).not.toBeNull();
      expect(notification?.title).toBe('Test');
    });

    it('should return null for non-existent notification', () => {
      expect(store.getNotification('non-existent')).toBeNull();
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.show({ title: 'Test' });

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = store.subscribe(subscriber);

      store.show({ title: 'Test 1' });
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      store.show({ title: 'Test 2' });

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

      expect(() => store.show({ title: 'Test' })).not.toThrow();

      expect(errorSubscriber).toHaveBeenCalledTimes(1);
      expect(goodSubscriber).toHaveBeenCalledTimes(1);
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Reset', () => {
    it('should reset store to initial state', () => {
      store.show({ title: 'Test 1' });
      store.show({ title: 'Test 2' });

      store.reset();

      expect(store.state.notifications).toEqual({});
      expect(store.state.order).toEqual([]);
    });

    it('should clear all timers on reset', () => {
      const id = store.show({ title: 'Test', duration: 5000 });

      store.reset();

      // Timer should be cleared, so advancing time should not cause issues
      vi.advanceTimersByTime(10000);

      // Notification was cleared, so we can't check dismissing
      expect(store.state.notifications[id]).toBeUndefined();
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getNotificationStore', () => {
      const store1 = getNotificationStore();
      const store2 = getNotificationStore();

      expect(store1).toBe(store2);
    });

    it('should create new instance after resetNotificationStore', () => {
      const store1 = getNotificationStore();
      resetNotificationStore();
      const store2 = getNotificationStore();

      expect(store1).not.toBe(store2);
    });
  });

  describe('Notification Lifecycle', () => {
    it('should handle complete notification lifecycle', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      // Show notification
      const id = store.show({
        title: 'Test',
        message: 'Test message',
        type: 'success',
        duration: 5000,
      });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:show',
        notificationId: id,
      });

      // Verify in state
      expect(store.state.notifications[id]).toBeDefined();
      expect(store.state.notifications[id].type).toBe('success');

      // Dismiss (starts animation)
      subscriber.mockClear();
      store.dismiss(id);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:dismiss',
        notificationId: id,
      });
      expect(store.state.notifications[id].dismissing).toBe(true);

      // Remove (after animation)
      subscriber.mockClear();
      store.remove(id);

      expect(subscriber).toHaveBeenCalledWith({
        type: 'notification:dismissed',
        notificationId: id,
      });
      expect(store.state.notifications[id]).toBeUndefined();
    });

    it('should handle multiple notifications correctly', () => {
      const id1 = store.info('Info');
      const id2 = store.success('Success');
      const id3 = store.error('Error');

      expect(store.getNotifications()).toHaveLength(3);
      expect(store.state.order).toEqual([id1, id2, id3]);

      // Dismiss middle one
      store.dismiss(id2);
      expect(store.state.notifications[id2].dismissing).toBe(true);

      // Remove it
      store.remove(id2);
      expect(store.getNotifications()).toHaveLength(2);
      expect(store.state.order).toEqual([id1, id3]);

      // Auto-dismiss others
      vi.advanceTimersByTime(DEFAULT_NOTIFICATION_DURATION);
      expect(store.state.notifications[id1].dismissing).toBe(true);
      expect(store.state.notifications[id3].dismissing).toBe(true);
    });
  });
});
