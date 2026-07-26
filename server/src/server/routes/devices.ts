import type { FastifyInstance } from "fastify";
import { hashToken, randomId, randomToken, sha256Hex } from "../../crypto.js";
import { verifyBearer } from "../auth.js";
import { verifySlotProof } from "../pairing-slot.js";
import { DEVICE_LABEL, SECRET_PROOF, UUID } from "./schemas.js";

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
            pair_id: UUID,
            secret_proof: SECRET_PROOF,
            label: DEVICE_LABEL,
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
      if (pairing.claimed_at === null) throw app.httpErrors.conflict("pair slot not claimed");
      verifySlotProof(app, pairing, secret_proof, now);

      const deviceId = randomId();
      const token = randomToken();
      const tokenHash = await hashToken(token);

      const tx = app.deps.repo.db.transaction(() => {
        app.deps.repo.memberships.create({
          user_id: pairing.user_id,
          device_id: deviceId,
          device_token_hash: tokenHash,
          token_sha256: sha256Hex(token),
          device_label: label,
          created_at: now,
          revoked_at: null,
        });
        app.deps.repo.pairings.markConsumed(pair_id, now, label);
      });
      tx();

      return reply.send({ device_token: token, device_id: deviceId, user_id: pairing.user_id });
    }
  );

  app.get("/me", async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const user = app.deps.repo.users.find(auth.user_id);
    if (!user) throw app.httpErrors.notFound("user not found");

    // Field-by-field, never a spread: MembershipRow carries `device_token_hash`
    // and `token_sha256`, and neither may ever reach a response body.
    // Revoked devices stay in the list so clients can still resolve the Origin
    // of entries captured on a device that has since been revoked.
    const devices = app.deps.repo.memberships.listByUser(auth.user_id).map((m) => ({
      device_id: m.device_id,
      label: m.device_label ?? null,
      created_at: m.created_at,
      revoked_at: m.revoked_at,
    }));

    return reply.send({ user: { id: user.id, username: user.username }, devices });
  });

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
