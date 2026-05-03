import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { openDb } from "../../src/db/index.js";
import { migrate } from "../../src/db/migrate.js";
import { Repository } from "../../src/db/repository.js";

const MAX_COUNT = 100;
const MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

let tmp: string;
let repo: Repository;

beforeEach(() => {
  tmp = mkdtempSync(path.join(tmpdir(), "sp-"));
  const db = openDb(path.join(tmp, "t.sqlite"));
  migrate(db);
  repo = new Repository(db);
  repo.users.create({ id: "u1", username: "alice" });
});

afterEach(() => rmSync(tmp, { recursive: true, force: true }));

describe("entries.insertAndPrune", () => {
  it("keeps only the most recent MAX_COUNT entries for that user", () => {
    const now = 1_700_000_000_000;
    for (let i = 0; i < MAX_COUNT + 10; i++) {
      repo.entries.insertAndPrune(
        {
          user_id: "u1",
          device_id: "d1",
          ciphertext: Buffer.from([i]),
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
    const now = 1_700_000_000_000;
    repo.entries.insertAndPrune(
      {
        user_id: "u1",
        device_id: "d1",
        ciphertext: Buffer.from("old"),
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
        ciphertext: Buffer.from("new"),
        size: 3,
        created_at: now,
      },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(repo.entries.countForUser("u1")).toBe(1);
  });

  it("does not affect other users", () => {
    repo.users.create({ id: "u2", username: "bob" });
    const now = 1_700_000_000_000;
    for (let i = 0; i < MAX_COUNT + 5; i++) {
      repo.entries.insertAndPrune(
        { user_id: "u1", device_id: "d1", ciphertext: Buffer.from([i]), size: 1, created_at: now + i },
        MAX_COUNT,
        MAX_AGE_MS
      );
    }
    repo.entries.insertAndPrune(
      { user_id: "u2", device_id: "d2", ciphertext: Buffer.from("x"), size: 1, created_at: now },
      MAX_COUNT,
      MAX_AGE_MS
    );
    expect(repo.entries.countForUser("u1")).toBe(MAX_COUNT);
    expect(repo.entries.countForUser("u2")).toBe(1);
  });
});
