import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";
import { randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /pair/start", () => {
  it("opens a slot bound to the caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
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
      expect(body.pair_id).toMatch(/^[0-9a-f-]{36}$/);

      const pairing = repo.pairings.find(body.pair_id);
      expect(pairing?.user_id).toBe(a.user_id);
      expect(pairing?.secret_hash).toBe(sha256Hex(secret));
      expect(pairing?.consumed_at).toBeNull();
      expect(pairing?.expires_at).toBeGreaterThan(Date.now());
    } finally {
      await close();
    }
  });

  it("rejects without a token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });

  it("rejects an invalid token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: "Bearer not-a-real-token" },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });
});

const startPair = async (app: any, repo: any) => {
  const a = await provisionDevice(repo);
  const secret = randomToken();
  const res = await app.inject({
    method: "POST",
    url: "/pair/start",
    headers: { authorization: `Bearer ${a.device_token}` },
    payload: { secret_hash: sha256Hex(secret) },
  });
  return { ...a, secret, pair_id: res.json().pair_id as string };
};

describe("POST /pair/claim", () => {
  it("accepts a correct secret_proof and marks the slot claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(200);
      expect(repo.pairings.find(ctx.pair_id)?.claimed_by).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 403 on a wrong secret_proof and increments failed_attempts", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(403);
      expect(repo.pairings.find(ctx.pair_id)?.failed_attempts).toBe(1);
    } finally {
      await close();
    }
  });

  it("burns the slot after 3 wrong attempts", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
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
      expect(repo.pairings.find(ctx.pair_id)?.consumed_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 410 on an expired slot", async () => {
    const { app, repo, close } = await buildTestApp({ pairingTtlMs: 0 });
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(410);
    } finally {
      await close();
    }
  });

  it("returns 404 for an unknown pair_id", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: "00000000-0000-0000-0000-000000000000", secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(404);
    } finally {
      await close();
    }
  });
});

describe("/pair/payload (upload + download)", () => {
  it("inviter uploads ciphertext, claimer downloads with correct proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
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
    } finally {
      await close();
    }
  });

  it("rejects payload upload from a non-inviter token", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const other = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${other.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: "AAAA" },
      });
      expect(res.statusCode).toBe(403);
    } finally {
      await close();
    }
  });

  it("rejects download with wrong proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
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
    } finally {
      await close();
    }
  });
});

describe("GET /pair/poll", () => {
  it("returns claimed status once the slot is claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "claimed" });
    } finally {
      await close();
    }
  });

  it("returns waiting before claim", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}&timeout_ms=10`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "waiting" });
    } finally {
      await close();
    }
  });

  it("returns expired/consumed states accordingly", async () => {
    const { app, repo, close } = await buildTestApp({ pairingTtlMs: 0 });
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "expired" });
    } finally {
      await close();
    }
  });
});
