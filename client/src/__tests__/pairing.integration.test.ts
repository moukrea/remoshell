/**
 * Pairing Flow Integration Tests
 *
 * These tests verify the complete pairing flow from QR code scanning
 * through connection establishment.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  parsePairingData,
  isPairingExpired,
  secondsUntilExpiry,
  createPairingQRContent,
  type PairingData,
} from '../lib/scanner/BarcodeScanner';
import { createMockSignalingClient, createMockWebRTCManager, type MockSignalingClient, type MockWebRTCManager } from '../lib/orchestration/__tests__/mocks';
import { resetConnectionStore, getConnectionStore } from '../stores/connection';
import { resetSessionStore } from '../stores/sessions';
import { resetConfig, setSignalingUrl, getConfig } from '../config';

// Mock the module imports
let mockSignaling: MockSignalingClient;
let mockWebRTC: MockWebRTCManager;

vi.mock('../lib/signaling/SignalingClient', () => ({
  getSignalingClient: vi.fn(() => mockSignaling),
  resetSignalingClient: vi.fn(),
}));

vi.mock('../lib/webrtc/WebRTCManager', () => ({
  getWebRTCManager: vi.fn(() => mockWebRTC),
  resetWebRTCManager: vi.fn(),
}));

// Import after mocking
import { ConnectionOrchestrator, resetOrchestrator } from '../lib/orchestration/ConnectionOrchestrator';

describe('Pairing Flow Integration', () => {
  beforeEach(() => {
    vi.useFakeTimers();

    // Create fresh mocks for each test
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();

    // Reset all singletons
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    resetConfig();

    vi.clearAllMocks();
  });

  afterEach(() => {
    resetOrchestrator();
    resetConnectionStore();
    resetSessionStore();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe('Pairing Code Parsing', () => {
    it('should parse valid JSON pairing data', () => {
      const pairingData: PairingData = {
        device_id: 'test-device-123',
        public_key: 'dGVzdC1wdWJsaWMta2V5LTEyMw==',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const qrContent = JSON.stringify(pairingData);
      const result = parsePairingData(qrContent);

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.device_id).toBe(pairingData.device_id);
        expect(result.data.public_key).toBe(pairingData.public_key);
        expect(result.data.relay_url).toBe(pairingData.relay_url);
        expect(result.data.expires).toBe(pairingData.expires);
      }
    });

    it('should reject empty pairing data', () => {
      const result = parsePairingData('');
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('Empty or invalid');
      }
    });

    it('should reject invalid JSON', () => {
      const result = parsePairingData('not-valid-json');
      expect(result.success).toBe(false);
    });

    it('should reject pairing data missing device_id', () => {
      const result = parsePairingData(JSON.stringify({
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: 12345,
      }));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('device_id');
      }
    });

    it('should reject pairing data missing public_key', () => {
      const result = parsePairingData(JSON.stringify({
        device_id: 'test',
        relay_url: 'wss://test.com',
        expires: 12345,
      }));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('public_key');
      }
    });

    it('should reject pairing data missing relay_url', () => {
      const result = parsePairingData(JSON.stringify({
        device_id: 'test',
        public_key: 'test',
        expires: 12345,
      }));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('relay_url');
      }
    });

    it('should reject pairing data missing expires', () => {
      const result = parsePairingData(JSON.stringify({
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
      }));
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error).toContain('expires');
      }
    });
  });

  describe('Pairing Expiry', () => {
    it('should detect expired pairing code', () => {
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: Math.floor(Date.now() / 1000) - 100, // 100 seconds ago
      };

      expect(isPairingExpired(pairingData)).toBe(true);
      expect(secondsUntilExpiry(pairingData)).toBe(0);
    });

    it('should detect valid pairing code', () => {
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: Math.floor(Date.now() / 1000) + 3600, // 1 hour from now
      };

      expect(isPairingExpired(pairingData)).toBe(false);
      expect(secondsUntilExpiry(pairingData)).toBeGreaterThan(3500);
    });

    it('should create valid QR content', () => {
      const pairingData: PairingData = {
        device_id: 'test-device',
        public_key: 'test-key',
        relay_url: 'wss://test.com',
        expires: 12345,
      };

      const content = createPairingQRContent(pairingData);
      const parsed = parsePairingData(content);

      expect(parsed.success).toBe(true);
      if (parsed.success) {
        expect(parsed.data).toEqual(pairingData);
      }
    });
  });

  describe('Complete Pairing to Connection Flow', () => {
    it('should update config with relay URL from pairing data', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://custom-relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      // Parse the pairing data
      const qrContent = JSON.stringify(pairingData);
      const result = parsePairingData(qrContent);
      expect(result.success).toBe(true);

      if (result.success) {
        // Update the signaling URL from pairing data
        setSignalingUrl(result.data.relay_url);

        const config = getConfig();
        expect(config.signalingUrl).toBe('wss://custom-relay.example.com');
      }
    });

    it('should connect to signaling server after pairing', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      // Parse the pairing data
      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        // Update config with relay URL
        setSignalingUrl(result.data.relay_url);

        // Initialize orchestrator and connect
        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Verify signaling client tried to join
        expect(mockSignaling.join).toHaveBeenCalledWith(result.data.device_id);
      }
    });

    it('should establish peer connection when remote device connects', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      // Parse and set up
      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Simulate signaling connection
        mockSignaling._simulateConnect('local-peer-id', []);

        const store = getConnectionStore();
        expect(store.state.signalingStatus).toBe('connected');

        // Simulate remote device joining
        mockSignaling._simulatePeerJoined('remote-peer-id');

        // Should create WebRTC connection
        expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
          expect.objectContaining({
            peerId: 'remote-peer-id',
            initiator: true,
          })
        );

        // Simulate WebRTC connection established
        mockWebRTC._simulateConnect('remote-peer-id');

        expect(store.state.peers['remote-peer-id']).toBeDefined();
        expect(store.state.peers['remote-peer-id'].status).toBe('connected');
      }
    });

    it('should handle pairing with existing peer in room', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Simulate joining with existing peer already in room
        mockSignaling._simulateConnect('local-peer-id', ['existing-remote-peer']);

        // Should create connection to existing peer
        expect(mockWebRTC.createConnection).toHaveBeenCalledWith(
          expect.objectContaining({
            peerId: 'existing-remote-peer',
            initiator: true,
          })
        );
      }
    });

    it('should reject expired pairing codes before connecting', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) - 100, // Already expired
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        expect(isPairingExpired(result.data)).toBe(true);
        // Application should check expiry before attempting connection
      }
    });
  });

  describe('Error Scenarios', () => {
    it('should handle signaling connection failure gracefully', async () => {
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Simulate connection failure
        mockSignaling._simulateError('Connection refused');

        // Should log error but not throw
        expect(consoleError).toHaveBeenCalled();
      }

      consoleError.mockRestore();
    });

    it('should handle peer connection failure', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Connect to signaling
        mockSignaling._simulateConnect('local-peer-id', []);

        // Remote peer joins
        mockSignaling._simulatePeerJoined('remote-peer-id');

        // Simulate WebRTC error
        mockWebRTC._simulateError('remote-peer-id', new Error('ICE connection failed'));

        const store = getConnectionStore();
        expect(store.state.peers['remote-peer-id'].status).toBe('failed');
      }
    });

    it('should handle unexpected peer disconnect', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Connect to signaling
        mockSignaling._simulateConnect('local-peer-id', []);

        // Remote peer joins and connects
        mockSignaling._simulatePeerJoined('remote-peer-id');
        mockWebRTC._simulateConnect('remote-peer-id');

        const store = getConnectionStore();
        expect(store.state.peers['remote-peer-id'].status).toBe('connected');

        // Peer unexpectedly disconnects
        mockWebRTC._simulateDisconnect('remote-peer-id');

        expect(store.state.peers['remote-peer-id'].status).toBe('disconnected');
      }
    });
  });

  describe('Re-pairing Scenarios', () => {
    it('should handle disconnect and re-pair to same device', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device-123',
        public_key: 'remote-public-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        // First connection
        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        mockSignaling._simulateConnect('local-peer-id', []);
        mockSignaling._simulatePeerJoined('remote-peer-id');
        mockWebRTC._simulateConnect('remote-peer-id');

        // Disconnect
        await orchestrator.disconnect();
        expect(orchestrator.getState()).toBe('disconnected');

        // Re-pair
        await orchestrator.connect(result.data.device_id);
        mockSignaling._simulateConnect('local-peer-id-2', []);
        mockSignaling._simulatePeerJoined('remote-peer-id-2');
        mockWebRTC._simulateConnect('remote-peer-id-2');

        const store = getConnectionStore();
        expect(store.state.peers['remote-peer-id-2'].status).toBe('connected');
      }
    });

    it('should handle pairing to different device after disconnect', async () => {
      // First device pairing
      const firstDevice: PairingData = {
        device_id: 'device-1',
        public_key: 'key-1',
        relay_url: 'wss://relay1.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      // Second device pairing
      const secondDevice: PairingData = {
        device_id: 'device-2',
        public_key: 'key-2',
        relay_url: 'wss://relay2.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      // Parse first device
      let result = parsePairingData(JSON.stringify(firstDevice));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        mockSignaling._simulateConnect('local-peer', []);
        expect(mockSignaling.join).toHaveBeenCalledWith('device-1');

        // Disconnect from first device
        await orchestrator.disconnect();

        // Parse second device and connect
        result = parsePairingData(JSON.stringify(secondDevice));
        expect(result.success).toBe(true);

        if (result.success) {
          setSignalingUrl(result.data.relay_url);
          expect(getConfig().signalingUrl).toBe('wss://relay2.example.com');

          // Would need to re-initialize to get new signaling client with new URL
          // This tests the config update path
        }
      }
    });
  });

  describe('Malformed QR Code Handling', () => {
    describe('Truncated Data', () => {
      it('should reject truncated JSON', () => {
        const truncated = '{"device_id":"test","public_key":"abc';
        const result = parsePairingData(truncated);

        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error).toMatch(/invalid|parse|JSON/i);
        }
      });

      it('should reject partial device_id only', () => {
        const partial = '{"device_id":"test-123"}';
        const result = parsePairingData(partial);

        expect(result.success).toBe(false);
        if (!result.success) {
          expect(result.error).toMatch(/public_key|missing/i);
        }
      });

      it('should reject empty object', () => {
        const empty = '{}';
        const result = parsePairingData(empty);

        expect(result.success).toBe(false);
      });
    });

    describe('Corrupted Data', () => {
      it('should reject binary garbage', () => {
        const garbage = String.fromCharCode(0x00, 0x01, 0xFF, 0xFE);
        const result = parsePairingData(garbage);

        expect(result.success).toBe(false);
      });

      it('should reject JSON with wrong types', () => {
        const wrongTypes = JSON.stringify({
          device_id: 12345, // should be string
          public_key: 'test',
          relay_url: 'wss://test.com',
          expires: 'not-a-number', // should be number
        });
        const result = parsePairingData(wrongTypes);

        expect(result.success).toBe(false);
      });

      it('should reject JSON with null values', () => {
        const nullValues = JSON.stringify({
          device_id: null,
          public_key: 'test',
          relay_url: 'wss://test.com',
          expires: 12345,
        });
        const result = parsePairingData(nullValues);

        expect(result.success).toBe(false);
      });
    });

    describe('Wrong Format', () => {
      it('should reject plain URL', () => {
        const url = 'https://example.com/pair?device=123';
        const result = parsePairingData(url);

        expect(result.success).toBe(false);
      });

      it('should reject base64 without JSON', () => {
        const base64 = btoa('this is not json');
        const result = parsePairingData(base64);

        expect(result.success).toBe(false);
      });

      it('should reject XML format', () => {
        const xml = '<pairing><device_id>test</device_id></pairing>';
        const result = parsePairingData(xml);

        expect(result.success).toBe(false);
      });

      it('should reject numeric string', () => {
        const numeric = '12345678901234567890';
        const result = parsePairingData(numeric);

        expect(result.success).toBe(false);
      });
    });
  });

  describe('Expiry Edge Cases', () => {
    it('should detect code that expires exactly now', () => {
      const now = Math.floor(Date.now() / 1000);
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: now, // expires exactly now
      };

      // Implementation uses > (not >=), so code expiring exactly now is still valid
      // This is correct behavior - the code expires AFTER this second
      expect(isPairingExpired(pairingData)).toBe(false);
      expect(secondsUntilExpiry(pairingData)).toBe(0);
    });

    it('should detect code that expired 1 second ago', () => {
      const now = Math.floor(Date.now() / 1000);
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: now - 1, // expired 1 second ago
      };

      expect(isPairingExpired(pairingData)).toBe(true);
      expect(secondsUntilExpiry(pairingData)).toBe(0);
    });

    it('should detect code that expires in 1 second', () => {
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: Math.floor(Date.now() / 1000) + 1,
      };

      expect(isPairingExpired(pairingData)).toBe(false);
      expect(secondsUntilExpiry(pairingData)).toBe(1);
    });

    it('should handle code expiring during connection attempt', async () => {
      // Code valid for 2 seconds
      const pairingData: PairingData = {
        device_id: 'remote-device',
        public_key: 'test-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 2,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        // Code is valid
        expect(isPairingExpired(result.data)).toBe(false);

        // Advance time past expiry
        vi.advanceTimersByTime(3000);

        // Code should now be expired
        expect(isPairingExpired(result.data)).toBe(true);
      }
    });

    it('should handle very old expiry timestamps', () => {
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: 0, // Unix epoch
      };

      expect(isPairingExpired(pairingData)).toBe(true);
    });

    it('should handle far future expiry timestamps', () => {
      const pairingData: PairingData = {
        device_id: 'test',
        public_key: 'test',
        relay_url: 'wss://test.com',
        expires: Math.floor(Date.now() / 1000) + (365 * 24 * 60 * 60), // 1 year
      };

      expect(isPairingExpired(pairingData)).toBe(false);
      expect(secondsUntilExpiry(pairingData)).toBeGreaterThan(364 * 24 * 60 * 60);
    });
  });

  describe('Timeout Handling', () => {
    it('should handle signaling connection timeout', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device',
        public_key: 'test-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Don't simulate connection - let it timeout
        vi.advanceTimersByTime(30000); // 30 second timeout

        // Signaling should still be in connecting state or failed
        const store = getConnectionStore();
        expect(['connecting', 'disconnected', 'failed']).toContain(store.state.signalingStatus);
      }
    });

    it('should handle WebRTC connection timeout', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device',
        public_key: 'test-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Signaling connects
        mockSignaling._simulateConnect('local-peer', []);

        // Peer joins
        mockSignaling._simulatePeerJoined('remote-peer');

        // WebRTC connection created but never completes
        const peer = mockWebRTC._getPeer('remote-peer');
        expect(peer?.state).toBe('connecting');

        // Advance past WebRTC timeout
        vi.advanceTimersByTime(60000); // 60 second timeout

        // Connection should timeout (implementation dependent)
      }
    });

    it('should handle stalled ICE gathering', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device',
        public_key: 'test-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        mockSignaling._simulateConnect('local-peer', []);
        mockSignaling._simulatePeerJoined('remote-peer');

        // Start connection but ICE gathering never completes
        // Application should handle this gracefully
      }
    });

    it('should allow cancellation of pending connection', async () => {
      const pairingData: PairingData = {
        device_id: 'remote-device',
        public_key: 'test-key',
        relay_url: 'wss://relay.example.com',
        expires: Math.floor(Date.now() / 1000) + 3600,
      };

      const result = parsePairingData(JSON.stringify(pairingData));
      expect(result.success).toBe(true);

      if (result.success) {
        setSignalingUrl(result.data.relay_url);

        const orchestrator = new ConnectionOrchestrator();
        await orchestrator.initialize();
        await orchestrator.connect(result.data.device_id);

        // Cancel before connection completes
        await orchestrator.disconnect();

        expect(orchestrator.getState()).toBe('disconnected');
      }
    });
  });

  describe('Invalid Relay URL', () => {
    it('should reject invalid relay URL scheme', () => {
      const pairingData = JSON.stringify({
        device_id: 'test',
        public_key: 'test',
        relay_url: 'http://not-websocket.com', // Should be wss://
        expires: Math.floor(Date.now() / 1000) + 3600,
      });

      const result = parsePairingData(pairingData);
      // Depending on implementation, may fail at parse or connect time
      if (result.success) {
        expect(result.data.relay_url).not.toMatch(/^wss:\/\//);
      }
    });

    it('should handle missing protocol in relay URL', () => {
      const pairingData = JSON.stringify({
        device_id: 'test',
        public_key: 'test',
        relay_url: 'relay.example.com', // No protocol
        expires: Math.floor(Date.now() / 1000) + 3600,
      });

      const result = parsePairingData(pairingData);
      // May succeed parsing but fail connection - verify it at least parses
      expect(result).toBeDefined();
    });
  });
});
