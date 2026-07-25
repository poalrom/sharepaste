import { describe, it, expect } from "vitest";
import { addDevice, provisionDevice, startAndClaim, startPair, withApp } from "../helpers.js";
import { randomToken } from "../../src/crypto.js";

describe("POST /devices", () => {
  it("issues a device_token and consumes the slot", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "ipad" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; device_id: string };

      const authed = await app.inject({
        method: "GET",
        url: "/entries?since=0",
        headers: { authorization: `Bearer ${body.device_token}` },
      });
      expect(authed.statusCode).not.toBe(401);

      const slot = repo.pairings.find(ctx.pair_id);
      expect(slot?.consumed_at).not.toBeNull();
      expect(slot?.encrypted_payload).toBeNull();

      const mem = repo.memberships.findByDeviceId(ctx.user_id, body.device_id);
      expect(mem?.device_label).toBe("ipad");
    }));

  it("returns 410 if called twice on the same slot", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "a" },
      });
      const res2 = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "b" },
      });
      expect(res2.statusCode).toBe(410);
    }));

  it("returns 403 with a wrong secret_proof", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken(), label: "x" },
      });
      expect(res.statusCode).toBe(403);
    }));

  it("returns 409 if the slot was never claimed", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "x" },
      });
      expect(res.statusCode).toBe(409);
    }));
});

describe("DELETE /devices/:id", () => {
  it("revokes a sibling device of the same user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const second = await addDevice(repo, a.user_id, "second");

      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${second.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);

      const mem = repo.memberships.findByDeviceId(a.user_id, second.device_id);
      expect(mem?.revoked_at).not.toBeNull();
    }));

  it("cannot revoke a device of a different user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${b.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(404);
      expect(repo.memberships.findByDeviceId(b.user_id, b.device_id)?.revoked_at).toBeNull();
    }));
});
