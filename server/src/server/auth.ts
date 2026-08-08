import type { FastifyInstance, FastifyRequest } from "fastify";
import { DeviceCredentials, type AuthedMembership } from "./device-credentials.js";

/**
 * The bearer check, as the four route modules that never mint a Device already
 * spell it. `DeviceCredentials` owns the rule now — the module that issues a
 * credential is the module that recognises one — and this stays a two-line
 * adapter from Fastify's request to it, so those routes are not rewritten for a
 * refactor that changes nothing they can see. Same precedent as `schemas.ts`.
 */
export const verifyBearer = (
  app: FastifyInstance,
  req: FastifyRequest
): Promise<AuthedMembership> =>
  DeviceCredentials.authenticate(app.deps.repo, req.headers.authorization);
