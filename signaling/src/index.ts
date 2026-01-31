/**
 * Cloudflare Worker for WebRTC signaling
 * Routes WebSocket connections to Room Durable Objects
 */

import { Room } from "./room";

export interface Env {
  ROOM: DurableObjectNamespace;
  ROOM_TTL_SECONDS: string;
  RATE_LIMIT_MESSAGES_PER_SECOND: string;
}

export default {
  async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;

    // CORS headers for preflight
    if (request.method === "OPTIONS") {
      return new Response(null, {
        headers: {
          "Access-Control-Allow-Origin": "*",
          "Access-Control-Allow-Methods": "GET, OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type, Upgrade, Connection, Sec-WebSocket-Key, Sec-WebSocket-Version, Sec-WebSocket-Protocol",
        },
      });
    }

    // Health check endpoint
    if (path === "/health" || path === "/") {
      return new Response(JSON.stringify({ status: "ok" }), {
        headers: {
          "Content-Type": "application/json",
          "Access-Control-Allow-Origin": "*",
        },
      });
    }

    // Room WebSocket endpoint: GET /room/:id
    const roomMatch = path.match(/^\/room\/([a-zA-Z0-9_-]+)$/);
    if (roomMatch && request.method === "GET") {
      const roomId = roomMatch[1];

      // Validate room ID length
      if (roomId.length > 64) {
        return new Response(JSON.stringify({ error: "Room ID too long" }), {
          status: 400,
          headers: {
            "Content-Type": "application/json",
            "Access-Control-Allow-Origin": "*",
          },
        });
      }

      // Check for WebSocket upgrade
      if (request.headers.get("Upgrade") !== "websocket") {
        return new Response(JSON.stringify({ error: "Expected WebSocket upgrade" }), {
          status: 426,
          headers: {
            "Content-Type": "application/json",
            "Access-Control-Allow-Origin": "*",
          },
        });
      }

      // Get or create the Durable Object for this room
      const id = env.ROOM.idFromName(roomId);
      const room = env.ROOM.get(id);

      // Forward the request to the Durable Object
      return room.fetch(request);
    }

    // 404 for unknown routes
    return new Response(JSON.stringify({ error: "Not found" }), {
      status: 404,
      headers: {
        "Content-Type": "application/json",
        "Access-Control-Allow-Origin": "*",
      },
    });
  },
};

// Export the Durable Object class
export { Room };
