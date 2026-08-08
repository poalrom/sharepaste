import { describe, it, expect } from "vitest";
import { seedInvite, withApp } from "../helpers.js";
import { randomToken, sha256Hex } from "../../src/crypto.js";
import { DeviceCredentials } from "../../src/server/device-credentials.js";

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

      const listed = DeviceCredentials.list(repo, "u1").find(
        (d) => d.device_id === body.device_id
      );
      expect(listed?.label).toBe("macbook");
      expect(listed?.revoked_at).toBeNull();

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

/**
 * The route no longer re-types these bounds: it shares `SECRET_PROOF` (16..256) with a
 * **Pair Slot**'s secret proof and `DEVICE_LABEL` (1..128) with a **Device Label**. A
 * length outside them is a shape refusal (400); one inside them that names no **Invite**
 * reaches the lookup and earns its 404, which is what tells the two apart.
 */
describe("POST /claim-invite body bounds", () => {
  const ROWS = [
    { what: "a token under 16 chars", token: "a".repeat(15), label: "x", status: 400 },
    { what: "a token at 16 chars", token: "a".repeat(16), label: "x", status: 404 },
    { what: "a token at 256 chars", token: "a".repeat(256), label: "x", status: 404 },
    { what: "a token over 256 chars", token: "a".repeat(257), label: "x", status: 400 },
    { what: "an empty device label", token: "a".repeat(32), label: "", status: 400 },
    { what: "a device label at 128 chars", token: "a".repeat(32), label: "x".repeat(128), status: 404 },
    { what: "a device label over 128 chars", token: "a".repeat(32), label: "x".repeat(129), status: 400 },
  ];

  for (const row of ROWS) {
    it(`answers ${row.status} for ${row.what}`, () =>
      withApp(async ({ app }) => {
        const res = await app.inject({
          method: "POST",
          url: "/claim-invite",
          payload: { token: row.token, device_label: row.label },
        });
        expect(res.statusCode).toBe(row.status);
      }));
  }
});
