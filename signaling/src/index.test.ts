/**
 * Integration tests for WebRTC signaling worker
 * Uses wrangler's unstable_dev for testing with full TypeScript support
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { unstable_dev, Unstable_DevWorker } from "wrangler";

interface SignalingMessage {
  type: string;
  peerId?: string;
  data?: Record<string, unknown>;
}

// Use type assertion for Node.js WebSocket which has different event handling
type NodeWebSocket = WebSocket & {
  onopen: (() => void) | null;
  onerror: ((err: Event) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
};

/**
 * Helper class to manage WebSocket connections and message queuing
 * This ensures we don't miss messages due to timing issues
 */
class TestWebSocket {
  public ws: NodeWebSocket;
  public messages: SignalingMessage[] = [];
  private waiters: Array<(msg: SignalingMessage) => void> = [];
  public connected: Promise<void>;
  private closed = false;

  constructor(url: string) {
    this.ws = new WebSocket(url) as NodeWebSocket;
    this.connected = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("Connection timeout")), 5000);
      this.ws.onopen = () => {
        clearTimeout(timeout);
        resolve();
      };
      this.ws.onerror = () => {
        clearTimeout(timeout);
        // Only reject if not intentionally closed
        if (!this.closed) {
          reject(new Error("WebSocket error"));
        }
      };
    });

    this.ws.onmessage = (event: MessageEvent) => {
      const msg = JSON.parse(event.data as string) as SignalingMessage;
      this.messages.push(msg);
      // If there's a waiter, resolve it
      if (this.waiters.length > 0) {
        const waiter = this.waiters.shift()!;
        waiter(msg);
      }
    };
  }

  async nextMessage(timeoutMs = 5000): Promise<SignalingMessage> {
    // Check if there's already an unprocessed message
    if (this.messages.length > 0) {
      return this.messages.shift()!;
    }

    // Wait for the next message
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        const index = this.waiters.indexOf(resolve);
        if (index >= 0) this.waiters.splice(index, 1);
        reject(new Error("Timeout waiting for message"));
      }, timeoutMs);

      this.waiters.push((msg) => {
        clearTimeout(timeout);
        // Remove from queue since we're handling it directly
        const msgIndex = this.messages.indexOf(msg);
        if (msgIndex >= 0) this.messages.splice(msgIndex, 1);
        resolve(msg);
      });
    });
  }

  send(data: unknown): void {
    this.ws.send(JSON.stringify(data));
  }

  close(): void {
    this.closed = true;
    this.ws.close();
  }
}

describe("Signaling Worker", () => {
  let worker: Unstable_DevWorker;

  beforeAll(async () => {
    worker = await unstable_dev("src/index.ts", {
      experimental: {
        disableExperimentalWarning: true,
      },
      vars: {
        ROOM_TTL_SECONDS: "60",
        RATE_LIMIT_MESSAGES_PER_SECOND: "10",
        PAIRING_TTL_SECONDS: "300",
      },
    });
  });

  afterAll(async () => {
    await worker.stop();
  });

  describe("Pairing Endpoints", () => {
    const validPairingBody = {
      code: "AXBK-7392",
      info: {
        device_id: "test-device-123",
        public_key: "dGVzdC1wdWJsaWMta2V5",
        relay_url: "wss://relay.example.com",
        expires: Math.floor(Date.now() / 1000) + 300,
      },
      ttl: 300,
    };

    it("should register a pairing code via POST /pair", async () => {
      const res = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(validPairingBody),
      });
      expect(res.status).toBe(200);
      const body = (await res.json()) as { ok: boolean };
      expect(body.ok).toBe(true);
    });

    it("should look up a registered pairing code via GET /pair/:code", async () => {
      // Register first
      await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(validPairingBody),
      });

      // Look up
      const res = await worker.fetch(`/pair/${validPairingBody.code}`);
      expect(res.status).toBe(200);
      const body = (await res.json()) as {
        device_id: string;
        public_key: string;
        relay_url: string;
        expires: number;
      };
      expect(body.device_id).toBe(validPairingBody.info.device_id);
      expect(body.public_key).toBe(validPairingBody.info.public_key);
      expect(body.relay_url).toBe(validPairingBody.info.relay_url);
    });

    it("should return 404 for unknown pairing code", async () => {
      const res = await worker.fetch("/pair/UNKNOWN-CODE");
      expect(res.status).toBe(404);
      const body = (await res.json()) as { error: string };
      expect(body.error).toBe("Not found");
    });

    it("should return 404 for expired pairing code", async () => {
      // Register with 1-second TTL
      const shortLivedBody = {
        code: "EXPR-TEST",
        info: validPairingBody.info,
        ttl: 1,
      };
      const registerRes = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(shortLivedBody),
      });
      expect(registerRes.status).toBe(200);

      // Verify it's accessible immediately
      const immediateRes = await worker.fetch("/pair/EXPR-TEST");
      expect(immediateRes.status).toBe(200);

      // Wait for expiry (1s TTL + buffer)
      await new Promise((resolve) => setTimeout(resolve, 1500));

      // Should now return 404 with "Expired" error
      const expiredRes = await worker.fetch("/pair/EXPR-TEST");
      expect(expiredRes.status).toBe(404);
      const body = (await expiredRes.json()) as { error: string };
      expect(body.error).toBe("Expired");
    });

    it("should return 400 for POST /pair with missing fields", async () => {
      const res = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code: "ABCD-1234" }),
      });
      expect(res.status).toBe(400);
    });

    it("should return 400 for POST /pair with invalid code format", async () => {
      const res = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code: "AB", info: validPairingBody.info }),
      });
      expect(res.status).toBe(400);
    });

    it("should return 400 for POST /pair with invalid info", async () => {
      const res = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          code: "ABCD-1234",
          info: { device_id: "x" }, // missing required fields
        }),
      });
      expect(res.status).toBe(400);
    });

    it("should include CORS headers on pairing responses", async () => {
      const res = await worker.fetch("/pair", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(validPairingBody),
      });
      expect(res.headers.get("Access-Control-Allow-Origin")).toBe("*");

      const getRes = await worker.fetch(`/pair/${validPairingBody.code}`);
      expect(getRes.headers.get("Access-Control-Allow-Origin")).toBe("*");
    });

    it("should include POST in OPTIONS preflight methods", async () => {
      const res = await worker.fetch("/pair", {
        method: "OPTIONS",
      });
      expect(res.status).toBe(200);
      const methods = res.headers.get("Access-Control-Allow-Methods");
      expect(methods).toContain("POST");
    });
  });

  describe("HTTP Endpoints", () => {
    it("should return health check on /", async () => {
      const res = await worker.fetch("/");
      expect(res.status).toBe(200);

      const body = await res.json() as { status: string };
      expect(body.status).toBe("ok");
    });

    it("should return health check on /health", async () => {
      const res = await worker.fetch("/health");
      expect(res.status).toBe(200);

      const body = await res.json() as { status: string };
      expect(body.status).toBe("ok");
    });

    it("should return 404 for unknown routes", async () => {
      const res = await worker.fetch("/unknown");
      expect(res.status).toBe(404);

      const body = await res.json() as { error: string };
      expect(body.error).toBe("Not found");
    });

    it("should reject non-WebSocket requests to /room/:id", async () => {
      const res = await worker.fetch("/room/test-room");
      expect(res.status).toBe(426);

      const body = await res.json() as { error: string };
      expect(body.error).toBe("Expected WebSocket upgrade");
    });

    it("should reject room IDs that are too long", async () => {
      const longId = "a".repeat(65);
      const res = await worker.fetch(`/room/${longId}`);
      expect(res.status).toBe(400);

      const body = await res.json() as { error: string };
      expect(body.error).toBe("Room ID too long");
    });

    it("should handle CORS preflight", async () => {
      const res = await worker.fetch("/room/test", {
        method: "OPTIONS",
      });
      expect(res.status).toBe(200);
      expect(res.headers.get("Access-Control-Allow-Origin")).toBe("*");
    });
  });

  describe("WebSocket Connections", () => {
    it("should accept WebSocket upgrade for valid room", async () => {
      const wsUrl = `ws://${worker.address}:${worker.port}/room/test-room`;
      const client = new TestWebSocket(wsUrl);
      await client.connected;
      expect(client.ws.readyState).toBe(WebSocket.OPEN);
      client.close();
    });

    it("should send join message with peer ID on connect", async () => {
      const wsUrl = `ws://${worker.address}:${worker.port}/room/join-test-${Date.now()}`;
      const client = new TestWebSocket(wsUrl);

      const message = await client.nextMessage();
      expect(message.type).toBe("join");
      expect(message.peerId).toBeDefined();
      expect(typeof message.peerId).toBe("string");
      expect(message.data!.peers).toEqual([]);

      client.close();
    });
  });

  describe("Message Relay", () => {
    it("should relay offer message between peers", async () => {
      const roomId = `relay-offer-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect first peer and wait for join
      const peer1 = new TestWebSocket(wsUrl);
      const join1 = await peer1.nextMessage();
      expect(join1.type).toBe("join");
      const peer1Id = join1.peerId;

      // Connect second peer and wait for join
      const peer2 = new TestWebSocket(wsUrl);
      const join2 = await peer2.nextMessage();
      expect(join2.type).toBe("join");
      expect(join2.data!.peers).toContain(peer1Id);

      // Wait for peer-joined notification on peer 1
      const peerJoined = await peer1.nextMessage();
      expect(peerJoined.type).toBe("peer-joined");

      // Send offer from peer 1
      peer1.send({
        type: "offer",
        data: { sdp: "test-offer-sdp" },
      });

      // Peer 2 should receive the offer
      const offer = await peer2.nextMessage();
      expect(offer.type).toBe("offer");
      expect(offer.peerId).toBe(peer1Id);
      expect(offer.data!.sdp).toBe("test-offer-sdp");

      peer1.close();
      peer2.close();
    });

    it("should relay answer message between peers", async () => {
      const roomId = `relay-answer-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect both peers
      const peer1 = new TestWebSocket(wsUrl);
      await peer1.nextMessage(); // join

      const peer2 = new TestWebSocket(wsUrl);
      const join2 = await peer2.nextMessage();
      const peer2Id = join2.peerId;
      await peer1.nextMessage(); // peer-joined

      // Send answer from peer 2
      peer2.send({
        type: "answer",
        data: { sdp: "test-answer-sdp" },
      });

      // Peer 1 should receive the answer
      const answer = await peer1.nextMessage();
      expect(answer.type).toBe("answer");
      expect(answer.peerId).toBe(peer2Id);
      expect(answer.data!.sdp).toBe("test-answer-sdp");

      peer1.close();
      peer2.close();
    });

    it("should relay ICE candidate between peers", async () => {
      const roomId = `relay-ice-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect both peers
      const peer1 = new TestWebSocket(wsUrl);
      await peer1.nextMessage(); // join

      const peer2 = new TestWebSocket(wsUrl);
      const join2 = await peer2.nextMessage();
      const peer2Id = join2.peerId;
      await peer1.nextMessage(); // peer-joined

      // Send ICE candidate from peer 2
      peer2.send({
        type: "ice",
        data: { candidate: "test-ice-candidate", sdpMLineIndex: 0 },
      });

      // Peer 1 should receive the ICE candidate
      const ice = await peer1.nextMessage();
      expect(ice.type).toBe("ice");
      expect(ice.peerId).toBe(peer2Id);
      expect(ice.data!.candidate).toBe("test-ice-candidate");

      peer1.close();
      peer2.close();
    });
  });

  describe("Room Join/Leave", () => {
    it("should notify peers when someone joins", async () => {
      const roomId = `join-notify-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect first peer
      const peer1 = new TestWebSocket(wsUrl);
      await peer1.nextMessage(); // join

      // Connect second peer
      const peer2 = new TestWebSocket(wsUrl);

      // Peer 1 should receive peer-joined
      const peerJoined = await peer1.nextMessage();
      expect(peerJoined.type).toBe("peer-joined");
      expect(peerJoined.peerId).toBeDefined();

      peer1.close();
      peer2.close();
    });

    it("should notify peers when someone leaves", async () => {
      const roomId = `leave-notify-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect both peers
      const peer1 = new TestWebSocket(wsUrl);
      await peer1.nextMessage(); // join

      const peer2 = new TestWebSocket(wsUrl);
      const join2 = await peer2.nextMessage();
      const peer2Id = join2.peerId;
      await peer1.nextMessage(); // peer-joined

      // Small delay to ensure connection is stable
      await new Promise((r) => setTimeout(r, 100));

      // Peer 2 disconnects
      peer2.close();

      // Peer 1 should receive peer-left
      const peerLeft = await peer1.nextMessage();
      expect(peerLeft.type).toBe("peer-left");
      expect(peerLeft.peerId).toBe(peer2Id);

      peer1.close();
    });

    it("should include existing peers in join message", async () => {
      const roomId = `existing-peers-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      // Connect first peer
      const peer1 = new TestWebSocket(wsUrl);
      const join1 = await peer1.nextMessage();
      const peer1Id = join1.peerId;

      // Connect second peer
      const peer2 = new TestWebSocket(wsUrl);
      const join2 = await peer2.nextMessage();

      // Second peer should see first peer in the list
      expect(join2.data!.peers).toContain(peer1Id);

      peer1.close();
      peer2.close();
    });
  });

  describe("Rate Limiting", () => {
    it("should reject messages when rate limit exceeded", async () => {
      const roomId = `rate-limit-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      const peer = new TestWebSocket(wsUrl);
      await peer.nextMessage(); // join

      // Send more messages than the rate limit (10/second)
      for (let i = 0; i < 15; i++) {
        peer.send({
          type: "ice",
          data: { candidate: `candidate-${i}` },
        });
      }

      // Wait for messages and check for error
      await new Promise((r) => setTimeout(r, 500));

      const hasRateLimitError = peer.messages.some(
        (m) => m.type === "error"
      );
      expect(hasRateLimitError).toBe(true);

      peer.close();
    });
  });

  describe("Error Handling", () => {
    it("should reject invalid JSON", async () => {
      const roomId = `error-json-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      const peer = new TestWebSocket(wsUrl);
      await peer.nextMessage(); // join

      // Send invalid JSON
      peer.ws.send("not valid json");

      const error = await peer.nextMessage();
      expect(error.type).toBe("error");
      expect(error.data!.message).toBe("Invalid JSON");

      peer.close();
    });

    it("should reject invalid message types", async () => {
      const roomId = `error-type-${Date.now()}`;
      const wsUrl = `ws://${worker.address}:${worker.port}/room/${roomId}`;

      const peer = new TestWebSocket(wsUrl);
      await peer.nextMessage(); // join

      // Send message with invalid type
      peer.send({
        type: "invalid",
        data: {},
      });

      const error = await peer.nextMessage();
      expect(error.type).toBe("error");
      expect(error.data!.message).toBe("Invalid message type");

      peer.close();
    });
  });
});
