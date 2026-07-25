import { setTimeout as sleep } from "node:timers/promises";
import type { FastifyInstance } from "fastify";
import { randomId } from "../../crypto.js";
import { verifyBearer } from "../auth.js";
import { loadUsableSlot, verifySlotProof } from "../pairing-slot.js";
import { HEX_64, SECRET_PROOF, UUID } from "./schemas.js";

export const registerPairingRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { secret_hash: string } }>(
    "/pair/start",
    {
      schema: {
        body: {
          type: "object",
          required: ["secret_hash"],
          additionalProperties: false,
          properties: { secret_hash: HEX_64 },
        },
      },
    },
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const id = randomId();
      const now = Date.now();
      app.deps.repo.pairings.create({
        id,
        user_id: auth.user_id,
        secret_hash: req.body.secret_hash.toLowerCase(),
        encrypted_payload: null,
        claimed_at: null,
        paired_device_label: null,
        failed_attempts: 0,
        consumed_at: null,
        expires_at: now + app.deps.pairingTtlMs,
      });
      return reply.send({ pair_id: id });
    }
  );

  app.post<{ Body: { pair_id: string; secret_proof: string } }>(
    "/pair/claim",
    {
      schema: {
        body: {
          type: "object",
          required: ["pair_id", "secret_proof"],
          additionalProperties: false,
          properties: {
            pair_id: UUID,
            secret_proof: SECRET_PROOF,
          },
        },
      },
    },
    async (req, reply) => {
      const { pair_id, secret_proof } = req.body;
      const now = Date.now();
      const pairing = loadUsableSlot(app, pair_id, now);
      verifySlotProof(app, pairing, secret_proof, now);
      app.deps.repo.pairings.markClaimed(pair_id, now);
      return reply.send({ ok: true });
    }
  );

  // POST /pair/payload (inviter uploads ciphertext)
  app.post<{ Body: { pair_id: string; encrypted_payload: string } }>(
    "/pair/payload",
    {
      schema: {
        body: {
          type: "object",
          required: ["pair_id", "encrypted_payload"],
          additionalProperties: false,
          properties: {
            pair_id: UUID,
            encrypted_payload: { type: "string", minLength: 1, maxLength: 16 * 1024 },
          },
        },
      },
    },
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const pairing = loadUsableSlot(app, req.body.pair_id, Date.now());
      if (pairing.user_id !== auth.user_id)
        throw app.httpErrors.forbidden("not the inviter");

      const buf = Buffer.from(req.body.encrypted_payload, "base64");
      app.deps.repo.pairings.setPayload(pairing.id, buf);
      return reply.send({ ok: true });
    }
  );

  // GET /pair/payload?id&proof (claimer downloads)
  app.get<{ Querystring: { id?: string; proof?: string } }>(
    "/pair/payload",
    async (req, reply) => {
      const id = req.query.id;
      const proof = req.query.proof;
      if (!id || !proof) throw app.httpErrors.badRequest("missing id or proof");
      const now = Date.now();
      const pairing = loadUsableSlot(app, id, now);
      verifySlotProof(app, pairing, proof, now);
      if (!pairing.encrypted_payload)
        throw app.httpErrors.notFound("payload not yet uploaded");
      return reply.send({
        encrypted_payload: pairing.encrypted_payload.toString("base64"),
      });
    }
  );

  // GET /pair/poll?id (long-poll, but bounded for tests by timeout_ms)
  app.get<{ Querystring: { id?: string; timeout_ms?: string } }>(
    "/pair/poll",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const id = req.query.id;
      if (!id) throw app.httpErrors.badRequest("missing id");
      const timeoutMs = Math.min(
        Number(req.query.timeout_ms ?? 25_000) || 25_000,
        30_000
      );
      const deadline = Date.now() + timeoutMs;
      while (true) {
        // Deliberately not `loadUsableSlot`: the poller reports expiry and
        // consumption as 200 statuses instead of throwing 410.
        const pairing = app.deps.repo.pairings.find(id);
        if (!pairing) throw app.httpErrors.notFound("pair not found");
        if (pairing.user_id !== auth.user_id)
          throw app.httpErrors.forbidden("not the inviter");
        const now = Date.now();
        if (pairing.consumed_at !== null) {
          return reply.send({
            status: "consumed",
            device_label: pairing.paired_device_label,
          });
        }
        if (pairing.expires_at <= now) return reply.send({ status: "expired" });
        if (pairing.claimed_at !== null) return reply.send({ status: "claimed" });
        if (Date.now() >= deadline) return reply.send({ status: "waiting" });
        await sleep(Math.min(250, deadline - Date.now()));
      }
    }
  );
};
