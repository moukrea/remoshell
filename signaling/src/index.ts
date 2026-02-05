/**
 * Cloudflare Worker for WebRTC signaling
 * Routes WebSocket connections to Room Durable Objects
 * and handles pairing code registration/lookup via PairingStore
 */

import { Room } from "./room";
import { PairingStore } from "./pairing";

export interface Env {
  ROOM: DurableObjectNamespace;
  PAIRING: DurableObjectNamespace;
  ROOM_TTL_SECONDS: string;
  RATE_LIMIT_MESSAGES_PER_SECOND: string;
  PAIRING_TTL_SECONDS: string;
}

/** CORS headers applied to all responses */
function corsHeaders(): Record<string, string> {
  return {
    "Access-Control-Allow-Origin": "*",
  };
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
          "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type, Upgrade, Connection, Sec-WebSocket-Key, Sec-WebSocket-Version, Sec-WebSocket-Protocol",
        },
      });
    }

    // Health check endpoint
    if (path === "/health" || path === "/") {
      return new Response(JSON.stringify({ status: "ok" }), {
        headers: {
          "Content-Type": "application/json",
          ...corsHeaders(),
        },
      });
    }

    // POST /pair — Register a pairing code
    if (path === "/pair" && request.method === "POST") {
      // Validate Content-Type
      const contentType = request.headers.get("Content-Type") || "";
      if (!contentType.includes("application/json")) {
        return new Response(
          JSON.stringify({ error: "Content-Type must be application/json" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json", ...corsHeaders() },
          }
        );
      }

      let body: { code?: string; info?: Record<string, unknown>; ttl?: number };
      try {
        body = await request.json();
      } catch {
        return new Response(
          JSON.stringify({ error: "Invalid JSON body" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json", ...corsHeaders() },
          }
        );
      }

      // Validate code
      if (!body.code || typeof body.code !== "string" || body.code.length < 3 || body.code.length > 20) {
        return new Response(
          JSON.stringify({ error: "Invalid code: must be a string of 3-20 characters" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json", ...corsHeaders() },
          }
        );
      }

      // Validate info
      if (!body.info || typeof body.info !== "object") {
        return new Response(
          JSON.stringify({ error: "Invalid info: must be an object" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json", ...corsHeaders() },
          }
        );
      }

      const info = body.info as Record<string, unknown>;
      if (!info.device_id || !info.public_key || !info.relay_url || typeof info.expires !== "number") {
        return new Response(
          JSON.stringify({ error: "Invalid info: must contain device_id, public_key, relay_url, and expires" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json", ...corsHeaders() },
          }
        );
      }

      // Forward to PairingStore DO
      const id = env.PAIRING.idFromName("global");
      const stub = env.PAIRING.get(id);
      const doResponse = await stub.fetch(
        new Request("https://pairing/register", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ code: body.code, info: body.info, ttl: body.ttl }),
        })
      );

      const responseBody = await doResponse.text();
      return new Response(responseBody, {
        status: doResponse.status,
        headers: {
          "Content-Type": "application/json",
          ...corsHeaders(),
        },
      });
    }

    // GET /pair/:code — Look up a pairing code
    const pairMatch = path.match(/^\/pair\/([A-Za-z0-9_-]+)$/);
    if (pairMatch && request.method === "GET") {
      const code = pairMatch[1];

      const id = env.PAIRING.idFromName("global");
      const stub = env.PAIRING.get(id);
      const doResponse = await stub.fetch(
        new Request(`https://pairing/lookup/${code}`, {
          method: "GET",
        })
      );

      const responseBody = await doResponse.text();
      return new Response(responseBody, {
        status: doResponse.status,
        headers: {
          "Content-Type": "application/json",
          ...corsHeaders(),
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
            ...corsHeaders(),
          },
        });
      }

      // Check for WebSocket upgrade
      if (request.headers.get("Upgrade") !== "websocket") {
        return new Response(JSON.stringify({ error: "Expected WebSocket upgrade" }), {
          status: 426,
          headers: {
            "Content-Type": "application/json",
            ...corsHeaders(),
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
        ...corsHeaders(),
      },
    });
  },
};

// Export the Durable Object classes
export { Room, PairingStore };
