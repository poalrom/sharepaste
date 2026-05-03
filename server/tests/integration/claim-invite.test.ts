import { describe, it, expect } from "vitest";
import { buildTestApp } from "../helpers.js";
import { hashToken, randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /claim-invite", () => {
  const seedInvite = async (repo: any, userId = "u1") => {
    repo.users.create({ id: userId, username: "alice" });
    const token = randomToken();
    repo.invites.create({
      token_hash: sha256Hex(token),
      user_id: userId,
      expires_at: Date.now() + 60_000,
      claimed_at: null,
    });
    return token;
  };

  it("issues a device_token and creates a membership on a fresh invite", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const token = await seedInvite(repo);
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "macbook" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; user_id: string; device_id: string };
      expect(body.user_id).toBe("u1");
      expect(body.device_token).toMatch(/^[A-Za-z0-9_-]{43}$/);
      expect(body.device_id).toMatch(/^[0-9a-f-]{36}$/);

      const mem = repo.memberships.findByDeviceId("u1", body.device_id);
      expect(mem?.device_label).toBe("macbook");
      expect(mem?.revoked_at).toBeNull();

      const inv = repo.invites.findByHash(sha256Hex(token));
      expect(inv?.claimed_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 409 if the invite was already claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const token = await seedInvite(repo);
      await app.inject({ method: "POST", url: "/claim-invite", payload: { token, device_label: "a" } });
      const res2 = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "b" },
      });
      expect(res2.statusCode).toBe(409);
    } finally {
      await close();
    }
  });

  it("returns 404 for an unknown token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token: randomToken(), device_label: "x" },
      });
      expect(res.statusCode).toBe(404);
    } finally {
      await close();
    }
  });

  it("returns 410 for an expired invite", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      repo.users.create({ id: "u1", username: "alice" });
      const token = randomToken();
      repo.invites.create({
        token_hash: sha256Hex(token),
        user_id: "u1",
        expires_at: Date.now() - 1,
        claimed_at: null,
      });
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "x" },
      });
      expect(res.statusCode).toBe(410);
    } finally {
      await close();
    }
  });

  it("returns 400 if body is malformed", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({ method: "POST", url: "/claim-invite", payload: {} });
      expect(res.statusCode).toBe(400);
    } finally {
      await close();
    }
  });
});
