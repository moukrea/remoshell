/**
 * Room Durable Object for WebRTC signaling
 * Manages WebSocket connections and message relay between peers
 */

export interface Env {
  ROOM_TTL_SECONDS: string;
  RATE_LIMIT_MESSAGES_PER_SECOND: string;
}

interface RateLimitState {
  count: number;
  windowStart: number;
}

interface SignalingMessage {
  type: "offer" | "answer" | "ice" | "join" | "leave" | "peer-joined" | "peer-left" | "error";
  peerId?: string;
  data?: unknown;
}

interface SessionAttachment {
  peerId: string;
}

export class Room implements DurableObject {
  private state: DurableObjectState;
  // Rate limit state per WebSocket (keyed by peerId since WebSocket identity may change after hibernation)
  private rateLimits: Map<string, RateLimitState> = new Map();
  private roomTtlMs: number;
  private rateLimitPerSecond: number;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.roomTtlMs = (parseInt(env.ROOM_TTL_SECONDS, 10) || 60) * 1000;
    this.rateLimitPerSecond = parseInt(env.RATE_LIMIT_MESSAGES_PER_SECOND, 10) || 10;
  }

  async fetch(request: Request): Promise<Response> {
    // Only handle WebSocket upgrade requests
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("Expected WebSocket upgrade", { status: 426 });
    }

    // Create WebSocket pair
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);

    // Generate unique peer ID
    const peerId = crypto.randomUUID();

    // Accept the WebSocket connection with hibernation support
    this.state.acceptWebSocket(server);

    // Store session info as attachment (survives hibernation)
    const attachment: SessionAttachment = { peerId };
    server.serializeAttachment(attachment);

    // Initialize rate limit for this peer
    this.rateLimits.set(peerId, { count: 0, windowStart: Date.now() });

    // Get existing peers from all current WebSockets
    const existingPeers: string[] = [];
    for (const ws of this.state.getWebSockets()) {
      if (ws !== server) {
        const att = ws.deserializeAttachment() as SessionAttachment | null;
        if (att?.peerId) {
          existingPeers.push(att.peerId);
        }
      }
    }

    // Notify the connecting peer of their ID and existing peers
    server.send(JSON.stringify({
      type: "join",
      peerId,
      data: { peers: existingPeers },
    } satisfies SignalingMessage));

    // Notify existing peers about the new peer
    this.broadcast({
      type: "peer-joined",
      peerId,
    }, server);

    // Reset TTL alarm when someone joins
    await this.resetTtlAlarm();

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, message: ArrayBuffer | string): Promise<void> {
    // Get peer ID from attachment (works across hibernation)
    const attachment = ws.deserializeAttachment() as SessionAttachment | null;
    if (!attachment?.peerId) {
      ws.close(1008, "Session not found");
      return;
    }
    const peerId = attachment.peerId;

    // Rate limiting check
    const now = Date.now();
    let rateLimit = this.rateLimits.get(peerId);
    if (!rateLimit) {
      // Initialize rate limit if not present (e.g., after hibernation)
      rateLimit = { count: 0, windowStart: now };
      this.rateLimits.set(peerId, rateLimit);
    }

    if (now - rateLimit.windowStart >= 1000) {
      // Reset window
      rateLimit.count = 0;
      rateLimit.windowStart = now;
    }

    rateLimit.count++;

    if (rateLimit.count > this.rateLimitPerSecond) {
      ws.send(JSON.stringify({
        type: "error",
        data: { message: "Rate limit exceeded" },
      } satisfies SignalingMessage));
      return;
    }

    // Parse and validate message
    let parsed: SignalingMessage;
    try {
      const msgStr = typeof message === "string" ? message : new TextDecoder().decode(message);
      parsed = JSON.parse(msgStr);
    } catch {
      ws.send(JSON.stringify({
        type: "error",
        data: { message: "Invalid JSON" },
      } satisfies SignalingMessage));
      return;
    }

    // Validate message type
    if (!["offer", "answer", "ice"].includes(parsed.type)) {
      ws.send(JSON.stringify({
        type: "error",
        data: { message: "Invalid message type" },
      } satisfies SignalingMessage));
      return;
    }

    // Relay message to all other peers
    const outgoingMessage: SignalingMessage = {
      type: parsed.type,
      peerId,
      data: parsed.data,
    };

    this.broadcast(outgoingMessage, ws);

    // Reset TTL on activity
    await this.resetTtlAlarm();
  }

  async webSocketClose(ws: WebSocket, _code: number, _reason: string, _wasClean: boolean): Promise<void> {
    await this.handleDisconnect(ws);
  }

  async webSocketError(ws: WebSocket, _error: unknown): Promise<void> {
    await this.handleDisconnect(ws);
  }

  private async handleDisconnect(ws: WebSocket): Promise<void> {
    const attachment = ws.deserializeAttachment() as SessionAttachment | null;
    if (attachment?.peerId) {
      // Clean up rate limit state
      this.rateLimits.delete(attachment.peerId);

      // Notify other peers about the disconnect
      this.broadcast({
        type: "peer-left",
        peerId: attachment.peerId,
      }, ws);
    }

    // If no more sessions, set TTL alarm
    const remaining = this.state.getWebSockets().filter((w) => w !== ws);
    if (remaining.length === 0) {
      await this.resetTtlAlarm();
    }
  }

  private broadcast(message: SignalingMessage, exclude?: WebSocket): void {
    const msgStr = JSON.stringify(message);
    for (const ws of this.state.getWebSockets()) {
      if (ws !== exclude) {
        try {
          ws.send(msgStr);
        } catch {
          // WebSocket may be closed, will be cleaned up in webSocketClose
        }
      }
    }
  }

  private async resetTtlAlarm(): Promise<void> {
    const alarmTime = Date.now() + this.roomTtlMs;
    await this.state.storage.setAlarm(alarmTime);
  }

  async alarm(): Promise<void> {
    // Check if there are still active sessions
    const activeSessions = this.state.getWebSockets();

    if (activeSessions.length === 0) {
      // No active sessions, room can be cleaned up
      // Durable Object will be evicted after this
      return;
    }

    // If there are still active sessions, the TTL was reset by activity
    // A new alarm would have been set if there was recent activity
  }
}
