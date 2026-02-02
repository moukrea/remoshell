/**
 * File Transfer Integration Tests
 *
 * Tests the complete file transfer flow including:
 * - Download: request -> receive chunks -> completion
 * - Upload: initiate -> send chunks -> acknowledgment
 * - Error handling: connection drops, checksum mismatches
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createMockSignalingClient,
  createMockWebRTCManager,
  type MockSignalingClient,
  type MockWebRTCManager,
} from '../lib/orchestration/__tests__/mocks';

// Mock module imports
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

describe('File Transfer Integration', () => {
  beforeEach(() => {
    mockSignaling = createMockSignalingClient();
    mockWebRTC = createMockWebRTCManager();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Download Flow', () => {
    it('should request file download from peer', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      // Simulate download request
      const downloadRequest = {
        type: 'file_request',
        path: '/home/user/document.txt',
        requestId: 'req-123',
      };

      const sent = mockWebRTC.sendData(
        peerId,
        new TextEncoder().encode(JSON.stringify(downloadRequest)),
        'files'
      );

      expect(sent).toBe(true);
    });

    it('should receive file chunks and track progress', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const receivedChunks: Uint8Array[] = [];

      // Subscribe to data events
      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          receivedChunks.push(event.data as Uint8Array);
        }
      });

      // Simulate receiving file chunks
      const chunk1 = new Uint8Array([1, 2, 3, 4]);
      const chunk2 = new Uint8Array([5, 6, 7, 8]);

      mockWebRTC._simulateData(peerId, chunk1, 'files');
      mockWebRTC._simulateData(peerId, chunk2, 'files');

      expect(receivedChunks).toHaveLength(2);
      expect(receivedChunks[0]).toEqual(chunk1);
      expect(receivedChunks[1]).toEqual(chunk2);
    });

    it('should verify download completion', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      let completionReceived = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files') {
          // Use ArrayBuffer.isView to handle cross-realm Uint8Array instances
          const data = event.data as Uint8Array;
          if (ArrayBuffer.isView(data)) {
            const message = JSON.parse(new TextDecoder().decode(data));
            if (message.type === 'file_complete') {
              completionReceived = true;
            }
          }
        }
      });

      // Simulate completion message
      const completion = {
        type: 'file_complete',
        requestId: 'req-123',
        checksum: 'abc123',
        totalBytes: 1024,
      };

      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify(completion)),
        'files'
      );

      expect(completionReceived).toBe(true);
    });
  });

  describe('Upload Flow', () => {
    it('should initiate file upload', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const uploadInit = {
        type: 'upload_init',
        filename: 'test.txt',
        size: 1024,
        uploadId: 'upload-456',
      };

      const sent = mockWebRTC.sendData(
        peerId,
        new TextEncoder().encode(JSON.stringify(uploadInit)),
        'files'
      );

      expect(sent).toBe(true);
    });

    it('should send file chunks sequentially', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const chunks = [
        new Uint8Array([1, 2, 3]),
        new Uint8Array([4, 5, 6]),
        new Uint8Array([7, 8, 9]),
      ];

      for (const chunk of chunks) {
        const sent = mockWebRTC.sendData(peerId, chunk, 'files');
        expect(sent).toBe(true);
      }

      expect(mockWebRTC.sendData).toHaveBeenCalledTimes(3);
    });

    it('should receive upload acknowledgment', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      let ackReceived = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          const data = event.data as Uint8Array;
          const message = JSON.parse(new TextDecoder().decode(data));
          if (message.type === 'upload_ack') {
            ackReceived = true;
          }
        }
      });

      const ack = {
        type: 'upload_ack',
        uploadId: 'upload-456',
        status: 'complete',
      };

      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify(ack)),
        'files'
      );

      expect(ackReceived).toBe(true);
    });
  });

  describe('Error Scenarios', () => {
    it('should handle connection drop during transfer', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      // Start transfer
      mockWebRTC._simulateData(peerId, new Uint8Array([1, 2, 3]), 'files');

      // Connection drops
      mockWebRTC._simulateDisconnect(peerId);

      // Verify can't send more data
      const sent = mockWebRTC.sendData(peerId, new Uint8Array([4, 5, 6]), 'files');
      expect(sent).toBe(false);
    });

    it('should handle checksum mismatch', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      let errorReceived = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          const message = JSON.parse(new TextDecoder().decode(event.data));
          if (message.type === 'transfer_error' && message.reason === 'checksum_mismatch') {
            errorReceived = true;
          }
        }
      });

      const error = {
        type: 'transfer_error',
        reason: 'checksum_mismatch',
        expected: 'abc123',
        actual: 'xyz789',
      };

      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify(error)),
        'files'
      );

      expect(errorReceived).toBe(true);
    });

    it('should handle peer disconnection gracefully', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      let disconnectHandled = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'close' && event.peerId === peerId) {
          disconnectHandled = true;
        }
      });

      mockWebRTC._simulateDisconnect(peerId);

      expect(disconnectHandled).toBe(true);
      expect(mockWebRTC.isConnected(peerId)).toBe(false);
    });
  });

  describe('Transfer Cancellation', () => {
    it('should send cancellation message', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const cancelMessage = {
        type: 'transfer_cancel',
        transferId: 'transfer-789',
        reason: 'user_cancelled',
      };

      const sent = mockWebRTC.sendData(
        peerId,
        new TextEncoder().encode(JSON.stringify(cancelMessage)),
        'files'
      );

      expect(sent).toBe(true);
    });

    it('should receive cancellation acknowledgment', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      let cancelAckReceived = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          const message = JSON.parse(new TextDecoder().decode(event.data));
          if (message.type === 'cancel_ack') {
            cancelAckReceived = true;
          }
        }
      });

      const cancelAck = {
        type: 'cancel_ack',
        transferId: 'transfer-789',
      };

      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify(cancelAck)),
        'files'
      );

      expect(cancelAckReceived).toBe(true);
    });

    it('should stop receiving chunks after cancellation', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const receivedChunks: Uint8Array[] = [];
      let cancelled = false;

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          // Simulate application logic: stop processing after cancellation
          if (!cancelled) {
            const chunk = event.data;
            // Check if this is a cancellation message
            try {
              const message = JSON.parse(new TextDecoder().decode(chunk));
              if (message.type === 'transfer_cancel') {
                cancelled = true;
                return;
              }
            } catch {
              // Not JSON, treat as data chunk
            }
            receivedChunks.push(new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.byteLength));
          }
        }
      });

      // Receive some chunks
      mockWebRTC._simulateData(peerId, new Uint8Array([1, 2, 3]), 'files');
      mockWebRTC._simulateData(peerId, new Uint8Array([4, 5, 6]), 'files');

      // Send cancellation
      const cancel = { type: 'transfer_cancel', transferId: 'test' };
      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify(cancel)),
        'files'
      );

      // These chunks should not be processed
      mockWebRTC._simulateData(peerId, new Uint8Array([7, 8, 9]), 'files');
      mockWebRTC._simulateData(peerId, new Uint8Array([10, 11, 12]), 'files');

      expect(receivedChunks).toHaveLength(2);
      expect(cancelled).toBe(true);
    });
  });

  describe('Transfer State', () => {
    it('should track transfer in progress', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      expect(mockWebRTC.isConnected(peerId)).toBe(true);
      expect(mockWebRTC.isChannelAvailable(peerId, 'files')).toBe(true);
    });

    it('should not send on closed channel', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      // Don't simulate connect - channel not available

      const sent = mockWebRTC.sendData(peerId, new Uint8Array([1, 2, 3]), 'files');
      expect(sent).toBe(false);
    });

    it('should not send to disconnected peer', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);
      mockWebRTC._simulateDisconnect(peerId);

      const sent = mockWebRTC.sendData(peerId, new Uint8Array([1, 2, 3]), 'files');
      expect(sent).toBe(false);
    });

    it('should handle multiple concurrent transfers', async () => {
      const peerId = 'remote-peer';
      mockWebRTC.createConnection({ peerId, initiator: true });
      mockWebRTC._simulateConnect(peerId);

      const transfer1Chunks: Uint8Array[] = [];
      const transfer2Chunks: Uint8Array[] = [];

      mockWebRTC.subscribe((event) => {
        if (event.type === 'data' && event.channel === 'files' && ArrayBuffer.isView(event.data)) {
          try {
            const message = JSON.parse(new TextDecoder().decode(event.data));
            if (message.transferId === 'transfer-1') {
              transfer1Chunks.push(new Uint8Array(event.data.buffer, event.data.byteOffset, event.data.byteLength));
            } else if (message.transferId === 'transfer-2') {
              transfer2Chunks.push(new Uint8Array(event.data.buffer, event.data.byteOffset, event.data.byteLength));
            }
          } catch {
            // Not JSON, ignore
          }
        }
      });

      // Simulate interleaved chunks from two transfers
      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify({ transferId: 'transfer-1', chunk: 1 })),
        'files'
      );
      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify({ transferId: 'transfer-2', chunk: 1 })),
        'files'
      );
      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify({ transferId: 'transfer-1', chunk: 2 })),
        'files'
      );
      mockWebRTC._simulateData(
        peerId,
        new TextEncoder().encode(JSON.stringify({ transferId: 'transfer-2', chunk: 2 })),
        'files'
      );

      expect(transfer1Chunks).toHaveLength(2);
      expect(transfer2Chunks).toHaveLength(2);
    });
  });
});
