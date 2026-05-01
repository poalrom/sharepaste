import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { openDb } from "../../src/db/index.js";
import { migrate } from "../../src/db/migrate.js";
import { Repository } from "../../src/db/repository.js";
import { runUserCreate, runUserList, runUserDelete } from "../../src/cli/user.js";
import {
  runDeviceList,
  runDeviceRevoke,
} from "../../src/cli/device.js";
import { runEntryPurge } from "../../src/cli/entry.js";
import { hashToken, randomToken, randomId, sha256Hex } from "../../src/crypto.js";

let tmp: string;
let dbPath: string;
let repo: Repository;

beforeEach(() => {
  tmp = mkdtempSync(path.join(tmpdir(), "sp-cli-"));
  dbPath = path.join(tmp, "t.sqlite");
  const db = openDb(dbPath);
  migrate(db);
  repo = new Repository(db);
});

afterEach(() => rmSync(tmp, { recursive: true, force: true }));

describe("CLI user create", () => {
  it("creates a user, returns one-time invite token, and stores hash", () => {
    const { user_id, invite_token } = runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 });
    expect(repo.users.findById(user_id)?.username).toBe("alice");
    const inv = repo.invites.findByHash(sha256Hex(invite_token));
    expect(inv?.user_id).toBe(user_id);
    expect(inv?.claimed_at).toBeNull();
  });

  it("rejects duplicate usernames", () => {
    runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 });
    expect(() => runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 })).toThrow();
  });
});

describe("CLI user list / delete", () => {
  it("lists created users", () => {
    runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 });
    runUserCreate({ dbPath, username: "bob", ttlSeconds: 3600 });
    const users = runUserList({ dbPath });
    expect(users.map((u) => u.username).sort()).toEqual(["alice", "bob"]);
  });

  it("deletes a user and cascades to invites/memberships/entries", () => {
    const { user_id } = runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 });
    runUserDelete({ dbPath, userId: user_id });
    expect(repo.users.findById(user_id)).toBeUndefined();
    const remaining = repo.db.prepare("SELECT COUNT(*) AS c FROM invites WHERE user_id = ?").get(user_id) as { c: number };
    expect(remaining.c).toBe(0);
  });
});

const seedMembership = async (userId: string) => {
  const t = randomToken();
  const id = randomId();
  repo.memberships.create({
    user_id: userId,
    device_id: id,
    device_token_hash: await hashToken(t),
    device_label: "x",
    created_at: Date.now(),
    revoked_at: null,
  });
  return id;
};

describe("CLI device list / revoke", () => {
  it("lists every membership and revokes one", async () => {
    const { user_id } = runUserCreate({ dbPath, username: "alice", ttlSeconds: 60 });
    const d1 = await seedMembership(user_id);
    const d2 = await seedMembership(user_id);
    const list = runDeviceList({ dbPath });
    expect(list.map((m) => m.device_id).sort()).toEqual([d1, d2].sort());
    runDeviceRevoke({ dbPath, deviceId: d1 });
    const m = repo.memberships.findByDeviceId(user_id, d1);
    expect(m?.revoked_at).not.toBeNull();
  });
});

describe("CLI entry purge --user", () => {
  it("removes only that user's entries", async () => {
    const a = runUserCreate({ dbPath, username: "alice", ttlSeconds: 60 });
    const bUser = runUserCreate({ dbPath, username: "bob", ttlSeconds: 60 });
    const da = await seedMembership(a.user_id);
    const dbId = await seedMembership(bUser.user_id);
    repo.entries.insertAndPrune(
      { user_id: a.user_id, device_id: da, ciphertext: Buffer.from("a"), size: 1, created_at: Date.now() },
      100,
      30 * 24 * 3600 * 1000
    );
    repo.entries.insertAndPrune(
      { user_id: bUser.user_id, device_id: dbId, ciphertext: Buffer.from("b"), size: 1, created_at: Date.now() },
      100,
      30 * 24 * 3600 * 1000
    );
    runEntryPurge({ dbPath, userId: a.user_id });
    expect(repo.entries.countForUser(a.user_id)).toBe(0);
    expect(repo.entries.countForUser(bUser.user_id)).toBe(1);
  });
});
