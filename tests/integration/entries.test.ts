import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";

const cipherB64 = (s: string) => Buffer.from(s).toString("base64");

describe("POST /entries", () => {
  it("stores ciphertext and returns id + created_at", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("opaque") },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { id: number; created_at: number };
      expect(typeof body.id).toBe("number");
      expect(body.created_at).toBeLessThanOrEqual(Date.now());
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    } finally {
      await close();
    }
  });

  it("rejects oversized ciphertext", async () => {
    const { app, repo, close } = await buildTestApp({ maxEntryBytes: 16 });
    try {
      const a = await provisionDevice(repo);
      const big = cipherB64("x".repeat(64));
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: big },
      });
      expect(res.statusCode).toBe(413);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });

  it("prunes the oldest entry past the count cap (inline)", async () => {
    const { app, repo, close } = await buildTestApp({ maxEntries: 2 });
    try {
      const a = await provisionDevice(repo);
      for (let i = 0; i < 5; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
      }
      expect(repo.entries.countForUser(a.user_id)).toBe(2);
    } finally {
      await close();
    }
  });

  it("rejects without auth", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        payload: { ciphertext: cipherB64("hi") },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });
});

describe("GET /entries", () => {
  it("returns entries since a given id, scoped to caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
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
      const list = res.json() as Array<{ id: number; ciphertext: string }>;
      expect(list).toHaveLength(3);
      expect(list[0]!.id).toBeLessThan(list[2]!.id);
    } finally {
      await close();
    }
  });

  it("respects since pagination", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const ids: number[] = [];
      for (let i = 0; i < 5; i++) {
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
        ids.push((r.json() as { id: number }).id);
      }
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=${ids[2]}&limit=100`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number }>;
      expect(list.map((e) => e.id)).toEqual(ids.slice(3));
    } finally {
      await close();
    }
  });
});

describe("DELETE /entries/:id and DELETE /entries", () => {
  it("removes a single entry for the caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const id = (r.json() as { id: number }).id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });

  it("cannot delete another user's entry", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const id = (r.json() as { id: number }).id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${b.device_token}` },
      });
      expect(del.statusCode).toBe(404);
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    } finally {
      await close();
    }
  });

  it("DELETE /entries purges all of caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64(String(i)) },
        });
      }
      const del = await app.inject({
        method: "DELETE",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });
});

describe("concurrent uploads", () => {
  it("two devices uploading at once each get a distinct id", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      // second device for same user
      const { hashToken, randomId, randomToken } = await import("../../src/crypto.js");
      const t2 = randomToken();
      const d2 = randomId();
      repo.memberships.create({
        user_id: a.user_id,
        device_id: d2,
        device_token_hash: await hashToken(t2),
        device_label: "two",
        created_at: Date.now(),
        revoked_at: null,
      });

      const cipher = Buffer.from("x").toString("base64");
      const [r1, r2] = await Promise.all([
        app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipher },
        }),
        app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${t2}` },
          payload: { ciphertext: cipher },
        }),
      ]);
      const id1 = (r1.json() as { id: number }).id;
      const id2 = (r2.json() as { id: number }).id;
      expect(id1).not.toBe(id2);
    } finally {
      await close();
    }
  });
});
