import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { addDevice, cipherB64, openTempDb, type TempDb } from "../helpers.js";
import { runUserCreate, runUserList, runUserDelete } from "../../src/cli/user.js";
import { runDeviceList, runDeviceRevoke } from "../../src/cli/device.js";
import { runEntryPurge } from "../../src/cli/entry.js";
import { sha256Hex } from "../../src/crypto.js";

let temp: TempDb;
let dbPath: string;

beforeEach(() => {
  temp = openTempDb();
  dbPath = temp.dbPath;
});

afterEach(() => temp.close());

describe("CLI user create", () => {
  it("creates a user, returns one-time invite token, and stores hash", () => {
    const { user_id, invite_token } = runUserCreate({ dbPath, username: "alice", ttlSeconds: 3600 });
    expect(temp.repo.users.list().map((u) => u.username)).toContain("alice");
    const inv = temp.repo.invites.findByHash(sha256Hex(invite_token));
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
    expect(temp.repo.users.list().map((u) => u.username)).not.toContain("alice");
    const remaining = temp.repo.db
      .prepare("SELECT COUNT(*) AS c FROM invites WHERE user_id = ?")
      .get(user_id) as { c: number };
    expect(remaining.c).toBe(0);
  });
});

describe("CLI device list / revoke", () => {
  it("lists every membership and revokes one", async () => {
    const { user_id } = runUserCreate({ dbPath, username: "alice", ttlSeconds: 60 });
    const d1 = await addDevice(temp.repo, user_id);
    const d2 = await addDevice(temp.repo, user_id);
    const list = runDeviceList({ dbPath });
    expect(list.map((m) => m.device_id).sort()).toEqual([d1.device_id, d2.device_id].sort());
    runDeviceRevoke({ dbPath, deviceId: d1.device_id });
    const m = temp.repo.memberships.findByDeviceId(user_id, d1.device_id);
    expect(m?.revoked_at).not.toBeNull();
  });
});

describe("CLI entry purge --user", () => {
  it("removes only that user's entries", async () => {
    const a = runUserCreate({ dbPath, username: "alice", ttlSeconds: 60 });
    const bUser = runUserCreate({ dbPath, username: "bob", ttlSeconds: 60 });
    const da = await addDevice(temp.repo, a.user_id);
    const dbDevice = await addDevice(temp.repo, bUser.user_id);
    temp.repo.entries.insertAndPrune(
      {
        user_id: a.user_id,
        device_id: da.device_id,
        ciphertext_b64: cipherB64("a"),
        size: 1,
        created_at: Date.now(),
      },
      100,
      30 * 24 * 3600 * 1000
    );
    temp.repo.entries.insertAndPrune(
      {
        user_id: bUser.user_id,
        device_id: dbDevice.device_id,
        ciphertext_b64: cipherB64("b"),
        size: 1,
        created_at: Date.now(),
      },
      100,
      30 * 24 * 3600 * 1000
    );
    runEntryPurge({ dbPath, userId: a.user_id });
    expect(temp.repo.entries.countForUser(a.user_id)).toBe(0);
    expect(temp.repo.entries.countForUser(bUser.user_id)).toBe(1);
  });
});
