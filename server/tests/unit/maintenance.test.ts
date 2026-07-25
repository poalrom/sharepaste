import { describe, it, expect, afterEach } from "vitest";
import { openTempDb, type TempDb } from "../helpers.js";
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
});
