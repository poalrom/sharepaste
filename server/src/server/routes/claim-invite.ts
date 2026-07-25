import type { FastifyInstance } from "fastify";
import { hashToken, randomId, randomToken, sha256Hex } from "../../crypto.js";

const SCHEMA = {
  body: {
    type: "object",
    required: ["token", "device_label"],
    additionalProperties: false,
    properties: {
      token: { type: "string", minLength: 16, maxLength: 256 },
      device_label: { type: "string", minLength: 1, maxLength: 128 },
    },
  },
} as const;

export const registerClaimInviteRoute = (app: FastifyInstance): void => {
  app.post<{ Body: { token: string; device_label: string } }>(
    "/claim-invite",
    { schema: SCHEMA },
    async (req, reply) => {
      const { token, device_label } = req.body;
      const tokenHash = sha256Hex(token);
      const invite = app.deps.repo.invites.findByHash(tokenHash);
      if (!invite) throw app.httpErrors.notFound("invite not found");
      if (invite.claimed_at !== null)
        throw app.httpErrors.conflict("invite already claimed");
      const now = Date.now();
      if (invite.expires_at < now) throw app.httpErrors.gone("invite expired");

      const deviceId = randomId();
      const deviceToken = randomToken();
      const deviceTokenHash = await hashToken(deviceToken);

      const tx = app.deps.repo.db.transaction(() => {
        app.deps.repo.invites.markClaimed(tokenHash, now);
        app.deps.repo.memberships.create({
          user_id: invite.user_id,
          device_id: deviceId,
          device_token_hash: deviceTokenHash,
          token_sha256: sha256Hex(deviceToken),
          device_label,
          created_at: now,
          revoked_at: null,
        });
      });
      tx();

      return reply.send({
        device_token: deviceToken,
        user_id: invite.user_id,
        device_id: deviceId,
      });
    }
  );
};
