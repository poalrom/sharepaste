import { setTimeout as sleep } from "node:timers/promises";
import type { FastifyInstance } from "fastify";
import { verifyBearer } from "../auth.js";
import { PairSlot } from "../pair-slot.js";
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
      const pairId = PairSlot.open(app, auth.user_id, req.body.secret_hash, Date.now());
      return reply.send({ pair_id: pairId });
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
      const now = Date.now();
      PairSlot.read(app, req.body.pair_id, now)
        .requireUsable()
        .claim(req.body.secret_proof, now);
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
      PairSlot.read(app, req.body.pair_id, Date.now())
        .requireUsable()
        .requireInviter(auth.user_id)
        .attachPayload(req.body.encrypted_payload);
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
      const encrypted_payload = PairSlot.read(app, id, now)
        .requireUsable()
        .takePayload(proof, now);
      return reply.send({ encrypted_payload });
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
        // `pollBody`, not `requireUsable`: the poller renders the same
        // classification as 200 statuses where the proof routes throw 410.
        const body = PairSlot.read(app, id, Date.now())
          .requireInviter(auth.user_id)
          .pollBody();
        if (body.status !== "waiting" || Date.now() >= deadline)
          return reply.send(body);
        await sleep(Math.min(250, deadline - Date.now()));
      }
    }
  );
};
