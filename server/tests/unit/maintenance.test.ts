import { describe, it, expect, afterEach } from "vitest";
import { cipherB64, openRawTempDb, openTempDb, type TempDb } from "../helpers.js";
import { migrate } from "../../src/db/migrate.js";

let t: TempDb;

afterEach(() => {
  t.close();
});

describe("maintenance.sweep", () => {
  it("removes expired pairings and dead invites, keeping live ones", () => {
    t = openTempDb();
    const repo = t.repo;
    const now = 1_700_000_000_000;

    repo.users.create({ id: "u1", username: "alice" });

    // Expired pairing
    repo.pairings.create({
      id: "pair1",
      user_id: "u1",
      secret_hash: "00".repeat(32),
      encrypted_payload: null,
      claimed_at: null,
      paired_device_label: null,
      failed_attempts: 0,
      consumed_at: null,
      expires_at: now - 1,
    });

    // Live pairing
    repo.pairings.create({
      id: "pair2",
      user_id: "u1",
      secret_hash: "11".repeat(32),
      encrypted_payload: null,
      claimed_at: null,
      paired_device_label: null,
      failed_attempts: 0,
      consumed_at: null,
      expires_at: now + 60_000,
    });

    // Expired invite
    repo.invites.create({
      token_hash: "aa".repeat(32),
      user_id: "u1",
      expires_at: now - 1,
      claimed_at: null,
    });

    // Claimed invite
    repo.invites.create({
      token_hash: "bb".repeat(32),
      user_id: "u1",
      expires_at: now + 60_000,
      claimed_at: now,
    });

    // Live invite
    repo.invites.create({
      token_hash: "cc".repeat(32),
      user_id: "u1",
      expires_at: now + 60_000,
      claimed_at: null,
    });

    const result = repo.maintenance.sweep(now);
    expect(result).toEqual({ pairings: 1, invites: 2 });

    // Check that live pairing survives
    expect(repo.pairings.find("pair2")).not.toBeUndefined();
    // Check that expired pairing is gone
    expect(repo.pairings.find("pair1")).toBeUndefined();

    // Check that only live invite survives
    expect(repo.invites.findByHash("cc".repeat(32))).not.toBeUndefined();
    expect(repo.invites.findByHash("aa".repeat(32))).toBeUndefined();
    expect(repo.invites.findByHash("bb".repeat(32))).toBeUndefined();
  });

  it("migrate is idempotent", () => {
    t = openTempDb();
    // openTempDb already called migrate once; call it a second time
    migrate(t.db);
    // Should not throw; verify a subsequent create still works
    const repo = t.repo;
    const user = repo.users.create({ id: "u1", username: "alice" });
    expect(user.id).toBe("u1");
  });

  it("upgrades an entries table created before last_use and seq", () => {
    t = openRawTempDb();
    const { db } = t;
    db.exec(`
      CREATE TABLE users (
        id          TEXT PRIMARY KEY,
        username    TEXT UNIQUE NOT NULL,
        created_at  INTEGER NOT NULL
      );
      CREATE TABLE entries (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        device_id      TEXT NOT NULL,
        ciphertext_b64 TEXT NOT NULL,
        size           INTEGER NOT NULL,
        created_at     INTEGER NOT NULL
      );
      CREATE INDEX entries_user_id_id ON entries (user_id, id);
    `);
    db.prepare("INSERT INTO users (id, username, created_at) VALUES (?, ?, ?)").run(
      "u1",
      "alice",
      1_700_000_000_000
    );
    const insert = db.prepare(
      "INSERT INTO entries (user_id, device_id, ciphertext_b64, size, created_at) VALUES (?, ?, ?, ?, ?)"
    );
    insert.run("u1", "d1", cipherB64("one"), 3, 1_700_000_000_000);
    insert.run("u1", "d1", cipherB64("two"), 3, 1_700_000_001_000);

    migrate(db);

    const readRows = db.prepare("SELECT id, created_at, last_use, seq FROM entries ORDER BY id");
    const seeded = readRows.all() as Array<{
      id: number;
      created_at: number;
      last_use: number;
      seq: number;
    }>;
    expect(seeded).toHaveLength(2);
    for (const row of seeded) {
      expect(row.last_use).toBe(row.created_at);
      expect(row.seq).toBe(row.id);
    }

    const indexes = db
      .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'entries'")
      .pluck()
      .all() as string[];
    expect(indexes).not.toContain("entries_user_id_id");
    expect(indexes).toContain("entries_user_id_seq");
    expect(indexes).toContain("entries_user_last_use");

    // The counter has to start above every sequence the backfill handed out, or
    // the first capture after the upgrade would wear one a client had passed.
    expect(db.prepare("SELECT next_seq FROM users WHERE id = 'u1'").pluck().get()).toBe(
      Math.max(...seeded.map((r) => r.seq))
    );

    // Twice over: the second pass must find the same shape and change nothing.
    migrate(db);
    expect(readRows.all()).toEqual(seeded);
  });
});
