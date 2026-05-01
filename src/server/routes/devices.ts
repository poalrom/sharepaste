import type { FastifyInstance } from "fastify";
import {
  hashToken,
  randomId,
  randomToken,
  sha256Hex,
  timingSafeEqualHex,
} from "../../crypto.js";
import { verifyBearer } from "../auth.js";

const UUID_PATTERN = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";

export const registerDeviceRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { pair_id: string; secret_proof: string; label: string } }>(
    "/devices",
    {
      schema: {
        body: {
          type: "object",
          required: ["pair_id", "secret_proof", "label"],
          additionalProperties: false,
          properties: {
            pair_id: { type: "string", pattern: UUID_PATTERN },
            secret_proof: { type: "string", minLength: 16, maxLength: 256 },
            label: { type: "string", minLength: 1, maxLength: 128 },
          },
        },
      },
    },
    async (req, reply) => {
      const { pair_id, secret_proof, label } = req.body;
      const pairing = app.deps.repo.pairings.find(pair_id);
      if (!pairing) throw app.httpErrors.notFound("pair not found");
      const now = Date.now();
      if (pairing.consumed_at !== null) throw app.httpErrors.gone("pair slot consumed");
      if (pairing.expires_at <= now) throw app.httpErrors.gone("pair slot expired");
      if (pairing.claimed_by === null) throw app.httpErrors.conflict("pair slot not claimed");
      if (!timingSafeEqualHex(sha256Hex(secret_proof), pairing.secret_hash))
        throw app.httpErrors.forbidden("wrong secret");

      const deviceId = randomId();
      const token = randomToken();
      const tokenHash = await hashToken(token);

      const tx = app.deps.repo.db.transaction(() => {
        app.deps.repo.memberships.create({
          user_id: pairing.user_id,
          device_id: deviceId,
          device_token_hash: tokenHash,
          device_label: label,
          created_at: now,
          revoked_at: null,
        });
        app.deps.repo.pairings.markConsumed(pair_id, now);
      });
      tx();

      return reply.send({ device_token: token, device_id: deviceId, user_id: pairing.user_id });
    }
  );

  app.delete<{ Params: { id: string } }>(
    "/devices/:id",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const target = app.deps.repo.memberships.findByDeviceId(auth.user_id, req.params.id);
      if (!target) throw app.httpErrors.notFound("device not found for this user");
      app.deps.repo.memberships.revoke(auth.user_id, req.params.id, Date.now());
      return reply.send({ ok: true });
    }
  );
};
