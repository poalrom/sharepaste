import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { openDb } from "../../src/db/index.js";
import { migrate } from "../../src/db/migrate.js";
import { Repository } from "../../src/db/repository.js";

let tmp: string;
let repo: Repository;

beforeEach(() => {
  tmp = mkdtempSync(path.join(tmpdir(), "sp-"));
  const db = openDb(path.join(tmp, "t.sqlite"));
  migrate(db);
  repo = new Repository(db);
});

afterEach(() => rmSync(tmp, { recursive: true, force: true }));

describe("Repository.users", () => {
  it("creates and looks up users by id and username", () => {
    const user = repo.users.create({ id: "u1", username: "alice" });
    expect(user.id).toBe("u1");
    expect(repo.users.findByUsername("alice")?.id).toBe("u1");
    expect(repo.users.findById("u1")?.username).toBe("alice");
    expect(repo.users.findByUsername("bob")).toBeUndefined();
  });

  it("rejects duplicate usernames", () => {
    repo.users.create({ id: "u1", username: "alice" });
    expect(() => repo.users.create({ id: "u2", username: "alice" })).toThrow();
  });
});
