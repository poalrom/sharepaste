import { describe, it, expect } from "vitest";
import { seedInvite, withApp } from "../helpers.js";
import { randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /claim-invite", () => {
  it("issues a device_token and creates a membership on a fresh invite", () =>
    withApp(async ({ app, repo }) => {
      const token = seedInvite(repo);
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "macbook" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; user_id: string; device_id: string };
      expect(body.user_id).toBe("u1");

      const mem = repo.memberships.findByDeviceId("u1", body.device_id);
      expect(mem?.device_label).toBe("macbook");
      expect(mem?.revoked_at).toBeNull();

      const inv = repo.invites.findByHash(sha256Hex(token));
      expect(inv?.claimed_at).not.toBeNull();
    }));

  it("returns 409 if the invite was already claimed", () =>
    withApp(async ({ app, repo }) => {
      const token = seedInvite(repo);
      await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "a" },
      });
      const res2 = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "b" },
      });
      expect(res2.statusCode).toBe(409);
    }));

  it("returns 404 for an unknown token", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token: randomToken(), device_label: "x" },
      });
      expect(res.statusCode).toBe(404);
    }));

  it("returns 410 for an expired invite", () =>
    withApp(async ({ app, repo }) => {
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
    }));

  it("returns 400 if body is malformed", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({ method: "POST", url: "/claim-invite", payload: {} });
      expect(res.statusCode).toBe(400);
    }));
});
