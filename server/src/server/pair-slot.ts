import type { FastifyInstance } from "fastify";
import { randomId, sha256Hex, timingSafeEqualHex } from "../crypto.js";
import type { PairSlotRow } from "../db/repository.js";

/**
 * What a Pair Slot is at one instant, derived from the row exactly once.
 *
 * Both renderings below — the HTTP refusal a route throws and the body
 * `/pair/poll` sends — are functions of this and of nothing else. A third reader
 * belongs here as a third rendering, never as a fourth derivation from the row.
 */
export type PairSlotState =
  /** A Device was minted from it, or a burn spent it. Terminal. */
  | { readonly kind: "consumed"; readonly pairedDeviceLabel: string | null }
  /** Its deadline passed before anyone finished with it. Terminal. */
  | { readonly kind: "expired" }
  /**
   * Out of proofs. Carries whether it had been claimed, because the poller does
   * not know about the cap and must keep answering what it answered before.
   */
  | { readonly kind: "burned"; readonly claimed: boolean }
  /** Exactly one device has proved the secret; the payload exchange may run. */
  | { readonly kind: "claimed" }
  /** Live, and nobody has proved the secret yet. */
  | { readonly kind: "waiting" };

/** The `/pair/poll` rendering: every state of a slot as a 200 body. */
export type PollBody =
  | { status: "consumed"; device_label: string | null }
  | { status: "expired" }
  | { status: "claimed" }
  | { status: "waiting" };

/**
 * The mark of a slot that has passed the classification. Neither it nor
 * `UsablePairSlot` leaves this module, so no caller can name the type, let alone
 * cast its way to one.
 *
 * The cap `proveSecret` enforces is only a cap if every proof-taking route
 * refuses a burned slot *first*, which used to be a rule each route had to
 * remember and one of them forgot. It is now a type: the transitions demand a
 * slot that `requireUsable` or `requireClaimed` has already classified, so there
 * is no route left that could forget.
 */
declare const usable: unique symbol;

/** A Pair Slot the classification has ruled consumed, expired and burned out of. */
type UsablePairSlot = PairSlot & { readonly [usable]: true };

const classify = (
  row: PairSlotRow,
  now: number,
  maxFailures: number
): PairSlotState => {
  if (row.consumed_at !== null)
    return { kind: "consumed", pairedDeviceLabel: row.paired_device_label };
  if (row.expires_at <= now) return { kind: "expired" };
  // Ahead of the claim test on purpose: a slot out of proofs is unusable however
  // far along the handshake it got.
  if (row.failed_attempts >= maxFailures)
    return { kind: "burned", claimed: row.claimed_at !== null };
  return row.claimed_at !== null ? { kind: "claimed" } : { kind: "waiting" };
};

/**
 * The relay's record of a pairing handshake in progress, read once and
 * classified. Nothing outside this module handles the row: a caller gets the
 * state, one of its two renderings, and the four transitions.
 */
export class PairSlot {
  /** Phantom: never assigned, and only `requireUsable`/`requireClaimed` claim it. */
  declare readonly [usable]?: true;

  /** The classification every reader renders. */
  readonly state: PairSlotState;

  private constructor(
    private readonly app: FastifyInstance,
    private readonly row: PairSlotRow,
    now: number
  ) {
    this.state = classify(row, now, app.deps.maxPairingFailures);
  }

  /** Opens a slot for `userId`, returning the pair id the inviter shows. */
  static open(
    app: FastifyInstance,
    userId: string,
    secretHash: string,
    now: number
  ): string {
    const id = randomId();
    app.deps.repo.pairSlots.create({
      id,
      user_id: userId,
      // Canonical lower-case hex: `timingSafeEqualHex` compares against this.
      secret_hash: secretHash.toLowerCase(),
      encrypted_payload: null,
      claimed_at: null,
      paired_device_label: null,
      failed_attempts: 0,
      consumed_at: null,
      expires_at: now + app.deps.pairingTtlMs,
    });
    return id;
  }

  /** Reads the slot named by `pairId` as it stands at `now`, or throws 404. */
  static read(app: FastifyInstance, pairId: string, now: number): PairSlot {
    const row = app.deps.repo.pairSlots.find(pairId);
    if (!row) throw app.httpErrors.notFound("pair not found");
    return new PairSlot(app, row, now);
  }

  /** The user who opened the slot. */
  get userId(): string {
    return this.row.user_id;
  }

  /** Throws 403 unless `userId` opened this slot. Chainable. */
  requireInviter(userId: string): this {
    if (this.row.user_id !== userId)
      throw this.app.httpErrors.forbidden("not the inviter");
    return this;
  }

  /**
   * The refusal the three `/pair/*` proof routes share: one 410 that names no
   * state, because a claimer holds no token and these routes have never told it
   * which of the three ended the slot.
   */
  requireUsable(): UsablePairSlot {
    switch (this.state.kind) {
      case "consumed":
      case "expired":
      case "burned":
        throw this.app.httpErrors.gone("pair slot unavailable");
      case "claimed":
      case "waiting":
        return this as UsablePairSlot;
    }
  }

  /**
   * `/devices`' refusal: the same classification, rendered state by state,
   * plus the 409 for a slot nobody has claimed. A Device may only be minted from
   * a slot a device has already proved the secret to.
   */
  requireClaimed(): UsablePairSlot {
    switch (this.state.kind) {
      case "consumed":
        throw this.app.httpErrors.gone("pair slot consumed");
      case "expired":
        throw this.app.httpErrors.gone("pair slot expired");
      case "burned":
        throw this.app.httpErrors.gone("pair slot burned");
      case "waiting":
        throw this.app.httpErrors.conflict("pair slot not claimed");
      case "claimed":
        return this as UsablePairSlot;
    }
  }

  /** The poller's rendering. `waiting` is the one the caller may sit on. */
  pollBody(): PollBody {
    switch (this.state.kind) {
      case "consumed":
        return { status: "consumed", device_label: this.state.pairedDeviceLabel };
      case "expired":
        return { status: "expired" };
      case "burned":
        // The inviter is never told about the cap: a burn consumes the slot on
        // the proof that reaches it, so the poller keeps reporting the handshake
        // it was already reporting.
        return this.state.claimed ? { status: "claimed" } : { status: "waiting" };
      case "claimed":
        return { status: "claimed" };
      case "waiting":
        return { status: "waiting" };
    }
  }

  /**
   * Verifies a secret proof, counting the failure and burning the slot at the
   * cap. Every transition that accepts a proof goes through here, otherwise
   * `maxPairingFailures` is bypassable by brute-forcing the ones that do not count.
   */
  proveSecret(this: UsablePairSlot, secretProof: string, now: number): void {
    if (timingSafeEqualHex(sha256Hex(secretProof), this.row.secret_hash)) return;
    const failed = this.app.deps.repo.pairSlots.incrementFailed(this.row.id);
    if (failed >= this.app.deps.maxPairingFailures) {
      this.app.deps.repo.pairSlots.markConsumed(this.row.id, now);
    }
    throw this.app.httpErrors.forbidden("wrong secret");
  }

  /** Claims the slot for the device that just proved the secret. */
  claim(this: UsablePairSlot, secretProof: string, now: number): void {
    this.proveSecret(secretProof, now);
    this.app.deps.repo.pairSlots.markClaimed(this.row.id, now);
  }

  /** Stores the inviter's ciphertext for the claimer to collect. */
  attachPayload(this: UsablePairSlot, encryptedPayloadB64: string): void {
    this.app.deps.repo.pairSlots.setPayload(
      this.row.id,
      Buffer.from(encryptedPayloadB64, "base64")
    );
  }

  /** The ciphertext, to a caller that proves the secret. 404 until it is there. */
  takePayload(this: UsablePairSlot, secretProof: string, now: number): string {
    this.proveSecret(secretProof, now);
    if (!this.row.encrypted_payload)
      throw this.app.httpErrors.notFound("payload not yet uploaded");
    return this.row.encrypted_payload.toString("base64");
  }

  /**
   * Spends the slot on the Device just minted from it, naming that Device so the
   * inviter's poll can report which machine paired. Single-use ends here.
   */
  consumeInto(this: UsablePairSlot, deviceLabel: string, now: number): void {
    this.app.deps.repo.pairSlots.markConsumed(this.row.id, now, deviceLabel);
  }
}
