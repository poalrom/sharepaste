import type { FastifyInstance } from "fastify";
import { sha256Hex } from "../../crypto.js";
import { DeviceCredentials } from "../device-credentials.js";
import { DEVICE_LABEL, SECRET_PROOF } from "./schemas.js";

const SCHEMA = {
  body: {
    type: "object",
    required: ["token", "device_label"],
    additionalProperties: false,
    properties: { token: SECRET_PROOF, device_label: DEVICE_LABEL },
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

      const issued = await DeviceCredentials.issue(
        app.deps.repo,
        { userId: invite.user_id, deviceLabel: device_label, now },
        // Single-use: the Invite is spent in the same transaction the Device is
        // minted in, so no second claim can reach a second credential.
        () => app.deps.repo.invites.markClaimed(tokenHash, now)
      );

      return reply.send({
        device_token: issued.device_token,
        user_id: issued.user_id,
        device_id: issued.device_id,
      });
    }
  );
};
