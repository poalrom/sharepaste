import { describe, it, expect } from "vitest";
import { cipherB64, provisionDevice, withApp } from "../helpers.js";

describe("POST /entries", () => {
  it("stores ciphertext and returns id + created_at", () =>
    withApp(async ({ app, repo }) => {
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
    }));

  it("rejects oversized ciphertext", () =>
    withApp(
      async ({ app, repo }) => {
        const a = await provisionDevice(repo);
        const res = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x".repeat(64)) },
        });
        expect(res.statusCode).toBe(413);
        expect(repo.entries.countForUser(a.user_id)).toBe(0);
      },
      { maxEntryBytes: 16 }
    ));

  it("rejects malformed base64", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: "!!!!" },
      });
      expect(res.statusCode).toBe(400);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
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

describe("GET /entries", () => {
  it("returns entries since a given id, scoped to caller's user", () =>
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
      const list = res.json() as Array<{ id: number; ciphertext: string }>;
      expect(list).toHaveLength(3);
      expect(list[0]!.id).toBeLessThan(list[2]!.id);
    }));

  it("respects since pagination", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const ids: number[] = [];
      for (let i = 0; i < 5; i++) {
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
        const created = r.json() as { id: number };
        ids.push(created.id);
      }
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=${ids[2]}&limit=100`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number }>;
      expect(list.map((e) => e.id)).toEqual(ids.slice(3));
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
