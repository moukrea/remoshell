import { createStore, produce } from 'solid-js/store';
import { createSignal, batch } from 'solid-js';

/**
 * Device platform types
 */
export type DevicePlatform = 'windows' | 'macos' | 'linux' | 'android' | 'ios' | 'unknown';

/**
 * Device online/offline status
 */
export type DeviceStatus = 'online' | 'offline';

/**
 * Represents a connection history entry
 */
export interface ConnectionHistoryEntry {
  connectedAt: number;
  disconnectedAt?: number;
  duration?: number;
  error?: string;
}

/**
 * Represents a paired device
 */
export interface Device {
  id: string;
  name: string;
  platform: DevicePlatform;
  status: DeviceStatus;
  lastSeen: number;
  pairedAt: number;
  connectionHistory: ConnectionHistoryEntry[];
}

/**
 * Device store state
 */
export interface DeviceState {
  devices: Record<string, Device>;
}

/**
 * Event types emitted by the device store
 */
export type DeviceEventType =
  | 'device:added'
  | 'device:removed'
  | 'device:renamed'
  | 'device:statusChanged'
  | 'device:connected'
  | 'device:disconnected';

/**
 * Event payload types
 */
export interface DeviceEvent {
  type: DeviceEventType;
  deviceId?: string;
  data?: unknown;
}

/**
 * Event subscriber callback type
 */
export type DeviceEventSubscriber = (event: DeviceEvent) => void;

/**
 * Options for adding a new device
 */
export interface AddDeviceOptions {
  id: string;
  name: string;
  platform: DevicePlatform;
}

/**
 * Initial state for the device store
 */
const initialState: DeviceState = {
  devices: {},
};

/**
 * Creates a device store for managing paired devices
 */
export function createDeviceStore() {
  const [state, setState] = createStore<DeviceState>({ ...initialState });
  const [subscribers] = createSignal<Set<DeviceEventSubscriber>>(new Set());

  /**
   * Emit an event to all subscribers
   */
  const emit = (event: DeviceEvent): void => {
    subscribers().forEach(subscriber => {
      try {
        subscriber(event);
      } catch (error) {
        console.error('Error in device event subscriber:', error);
      }
    });
  };

  /**
   * Subscribe to device events
   */
  const subscribe = (callback: DeviceEventSubscriber): (() => void) => {
    subscribers().add(callback);
    return () => {
      subscribers().delete(callback);
    };
  };

  /**
   * Add a new paired device
   */
  const addDevice = (options: AddDeviceOptions): void => {
    const now = Date.now();
    const device: Device = {
      id: options.id,
      name: options.name,
      platform: options.platform,
      status: 'offline',
      lastSeen: now,
      pairedAt: now,
      connectionHistory: [],
    };

    setState(
      produce((s) => {
        s.devices[options.id] = device;
      })
    );
    emit({ type: 'device:added', deviceId: options.id });
  };

  /**
   * Remove a device
   */
  const removeDevice = (deviceId: string): boolean => {
    const device = state.devices[deviceId];
    if (!device) {
      console.warn(`Cannot remove device: device ${deviceId} not found`);
      return false;
    }

    setState(
      produce((s) => {
        delete s.devices[deviceId];
      })
    );
    emit({ type: 'device:removed', deviceId });
    return true;
  };

  /**
   * Rename a device
   */
  const renameDevice = (deviceId: string, newName: string): boolean => {
    const device = state.devices[deviceId];
    if (!device) {
      console.warn(`Cannot rename device: device ${deviceId} not found`);
      return false;
    }

    if (newName.trim().length === 0) {
      console.warn('Cannot rename device: name cannot be empty');
      return false;
    }

    setState(
      produce((s) => {
        const dev = s.devices[deviceId];
        if (dev) {
          dev.name = newName.trim();
        }
      })
    );
    emit({ type: 'device:renamed', deviceId, data: { newName: newName.trim() } });
    return true;
  };

  /**
   * Update device status (online/offline)
   */
  const setDeviceStatus = (deviceId: string, status: DeviceStatus): boolean => {
    const device = state.devices[deviceId];
    if (!device) {
      console.warn(`Cannot update device status: device ${deviceId} not found`);
      return false;
    }

    batch(() => {
      setState(
        produce((s) => {
          const dev = s.devices[deviceId];
          if (dev) {
            const previousStatus = dev.status;
            dev.status = status;
            dev.lastSeen = Date.now();

            // Handle connection history
            if (status === 'online' && previousStatus === 'offline') {
              // Starting a new connection
              dev.connectionHistory.push({
                connectedAt: Date.now(),
              });
            } else if (status === 'offline' && previousStatus === 'online') {
              // Connection ended
              const lastEntry = dev.connectionHistory[dev.connectionHistory.length - 1];
              if (lastEntry && !lastEntry.disconnectedAt) {
                lastEntry.disconnectedAt = Date.now();
                lastEntry.duration = lastEntry.disconnectedAt - lastEntry.connectedAt;
              }
            }
          }
        })
      );
    });
    emit({ type: 'device:statusChanged', deviceId, data: { status } });
    return true;
  };

  /**
   * Record a connection event
   */
  const recordConnection = (deviceId: string): boolean => {
    const device = state.devices[deviceId];
    if (!device) {
      console.warn(`Cannot record connection: device ${deviceId} not found`);
      return false;
    }

    batch(() => {
      setState(
        produce((s) => {
          const dev = s.devices[deviceId];
          if (dev) {
            dev.status = 'online';
            dev.lastSeen = Date.now();
            dev.connectionHistory.push({
              connectedAt: Date.now(),
            });
          }
        })
      );
    });
    emit({ type: 'device:connected', deviceId });
    return true;
  };

  /**
   * Record a disconnection event
   */
  const recordDisconnection = (deviceId: string, error?: string): boolean => {
    const device = state.devices[deviceId];
    if (!device) {
      console.warn(`Cannot record disconnection: device ${deviceId} not found`);
      return false;
    }

    batch(() => {
      setState(
        produce((s) => {
          const dev = s.devices[deviceId];
          if (dev) {
            dev.status = 'offline';
            dev.lastSeen = Date.now();

            // Update the last connection history entry
            const lastEntry = dev.connectionHistory[dev.connectionHistory.length - 1];
            if (lastEntry && !lastEntry.disconnectedAt) {
              lastEntry.disconnectedAt = Date.now();
              lastEntry.duration = lastEntry.disconnectedAt - lastEntry.connectedAt;
              if (error) {
                lastEntry.error = error;
              }
            }
          }
        })
      );
    });
    emit({ type: 'device:disconnected', deviceId, data: { error } });
    return true;
  };

  /**
   * Get a device by ID
   */
  const getDevice = (deviceId: string): Device | null => {
    return state.devices[deviceId] ?? null;
  };

  /**
   * Get all devices
   */
  const getAllDevices = (): Device[] => {
    return Object.values(state.devices);
  };

  /**
   * Get online devices
   */
  const getOnlineDevices = (): Device[] => {
    return Object.values(state.devices).filter(device => device.status === 'online');
  };

  /**
   * Get offline devices
   */
  const getOfflineDevices = (): Device[] => {
    return Object.values(state.devices).filter(device => device.status === 'offline');
  };

  /**
   * Get devices sorted by last seen (most recent first)
   */
  const getDevicesByLastSeen = (): Device[] => {
    return Object.values(state.devices).sort((a, b) => b.lastSeen - a.lastSeen);
  };

  /**
   * Get connection history for a device
   */
  const getConnectionHistory = (deviceId: string): ConnectionHistoryEntry[] => {
    const device = state.devices[deviceId];
    return device ? [...device.connectionHistory] : [];
  };

  /**
   * Check if a device exists
   */
  const hasDevice = (deviceId: string): boolean => {
    return deviceId in state.devices;
  };

  /**
   * Reset the store to initial state
   */
  const reset = (): void => {
    setState(
      produce((s) => {
        for (const key of Object.keys(s.devices)) {
          delete s.devices[key];
        }
      })
    );
  };

  return {
    // State (readonly)
    state,

    // Device actions
    addDevice,
    removeDevice,
    renameDevice,
    setDeviceStatus,
    recordConnection,
    recordDisconnection,

    // Getters
    getDevice,
    getAllDevices,
    getOnlineDevices,
    getOfflineDevices,
    getDevicesByLastSeen,
    getConnectionHistory,
    hasDevice,

    // Event subscriptions
    subscribe,

    // Utility
    reset,
  };
}

/**
 * Type for the device store instance
 */
export type DeviceStore = ReturnType<typeof createDeviceStore>;

/**
 * Singleton instance of the device store
 */
let deviceStoreInstance: DeviceStore | null = null;

/**
 * Get or create the singleton device store instance
 */
export function getDeviceStore(): DeviceStore {
  if (!deviceStoreInstance) {
    deviceStoreInstance = createDeviceStore();
  }
  return deviceStoreInstance;
}

/**
 * Reset the singleton instance (useful for testing)
 */
export function resetDeviceStore(): void {
  if (deviceStoreInstance) {
    deviceStoreInstance.reset();
  }
  deviceStoreInstance = null;
}
