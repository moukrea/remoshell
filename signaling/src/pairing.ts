/**
 * PairingStore Durable Object
 *
 * Stores pairing code -> PairingInfo mappings with TTL-based expiry.
 * Used to enable short-code pairing flow where the daemon registers a
 * human-readable code and the client looks it up.
 */

export interface PairingEntry {
  info: unknown;
  expires: number;
}

export class PairingStore implements DurableObject {
  private state: DurableObjectState;

  constructor(state: DurableObjectState, _env: unknown) {
    this.state = state;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    if (request.method === "POST") {
      const body = (await request.json()) as {
        code: string;
        info: unknown;
        ttl?: number;
      };
      const ttl = ((body.ttl && body.ttl > 0 ? body.ttl : 300) * 1000);
      const entry: PairingEntry = {
        info: body.info,
        expires: Date.now() + ttl,
      };
      await this.state.storage.put(`pair:${body.code}`, JSON.stringify(entry));
      // Schedule alarm for cleanup after TTL
      await this.scheduleCleanup(Date.now() + ttl + 1000);
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    if (request.method === "GET") {
      const code = url.pathname.split("/").pop();
      if (!code) {
        return new Response(JSON.stringify({ error: "No code provided" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        });
      }
      const raw = (await this.state.storage.get(`pair:${code}`)) as
        | string
        | undefined;
      if (!raw) {
        return new Response(JSON.stringify({ error: "Not found" }), {
          status: 404,
          headers: { "Content-Type": "application/json" },
        });
      }
      const entry = JSON.parse(raw) as PairingEntry;
      if (Date.now() > entry.expires) {
        await this.state.storage.delete(`pair:${code}`);
        return new Response(JSON.stringify({ error: "Expired" }), {
          status: 404,
          headers: { "Content-Type": "application/json" },
        });
      }
      return new Response(JSON.stringify(entry.info), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("Method not allowed", { status: 405 });
  }

  private async scheduleCleanup(time: number): Promise<void> {
    const current = await this.state.storage.getAlarm();
    if (!current || current > time) {
      await this.state.storage.setAlarm(time);
    }
  }

  async alarm(): Promise<void> {
    const entries = await this.state.storage.list({ prefix: "pair:" });
    const now = Date.now();
    for (const [key, raw] of entries) {
      const entry = JSON.parse(raw as string) as PairingEntry;
      if (now > entry.expires) {
        await this.state.storage.delete(key);
      }
    }
  }
}
