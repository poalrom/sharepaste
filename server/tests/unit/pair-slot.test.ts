import { describe, it, expect } from "vitest";
import { PairSlot } from "../../src/server/pair-slot.js";
import { startAndClaim, withApp } from "../helpers.js";

/**
 * The cap is a property of the module, not of any one endpoint: it is only a cap
 * if *no* door reaches a usable Pair Slot past it. `pairing.test.ts` proves the
 * burn through `/pair/claim` and `devices.test.ts` proves the 410 through
 * `/devices`; these prove there is no third door to find.
 *
 * The fixture drives `failed_attempts` directly because a proof that reaches the
 * cap also consumes the slot — the only way to hold "burned" and "consumed" apart.
 */
describe("PairSlot and the failure cap", () => {
  it("classifies a slot at the cap as burned, not as the claimed slot it still looks like", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      for (let i = 0; i < 3; i++) repo.pairSlots.incrementFailed(ctx.pair_id);

      expect(PairSlot.read(app, ctx.pair_id, Date.now()).state).toEqual({
        kind: "burned",
        claimed: true,
      });
    }));

  it("hands out no usable slot at the cap, by either rendering", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      for (let i = 0; i < 3; i++) repo.pairSlots.incrementFailed(ctx.pair_id);
      const slot = PairSlot.read(app, ctx.pair_id, Date.now());

      expect(() => slot.requireUsable()).toThrow(
        expect.objectContaining({ statusCode: 410 })
      );
      expect(() => slot.requireClaimed()).toThrow(
        expect.objectContaining({ statusCode: 410 })
      );
    }));

  it("still hands one out one proof below the cap", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      for (let i = 0; i < 2; i++) repo.pairSlots.incrementFailed(ctx.pair_id);
      const slot = PairSlot.read(app, ctx.pair_id, Date.now());

      expect(slot.state.kind).toBe("claimed");
      expect(() => slot.requireUsable()).not.toThrow();
      expect(() => slot.requireClaimed()).not.toThrow();
    }));

  it("does not let a caller spell a transition without the classification", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const slot = PairSlot.read(app, ctx.pair_id, Date.now());

      // The compile-time half of the cap, enforced by `npm run typecheck` and by
      // nothing at runtime: every transition demands a slot `requireUsable` or
      // `requireClaimed` has already ruled the cap out of, and that type cannot
      // be spelled outside the module. The day this stops erroring, the cap has
      // a second door.
      // @ts-expect-error
      void (() => slot.claim(ctx.secret, Date.now()));

      expect(slot.state.kind).toBe("claimed");
    }));
});
