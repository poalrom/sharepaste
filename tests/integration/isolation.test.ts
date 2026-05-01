import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";

const cipherB64 = (s: string) => Buffer.from(s).toString("base64");

describe("multi-user isolation", () => {
  it("user A cannot see, list, or delete user B entries", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo, "alice");
      const b = await provisionDevice(repo, "bob");

      const post = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: cipherB64("bob-secret") },
      });
      const bId = (post.json() as { id: number }).id;

      // GET as A: empty
      const list = await app.inject({
        method: "GET",
        url: "/entries?since=0",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(list.json()).toEqual([]);

      // DELETE B's entry as A: 404
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${bId}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(404);

      // DELETE all as A: B's row survives
      await app.inject({
        method: "DELETE",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(repo.entries.countForUser(b.user_id)).toBe(1);
    } finally {
      await close();
    }
  });
});
