import { describe, it, expect, vi, beforeEach, afterEach, Mock } from 'vitest';
import {
  TauriIPCBridge,
  getTauriIPCBridge,
  resetTauriIPCBridge,
  TauriNotAvailableError,
  TauriCommandError,
  uint8ArrayToBase64,
  base64ToUint8Array,
  type ConnectionEvent,
  type PairedDevice,
} from './TauriIPCBridge';

// ============================================================================
// Mock Setup
// ============================================================================

/**
 * Create a mock Tauri API
 */
function createMockTauriAPI() {
  const eventHandlers: Map<string, ((event: { payload: unknown }) => void)[]> = new Map();

  return {
    core: {
      invoke: vi.fn(),
    },
    event: {
      listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
        if (!eventHandlers.has(event)) {
          eventHandlers.set(event, []);
        }
        eventHandlers.get(event)!.push(handler);

        // Return unlisten function
        return () => {
          const handlers = eventHandlers.get(event);
          if (handlers) {
            const index = handlers.indexOf(handler);
            if (index > -1) {
              handlers.splice(index, 1);
            }
          }
        };
      }),
    },
    // Helper to emit events for testing
    _emitEvent: (event: string, payload: unknown) => {
      const handlers = eventHandlers.get(event);
      if (handlers) {
        handlers.forEach((handler) => handler({ payload }));
      }
    },
    _eventHandlers: eventHandlers,
  };
}

describe('TauriIPCBridge', () => {
  let mockTauriAPI: ReturnType<typeof createMockTauriAPI>;
  let bridge: TauriIPCBridge;

  beforeEach(() => {
    resetTauriIPCBridge();
    mockTauriAPI = createMockTauriAPI();
    (window as unknown as { __TAURI__: unknown }).__TAURI__ = mockTauriAPI;
    bridge = new TauriIPCBridge();
  });

  afterEach(() => {
    bridge.destroy();
    resetTauriIPCBridge();
    delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
  });

  // ==========================================================================
  // isAvailable Tests
  // ==========================================================================

  describe('isAvailable', () => {
    it('should return true when Tauri is available', () => {
      expect(TauriIPCBridge.isAvailable()).toBe(true);
    });

    it('should return false when Tauri is not available', () => {
      delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
      expect(TauriIPCBridge.isAvailable()).toBe(false);
    });

    it('should return false when window is undefined', () => {
      const originalWindow = global.window;
      // @ts-expect-error - Testing edge case
      delete global.window;

      // Need to re-check with no window
      expect(typeof window === 'undefined' || !(window as unknown as { __TAURI__?: unknown }).__TAURI__).toBe(true);

      global.window = originalWindow;
    });
  });

  // ==========================================================================
  // Error Handling Tests
  // ==========================================================================

  describe('Error Handling', () => {
    it('should throw TauriNotAvailableError when Tauri is not available', async () => {
      delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
      const localBridge = new TauriIPCBridge();

      await expect(localBridge.getDeviceKeys()).rejects.toThrow(TauriNotAvailableError);
    });

    it('should wrap command errors as TauriCommandError', async () => {
      mockTauriAPI.core.invoke.mockRejectedValue({
        code: 'TEST_ERROR',
        message: 'Test error message',
      });

      await expect(bridge.getDeviceKeys()).rejects.toThrow(TauriCommandError);

      try {
        await bridge.getDeviceKeys();
      } catch (error) {
        expect(error).toBeInstanceOf(TauriCommandError);
        expect((error as TauriCommandError).code).toBe('TEST_ERROR');
        expect((error as TauriCommandError).message).toBe('Test error message');
      }
    });

    it('should wrap unknown errors with UNKNOWN_ERROR code', async () => {
      mockTauriAPI.core.invoke.mockRejectedValue('Some string error');

      try {
        await bridge.getDeviceKeys();
      } catch (error) {
        expect(error).toBeInstanceOf(TauriCommandError);
        expect((error as TauriCommandError).code).toBe('UNKNOWN_ERROR');
        expect((error as TauriCommandError).message).toBe('Some string error');
      }
    });

    it('should wrap Error objects with UNKNOWN_ERROR code', async () => {
      mockTauriAPI.core.invoke.mockRejectedValue(new Error('Native error'));

      try {
        await bridge.getDeviceKeys();
      } catch (error) {
        expect(error).toBeInstanceOf(TauriCommandError);
        expect((error as TauriCommandError).code).toBe('UNKNOWN_ERROR');
        expect((error as TauriCommandError).message).toBe('Native error');
      }
    });
  });

  // ==========================================================================
  // Initialization Commands
  // ==========================================================================

  describe('initializeApp', () => {
    it('should call initialize_app command with correct parameters', async () => {
      const mockResponse = {
        initialized: true,
        node_id: 'node-123',
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const result = await bridge.initializeApp({
        database_path: '/path/to/db',
        relay_url: 'https://relay.example.com',
      });

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('initialize_app', {
        request: {
          database_path: '/path/to/db',
          relay_url: 'https://relay.example.com',
        },
      });
      expect(result).toEqual(mockResponse);
    });
  });

  // ==========================================================================
  // QUIC Connection Commands
  // ==========================================================================

  describe('connectQuic', () => {
    it('should call connect_quic command with node_id', async () => {
      const mockResponse = {
        connected: true,
        peer_node_id: 'peer-123',
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const result = await bridge.connectQuic({
        node_id: 'peer-123',
      });

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('connect_quic', {
        request: { node_id: 'peer-123' },
      });
      expect(result).toEqual(mockResponse);
    });

    it('should pass relay_url and direct_addresses', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue({ connected: true, peer_node_id: 'peer-123' });

      await bridge.connectQuic({
        node_id: 'peer-123',
        relay_url: 'https://relay.example.com',
        direct_addresses: ['127.0.0.1:9000', '192.168.1.1:9000'],
      });

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('connect_quic', {
        request: {
          node_id: 'peer-123',
          relay_url: 'https://relay.example.com',
          direct_addresses: ['127.0.0.1:9000', '192.168.1.1:9000'],
        },
      });
    });
  });

  describe('disconnectQuic', () => {
    it('should call disconnect_quic command', async () => {
      const mockResponse = { disconnected: true };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const result = await bridge.disconnectQuic();

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('disconnect_quic', undefined);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('sendQuicData', () => {
    it('should call send_quic_data with base64-encoded data', async () => {
      const mockResponse = { sent: true, bytes_sent: 5 };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const data = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
      const result = await bridge.sendQuicData('Control', data);

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('send_quic_data', {
        request: {
          channel: 'Control',
          data: 'SGVsbG8=', // base64 of "Hello"
        },
      });
      expect(result).toEqual(mockResponse);
    });

    it('should work with Terminal and Files channels', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue({ sent: true, bytes_sent: 1 });

      await bridge.sendQuicData('Terminal', new Uint8Array([1]));
      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('send_quic_data', {
        request: { channel: 'Terminal', data: 'AQ==' },
      });

      await bridge.sendQuicData('Files', new Uint8Array([2]));
      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('send_quic_data', {
        request: { channel: 'Files', data: 'Ag==' },
      });
    });
  });

  describe('getConnectionStatus', () => {
    it('should call get_connection_status command', async () => {
      const mockResponse = {
        state: 'Connected' as const,
        peer_node_id: 'peer-123',
        local_node_id: 'local-456',
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const result = await bridge.getConnectionStatus();

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('get_connection_status', undefined);
      expect(result).toEqual(mockResponse);
    });
  });

  // ==========================================================================
  // Device Key Commands
  // ==========================================================================

  describe('getDeviceKeys', () => {
    it('should call get_device_keys command', async () => {
      const mockResponse = {
        secret_key: 'base64-secret-key',
        newly_generated: false,
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockResponse);

      const result = await bridge.getDeviceKeys();

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('get_device_keys', undefined);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('hasDeviceKeys', () => {
    it('should call has_device_keys command', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue(true);

      const result = await bridge.hasDeviceKeys();

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('has_device_keys', undefined);
      expect(result).toBe(true);
    });
  });

  // ==========================================================================
  // Paired Device Commands
  // ==========================================================================

  describe('getPairedDevices', () => {
    it('should call get_paired_devices command', async () => {
      const mockDevices: PairedDevice[] = [
        {
          id: 'device-1',
          name: 'Test Device',
          public_key: 'public-key-123',
          created_at: 1234567890,
          last_seen: 1234567900,
        },
      ];
      mockTauriAPI.core.invoke.mockResolvedValue(mockDevices);

      const result = await bridge.getPairedDevices();

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('get_paired_devices', undefined);
      expect(result).toEqual(mockDevices);
    });
  });

  describe('getPairedDevice', () => {
    it('should call get_paired_device with device_id', async () => {
      const mockDevice: PairedDevice = {
        id: 'device-1',
        name: 'Test Device',
        public_key: 'public-key-123',
        created_at: 1234567890,
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockDevice);

      const result = await bridge.getPairedDevice('device-1');

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('get_paired_device', {
        device_id: 'device-1',
      });
      expect(result).toEqual(mockDevice);
    });

    it('should return null for non-existent device', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue(null);

      const result = await bridge.getPairedDevice('non-existent');

      expect(result).toBeNull();
    });
  });

  describe('storePairedDevice', () => {
    it('should call store_paired_device command', async () => {
      const mockDevice: PairedDevice = {
        id: 'device-1',
        name: 'New Device',
        public_key: 'public-key-123',
        created_at: 1234567890,
      };
      mockTauriAPI.core.invoke.mockResolvedValue(mockDevice);

      const result = await bridge.storePairedDevice({
        id: 'device-1',
        name: 'New Device',
        public_key: 'public-key-123',
      });

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('store_paired_device', {
        request: {
          id: 'device-1',
          name: 'New Device',
          public_key: 'public-key-123',
        },
      });
      expect(result).toEqual(mockDevice);
    });
  });

  describe('removePairedDevice', () => {
    it('should call remove_paired_device command', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue({ removed: true });

      const result = await bridge.removePairedDevice('device-1');

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('remove_paired_device', {
        device_id: 'device-1',
      });
      expect(result).toEqual({ removed: true });
    });
  });

  describe('updateDeviceLastSeen', () => {
    it('should call update_device_last_seen command', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue(true);

      const result = await bridge.updateDeviceLastSeen('device-1');

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('update_device_last_seen', {
        device_id: 'device-1',
      });
      expect(result).toBe(true);
    });
  });

  // ==========================================================================
  // Notification Commands
  // ==========================================================================

  describe('showNotification', () => {
    it('should call show_native_notification command', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue({ shown: true });

      const result = await bridge.showNotification('Title', 'Body');

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('show_native_notification', {
        request: {
          title: 'Title',
          body: 'Body',
        },
      });
      expect(result).toEqual({ shown: true });
    });

    it('should include icon when provided', async () => {
      mockTauriAPI.core.invoke.mockResolvedValue({ shown: true });

      await bridge.showNotification('Title', 'Body', 'icon.png');

      expect(mockTauriAPI.core.invoke).toHaveBeenCalledWith('show_native_notification', {
        request: {
          title: 'Title',
          body: 'Body',
          icon: 'icon.png',
        },
      });
    });
  });

  // ==========================================================================
  // Event Listening Tests
  // ==========================================================================

  describe('Event Listening', () => {
    it('should subscribe to all event types when startListening is called', async () => {
      await bridge.startListening();

      expect(mockTauriAPI.event.listen).toHaveBeenCalledWith('quic_state_changed', expect.any(Function));
      expect(mockTauriAPI.event.listen).toHaveBeenCalledWith('quic_data_received', expect.any(Function));
      expect(mockTauriAPI.event.listen).toHaveBeenCalledWith('quic_error', expect.any(Function));
      expect(mockTauriAPI.event.listen).toHaveBeenCalledWith('quic_peer_info', expect.any(Function));
    });

    it('should emit StateChanged event to subscribers', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_state_changed', 'Connected');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'StateChanged',
        payload: 'Connected',
      });
    });

    it('should emit DataReceived event to subscribers', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_data_received', {
        channel: 'Terminal',
        data: 'SGVsbG8=',
      });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'DataReceived',
        payload: { channel: 'Terminal', data: 'SGVsbG8=' },
      });
    });

    it('should emit Error event to subscribers', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_error', 'Connection failed');

      expect(subscriber).toHaveBeenCalledWith({
        type: 'Error',
        payload: 'Connection failed',
      });
    });

    it('should emit PeerInfo event to subscribers', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_peer_info', { node_id: 'peer-123' });

      expect(subscriber).toHaveBeenCalledWith({
        type: 'PeerInfo',
        payload: { node_id: 'peer-123' },
      });
    });

    it('should not call startListening multiple times', async () => {
      await bridge.startListening();
      await bridge.startListening();
      await bridge.startListening();

      // Should only register listeners once
      expect(mockTauriAPI.event.listen).toHaveBeenCalledTimes(4);
    });

    it('should stop listening and clean up unlisten functions', async () => {
      await bridge.startListening();

      bridge.stopListening();

      // Verify event handlers are removed (each event type has empty array)
      let totalHandlers = 0;
      mockTauriAPI._eventHandlers.forEach((handlers) => {
        totalHandlers += handlers.length;
      });
      expect(totalHandlers).toBe(0);
    });

    it('should allow re-listening after stopListening', async () => {
      await bridge.startListening();
      bridge.stopListening();
      await bridge.startListening();

      expect(mockTauriAPI.event.listen).toHaveBeenCalledTimes(8); // 4 + 4
    });
  });

  // ==========================================================================
  // Subscription Tests
  // ==========================================================================

  describe('Subscriptions', () => {
    it('should allow subscribing to events', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_state_changed', 'Connecting');

      expect(subscriber).toHaveBeenCalled();
    });

    it('should allow unsubscribing from events', async () => {
      const subscriber = vi.fn();
      const unsubscribe = bridge.subscribe(subscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_state_changed', 'Connecting');
      expect(subscriber).toHaveBeenCalledTimes(1);

      unsubscribe();
      mockTauriAPI._emitEvent('quic_state_changed', 'Connected');

      expect(subscriber).toHaveBeenCalledTimes(1);
    });

    it('should handle subscriber errors gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
      const errorSubscriber = vi.fn(() => {
        throw new Error('Subscriber error');
      });
      const goodSubscriber = vi.fn();

      bridge.subscribe(errorSubscriber);
      bridge.subscribe(goodSubscriber);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_state_changed', 'Connected');

      expect(errorSubscriber).toHaveBeenCalled();
      expect(goodSubscriber).toHaveBeenCalled();
      expect(consoleError).toHaveBeenCalled();

      consoleError.mockRestore();
    });

    it('should support multiple subscribers', async () => {
      const subscriber1 = vi.fn();
      const subscriber2 = vi.fn();
      const subscriber3 = vi.fn();

      bridge.subscribe(subscriber1);
      bridge.subscribe(subscriber2);
      bridge.subscribe(subscriber3);
      await bridge.startListening();

      mockTauriAPI._emitEvent('quic_state_changed', 'Connected');

      expect(subscriber1).toHaveBeenCalled();
      expect(subscriber2).toHaveBeenCalled();
      expect(subscriber3).toHaveBeenCalled();
    });
  });

  // ==========================================================================
  // Cleanup Tests
  // ==========================================================================

  describe('Cleanup', () => {
    it('should clean up all resources on destroy', async () => {
      const subscriber = vi.fn();
      bridge.subscribe(subscriber);
      await bridge.startListening();

      bridge.destroy();

      // Subscribers should be cleared
      mockTauriAPI._emitEvent('quic_state_changed', 'Connected');
      expect(subscriber).not.toHaveBeenCalled();
    });
  });

  // ==========================================================================
  // Singleton Tests
  // ==========================================================================

  describe('Singleton', () => {
    it('should return the same instance from getTauriIPCBridge', () => {
      const instance1 = getTauriIPCBridge();
      const instance2 = getTauriIPCBridge();

      expect(instance1).toBe(instance2);
    });

    it('should create new instance after resetTauriIPCBridge', () => {
      const instance1 = getTauriIPCBridge();
      resetTauriIPCBridge();
      const instance2 = getTauriIPCBridge();

      expect(instance1).not.toBe(instance2);
    });
  });
});

// ============================================================================
// Utility Function Tests
// ============================================================================

describe('Base64 Utilities', () => {
  describe('uint8ArrayToBase64', () => {
    it('should convert empty array', () => {
      expect(uint8ArrayToBase64(new Uint8Array([]))).toBe('');
    });

    it('should convert "Hello"', () => {
      const data = new Uint8Array([72, 101, 108, 108, 111]);
      expect(uint8ArrayToBase64(data)).toBe('SGVsbG8=');
    });

    it('should convert "foobar"', () => {
      const data = new Uint8Array([102, 111, 111, 98, 97, 114]);
      expect(uint8ArrayToBase64(data)).toBe('Zm9vYmFy');
    });

    it('should handle binary data', () => {
      const data = new Uint8Array([0, 1, 2, 255, 254, 253]);
      const encoded = uint8ArrayToBase64(data);
      expect(encoded).toBe('AAEC//79');
    });
  });

  describe('base64ToUint8Array', () => {
    it('should convert empty string', () => {
      expect(base64ToUint8Array('')).toEqual(new Uint8Array([]));
    });

    it('should convert "SGVsbG8=" to "Hello"', () => {
      const result = base64ToUint8Array('SGVsbG8=');
      expect(Array.from(result)).toEqual([72, 101, 108, 108, 111]);
    });

    it('should convert "Zm9vYmFy" to "foobar"', () => {
      const result = base64ToUint8Array('Zm9vYmFy');
      expect(Array.from(result)).toEqual([102, 111, 111, 98, 97, 114]);
    });

    it('should handle binary data round-trip', () => {
      const original = new Uint8Array([0, 1, 2, 255, 254, 253]);
      const encoded = uint8ArrayToBase64(original);
      const decoded = base64ToUint8Array(encoded);
      expect(Array.from(decoded)).toEqual(Array.from(original));
    });
  });

  describe('Round-trip conversion', () => {
    it('should maintain data integrity', () => {
      const testCases = [
        new Uint8Array([]),
        new Uint8Array([0]),
        new Uint8Array([0, 1]),
        new Uint8Array([0, 1, 2]),
        new Uint8Array([255]),
        new Uint8Array(256).map((_, i) => i),
      ];

      for (const original of testCases) {
        const encoded = uint8ArrayToBase64(original);
        const decoded = base64ToUint8Array(encoded);
        expect(Array.from(decoded)).toEqual(Array.from(original));
      }
    });
  });
});
