import { describe, it, expect, afterEach } from "vitest";
import { openTempDb, cipherB64, type TempDb } from "../helpers.js";
import { Repository } from "../../src/db/repository.js";

const MAX_COUNT = 100;
const MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

let t: TempDb;
let repo: Repository;

afterEach(() => {
  t.close();
});

describe("entries.insertAndPrune", () => {
  it("keeps only the most recent MAX_COUNT entries for that user", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });

    const now = 1_700_000_000_000;
    for (let i = 0; i < MAX_COUNT + 10; i++) {
      repo.entries.insertAndPrune(
        {
          user_id: "u1",
          device_id: "d1",
          ciphertext_b64: cipherB64(String(i)),
          size: 1,
          created_at: now + i,
        },
        MAX_COUNT,
        MAX_AGE_MS
      );
    }
    expect(repo.entries.countForUser("u1")).toBe(MAX_COUNT);
  });

  it("drops entries older than MAX_AGE_MS even if under count cap", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });

    const now = 1_700_000_000_000;
    repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext_b64: cipherB64("old"),
        size: 3,
        created_at: now - MAX_AGE_MS - 1000,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext_b64: cipherB64("new"),
        size: 3,
        created_at: now,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(repo.entries.countForUser("u1")).toBe(1);
  });

  it("does not affect other users", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });
    repo.users.create({ id: "u2", username: "bob" });
    const now = 1_700_000_000_000;
    for (let i = 0; i < MAX_COUNT + 5; i++) {
      repo.entries.insertAndPrune(
        { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64(String(i)), size: 1, created_at: now + i },
        MAX_COUNT,
        MAX_AGE_MS
      );
    }
    repo.entries.insertAndPrune(
      { user_id: "u2", device_id: "d2", ciphertext_b64: cipherB64("x"), size: 1, created_at: now },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(repo.entries.countForUser("u1")).toBe(MAX_COUNT);
    expect(repo.entries.countForUser("u2")).toBe(1);
  });

  it("the count cap keeps a used old entry and evicts a never-used newer one", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });

    const now = 1_700_000_000_000;
    const cap = 3;
    const oldest = repo.entries.insertAndPrune(
      { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64("old"), size: 3, created_at: now },
      cap,
      MAX_AGE_MS
    );
    for (let i = 1; i <= cap - 1; i++) {
      repo.entries.insertAndPrune(
        {
          user_id: "u1",
          device_id: "d1",
          ciphertext_b64: cipherB64("filler" + i),
          size: 6,
          created_at: now + i,
        },
        cap,
        MAX_AGE_MS
      );
    }
    // The oldest capture is the entry at the head of the list, so the cap must
    // evict the untouched one below it rather than the low id.
    repo.entries.recordUse("u1", oldest.id, now + 100);
    const doomed = repo.entries.insertAndPrune(
      { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64("x"), size: 1, created_at: now + 1 },
      cap,
      MAX_AGE_MS
    );
    const survivors = repo.entries.listSince("u1", 0, 100).map((e) => e.id);

    expect(survivors).toHaveLength(cap);
    expect(survivors).toContain(oldest.id);
    expect(survivors).toContain(doomed.id);
  });

  it("the age cap measures from last_use, so an old capture used recently survives", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });

    const now = 1_700_000_000_000;
    const ancient = repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext_b64: cipherB64("ancient"),
        size: 7,
        created_at: now - MAX_AGE_MS - 1000,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    const stale = repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext_b64: cipherB64("stale"),
        size: 5,
        created_at: now - MAX_AGE_MS - 1000,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    repo.entries.recordUse("u1", ancient.id, now - 1000);
    repo.entries.insertAndPrune(
      { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64("new"), size: 3, created_at: now },
      MAX_COUNT,
      MAX_AGE_MS
    );

    const survivors = repo.entries.listSince("u1", 0, 100).map((e) => e.id);
    expect(survivors).toContain(ancient.id);
    expect(survivors).not.toContain(stale.id);
  });

  it("recordUse re-allocates seq and never prunes", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });

    const now = 1_700_000_000_000;
    const first = repo.entries.insertAndPrune(
      { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64("a"), size: 1, created_at: now },
      MAX_COUNT,
      MAX_AGE_MS
    );
    const second = repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext_b64: cipherB64("b"),
        size: 1,
        created_at: now + 1,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(second.seq).toBeGreaterThan(first.seq);

    const used = repo.entries.recordUse("u1", first.id, now + 2);
    expect(used).toMatchObject({
      id: first.id,
      created_at: first.created_at,
      device_id: "d1",
      last_use: now + 2,
      seq: second.seq + 1,
    });
    expect(repo.entries.countForUser("u1")).toBe(2);
  });

  it("recordUse leaves another user's entry alone", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });
    repo.users.create({ id: "u2", username: "bob" });
    const now = 1_700_000_000_000;
    const mine = repo.entries.insertAndPrune(
      { user_id: "u1", device_id: "d1", ciphertext_b64: cipherB64("a"), size: 1, created_at: now },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(repo.entries.recordUse("u2", mine.id, now + 1)).toBeUndefined();
    expect(repo.entries.listSince("u1", 0, 100)[0]!.last_use).toBe(now);
  });

  /*
   * A sequence must never be handed out twice. Derived from `MAX(entries.seq)`
   * it would roll back on a delete — and `DELETE /entries` would reset it to
   * zero — so an entry captured afterwards would wear a sequence every client's
   * watermark had already passed, and `listSince` (`seq > ?`) would never hand
   * it over again. The row would be lost for good on every device that was
   * up to date, which is precisely the loss the client's watermark invariant
   * exists to prevent.
   */
  it("a sequence is never reused, however much of the history is deleted", () => {
    t = openTempDb();
    repo = t.repo;
    repo.users.create({ id: "u1", username: "alice" });
    const now = 1_700_000_000_000;
    const capture = (n: number) =>
      repo.entries.insertAndPrune(
        {
          user_id: "u1",
          device_id: "d1",
          ciphertext_b64: cipherB64(`e${n}`),
          size: 1,
          created_at: now + n,
        },
        MAX_COUNT,
        MAX_AGE_MS
      );

    const first = capture(1);
    const second = capture(2);
    // The watermark of a device that has fetched everything.
    const watermark = second.seq;

    repo.entries.delete("u1", second.id);
    const afterOneDelete = capture(3);
    expect(afterOneDelete.seq).toBeGreaterThan(watermark);

    repo.entries.deleteAll("u1");
    const afterAWipe = capture(4);
    expect(afterAWipe.seq).toBeGreaterThan(afterOneDelete.seq);

    // And the surviving entry is reachable from that watermark, which is the
    // whole point of the numbers climbing.
    expect(repo.entries.listSince("u1", watermark, 100).map((r) => r.id)).toEqual([afterAWipe.id]);
    expect(first.seq).toBeLessThan(watermark);
  });
});
