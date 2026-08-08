import { describe, it, expect } from "vitest";
import { cipherB64 } from "../helpers.js";
import { entryRules, type RefusalStatus } from "../../src/server/refusal.js";

/**
 * The Refusal module's verdict on one input: either the fact the route asked for, or
 * the status a refusal is rendered as. `RefusalStatus` is what makes this table a
 * contract rather than a snapshot — a row cannot expect a 5xx, because a 5xx is a fact
 * about the moment and the queue must hold order through it (ADR 0015).
 */
type Expected = { accepted: number } | { status: RefusalStatus; reason: string };

/** The same shape, but read off a real call, so an unrendered throw shows up as 0. */
type Verdict = { accepted: number } | { status: number; reason: string };

const verdictOf = (act: () => number): Verdict => {
  try {
    return { accepted: act() };
  } catch (err) {
    const e = err as { statusCode?: number; message?: string };
    return { status: e.statusCode ?? 0, reason: e.message ?? String(err) };
  }
};

const refusalsIn = (verdicts: Verdict[]): { status: number; reason: string }[] =>
  verdicts.filter((v): v is { status: number; reason: string } => "status" in v);

const CAP = 16;

/**
 * The base64 pattern is not in this table: it is the one shape rule Fastify enforces
 * against `bodySchema`, and `tests/integration/entries.test.ts` covers it end to end.
 * `sizeOf` weighs a string the pattern has already accepted — which is why "!!!!"
 * weighs 3 bytes here rather than earning a verdict.
 */
const SIZE_ROWS: { what: string; ciphertext: string; maxEntryBytes: number; verdict: Expected }[] = [
  { what: "four base64 chars", ciphertext: "AAAA", maxEntryBytes: CAP, verdict: { accepted: 3 } },
  { what: "one pad char", ciphertext: "AAA=", maxEntryBytes: CAP, verdict: { accepted: 2 } },
  { what: "two pad chars", ciphertext: "AB==", maxEntryBytes: CAP, verdict: { accepted: 1 } },
  {
    what: "a ciphertext exactly at the cap",
    ciphertext: cipherB64("x".repeat(CAP)),
    maxEntryBytes: CAP,
    verdict: { accepted: CAP },
  },
  {
    what: "one byte over the cap",
    ciphertext: cipherB64("x".repeat(CAP + 1)),
    maxEntryBytes: CAP,
    verdict: { status: 413, reason: "ciphertext exceeds maxEntryBytes" },
  },
  {
    what: "a ciphertext over the shipped 64 KiB cap",
    ciphertext: cipherB64("x".repeat(64 * 1024 + 1)),
    maxEntryBytes: 64 * 1024,
    verdict: { status: 413, reason: "ciphertext exceeds maxEntryBytes" },
  },
  {
    what: "a length that is not a whole number of base64 groups",
    ciphertext: "AAA",
    maxEntryBytes: CAP,
    verdict: { status: 400, reason: "malformed base64" },
  },
  {
    what: "a trailing group of one char",
    ciphertext: "AAAA=",
    maxEntryBytes: CAP,
    verdict: { status: 400, reason: "malformed base64" },
  },
  // Only reachable here: `bodySchema`'s minLength refuses an empty string at the wire,
  // so this verdict guards the module's own contract for any other caller.
  {
    what: "nothing at all",
    ciphertext: "",
    maxEntryBytes: CAP,
    verdict: { status: 400, reason: "empty ciphertext" },
  },
];

/**
 * `idFrom` is `Number()`, deliberately: it accepts " 1", "1e2" and "0x10" today, and
 * decision 3 keeps every status the Relay answers. An id that is an integer but larger
 * than any the Relay has issued is accepted here and misses in the lookup — a fact about
 * the Entry (404), not about the shape of the request.
 */
const ID_ROWS: { what: string; raw: string; verdict: Expected }[] = [
  { what: "a plain id", raw: "1", verdict: { accepted: 1 } },
  { what: "leading whitespace", raw: " 1", verdict: { accepted: 1 } },
  { what: "exponent notation", raw: "1e2", verdict: { accepted: 100 } },
  { what: "hex notation", raw: "0x10", verdict: { accepted: 16 } },
  { what: "an id past every issued one", raw: "1e20", verdict: { accepted: 1e20 } },
  { what: "letters", raw: "abc", verdict: { status: 400, reason: "bad id" } },
  { what: "an empty segment", raw: "", verdict: { status: 400, reason: "bad id" } },
  { what: "zero", raw: "0", verdict: { status: 400, reason: "bad id" } },
  { what: "a negative id", raw: "-1", verdict: { status: 400, reason: "bad id" } },
  { what: "a fraction", raw: "1.5", verdict: { status: 400, reason: "bad id" } },
  { what: "an overflowing exponent", raw: "1e400", verdict: { status: 400, reason: "bad id" } },
  { what: "not a number", raw: "NaN", verdict: { status: 400, reason: "bad id" } },
];

describe("entryRules().sizeOf", () => {
  for (const row of SIZE_ROWS) {
    it(`weighs ${row.what}`, () => {
      const rules = entryRules({ maxEntryBytes: row.maxEntryBytes });
      expect(verdictOf(() => rules.sizeOf(row.ciphertext))).toEqual(row.verdict);
    });
  }
});

describe("entryRules().idFrom", () => {
  for (const row of ID_ROWS) {
    it(`reads ${row.what}`, () => {
      const rules = entryRules({ maxEntryBytes: 64 * 1024 });
      expect(verdictOf(() => rules.idFrom(row.raw))).toEqual(row.verdict);
    });
  }
});

describe("the refusable statuses", () => {
  it("renders every verdict as one the client reads as Refused, never as 5xx", () => {
    const rules = entryRules({ maxEntryBytes: CAP });
    const verdicts = refusalsIn([
      ...SIZE_ROWS.map((r) => verdictOf(() => entryRules({ maxEntryBytes: r.maxEntryBytes }).sizeOf(r.ciphertext))),
      ...ID_ROWS.map((r) => verdictOf(() => rules.idFrom(r.raw))),
    ]);

    expect(verdicts.length).toBeGreaterThan(0);
    for (const v of verdicts) {
      expect(v.status).toBeGreaterThanOrEqual(400);
      expect(v.status).toBeLessThan(500);
    }
  });

  it("has no status a client would hold in the queue instead of refusing", () => {
    // @ts-expect-error 500 is not a RefusalStatus. This line fails `npm run typecheck` if
    // the union is ever widened, which is what keeps the property above true by
    // construction rather than by luck: every verdict goes through one renderer, and that
    // renderer takes nothing a client would read as transient.
    const transient: RefusalStatus = 500;
    expect(transient).toBe(500);
  });

  it("keeps the body limit above the cap, so the framework refuses only what the cap would", () => {
    const rules = entryRules({ maxEntryBytes: 64 * 1024 });
    // base64 costs four chars per three bytes, and the body carries a JSON envelope
    // around it. A limit under that would answer 413 for an Entry the cap allows.
    expect(rules.bodyLimit).toBeGreaterThan(Math.ceil((rules.maxEntryBytes / 3) * 4));
  });
});
