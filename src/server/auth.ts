import type { FastifyInstance, FastifyRequest } from "fastify";
import type { MembershipRow } from "../db/repository.js";
import { verifyToken } from "../crypto.js";

export interface AuthedMembership {
  user_id: string;
  device_id: string;
}

const extractToken = (req: FastifyRequest): string | null => {
  const h = req.headers.authorization;
  if (h && h.startsWith("Bearer ")) return h.slice("Bearer ".length).trim();
  const q = (req.query as Record<string, unknown> | undefined)?.token;
  return typeof q === "string" && q.length > 0 ? q : null;
};

export const verifyBearer = async (
  app: FastifyInstance,
  req: FastifyRequest
): Promise<AuthedMembership> => {
  const token = extractToken(req);
  if (!token) throw app.httpErrors.unauthorized("missing bearer token");

  const candidates: MembershipRow[] = app.deps.repo.memberships.listActive();
  for (const m of candidates) {
    if (await verifyToken(m.device_token_hash, token)) {
      return { user_id: m.user_id, device_id: m.device_id };
    }
  }
  throw app.httpErrors.unauthorized("invalid token");
};
