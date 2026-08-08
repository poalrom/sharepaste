import { describe, it, expect } from "vitest";
import { addDevice, cipherB64, provisionDevice, withApp } from "../helpers.js";
import { entryRules } from "../../src/server/refusal.js";
import { SseHub, type SseEvent } from "../../src/server/sse-hub.js";

describe("POST /entries", () => {
  it("stores ciphertext and returns id + created_at + seq + last_use", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("opaque") },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { id: number; created_at: number; seq: number; last_use: number };
      expect(typeof body.id).toBe("number");
      expect(body.created_at).toBeLessThanOrEqual(Date.now());
      expect(body.seq).toBe(1);
      expect(body.last_use).toBe(body.created_at);
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    }));

  it("rejects without auth", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        payload: { ciphertext: cipherB64("hi") },
      });
      expect(res.statusCode).toBe(401);
    }));
});

/**
 * Every status a refusable request earns, end to end. The client maps 413 and 400..=499 to
 * BadInput (`clients/core/src/http/client.rs:78-87`) and BadInput to Refused
 * (`sync/uploader.rs:259-264`), so each status below is this Relay's
 * Refused-versus-unreachable verdict on one act (ADR 0015). The rows cover all four
 * validators: the request body limit, the wire schema, the size rule and the id.
 */
describe("refusable requests", () => {
  const CAP = 16;
  const bearer = (token: string) => ({ authorization: `Bearer ${token}` });
  const json = (token: string) => ({ ...bearer(token), "content-type": "application/json" });

  interface Request {
    method: "POST" | "DELETE";
    url: string;
    headers?: Record<string, string>;
    payload?: object | string;
  }

  const ROWS: { what: string; status: number; request: (token: string) => Request }[] = [
    {
      what: "a ciphertext over the cap",
      status: 413,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: bearer(t),
        payload: { ciphertext: cipherB64("x".repeat(CAP + 1)) },
      }),
    },
    {
      what: "a ciphertext that is not base64",
      status: 400,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: bearer(t),
        payload: { ciphertext: "!!!!" },
      }),
    },
    {
      what: "an empty ciphertext",
      status: 400,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: bearer(t),
        payload: { ciphertext: "" },
      }),
    },
    {
      what: "a partial base64 group",
      status: 400,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: bearer(t),
        payload: { ciphertext: "AAA" },
      }),
    },
    {
      what: "no ciphertext at all",
      status: 400,
      request: (t) => ({ method: "POST", url: "/entries", headers: bearer(t), payload: {} }),
    },
    {
      what: "a body that is not an object",
      status: 400,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: { ...bearer(t), "content-type": "text/plain" },
        payload: "hello",
      }),
    },
    {
      what: "a body that is not JSON",
      status: 400,
      request: (t) => ({ method: "POST", url: "/entries", headers: json(t), payload: "{" }),
    },
    {
      what: "a body past the request limit",
      status: 413,
      request: (t) => ({
        method: "POST",
        url: "/entries",
        headers: json(t),
        payload: JSON.stringify({ ciphertext: "A".repeat(2 * 1024 * 1024) }),
      }),
    },
    {
      what: "an id that is not a number",
      status: 400,
      request: (t) => ({ method: "POST", url: "/entries/abc/use", headers: bearer(t) }),
    },
    {
      what: "an id of zero",
      status: 400,
      request: (t) => ({ method: "POST", url: "/entries/0/use", headers: bearer(t) }),
    },
    {
      what: "a malformed id on delete",
      status: 400,
      request: (t) => ({ method: "DELETE", url: "/entries/abc", headers: bearer(t) }),
    },
    // The size rule sits behind the bearer check while the wire schema sits in front of
    // it, so an unauthenticated device is told which of the two it tripped. Neither answer
    // moves because a refusal is now decided in one place.
    {
      what: "an oversized ciphertext with no bearer token",
      status: 401,
      request: () => ({
        method: "POST",
        url: "/entries",
        payload: { ciphertext: cipherB64("x".repeat(CAP + 1)) },
      }),
    },
    {
      what: "a non-base64 ciphertext with no bearer token",
      status: 400,
      request: () => ({ method: "POST", url: "/entries", payload: { ciphertext: "!!!!" } }),
    },
  ];

  for (const row of ROWS) {
    it(`answers ${row.status} for ${row.what}`, () =>
      withApp(
        async ({ app, repo }) => {
          const a = await provisionDevice(repo);
          const res = await app.inject(row.request(a.device_token));
          expect(res.statusCode).toBe(row.status);
          expect(repo.entries.countForUser(a.user_id)).toBe(0);
        },
        { entryRules: entryRules({ maxEntryBytes: CAP }) }
      ));
  }

  it("never answers 5xx for a shape problem, so a refusal is never mistaken for unreachable", () =>
    withApp(
      async ({ app, repo }) => {
        const a = await provisionDevice(repo);
        for (const row of ROWS) {
          const res = await app.inject(row.request(a.device_token));
          expect(res.statusCode, row.what).toBeGreaterThanOrEqual(400);
          expect(res.statusCode, row.what).toBeLessThan(500);
        }
      },
      { entryRules: entryRules({ maxEntryBytes: CAP }) }
    ));
});

describe("GET /entries", () => {
  it("returns entries since a given sequence, scoped to caller's user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("a" + i) },
        });
      }
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: cipherB64("b") },
      });

      const res = await app.inject({
        method: "GET",
        url: "/entries?since=0&limit=100",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number; seq: number; last_use: number }>;
      expect(list).toHaveLength(3);
      expect(list.map((e) => e.seq)).toEqual([1, 2, 3]);
      expect(list[0]!.id).toBeLessThan(list[2]!.id);
      expect(list[0]!.last_use).toBeLessThanOrEqual(Date.now());
    }));

  it("respects since pagination", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const posted: Array<{ id: number; seq: number }> = [];
      for (let i = 0; i < 5; i++) {
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
        posted.push(r.json() as { id: number; seq: number });
      }
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=${posted[2]!.seq}&limit=100`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number }>;
      expect(list.map((e) => e.id)).toEqual(posted.slice(3).map((e) => e.id));
    }));

  it("hands a used entry back to a client whose watermark is already past it", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const post = async (text: string) => {
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64(text) },
        });
        return r.json() as { id: number; seq: number };
      };
      const first = await post("first");
      const second = await post("second");

      const used = await app.inject({
        method: "POST",
        url: `/entries/${first.id}/use`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const { seq } = used.json() as { seq: number };
      expect(seq).toBeGreaterThan(second.seq);

      // A device caught up to the later capture still learns about the use.
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=${second.seq}&limit=100`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number; seq: number; ciphertext: string }>;
      expect(list.map((e) => e.id)).toEqual([first.id]);
      expect(list[0]!.seq).toBe(seq);
      expect(list[0]!.ciphertext).toBe(cipherB64("first"));
    }));
});

describe("DELETE /entries/:id and DELETE /entries", () => {
  it("removes a single entry for the caller's user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const created = r.json() as { id: number };
      const id = created.id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    }));

  it("cannot delete another user's entry", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const created = r.json() as { id: number };
      const id = created.id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${b.device_token}` },
      });
      expect(del.statusCode).toBe(404);
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    }));

  it("DELETE /entries purges all of caller's user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64(String(i)) },
        });
      }
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: cipherB64("bob-secret") },
      });

      const del = await app.inject({
        method: "DELETE",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
      expect(repo.entries.countForUser(b.user_id)).toBe(1);
    }));
});

describe("POST /entries/:id/use", () => {
  it("stamps the entry and republishes it on the entry frame", async () => {
    const hub = new SseHub();
    const frames: SseEvent[] = [];
    await withApp(
      async ({ app, repo }) => {
        const a = await provisionDevice(repo);
        const origin = await addDevice(repo, a.user_id, "laptop");
        const created = repo.entries.insertAndPrune(
          {
            user_id: a.user_id,
            device_id: origin.device_id,
            ciphertext_b64: cipherB64("secret"),
            size: 6,
            created_at: Date.now() - 60_000,
          },
          100,
          30 * 24 * 3600 * 1000
        );
        hub.subscribe(a.user_id, (e) => frames.push(e));

        const res = await app.inject({
          method: "POST",
          url: `/entries/${created.id}/use`,
          headers: { authorization: `Bearer ${a.device_token}` },
        });
        expect(res.statusCode).toBe(200);
        const body = res.json() as { seq: number; last_use: number };
        expect(body.seq).toBe(created.seq + 1);
        expect(body.last_use).toBeGreaterThan(created.created_at);

        // The Origin device, not the one that asked: the relay records no user device.
        expect(frames).toEqual([
          {
            type: "entry",
            id: created.id,
            ciphertext: cipherB64("secret"),
            created_at: created.created_at,
            device_id: origin.device_id,
            seq: body.seq,
            last_use: body.last_use,
          },
        ]);
      },
      { hub }
    );
  });

  it("cannot use another user's entry, and publishes nothing", async () => {
    const hub = new SseHub();
    const frames: SseEvent[] = [];
    await withApp(
      async ({ app, repo }) => {
        const a = await provisionDevice(repo);
        const b = await provisionDevice(repo, "bob");
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("hi") },
        });
        const created = r.json() as { id: number; seq: number; last_use: number };
        hub.subscribe(a.user_id, (e) => frames.push(e));
        hub.subscribe(b.user_id, (e) => frames.push(e));

        const res = await app.inject({
          method: "POST",
          url: `/entries/${created.id}/use`,
          headers: { authorization: `Bearer ${b.device_token}` },
        });
        expect(res.statusCode).toBe(404);
        expect(frames).toEqual([]);
        expect(repo.entries.listSince(a.user_id, 0, 10)[0]!.last_use).toBe(created.last_use);
      },
      { hub }
    );
  });

  it("rejects a non-integer id", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "POST",
        url: "/entries/abc/use",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(400);
    }));

  it("rejects without auth", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({ method: "POST", url: "/entries/1/use" });
      expect(res.statusCode).toBe(401);
    }));
});
