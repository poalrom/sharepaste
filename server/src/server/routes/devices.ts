import type { FastifyInstance } from "fastify";
import { verifyBearer } from "../auth.js";
import { DeviceCredentials } from "../device-credentials.js";
import { PairSlot } from "../pair-slot.js";
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
      const now = Date.now();
      const slot = PairSlot.read(app, pair_id, now).requireClaimed();
      slot.proveSecret(secret_proof, now);

      const issued = await DeviceCredentials.issue(
        app.deps.repo,
        { userId: slot.userId, deviceLabel: label, now },
        // Single-use: the Pair Slot is spent on the Device minted from it, in
        // the one transaction, so a slot never yields two credentials.
        () => slot.consumeInto(label, now)
      );

      return reply.send({
        device_token: issued.device_token,
        device_id: issued.device_id,
        user_id: issued.user_id,
      });
    }
  );

  app.get("/me", async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const user = app.deps.repo.users.find(auth.user_id);
    if (!user) throw app.httpErrors.notFound("user not found");

    // ADR 0001's shape, and the module's to build: a route never handles a row
    // that could carry credential material, so there is nothing here to redact.
    // Revoked Devices stay listed so a client can still resolve the Origin of
    // entries captured on one.
    const devices = DeviceCredentials.list(app.deps.repo, auth.user_id);

    return reply.send({ user: { id: user.id, username: user.username }, devices });
  });

  app.delete<{ Params: { id: string } }>(
    "/devices/:id",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      DeviceCredentials.revoke(app.deps.repo, auth.user_id, req.params.id, Date.now());
      return reply.send({ ok: true });
    }
  );
};
