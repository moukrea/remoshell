import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createDeviceStore,
  getDeviceStore,
  resetDeviceStore,
  type DeviceStore,
} from './devices';

describe('Device Store', () => {
  let store: DeviceStore;

  beforeEach(() => {
    resetDeviceStore();
    store = createDeviceStore();
  });

  afterEach(() => {
    resetDeviceStore();
  });

  describe('Initial State', () => {
    it('should have correct initial state', () => {
      expect(store.state.devices).toEqual({});
    });

    it('should return empty array for getAllDevices initially', () => {
      expect(store.getAllDevices()).toEqual([]);
    });
  });

  describe('Add Device', () => {
    it('should add a device with correct properties', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      const device = store.getDevice('device-1');
      expect(device).not.toBeNull();
      expect(device?.id).toBe('device-1');
      expect(device?.name).toBe('My Laptop');
      expect(device?.platform).toBe('windows');
      expect(device?.status).toBe('offline');
      expect(device?.connectionHistory).toEqual([]);
    });

    it('should emit device:added event', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:added',
        deviceId: 'device-1',
      });
    });

    it('should set lastSeen and pairedAt to current time', () => {
      const before = Date.now();
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'linux',
      });
      const after = Date.now();

      const device = store.getDevice('device-1');
      expect(device?.lastSeen).toBeGreaterThanOrEqual(before);
      expect(device?.lastSeen).toBeLessThanOrEqual(after);
      expect(device?.pairedAt).toBeGreaterThanOrEqual(before);
      expect(device?.pairedAt).toBeLessThanOrEqual(after);
    });
  });

  describe('Remove Device', () => {
    it('should remove an existing device', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'macos',
      });

      const result = store.removeDevice('device-1');

      expect(result).toBe(true);
      expect(store.getDevice('device-1')).toBeNull();
    });

    it('should emit device:removed event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'macos',
      });

      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.removeDevice('device-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:removed',
        deviceId: 'device-1',
      });
    });

    it('should return false when removing non-existent device', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = store.removeDevice('non-existent');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Rename Device', () => {
    it('should rename an existing device', () => {
      store.addDevice({
        id: 'device-1',
        name: 'Old Name',
        platform: 'linux',
      });

      const result = store.renameDevice('device-1', 'New Name');

      expect(result).toBe(true);
      expect(store.getDevice('device-1')?.name).toBe('New Name');
    });

    it('should emit device:renamed event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'Old Name',
        platform: 'linux',
      });

      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.renameDevice('device-1', 'New Name');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:renamed',
        deviceId: 'device-1',
        data: { newName: 'New Name' },
      });
    });

    it('should trim whitespace from new name', () => {
      store.addDevice({
        id: 'device-1',
        name: 'Old Name',
        platform: 'linux',
      });

      store.renameDevice('device-1', '  Trimmed Name  ');

      expect(store.getDevice('device-1')?.name).toBe('Trimmed Name');
    });

    it('should return false for empty name', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      store.addDevice({
        id: 'device-1',
        name: 'Old Name',
        platform: 'linux',
      });

      const result = store.renameDevice('device-1', '   ');

      expect(result).toBe(false);
      expect(store.getDevice('device-1')?.name).toBe('Old Name');

      consoleWarn.mockRestore();
    });

    it('should return false when renaming non-existent device', () => {
      const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = store.renameDevice('non-existent', 'New Name');

      expect(result).toBe(false);
      expect(consoleWarn).toHaveBeenCalled();

      consoleWarn.mockRestore();
    });
  });

  describe('Device Status', () => {
    it('should update device status', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      expect(store.getDevice('device-1')?.status).toBe('offline');

      store.setDeviceStatus('device-1', 'online');

      expect(store.getDevice('device-1')?.status).toBe('online');
    });

    it('should emit device:statusChanged event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.setDeviceStatus('device-1', 'online');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:statusChanged',
        deviceId: 'device-1',
        data: { status: 'online' },
      });
    });

    it('should update lastSeen when status changes', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      const originalLastSeen = store.getDevice('device-1')?.lastSeen;

      // Small delay to ensure time difference
      const before = Date.now();
      store.setDeviceStatus('device-1', 'online');
      const after = Date.now();

      const newLastSeen = store.getDevice('device-1')?.lastSeen;
      expect(newLastSeen).toBeGreaterThanOrEqual(before);
      expect(newLastSeen).toBeLessThanOrEqual(after);
    });

    it('should add connection history entry when going online', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.setDeviceStatus('device-1', 'online');

      const history = store.getConnectionHistory('device-1');
      expect(history).toHaveLength(1);
      expect(history[0].connectedAt).toBeDefined();
      expect(history[0].disconnectedAt).toBeUndefined();
    });

    it('should complete connection history entry when going offline', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.setDeviceStatus('device-1', 'online');
      store.setDeviceStatus('device-1', 'offline');

      const history = store.getConnectionHistory('device-1');
      expect(history).toHaveLength(1);
      expect(history[0].disconnectedAt).toBeDefined();
      expect(history[0].duration).toBeDefined();
    });
  });

  describe('Record Connection/Disconnection', () => {
    it('should record connection event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.recordConnection('device-1');

      expect(store.getDevice('device-1')?.status).toBe('online');
      const history = store.getConnectionHistory('device-1');
      expect(history).toHaveLength(1);
    });

    it('should emit device:connected event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.recordConnection('device-1');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:connected',
        deviceId: 'device-1',
      });
    });

    it('should record disconnection event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.recordConnection('device-1');
      store.recordDisconnection('device-1');

      expect(store.getDevice('device-1')?.status).toBe('offline');
      const history = store.getConnectionHistory('device-1');
      expect(history[0].disconnectedAt).toBeDefined();
    });

    it('should record disconnection with error', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.recordConnection('device-1');
      store.recordDisconnection('device-1', 'Connection timeout');

      const history = store.getConnectionHistory('device-1');
      expect(history[0].error).toBe('Connection timeout');
    });

    it('should emit device:disconnected event', () => {
      store.addDevice({
        id: 'device-1',
        name: 'My Laptop',
        platform: 'windows',
      });

      store.recordConnection('device-1');

      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.recordDisconnection('device-1', 'Error');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'device:disconnected',
        deviceId: 'device-1',
        data: { error: 'Error' },
      });
    });
  });

  describe('Getters', () => {
    beforeEach(() => {
      store.addDevice({ id: 'device-1', name: 'Windows PC', platform: 'windows' });
      store.addDevice({ id: 'device-2', name: 'MacBook', platform: 'macos' });
      store.addDevice({ id: 'device-3', name: 'Linux Server', platform: 'linux' });

      store.setDeviceStatus('device-1', 'online');
      store.setDeviceStatus('device-3', 'online');
    });

    it('should return all devices', () => {
      const devices = store.getAllDevices();
      expect(devices).toHaveLength(3);
    });

    it('should return online devices', () => {
      const online = store.getOnlineDevices();
      expect(online).toHaveLength(2);
      expect(online.map(d => d.id)).toContain('device-1');
      expect(online.map(d => d.id)).toContain('device-3');
    });

    it('should return offline devices', () => {
      const offline = store.getOfflineDevices();
      expect(offline).toHaveLength(1);
      expect(offline[0].id).toBe('device-2');
    });

    it('should return devices sorted by last seen', () => {
      // All devices have lastSeen set during the beforeEach
      // The sorting should work based on those timestamps
      const sorted = store.getDevicesByLastSeen();

      // Verify we get all devices back
      expect(sorted).toHaveLength(3);

      // Verify they are sorted by lastSeen descending (most recent first)
      for (let i = 0; i < sorted.length - 1; i++) {
        expect(sorted[i].lastSeen).toBeGreaterThanOrEqual(sorted[i + 1].lastSeen);
      }
    });

    it('should check if device exists', () => {
      expect(store.hasDevice('device-1')).toBe(true);
      expect(store.hasDevice('non-existent')).toBe(false);
    });
  });

  describe('Event Subscriptions', () => {
    it('should allow subscribing to events', () => {
      const subscriber = vi.fn();
      store.subscribe(subscriber);

      store.addDevice({ id: 'device-1', name: 'Test', platform: 'windows' });

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should allow unsubscribing from events', () => {
      const subscriber = vi.fn();
      const unsubscribe = store.subscribe(subscriber);

      store.addDevice({ id: 'device-1', name: 'Test 1', platform: 'windows' });
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      store.addDevice({ id: 'device-2', name: 'Test 2', platform: 'macos' });

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

      expect(() => store.addDevice({ id: 'device-1', name: 'Test', platform: 'windows' })).not.toThrow();

      expect(errorSubscriber).toHaveBeenCalledTimes(1);
      expect(goodSubscriber).toHaveBeenCalledTimes(1);
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });
  });

  describe('Reset', () => {
    it('should reset store to initial state', () => {
      store.addDevice({ id: 'device-1', name: 'Test 1', platform: 'windows' });
      store.addDevice({ id: 'device-2', name: 'Test 2', platform: 'macos' });

      store.reset();

      expect(store.getAllDevices()).toEqual([]);
    });
  });

  describe('Singleton', () => {
    it('should return the same instance from getDeviceStore', () => {
      const store1 = getDeviceStore();
      const store2 = getDeviceStore();

      expect(store1).toBe(store2);
    });

    it('should create new instance after resetDeviceStore', () => {
      const store1 = getDeviceStore();
      resetDeviceStore();
      const store2 = getDeviceStore();

      expect(store1).not.toBe(store2);
    });
  });

  describe('Connection History', () => {
    it('should maintain multiple connection history entries', () => {
      store.addDevice({ id: 'device-1', name: 'Test', platform: 'windows' });

      // First connection
      store.recordConnection('device-1');
      store.recordDisconnection('device-1');

      // Second connection
      store.recordConnection('device-1');
      store.recordDisconnection('device-1');

      // Third connection
      store.recordConnection('device-1');

      const history = store.getConnectionHistory('device-1');
      expect(history).toHaveLength(3);
      expect(history[0].disconnectedAt).toBeDefined();
      expect(history[1].disconnectedAt).toBeDefined();
      expect(history[2].disconnectedAt).toBeUndefined(); // Still connected
    });

    it('should return empty array for non-existent device', () => {
      const history = store.getConnectionHistory('non-existent');
      expect(history).toEqual([]);
    });

    it('should return a copy of connection history', () => {
      store.addDevice({ id: 'device-1', name: 'Test', platform: 'windows' });
      store.recordConnection('device-1');

      const history1 = store.getConnectionHistory('device-1');
      const history2 = store.getConnectionHistory('device-1');

      expect(history1).not.toBe(history2);
      expect(history1).toEqual(history2);
    });
  });
});
