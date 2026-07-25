import type { FastifyInstance } from "fastify";
import { sha256Hex, timingSafeEqualHex } from "../crypto.js";
import type { PairingRow } from "../db/repository.js";

/** Loads a pairing slot that is still usable, or throws the right HTTP error. */
export const loadUsableSlot = (
  app: FastifyInstance,
  pairId: string,
  now: number
): PairingRow => {
  const pairing = app.deps.repo.pairings.find(pairId);
  if (!pairing) throw app.httpErrors.notFound("pair not found");
  if (
    pairing.consumed_at !== null ||
    pairing.expires_at <= now ||
    pairing.failed_attempts >= app.deps.maxPairingFailures
  ) {
    throw app.httpErrors.gone("pair slot unavailable");
  }
  return pairing;
};

/**
 * Verifies a secret proof against a slot, counting failures and burning the slot
 * at the cap. Every endpoint that accepts a proof must go through here, otherwise
 * `maxPairingFailures` is bypassable by brute-forcing the endpoints that do not count.
 */
export const verifySlotProof = (
  app: FastifyInstance,
  pairing: PairingRow,
  secretProof: string,
  now: number
): void => {
  if (timingSafeEqualHex(sha256Hex(secretProof), pairing.secret_hash)) return;
  const failed = app.deps.repo.pairings.incrementFailed(pairing.id);
  if (failed >= app.deps.maxPairingFailures) {
    app.deps.repo.pairings.markConsumed(pairing.id, now);
  }
  throw app.httpErrors.forbidden("wrong secret");
};
