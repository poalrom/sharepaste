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
});
