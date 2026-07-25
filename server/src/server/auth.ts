import type { FastifyInstance, FastifyRequest } from "fastify";
import { sha256Hex, verifyToken } from "../crypto.js";

export interface AuthedMembership {
  user_id: string;
  device_id: string;
}

export const verifyBearer = async (
  app: FastifyInstance,
  req: FastifyRequest
): Promise<AuthedMembership> => {
  const header = req.headers.authorization;
  if (!header?.startsWith("Bearer "))
    throw app.httpErrors.unauthorized("missing bearer token");
  const token = header.slice("Bearer ".length).trim();
  if (!token) throw app.httpErrors.unauthorized("missing bearer token");

  const hash = sha256Hex(token);
  const indexed = app.deps.repo.memberships.findActiveByTokenSha256(hash);
  if (indexed) return { user_id: indexed.user_id, device_id: indexed.device_id };

  // Backfill path for memberships issued before the sha256 index existed: each
  // costs one argon2 scan, after which the row is indexed and never scanned
  // again. Delete this loop once no active membership has a null token_sha256.
  for (const m of app.deps.repo.memberships.listUnindexed()) {
    if (await verifyToken(m.device_token_hash, token)) {
      app.deps.repo.memberships.setTokenSha256(m.user_id, m.device_id, hash);
      return { user_id: m.user_id, device_id: m.device_id };
    }
  }
  throw app.httpErrors.unauthorized("invalid token");
};
