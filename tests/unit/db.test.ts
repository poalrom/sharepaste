import { describe, it, expect } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { openDb } from "../../src/db/index.js";
import { migrate } from "../../src/db/migrate.js";

describe("migrate()", () => {
  it("creates all required tables and is idempotent", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "sp-"));
    const dbPath = path.join(dir, "test.sqlite");
    try {
      const db = openDb(dbPath);
      migrate(db);
      migrate(db); // idempotent

      const tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .all()
        .map((r: any) => r.name);

      expect(tables).toEqual(
        expect.arrayContaining([
          "users",
          "invites",
          "memberships",
          "pairings",
          "entries",
        ])
      );

      const fk = db.prepare("PRAGMA foreign_keys").get() as { foreign_keys: number };
      expect(fk.foreign_keys).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
