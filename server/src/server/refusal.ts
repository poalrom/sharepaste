import { httpErrors } from "@fastify/sensible";

/**
 * The statuses a refusal may be answered with.
 *
 * A **Refused** pending leaves the device's queue and stays on the device; being out of
 * reach is never a refusal, because surviving a relay that is not there is what the queue
 * is for (ADR 0015). The client draws that line from the status alone:
 * `clients/core/src/http/client.rs:78-87` maps 413 and 400..=499 to `AppError::BadInput`,
 * `sync/uploader.rs:259-264` turns `BadInput` into Refused, and everything else stays
 * transient so the queue holds order. The status this module picks *is* that
 * classification, which is why the union is closed: a verdict here is a fact about the
 * act — too large, malformed — and will be answered identically forever. A 5xx is a fact
 * about the moment and is never a verdict.
 */
export type RefusalStatus = 400 | 413;

/**
 * Renders a verdict as the status the queue contract expects.
 *
 * The one line earns being a function: it is the only place in the Relay that chooses a
 * refusal's status, so `RefusalStatus` closes the set for all four verdicts at once. Every
 * `httpErrors.*` call written by hand is a status nothing constrains.
 *
 * `reason` is not decoration either: the client carries the response body into
 * `last_error`, so it is the sentence a person reads off a refused row.
 */
const refusal = (status: RefusalStatus, reason: string) => httpErrors.getHttpError(status, reason);

/**
 * The wire shapes a malformed request is refused against, all of them a 400 by way of
 * Fastify's schema validation.
 *
 * `SECRET_PROOF` bounds both a **Pair Slot**'s secret proof and an **Invite**'s token:
 * both are opaque high-entropy strings a device echoes back, so both are refusable on the
 * same grounds. They are re-exported from `routes/schemas.ts`, the path every route
 * already imports them from.
 */
export const UUID = {
  type: "string",
  pattern: "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
} as const;

export const HEX_64 = { type: "string", pattern: "^[0-9a-fA-F]{64}$" } as const;

export const SECRET_PROOF = { type: "string", minLength: 16, maxLength: 256 } as const;

export const DEVICE_LABEL = { type: "string", minLength: 1, maxLength: 128 } as const;

/**
 * The wire shape of one **Entry**, as Fastify validates it before the handler runs.
 *
 * Two rules that could live here deliberately do not. `maxEntryBytes` as a `maxLength`
 * would answer 400 where the Relay answers 413, and the base64 grouping rule folded into
 * the pattern would replace "malformed base64" with ajv's wording on the row a person
 * reads. Both are why `sizeOf` exists rather than being an arithmetic accident.
 */
const BODY_SCHEMA = {
  type: "object",
  required: ["ciphertext"],
  additionalProperties: false,
  properties: {
    ciphertext: { type: "string", minLength: 1, pattern: "^[A-Za-z0-9+/]+={0,2}$" },
  },
} as const;

/** Everything that makes one Entry's act refusable, with the caps it is measured against. */
export interface EntryRules {
  /** The largest Entry this Relay stores. Named in the 413's reason, so keep the name. */
  readonly maxEntryBytes: number;
  /**
   * The largest request body Fastify will read, answering its own 413 above every route.
   * It must stay clear of `maxEntryBytes` — base64 costs four chars per three bytes and
   * the body adds a JSON envelope — or the framework would refuse an Entry the cap allows,
   * and no reason would reach the row.
   */
  readonly bodyLimit: number;
  readonly bodySchema: typeof BODY_SCHEMA;
  /**
   * The byte size of a ciphertext this Relay will store.
   *
   * @throws a 400 for a length that is not whole base64 groups or decodes to nothing, and
   * a 413 past `maxEntryBytes`. Runs after the bearer check, so an unauthenticated device
   * still hears 401 first: whose problem it is comes before what the problem is.
   */
  sizeOf(ciphertext: string): number;
  /**
   * The Entry id a path segment names.
   *
   * @throws a 400 when the segment is not a positive integer. An integer larger than any
   * id the Relay has issued is not a shape problem — it misses in the lookup and earns the
   * 404 that says so.
   */
  idFrom(raw: string): number;
}

export const entryRules = ({ maxEntryBytes }: { maxEntryBytes: number }): EntryRules => ({
  maxEntryBytes,
  bodyLimit: 1024 * 1024,
  bodySchema: BODY_SCHEMA,

  sizeOf(ciphertext) {
    if (ciphertext.length % 4 !== 0) throw refusal(400, "malformed base64");
    const padding = ciphertext.endsWith("==") ? 2 : ciphertext.endsWith("=") ? 1 : 0;
    const size = (ciphertext.length / 4) * 3 - padding;
    if (size === 0) throw refusal(400, "empty ciphertext");
    if (size > maxEntryBytes) throw refusal(413, "ciphertext exceeds maxEntryBytes");
    return size;
  },

  idFrom(raw) {
    const id = Number(raw);
    if (!Number.isInteger(id) || id <= 0) throw refusal(400, "bad id");
    return id;
  },
});
