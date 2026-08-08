import { describe, it, expect } from "vitest";
import { provisionDevice, startAndClaim, startPair, withApp } from "../helpers.js";
import { randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /pair/start", () => {
  it("opens a slot bound to the caller's user", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const secret = randomToken();
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: sha256Hex(secret) },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { pair_id: string };

      const pairing = repo.pairSlots.find(body.pair_id);
      expect(pairing?.user_id).toBe(a.user_id);
      expect(pairing?.secret_hash).toBe(sha256Hex(secret));
      expect(pairing?.consumed_at).toBeNull();
      expect(pairing?.expires_at).toBeGreaterThan(Date.now());
    }));
});

describe("POST /pair/claim", () => {
  it("accepts a correct secret_proof and marks the slot claimed", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(200);
      expect(repo.pairSlots.find(ctx.pair_id)?.claimed_at).not.toBeNull();
    }));

  it("returns 403 on a wrong secret_proof and increments failed_attempts", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(403);
      expect(repo.pairSlots.find(ctx.pair_id)?.failed_attempts).toBe(1);
    }));

  it("burns the slot after 3 wrong attempts", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/pair/claim",
          payload: { pair_id: ctx.pair_id, secret_proof: randomToken() },
        });
      }
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(410);
      expect(repo.pairSlots.find(ctx.pair_id)?.consumed_at).not.toBeNull();
    }));

  it("returns 410 on an expired slot", () =>
    withApp(
      async ({ app, repo }) => {
        const ctx = await startPair(app, repo);
        const res = await app.inject({
          method: "POST",
          url: "/pair/claim",
          payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
        });
        expect(res.statusCode).toBe(410);
      },
      { pairingTtlMs: 0 }
    ));

  it("returns 404 for an unknown pair_id", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: "00000000-0000-0000-0000-000000000000", secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(404);
    }));
});

describe("/pair/payload (upload + download)", () => {
  it("inviter uploads ciphertext, claimer downloads with correct proof", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const cipher = Buffer.from("opaque-pair-payload").toString("base64");
      const up = await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${ctx.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: cipher },
      });
      expect(up.statusCode).toBe(200);

      const down = await app.inject({
        method: "GET",
        url: `/pair/payload?id=${ctx.pair_id}&proof=${ctx.secret}`,
      });
      expect(down.statusCode).toBe(200);
      expect(down.json()).toEqual({ encrypted_payload: cipher });
    }));

  it("rejects payload upload from a non-inviter token", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      const other = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${other.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: "AAAA" },
      });
      expect(res.statusCode).toBe(403);
    }));

  it("rejects download with wrong proof", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${ctx.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: "AAAA" },
      });
      const down = await app.inject({
        method: "GET",
        url: `/pair/payload?id=${ctx.pair_id}&proof=wrongproofvalue1234567890`,
      });
      expect(down.statusCode).toBe(403);
    }));
});

describe("GET /pair/poll", () => {
  it("returns claimed status once the slot is claimed", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "claimed" });
    }));

  it("returns the paired device label once the slot is consumed", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const paired = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "Pixel 9" },
      });
      expect(paired.statusCode).toBe(200);

      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "consumed", device_label: "Pixel 9" });
    }));

  it("returns waiting before claim", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}&timeout_ms=10`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "waiting" });
    }));

  it("returns expired/consumed states accordingly", () =>
    withApp(
      async ({ app, repo }) => {
        const ctx = await startPair(app, repo);
        const res = await app.inject({
          method: "GET",
          url: `/pair/poll?id=${ctx.pair_id}`,
          headers: { authorization: `Bearer ${ctx.device_token}` },
        });
        expect(res.statusCode).toBe(200);
        expect(res.json()).toEqual({ status: "expired" });
      },
      { pairingTtlMs: 0 }
    ));
});
