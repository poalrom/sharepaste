import { describe, it, expect } from "vitest";
import { addDevice, provisionDevice, startAndClaim, startPair, withApp } from "../helpers.js";
import { hashToken, randomId, randomToken, sha256Hex } from "../../src/crypto.js";

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

interface MeBody {
  user: { id: string; username: string };
  devices: {
    device_id: string;
    label: string | null;
    created_at: number;
    revoked_at: number | null;
  }[];
}

describe("GET /me", () => {
  it("returns the caller's user and devices", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const second = await addDevice(repo, a.user_id, "ipad");

      const res = await app.inject({
        method: "GET",
        url: "/me",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);

      const body = res.json() as MeBody;
      expect(body.user).toEqual({ id: a.user_id, username: "alice" });
      expect(body.devices.map((d) => d.device_id).sort()).toEqual(
        [a.device_id, second.device_id].sort()
      );

      const ipad = body.devices.find((d) => d.device_id === second.device_id);
      expect(ipad?.label).toBe("ipad");
      expect(ipad?.revoked_at).toBeNull();
      expect(typeof ipad?.created_at).toBe("number");
    }));

  it("never emits token material", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const mem = repo.memberships.findByDeviceId(a.user_id, a.device_id);
      const tokenHash = mem?.device_token_hash ?? "";
      const tokenSha = mem?.token_sha256 ?? "";
      expect(tokenHash.length).toBeGreaterThan(0);
      expect(tokenSha.length).toBeGreaterThan(0);

      const res = await app.inject({
        method: "GET",
        url: "/me",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const raw = res.body;

      expect(raw).not.toContain("device_token_hash");
      expect(raw).not.toContain("token_sha256");
      expect(raw).not.toContain(tokenHash);
      expect(raw).not.toContain(tokenSha);
      expect(raw).not.toContain(a.device_token);

      for (const device of (res.json() as MeBody).devices) {
        expect(Object.keys(device).sort()).toEqual([
          "created_at",
          "device_id",
          "label",
          "revoked_at",
        ]);
      }
    }));

  it("does not leak another user's devices", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");

      const res = await app.inject({
        method: "GET",
        url: "/me",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const body = res.json() as MeBody;

      expect(body.user.id).toBe(a.user_id);
      expect(body.devices).toHaveLength(1);
      expect(body.devices[0]?.device_id).toBe(a.device_id);
      expect(res.body).not.toContain(b.device_id);
      expect(res.body).not.toContain(b.user_id);
    }));

  it("includes a revoked device with its revoked_at", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const second = await addDevice(repo, a.user_id, "retired");
      const revokedAt = Date.now();
      repo.memberships.revoke(a.user_id, second.device_id, revokedAt);

      const res = await app.inject({
        method: "GET",
        url: "/me",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const retired = (res.json() as MeBody).devices.find(
        (d) => d.device_id === second.device_id
      );
      expect(retired?.revoked_at).toBe(revokedAt);
    }));

  it("surfaces a null device_label as label: null", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const unlabelled = randomId();
      const token = randomToken();
      repo.memberships.create({
        user_id: a.user_id,
        device_id: unlabelled,
        device_token_hash: await hashToken(token),
        token_sha256: sha256Hex(token),
        device_label: null,
        created_at: Date.now(),
        revoked_at: null,
      });

      const res = await app.inject({
        method: "GET",
        url: "/me",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const found = (res.json() as MeBody).devices.find((d) => d.device_id === unlabelled);
      expect(found).toBeDefined();
      expect(found?.label).toBeNull();
    }));

  it("returns 401 without a bearer token", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({ method: "GET", url: "/me" });
      expect(res.statusCode).toBe(401);
    }));
});
