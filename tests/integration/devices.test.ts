import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";
import {
  randomToken,
  sha256Hex,
} from "../../src/crypto.js";

describe("POST /devices", () => {
  const startAndClaim = async (app: any, repo: any) => {
    const a = await provisionDevice(repo);
    const secret = randomToken();
    const start = await app.inject({
      method: "POST",
      url: "/pair/start",
      headers: { authorization: `Bearer ${a.device_token}` },
      payload: { secret_hash: sha256Hex(secret) },
    });
    const pair_id = start.json().pair_id as string;
    await app.inject({
      method: "POST",
      url: "/pair/claim",
      payload: { pair_id, secret_proof: secret },
    });
    return { ...a, secret, pair_id };
  };

  it("issues a device_token and consumes the slot", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "ipad" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; device_id: string };
      expect(body.device_token).toMatch(/^[A-Za-z0-9_-]{43}$/);

      const slot = repo.pairings.find(ctx.pair_id);
      expect(slot?.consumed_at).not.toBeNull();
      expect(slot?.encrypted_payload).toBeNull();

      const mem = repo.memberships.findByDeviceId(ctx.user_id, body.device_id);
      expect(mem?.device_label).toBe("ipad");
    } finally {
      await close();
    }
  });

  it("returns 410 if called twice on the same slot", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
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
    } finally {
      await close();
    }
  });

  it("returns 403 with a wrong secret_proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken(), label: "x" },
      });
      expect(res.statusCode).toBe(403);
    } finally {
      await close();
    }
  });

  it("returns 409 if the slot was never claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const secret = randomToken();
      const start = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: sha256Hex(secret) },
      });
      const pair_id = start.json().pair_id as string;
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id, secret_proof: secret, label: "x" },
      });
      expect(res.statusCode).toBe(409);
    } finally {
      await close();
    }
  });
});

describe("DELETE /devices/:id", () => {
  it("revokes a sibling device of the same user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const secondToken = randomToken();
      const secondId = (await import("node:crypto")).randomUUID();
      const { hashToken } = await import("../../src/crypto.js");
      repo.memberships.create({
        user_id: a.user_id,
        device_id: secondId,
        device_token_hash: await hashToken(secondToken),
        device_label: "second",
        created_at: Date.now(),
        revoked_at: null,
      });

      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${secondId}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);

      const mem = repo.memberships.findByDeviceId(a.user_id, secondId);
      expect(mem?.revoked_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("a revoked token can no longer be used", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${a.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      const res2 = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res2.statusCode).toBe(401);
    } finally {
      await close();
    }
  });

  it("cannot revoke a device of a different user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${b.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(404);
      expect(repo.memberships.findByDeviceId(b.user_id, b.device_id)?.revoked_at).toBeNull();
    } finally {
      await close();
    }
  });
});
