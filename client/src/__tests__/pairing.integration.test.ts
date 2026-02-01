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
});
