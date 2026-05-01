# Sharepaste Server + CLI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the self-hosted Node.js server (HTTP API + SSE) and operator CLI for Sharepaste, fully exercising every endpoint and SQL path with TDD-driven integration tests against real SQLite. Four client tracks (macOS, Windows, Android, iPadOS) are out of scope and will receive their own plans.

**Architecture:** A single Node binary (`sharepaste`) with subcommand dispatch. `sharepaste serve` runs Fastify HTTP + an in-memory SSE hub backed by `better-sqlite3` for storage. `sharepaste user|device|entry …` open the same SQLite database directly for operator tasks. The server holds only ciphertext + opaque hashes; it never sees plaintext or `user_key`.

**Tech Stack:**
- Node.js 20 LTS, TypeScript (ESM, NodeNext)
- Fastify 4 + `@fastify/sensible`
- `better-sqlite3` for storage
- `argon2` (npm) for `argon2id` token hashing
- `node:crypto` for SHA-256, random tokens, timing-safe compares
- `commander` for CLI dispatch
- `vitest` for tests, `supertest`-style usage via Fastify `.inject()` plus a real listening server for SSE
- `pino` (Fastify default) for logs, silenced in tests
- Docker (multi-stage Node 20 alpine)

---

## File structure

```
sharepaste/
├── Dockerfile
├── .dockerignore
├── .gitignore
├── package.json
├── tsconfig.json
├── vitest.config.ts
├── src/
│   ├── index.ts                  # CLI entry (subcommand dispatch via commander)
│   ├── config.ts                 # serve-mode env-var parsing
│   ├── crypto.ts                 # ids, sha256, argon2id, timing-safe compare
│   ├── time.ts                   # now() ms, configurable for tests
│   ├── db/
│   │   ├── index.ts              # better-sqlite3 open + foreign_keys + WAL
│   │   ├── migrate.ts            # idempotent CREATE TABLEs / INDEX
│   │   └── repository.ts         # typed CRUD helpers
│   ├── server/
│   │   ├── app.ts                # buildApp(deps) → Fastify instance
│   │   ├── auth.ts               # verifyBearer → membership or 401
│   │   ├── sse-hub.ts            # in-memory pub/sub keyed by user_id
│   │   └── routes/
│   │       ├── claim-invite.ts
│   │       ├── pairing.ts
│   │       ├── devices.ts
│   │       ├── entries.ts
│   │       └── events.ts
│   └── cli/
│       ├── user.ts               # user create | list | delete
│       ├── device.ts             # device list | revoke
│       └── entry.ts              # entry purge --user
└── tests/
    ├── helpers.ts                # buildTestApp(): tempfile db + Fastify
    ├── unit/
    │   ├── crypto.test.ts
    │   ├── retention-sql.test.ts
    │   └── sse-hub.test.ts
    └── integration/
        ├── claim-invite.test.ts
        ├── pairing.test.ts
        ├── devices.test.ts
        ├── entries.test.ts
        ├── events-sse.test.ts
        ├── isolation.test.ts
        ├── retention.test.ts
        └── cli.test.ts
```

---

## Task 1: Project bootstrap

**Files:**
- Create: `sharepaste/package.json`
- Create: `sharepaste/tsconfig.json`
- Create: `sharepaste/vitest.config.ts`
- Create: `sharepaste/.gitignore`
- Create: `sharepaste/.dockerignore`
- Create: `sharepaste/src/index.ts`
- Create: `sharepaste/tests/unit/crypto.test.ts` (placeholder, replaced in Task 3)

- [ ] **Step 1: Initialize npm package and install dependencies**

```bash
cd /Users/poalrom/private/sharepaste
npm init -y
npm install fastify@4 @fastify/sensible better-sqlite3 argon2 commander
npm install -D typescript @types/node @types/better-sqlite3 vitest tsx
```

- [ ] **Step 2: Write tsconfig**

`sharepaste/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "outDir": "dist",
    "rootDir": ".",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "resolveJsonModule": true,
    "declaration": false,
    "sourceMap": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true
  },
  "include": ["src/**/*", "tests/**/*"]
}
```

- [ ] **Step 3: Write package.json scripts and metadata**

Edit `sharepaste/package.json` to set:
```json
{
  "name": "sharepaste",
  "version": "0.1.0",
  "type": "module",
  "bin": { "sharepaste": "dist/src/index.js" },
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "start": "tsx src/index.ts",
    "test": "vitest run",
    "test:watch": "vitest"
  }
}
```

- [ ] **Step 4: Write vitest config**

`sharepaste/vitest.config.ts`:
```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    pool: "threads",
    poolOptions: { threads: { singleThread: false } },
    testTimeout: 10000,
  },
});
```

- [ ] **Step 5: Write `.gitignore` and `.dockerignore`**

`sharepaste/.gitignore`:
```
node_modules/
dist/
*.sqlite*
coverage/
.env
.env.*
```

`sharepaste/.dockerignore`:
```
node_modules
dist
tests
docs
.git
.gitignore
*.sqlite*
```

- [ ] **Step 6: Stub CLI entry that just prints help**

`sharepaste/src/index.ts`:
```ts
#!/usr/bin/env node
import { Command } from "commander";

const program = new Command()
  .name("sharepaste")
  .description("Sharepaste server and operator CLI")
  .version("0.1.0");

program.command("serve").description("Run HTTP server").action(() => {
  console.log("serve: not implemented yet");
});

program.parseAsync(process.argv);
```

- [ ] **Step 7: Add a smoke test to confirm the toolchain runs**

`sharepaste/tests/unit/crypto.test.ts`:
```ts
import { describe, it, expect } from "vitest";

describe("toolchain smoke", () => {
  it("runs vitest", () => {
    expect(1 + 1).toBe(2);
  });
});
```

- [ ] **Step 8: Run tests**

```bash
cd /Users/poalrom/private/sharepaste && npm test
```

Expected: 1 passed.

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "chore: bootstrap server package, tsconfig, vitest, CLI stub"
```

---

## Task 2: Database schema and migrations

**Files:**
- Create: `sharepaste/src/db/index.ts`
- Create: `sharepaste/src/db/migrate.ts`
- Create: `sharepaste/tests/unit/db.test.ts`

- [ ] **Step 1: Write the failing test**

`sharepaste/tests/unit/db.test.ts`:
```ts
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
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/unit/db.test.ts
```

Expected: FAIL — `Cannot find module '../../src/db/index.js'`.

- [ ] **Step 3: Implement `db/index.ts`**

`sharepaste/src/db/index.ts`:
```ts
import Database, { type Database as DbType } from "better-sqlite3";

export type Db = DbType;

export const openDb = (filePath: string): Db => {
  const db = new Database(filePath);
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  db.pragma("synchronous = NORMAL");
  return db;
};
```

- [ ] **Step 4: Implement `db/migrate.ts`**

`sharepaste/src/db/migrate.ts`:
```ts
import type { Db } from "./index.js";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS users (
  id          TEXT PRIMARY KEY,
  username    TEXT UNIQUE NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS invites (
  token_hash  TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at  INTEGER NOT NULL,
  claimed_at  INTEGER
);

CREATE TABLE IF NOT EXISTS memberships (
  user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id          TEXT NOT NULL,
  device_token_hash  TEXT NOT NULL,
  device_label       TEXT,
  created_at         INTEGER NOT NULL,
  revoked_at         INTEGER,
  PRIMARY KEY (user_id, device_id)
);
CREATE INDEX IF NOT EXISTS memberships_token_hash
  ON memberships (device_token_hash);

CREATE TABLE IF NOT EXISTS pairings (
  id                  TEXT PRIMARY KEY,
  user_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  secret_hash         TEXT NOT NULL,
  encrypted_payload   BLOB,
  claimed_by          TEXT,
  failed_attempts     INTEGER NOT NULL DEFAULT 0,
  consumed_at         INTEGER,
  expires_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id   TEXT NOT NULL,
  ciphertext  BLOB NOT NULL,
  size        INTEGER NOT NULL,
  created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_user_id_id ON entries (user_id, id);
`;

export const migrate = (db: Db): void => {
  db.exec(SCHEMA);
};
```

Note: `pairings.consumed_at` is added beyond the spec's table. The spec says the server "marks the slot consumed"; this column carries that mark explicitly so `POST /devices` can return 410 on subsequent calls.

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/unit/db.test.ts
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(db): add SQLite schema and idempotent migrations"
```

---

## Task 3: Crypto helpers

**Files:**
- Create: `sharepaste/src/crypto.ts`
- Replace: `sharepaste/tests/unit/crypto.test.ts`

- [ ] **Step 1: Write failing tests**

Replace `sharepaste/tests/unit/crypto.test.ts` with:
```ts
import { describe, it, expect } from "vitest";
import {
  randomId,
  randomToken,
  sha256Hex,
  hashToken,
  verifyToken,
  timingSafeEqualHex,
} from "../../src/crypto.js";

describe("randomId", () => {
  it("returns a unique string each call", () => {
    const a = randomId();
    const b = randomId();
    expect(a).not.toBe(b);
    expect(a).toMatch(/^[0-9a-f-]{36}$/);
  });
});

describe("randomToken", () => {
  it("returns 43-char base64url string for 32 random bytes", () => {
    const t = randomToken();
    expect(t).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(t).not.toBe(randomToken());
  });
});

describe("sha256Hex", () => {
  it("matches a known vector", () => {
    expect(sha256Hex("abc")).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
  });
});

describe("hashToken / verifyToken", () => {
  it("round-trips an argon2id hash", async () => {
    const t = randomToken();
    const h = await hashToken(t);
    expect(h).not.toContain(t);
    expect(await verifyToken(h, t)).toBe(true);
    expect(await verifyToken(h, "wrong-" + t)).toBe(false);
  });
});

describe("timingSafeEqualHex", () => {
  it("returns true for equal hex strings of any case and false otherwise", () => {
    expect(timingSafeEqualHex("ab", "ab")).toBe(true);
    expect(timingSafeEqualHex("ab", "AB")).toBe(true);
    expect(timingSafeEqualHex("ab", "ac")).toBe(false);
    expect(timingSafeEqualHex("ab", "abc")).toBe(false);
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/unit/crypto.test.ts
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement crypto helpers**

`sharepaste/src/crypto.ts`:
```ts
import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import argon2 from "argon2";

export const randomId = (): string => randomUUID();

export const randomToken = (): string =>
  randomBytes(32).toString("base64url");

export const sha256Hex = (input: string | Buffer): string =>
  createHash("sha256").update(input).digest("hex");

const ARGON_OPTS: argon2.Options = {
  type: argon2.argon2id,
  memoryCost: 19_456, // 19 MiB
  timeCost: 2,
  parallelism: 1,
};

export const hashToken = (token: string): Promise<string> =>
  argon2.hash(token, ARGON_OPTS);

export const verifyToken = (hash: string, token: string): Promise<boolean> =>
  argon2.verify(hash, token);

export const timingSafeEqualHex = (a: string, b: string): boolean => {
  const al = a.toLowerCase();
  const bl = b.toLowerCase();
  if (al.length !== bl.length) return false;
  const ab = Buffer.from(al, "hex");
  const bb = Buffer.from(bl, "hex");
  if (ab.length !== bb.length || ab.length === 0) return false;
  return timingSafeEqual(ab, bb);
};
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/unit/crypto.test.ts
```

Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(crypto): random ids/tokens, sha256, argon2id token hashing"
```

---

## Task 4: Repository layer (typed CRUD helpers)

**Files:**
- Create: `sharepaste/src/db/repository.ts`
- Create: `sharepaste/tests/unit/repository.test.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/unit/repository.test.ts`:
```ts
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
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/unit/repository.test.ts
```

Expected: FAIL — Repository missing.

- [ ] **Step 3: Implement repository**

`sharepaste/src/db/repository.ts`:
```ts
import type { Db } from "./index.js";

export interface UserRow {
  id: string;
  username: string;
  created_at: number;
}

export interface InviteRow {
  token_hash: string;
  user_id: string;
  expires_at: number;
  claimed_at: number | null;
}

export interface MembershipRow {
  user_id: string;
  device_id: string;
  device_token_hash: string;
  device_label: string | null;
  created_at: number;
  revoked_at: number | null;
}

export interface PairingRow {
  id: string;
  user_id: string;
  secret_hash: string;
  encrypted_payload: Buffer | null;
  claimed_by: string | null;
  failed_attempts: number;
  consumed_at: number | null;
  expires_at: number;
}

export interface EntryRow {
  id: number;
  user_id: string;
  device_id: string;
  ciphertext: Buffer;
  size: number;
  created_at: number;
}

export class Repository {
  constructor(public readonly db: Db) {}

  readonly users = {
    create: ({ id, username }: { id: string; username: string }): UserRow => {
      const created_at = Date.now();
      this.db
        .prepare("INSERT INTO users (id, username, created_at) VALUES (?, ?, ?)")
        .run(id, username, created_at);
      return { id, username, created_at };
    },
    findById: (id: string): UserRow | undefined =>
      this.db.prepare("SELECT * FROM users WHERE id = ?").get(id) as UserRow | undefined,
    findByUsername: (username: string): UserRow | undefined =>
      this.db.prepare("SELECT * FROM users WHERE username = ?").get(username) as UserRow | undefined,
    list: (): UserRow[] =>
      this.db.prepare("SELECT * FROM users ORDER BY created_at").all() as UserRow[],
    delete: (id: string): void => {
      this.db.prepare("DELETE FROM users WHERE id = ?").run(id);
    },
  };

  readonly invites = {
    create: (row: InviteRow): void => {
      this.db
        .prepare(
          "INSERT INTO invites (token_hash, user_id, expires_at, claimed_at) VALUES (?, ?, ?, ?)"
        )
        .run(row.token_hash, row.user_id, row.expires_at, row.claimed_at);
    },
    findByHash: (hash: string): InviteRow | undefined =>
      this.db.prepare("SELECT * FROM invites WHERE token_hash = ?").get(hash) as InviteRow | undefined,
    markClaimed: (hash: string, at: number): void => {
      this.db.prepare("UPDATE invites SET claimed_at = ? WHERE token_hash = ?").run(at, hash);
    },
  };

  readonly memberships = {
    create: (row: MembershipRow): void => {
      this.db
        .prepare(
          `INSERT INTO memberships
           (user_id, device_id, device_token_hash, device_label, created_at, revoked_at)
           VALUES (?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.user_id,
          row.device_id,
          row.device_token_hash,
          row.device_label,
          row.created_at,
          row.revoked_at
        );
    },
    findByDeviceId: (user_id: string, device_id: string): MembershipRow | undefined =>
      this.db
        .prepare("SELECT * FROM memberships WHERE user_id = ? AND device_id = ?")
        .get(user_id, device_id) as MembershipRow | undefined,
    listActive: (): MembershipRow[] =>
      this.db.prepare("SELECT * FROM memberships WHERE revoked_at IS NULL").all() as MembershipRow[],
    listAll: (): MembershipRow[] =>
      this.db.prepare("SELECT * FROM memberships").all() as MembershipRow[],
    revoke: (user_id: string, device_id: string, at: number): number =>
      this.db
        .prepare(
          "UPDATE memberships SET revoked_at = ? WHERE user_id = ? AND device_id = ? AND revoked_at IS NULL"
        )
        .run(at, user_id, device_id).changes,
  };

  readonly pairings = {
    create: (row: PairingRow): void => {
      this.db
        .prepare(
          `INSERT INTO pairings
           (id, user_id, secret_hash, encrypted_payload, claimed_by, failed_attempts, consumed_at, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.id,
          row.user_id,
          row.secret_hash,
          row.encrypted_payload,
          row.claimed_by,
          row.failed_attempts,
          row.consumed_at,
          row.expires_at
        );
    },
    find: (id: string): PairingRow | undefined =>
      this.db.prepare("SELECT * FROM pairings WHERE id = ?").get(id) as PairingRow | undefined,
    incrementFailed: (id: string): number =>
      this.db
        .prepare("UPDATE pairings SET failed_attempts = failed_attempts + 1 WHERE id = ?")
        .run(id).changes,
    setClaimedBy: (id: string, claimed_by: string): number =>
      this.db
        .prepare("UPDATE pairings SET claimed_by = ? WHERE id = ? AND claimed_by IS NULL")
        .run(claimed_by, id).changes,
    setPayload: (id: string, payload: Buffer): void => {
      this.db.prepare("UPDATE pairings SET encrypted_payload = ? WHERE id = ?").run(payload, id);
    },
    markConsumed: (id: string, at: number): void => {
      this.db
        .prepare("UPDATE pairings SET consumed_at = ?, encrypted_payload = NULL WHERE id = ?")
        .run(at, id);
    },
  };

  readonly entries = {
    insertAndPrune: (
      row: Omit<EntryRow, "id">,
      maxCount: number,
      maxAgeMs: number
    ): EntryRow => {
      const tx = this.db.transaction(() => {
        const result = this.db
          .prepare(
            "INSERT INTO entries (user_id, device_id, ciphertext, size, created_at) VALUES (?, ?, ?, ?, ?)"
          )
          .run(row.user_id, row.device_id, row.ciphertext, row.size, row.created_at);
        const id = Number(result.lastInsertRowid);
        this.db
          .prepare(
            `DELETE FROM entries
             WHERE user_id = ?
               AND (
                 created_at < ?
                 OR id NOT IN (
                   SELECT id FROM entries
                   WHERE user_id = ?
                   ORDER BY id DESC
                   LIMIT ?
                 )
               )`
          )
          .run(row.user_id, row.created_at - maxAgeMs, row.user_id, maxCount);
        return { id, ...row };
      });
      return tx();
    },
    listSince: (user_id: string, sinceId: number, limit: number): EntryRow[] =>
      this.db
        .prepare(
          "SELECT * FROM entries WHERE user_id = ? AND id > ? ORDER BY id ASC LIMIT ?"
        )
        .all(user_id, sinceId, limit) as EntryRow[],
    delete: (user_id: string, id: number): number =>
      this.db
        .prepare("DELETE FROM entries WHERE user_id = ? AND id = ?")
        .run(user_id, id).changes,
    deleteAll: (user_id: string): number =>
      this.db.prepare("DELETE FROM entries WHERE user_id = ?").run(user_id).changes,
    countForUser: (user_id: string): number =>
      (this.db.prepare("SELECT COUNT(*) AS c FROM entries WHERE user_id = ?").get(user_id) as { c: number }).c,
  };
}
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/unit/repository.test.ts
```

Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(db): add typed Repository with users/invites/memberships/pairings/entries"
```

---

## Task 5: Inline retention pruning unit test

**Files:**
- Create: `sharepaste/tests/unit/retention-sql.test.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/unit/retention-sql.test.ts`:
```ts
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
```

- [ ] **Step 2: Run test and verify it passes**

```bash
npm test -- tests/unit/retention-sql.test.ts
```

Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test(retention): cover count cap, age cap, multi-user isolation"
```

---

## Task 6: SSE hub

**Files:**
- Create: `sharepaste/src/server/sse-hub.ts`
- Create: `sharepaste/tests/unit/sse-hub.test.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/unit/sse-hub.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { SseHub } from "../../src/server/sse-hub.js";

describe("SseHub", () => {
  it("delivers events only to subscribers of the matching user", () => {
    const hub = new SseHub();
    const aReceived: unknown[] = [];
    const bReceived: unknown[] = [];

    const unsubA = hub.subscribe("user-a", (e) => aReceived.push(e));
    const unsubB = hub.subscribe("user-b", (e) => bReceived.push(e));

    hub.publish("user-a", { type: "entry", id: 1 });
    hub.publish("user-b", { type: "delete", id: 7 });

    expect(aReceived).toEqual([{ type: "entry", id: 1 }]);
    expect(bReceived).toEqual([{ type: "delete", id: 7 }]);

    unsubA();
    hub.publish("user-a", { type: "entry", id: 2 });
    expect(aReceived).toEqual([{ type: "entry", id: 1 }]);
    unsubB();
  });

  it("supports multiple subscribers per user", () => {
    const hub = new SseHub();
    const r1: unknown[] = [];
    const r2: unknown[] = [];
    hub.subscribe("user-a", (e) => r1.push(e));
    hub.subscribe("user-a", (e) => r2.push(e));
    hub.publish("user-a", { type: "entry", id: 5 });
    expect(r1).toEqual([{ type: "entry", id: 5 }]);
    expect(r2).toEqual([{ type: "entry", id: 5 }]);
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/unit/sse-hub.test.ts
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement SSE hub**

`sharepaste/src/server/sse-hub.ts`:
```ts
export type SseEvent =
  | { type: "entry"; id: number; ciphertext: string; created_at: number; device_id: string }
  | { type: "delete"; id: number };

export type SseListener = (event: SseEvent) => void;

export class SseHub {
  private readonly subscribers = new Map<string, Set<SseListener>>();

  subscribe(userId: string, listener: SseListener): () => void {
    let set = this.subscribers.get(userId);
    if (!set) {
      set = new Set();
      this.subscribers.set(userId, set);
    }
    set.add(listener);
    return () => {
      const s = this.subscribers.get(userId);
      if (!s) return;
      s.delete(listener);
      if (s.size === 0) this.subscribers.delete(userId);
    };
  }

  publish(userId: string, event: SseEvent): void {
    const s = this.subscribers.get(userId);
    if (!s) return;
    for (const fn of s) fn(event);
  }
}
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/unit/sse-hub.test.ts
```

Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(sse): in-memory pub/sub keyed by user_id"
```

---

## Task 7: App factory and test harness

**Files:**
- Create: `sharepaste/src/server/app.ts`
- Create: `sharepaste/tests/helpers.ts`
- Create: `sharepaste/tests/integration/health.test.ts`

- [ ] **Step 1: Write the failing test**

`sharepaste/tests/integration/health.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp } from "../helpers.js";

describe("buildApp()", () => {
  it("answers GET /healthz with 200 ok", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({ method: "GET", url: "/healthz" });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ ok: true });
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Implement the test harness**

`sharepaste/tests/helpers.ts`:
```ts
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { FastifyInstance } from "fastify";
import { buildApp, type AppDeps } from "../src/server/app.js";
import { openDb } from "../src/db/index.js";
import { migrate } from "../src/db/migrate.js";
import { Repository } from "../src/db/repository.js";
import { SseHub } from "../src/server/sse-hub.js";

export interface TestApp {
  app: FastifyInstance;
  repo: Repository;
  hub: SseHub;
  close: () => Promise<void>;
}

export const buildTestApp = async (overrides: Partial<AppDeps> = {}): Promise<TestApp> => {
  const dir = mkdtempSync(path.join(tmpdir(), "sp-test-"));
  const db = openDb(path.join(dir, "t.sqlite"));
  migrate(db);
  const repo = new Repository(db);
  const hub = new SseHub();
  const deps: AppDeps = {
    repo,
    hub,
    pairingTtlMs: 2 * 60 * 1000,
    maxEntries: 100,
    maxEntryAgeMs: 30 * 24 * 60 * 60 * 1000,
    maxEntryBytes: 64 * 1024,
    maxPairingFailures: 3,
    logger: false,
    ...overrides,
  };
  const app = await buildApp(deps);
  await app.ready();
  return {
    app,
    repo,
    hub,
    close: async () => {
      await app.close();
      db.close();
      rmSync(dir, { recursive: true, force: true });
    },
  };
};
```

- [ ] **Step 3: Implement app factory**

`sharepaste/src/server/app.ts`:
```ts
import Fastify, { type FastifyInstance } from "fastify";
import sensible from "@fastify/sensible";
import type { Repository } from "../db/repository.js";
import type { SseHub } from "./sse-hub.js";

export interface AppDeps {
  repo: Repository;
  hub: SseHub;
  pairingTtlMs: number;
  maxEntries: number;
  maxEntryAgeMs: number;
  maxEntryBytes: number;
  maxPairingFailures: number;
  logger: boolean | object;
}

export const buildApp = async (deps: AppDeps): Promise<FastifyInstance> => {
  const app = Fastify({ logger: deps.logger, bodyLimit: 1024 * 1024 });
  await app.register(sensible);
  app.decorate("deps", deps);

  app.get("/healthz", async () => ({ ok: true }));

  return app;
};

declare module "fastify" {
  interface FastifyInstance {
    deps: AppDeps;
  }
}
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/integration/health.test.ts
```

Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(server): app factory + integration test harness with /healthz"
```

---

## Task 8: Auth middleware (Bearer device token)

**Files:**
- Create: `sharepaste/src/server/auth.ts`

This task adds the verifier but no route uses it yet. Coverage is in subsequent integration tests. We commit it together with the first endpoint that depends on it (Task 10), so this task only writes the code.

- [ ] **Step 1: Implement `verifyBearer`**

`sharepaste/src/server/auth.ts`:
```ts
import type { FastifyInstance, FastifyRequest } from "fastify";
import type { MembershipRow } from "../db/repository.js";
import { verifyToken } from "../crypto.js";

export interface AuthedMembership {
  user_id: string;
  device_id: string;
}

const extractToken = (req: FastifyRequest): string | null => {
  const h = req.headers.authorization;
  if (h && h.startsWith("Bearer ")) return h.slice("Bearer ".length).trim();
  const q = (req.query as Record<string, unknown> | undefined)?.token;
  return typeof q === "string" && q.length > 0 ? q : null;
};

export const verifyBearer = async (
  app: FastifyInstance,
  req: FastifyRequest
): Promise<AuthedMembership> => {
  const token = extractToken(req);
  if (!token) throw app.httpErrors.unauthorized("missing bearer token");

  // Naive scan over active memberships. Argon2 verify is the cost; active
  // membership count is small for self-hosted use.
  const candidates: MembershipRow[] = app.deps.repo.memberships.listActive();
  for (const m of candidates) {
    if (await verifyToken(m.device_token_hash, token)) {
      return { user_id: m.user_id, device_id: m.device_id };
    }
  }
  throw app.httpErrors.unauthorized("invalid token");
};
```

Note: O(n) argon2 verifies per request is acceptable for self-hosted scale (≤ ~50 active memberships). If this ever becomes a bottleneck, swap argon2 for a cheap keyed hash (HMAC) over the device token at login time, or cache verified tokens.

- [ ] **Step 2: Run all tests** (no behavior change yet)

```bash
npm test
```

Expected: previously passing tests still pass.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(auth): verifyBearer scans active memberships via argon2id"
```

---

## Task 9: `POST /claim-invite`

**Files:**
- Create: `sharepaste/src/server/routes/claim-invite.ts`
- Create: `sharepaste/tests/integration/claim-invite.test.ts`
- Modify: `sharepaste/src/server/app.ts` — register the route.

- [ ] **Step 1: Write failing tests**

`sharepaste/tests/integration/claim-invite.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp } from "../helpers.js";
import { hashToken, randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /claim-invite", () => {
  const seedInvite = async (repo: any, userId = "u1") => {
    repo.users.create({ id: userId, username: "alice" });
    const token = randomToken();
    repo.invites.create({
      token_hash: sha256Hex(token),
      user_id: userId,
      expires_at: Date.now() + 60_000,
      claimed_at: null,
    });
    return token;
  };

  it("issues a device_token and creates a membership on a fresh invite", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const token = await seedInvite(repo);
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "macbook" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; user_id: string; device_id: string };
      expect(body.user_id).toBe("u1");
      expect(body.device_token).toMatch(/^[A-Za-z0-9_-]{43}$/);
      expect(body.device_id).toMatch(/^[0-9a-f-]{36}$/);

      const mem = repo.memberships.findByDeviceId("u1", body.device_id);
      expect(mem?.device_label).toBe("macbook");
      expect(mem?.revoked_at).toBeNull();

      const inv = repo.invites.findByHash(sha256Hex(token));
      expect(inv?.claimed_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 409 if the invite was already claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const token = await seedInvite(repo);
      await app.inject({ method: "POST", url: "/claim-invite", payload: { token, device_label: "a" } });
      const res2 = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "b" },
      });
      expect(res2.statusCode).toBe(409);
    } finally {
      await close();
    }
  });

  it("returns 404 for an unknown token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token: randomToken(), device_label: "x" },
      });
      expect(res.statusCode).toBe(404);
    } finally {
      await close();
    }
  });

  it("returns 410 for an expired invite", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      repo.users.create({ id: "u1", username: "alice" });
      const token = randomToken();
      repo.invites.create({
        token_hash: sha256Hex(token),
        user_id: "u1",
        expires_at: Date.now() - 1,
        claimed_at: null,
      });
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token, device_label: "x" },
      });
      expect(res.statusCode).toBe(410);
    } finally {
      await close();
    }
  });

  it("returns 400 if body is malformed", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({ method: "POST", url: "/claim-invite", payload: {} });
      expect(res.statusCode).toBe(400);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/claim-invite.test.ts
```

Expected: FAIL — route returns 404 for everything.

- [ ] **Step 3: Implement the route**

`sharepaste/src/server/routes/claim-invite.ts`:
```ts
import type { FastifyInstance } from "fastify";
import { hashToken, randomId, randomToken, sha256Hex } from "../../crypto.js";

const SCHEMA = {
  body: {
    type: "object",
    required: ["token", "device_label"],
    additionalProperties: false,
    properties: {
      token: { type: "string", minLength: 16, maxLength: 256 },
      device_label: { type: "string", minLength: 1, maxLength: 128 },
    },
  },
} as const;

export const registerClaimInviteRoute = (app: FastifyInstance): void => {
  app.post<{ Body: { token: string; device_label: string } }>(
    "/claim-invite",
    { schema: SCHEMA },
    async (req, reply) => {
      const { token, device_label } = req.body;
      const tokenHash = sha256Hex(token);
      const invite = app.deps.repo.invites.findByHash(tokenHash);
      if (!invite) throw app.httpErrors.notFound("invite not found");
      if (invite.claimed_at !== null)
        throw app.httpErrors.conflict("invite already claimed");
      const now = Date.now();
      if (invite.expires_at < now) throw app.httpErrors.gone("invite expired");

      const deviceId = randomId();
      const deviceToken = randomToken();
      const deviceTokenHash = await hashToken(deviceToken);

      const tx = app.deps.repo.db.transaction(() => {
        app.deps.repo.invites.markClaimed(tokenHash, now);
        app.deps.repo.memberships.create({
          user_id: invite.user_id,
          device_id: deviceId,
          device_token_hash: deviceTokenHash,
          device_label,
          created_at: now,
          revoked_at: null,
        });
      });
      tx();

      return reply.send({
        device_token: deviceToken,
        user_id: invite.user_id,
        device_id: deviceId,
      });
    }
  );
};
```

- [ ] **Step 4: Wire the route into the app**

In `sharepaste/src/server/app.ts`, after the `/healthz` route, register:
```ts
import { registerClaimInviteRoute } from "./routes/claim-invite.js";
// inside buildApp, after /healthz:
registerClaimInviteRoute(app);
```

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/integration/claim-invite.test.ts
```

Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(invite): POST /claim-invite issues device_token and creates membership"
```

---

## Task 10: `POST /pair/start` (first authed endpoint)

**Files:**
- Create: `sharepaste/src/server/routes/pairing.ts`
- Create: `sharepaste/tests/integration/pairing.test.ts`
- Modify: `sharepaste/src/server/app.ts`
- Modify: `sharepaste/tests/helpers.ts` — add `provisionDevice` helper

- [ ] **Step 1: Add `provisionDevice` to `tests/helpers.ts`**

In `sharepaste/tests/helpers.ts`:

1. Extend the existing `import` from `"../src/crypto.js"` (or add it if not present) so it pulls in `hashToken, randomId, randomToken`.
2. Append the helper at the bottom of the file:

```ts
export interface ProvisionedDevice {
  user_id: string;
  device_id: string;
  device_token: string;
}

export const provisionDevice = async (
  repo: Repository,
  username = "alice"
): Promise<ProvisionedDevice> => {
  const user = repo.users.create({ id: randomId(), username });
  const token = randomToken();
  const device_id = randomId();
  repo.memberships.create({
    user_id: user.id,
    device_id,
    device_token_hash: await hashToken(token),
    device_label: "test",
    created_at: Date.now(),
    revoked_at: null,
  });
  return { user_id: user.id, device_id, device_token: token };
};
```

Required import line (add at top of helpers.ts if not already present):
```ts
import { hashToken, randomId, randomToken } from "../src/crypto.js";
```

- [ ] **Step 2: Write failing test (only `pair/start` cases for now — others come in later tasks)**

`sharepaste/tests/integration/pairing.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";
import { randomToken, sha256Hex } from "../../src/crypto.js";

describe("POST /pair/start", () => {
  it("opens a slot bound to the caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const secret = randomToken();
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: sha256Hex(secret) },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { pair_id: string };
      expect(body.pair_id).toMatch(/^[0-9a-f-]{36}$/);

      const pairing = repo.pairings.find(body.pair_id);
      expect(pairing?.user_id).toBe(a.user_id);
      expect(pairing?.secret_hash).toBe(sha256Hex(secret));
      expect(pairing?.consumed_at).toBeNull();
      expect(pairing?.expires_at).toBeGreaterThan(Date.now());
    } finally {
      await close();
    }
  });

  it("rejects without a token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });

  it("rejects an invalid token", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: "Bearer not-a-real-token" },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 3: Run test and verify it fails**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: FAIL — `/pair/start` returns 404.

- [ ] **Step 4: Implement `pair/start` in pairing.ts**

`sharepaste/src/server/routes/pairing.ts`:
```ts
import type { FastifyInstance } from "fastify";
import { randomId } from "../../crypto.js";
import { verifyBearer } from "../auth.js";

const HEX_64 = { type: "string", pattern: "^[0-9a-fA-F]{64}$" } as const;

export const registerPairingRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { secret_hash: string } }>(
    "/pair/start",
    {
      schema: {
        body: {
          type: "object",
          required: ["secret_hash"],
          additionalProperties: false,
          properties: { secret_hash: HEX_64 },
        },
      },
    },
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const id = randomId();
      const now = Date.now();
      app.deps.repo.pairings.create({
        id,
        user_id: auth.user_id,
        secret_hash: req.body.secret_hash.toLowerCase(),
        encrypted_payload: null,
        claimed_by: null,
        failed_attempts: 0,
        consumed_at: null,
        expires_at: now + app.deps.pairingTtlMs,
      });
      return reply.send({ pair_id: id });
    }
  );
};
```

- [ ] **Step 5: Wire route into app**

Add to `app.ts` inside `buildApp`:
```ts
import { registerPairingRoutes } from "./routes/pairing.js";
// after registerClaimInviteRoute:
registerPairingRoutes(app);
```

- [ ] **Step 6: Run test and verify it passes**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(pair): POST /pair/start opens a 2-min slot bound to the caller's user; provisionDevice helper"
```

---

## Task 11: `POST /pair/claim` and brute-force lockout

**Files:**
- Modify: `sharepaste/src/server/routes/pairing.ts`
- Modify: `sharepaste/tests/integration/pairing.test.ts`

- [ ] **Step 1: Append failing tests to pairing.test.ts**

```ts
describe("POST /pair/claim", () => {
  const startPair = async (app: any, repo: any) => {
    const a = await provisionDevice(repo);
    const secret = randomToken();
    const res = await app.inject({
      method: "POST",
      url: "/pair/start",
      headers: { authorization: `Bearer ${a.device_token}` },
      payload: { secret_hash: sha256Hex(secret) },
    });
    return { ...a, secret, pair_id: res.json().pair_id as string };
  };

  it("accepts a correct secret_proof and marks the slot claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(200);
      expect(repo.pairings.find(ctx.pair_id)?.claimed_by).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 403 on a wrong secret_proof and increments failed_attempts", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(403);
      expect(repo.pairings.find(ctx.pair_id)?.failed_attempts).toBe(1);
    } finally {
      await close();
    }
  });

  it("burns the slot after 3 wrong attempts", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/pair/claim",
          payload: { pair_id: ctx.pair_id, secret_proof: randomToken() },
        });
      }
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(410);
      expect(repo.pairings.find(ctx.pair_id)?.consumed_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("returns 410 on an expired slot", async () => {
    const { app, repo, close } = await buildTestApp({ pairingTtlMs: 0 });
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      expect(res.statusCode).toBe(410);
    } finally {
      await close();
    }
  });

  it("returns 404 for an unknown pair_id", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: "00000000-0000-0000-0000-000000000000", secret_proof: randomToken() },
      });
      expect(res.statusCode).toBe(404);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: FAIL on the new cases — `/pair/claim` returns 404 for everything.

- [ ] **Step 3: Implement `/pair/claim`**

Append to `sharepaste/src/server/routes/pairing.ts` (inside `registerPairingRoutes`, after `/pair/start`):
```ts
import { sha256Hex, timingSafeEqualHex } from "../../crypto.js";

// add to registerPairingRoutes:
app.post<{ Body: { pair_id: string; secret_proof: string } }>(
  "/pair/claim",
  {
    schema: {
      body: {
        type: "object",
        required: ["pair_id", "secret_proof"],
        additionalProperties: false,
        properties: {
          pair_id: { type: "string", format: "uuid" },
          secret_proof: { type: "string", minLength: 16, maxLength: 256 },
        },
      },
    },
  },
  async (req, reply) => {
    const { pair_id, secret_proof } = req.body;
    const pairing = app.deps.repo.pairings.find(pair_id);
    if (!pairing) throw app.httpErrors.notFound("pair not found");
    const now = Date.now();
    if (
      pairing.consumed_at !== null ||
      pairing.expires_at < now ||
      pairing.failed_attempts >= app.deps.maxPairingFailures
    ) {
      throw app.httpErrors.gone("pair slot unavailable");
    }
    const proofHash = sha256Hex(secret_proof);
    if (!timingSafeEqualHex(proofHash, pairing.secret_hash)) {
      app.deps.repo.pairings.incrementFailed(pair_id);
      const updated = app.deps.repo.pairings.find(pair_id);
      if (updated && updated.failed_attempts >= app.deps.maxPairingFailures) {
        app.deps.repo.pairings.markConsumed(pair_id, now);
      }
      throw app.httpErrors.forbidden("wrong secret");
    }
    app.deps.repo.pairings.setClaimedBy(pair_id, sha256Hex(secret_proof));
    return reply.send({ ok: true });
  }
);
```

Note: `claimed_by` here is set to `sha256Hex(secret_proof)`, which equals `secret_hash`. That's only used as a non-null marker; the schema column type is `TEXT`. We pick this value because it has no privacy cost (already in `secret_hash`) and lets later `/pair/payload` and `/devices` calls re-prove knowledge of the same secret without server-side per-call state changes.

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pair): POST /pair/claim with timing-safe compare and 3-strike lockout"
```

---

## Task 12: `POST /pair/payload`, `GET /pair/payload`, `GET /pair/poll`

**Files:**
- Modify: `sharepaste/src/server/routes/pairing.ts`
- Modify: `sharepaste/tests/integration/pairing.test.ts`

- [ ] **Step 1: Append failing tests**

```ts
describe("/pair/payload (upload + download)", () => {
  it("inviter uploads ciphertext, claimer downloads with correct proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      const cipher = Buffer.from("opaque-pair-payload").toString("base64");
      const up = await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${ctx.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: cipher },
      });
      expect(up.statusCode).toBe(200);

      const down = await app.inject({
        method: "GET",
        url: `/pair/payload?id=${ctx.pair_id}&proof=${ctx.secret}`,
      });
      expect(down.statusCode).toBe(200);
      expect(down.json()).toEqual({ encrypted_payload: cipher });
    } finally {
      await close();
    }
  });

  it("rejects payload upload from a non-inviter token", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const other = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${other.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: "AAAA" },
      });
      expect(res.statusCode).toBe(403);
    } finally {
      await close();
    }
  });

  it("rejects download with wrong proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/payload",
        headers: { authorization: `Bearer ${ctx.device_token}` },
        payload: { pair_id: ctx.pair_id, encrypted_payload: "AAAA" },
      });
      const down = await app.inject({
        method: "GET",
        url: `/pair/payload?id=${ctx.pair_id}&proof=wrongproofvalue1234567890`,
      });
      expect(down.statusCode).toBe(403);
    } finally {
      await close();
    }
  });
});

describe("GET /pair/poll", () => {
  it("returns claimed status once the slot is claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      await app.inject({
        method: "POST",
        url: "/pair/claim",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
      });
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "claimed" });
    } finally {
      await close();
    }
  });

  it("returns waiting before claim", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}&timeout_ms=10`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "waiting" });
    } finally {
      await close();
    }
  });

  it("returns expired/consumed states accordingly", async () => {
    const { app, repo, close } = await buildTestApp({ pairingTtlMs: 0 });
    try {
      const ctx = await startPair(app, repo);
      const res = await app.inject({
        method: "GET",
        url: `/pair/poll?id=${ctx.pair_id}`,
        headers: { authorization: `Bearer ${ctx.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: "expired" });
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: FAIL — payload/poll routes return 404.

- [ ] **Step 3: Implement payload upload + download + poll**

Append to `registerPairingRoutes` in `pairing.ts`:
```ts
import { setTimeout as sleep } from "node:timers/promises";

// POST /pair/payload (inviter uploads ciphertext)
app.post<{ Body: { pair_id: string; encrypted_payload: string } }>(
  "/pair/payload",
  {
    schema: {
      body: {
        type: "object",
        required: ["pair_id", "encrypted_payload"],
        additionalProperties: false,
        properties: {
          pair_id: { type: "string", format: "uuid" },
          encrypted_payload: { type: "string", minLength: 1, maxLength: 16 * 1024 },
        },
      },
    },
  },
  async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const pairing = app.deps.repo.pairings.find(req.body.pair_id);
    if (!pairing) throw app.httpErrors.notFound("pair not found");
    if (pairing.user_id !== auth.user_id)
      throw app.httpErrors.forbidden("not the inviter");
    const now = Date.now();
    if (pairing.consumed_at !== null || pairing.expires_at < now)
      throw app.httpErrors.gone("pair slot unavailable");

    const buf = Buffer.from(req.body.encrypted_payload, "base64");
    app.deps.repo.pairings.setPayload(pairing.id, buf);
    return reply.send({ ok: true });
  }
);

// GET /pair/payload?id&proof (claimer downloads)
app.get<{ Querystring: { id?: string; proof?: string } }>(
  "/pair/payload",
  async (req, reply) => {
    const id = req.query.id;
    const proof = req.query.proof;
    if (!id || !proof) throw app.httpErrors.badRequest("missing id or proof");
    const pairing = app.deps.repo.pairings.find(id);
    if (!pairing) throw app.httpErrors.notFound("pair not found");
    const now = Date.now();
    if (pairing.consumed_at !== null || pairing.expires_at < now)
      throw app.httpErrors.gone("pair slot unavailable");
    if (!timingSafeEqualHex(sha256Hex(proof), pairing.secret_hash))
      throw app.httpErrors.forbidden("wrong secret");
    if (!pairing.encrypted_payload)
      throw app.httpErrors.notFound("payload not yet uploaded");
    return reply.send({
      encrypted_payload: pairing.encrypted_payload.toString("base64"),
    });
  }
);

// GET /pair/poll?id (long-poll, but bounded for tests by timeout_ms)
app.get<{ Querystring: { id?: string; timeout_ms?: string } }>(
  "/pair/poll",
  async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const id = req.query.id;
    if (!id) throw app.httpErrors.badRequest("missing id");
    const timeoutMs = Math.min(
      Number(req.query.timeout_ms ?? 25_000) || 25_000,
      30_000
    );
    const deadline = Date.now() + timeoutMs;
    while (true) {
      const pairing = app.deps.repo.pairings.find(id);
      if (!pairing) throw app.httpErrors.notFound("pair not found");
      if (pairing.user_id !== auth.user_id)
        throw app.httpErrors.forbidden("not the inviter");
      const now = Date.now();
      if (pairing.consumed_at !== null) return reply.send({ status: "consumed" });
      if (pairing.expires_at < now) return reply.send({ status: "expired" });
      if (pairing.claimed_by !== null) return reply.send({ status: "claimed" });
      if (Date.now() >= deadline) return reply.send({ status: "waiting" });
      await sleep(Math.min(250, deadline - Date.now()));
    }
  }
);
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/integration/pairing.test.ts
```

Expected: PASS (14 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pair): payload upload/download and long-poll endpoints"
```

---

## Task 13: `POST /devices` and `DELETE /devices/:id`

**Files:**
- Create: `sharepaste/src/server/routes/devices.ts`
- Create: `sharepaste/tests/integration/devices.test.ts`
- Modify: `sharepaste/src/server/app.ts`

- [ ] **Step 1: Write failing tests**

`sharepaste/tests/integration/devices.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";
import {
  randomToken,
  sha256Hex,
} from "../../src/crypto.js";

describe("POST /devices", () => {
  const startAndClaim = async (app: any, repo: any) => {
    const a = await provisionDevice(repo);
    const secret = randomToken();
    const start = await app.inject({
      method: "POST",
      url: "/pair/start",
      headers: { authorization: `Bearer ${a.device_token}` },
      payload: { secret_hash: sha256Hex(secret) },
    });
    const pair_id = start.json().pair_id as string;
    await app.inject({
      method: "POST",
      url: "/pair/claim",
      payload: { pair_id, secret_proof: secret },
    });
    return { ...a, secret, pair_id };
  };

  it("issues a device_token and consumes the slot", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "ipad" },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { device_token: string; device_id: string };
      expect(body.device_token).toMatch(/^[A-Za-z0-9_-]{43}$/);

      const slot = repo.pairings.find(ctx.pair_id);
      expect(slot?.consumed_at).not.toBeNull();
      expect(slot?.encrypted_payload).toBeNull();

      const mem = repo.memberships.findByDeviceId(ctx.user_id, body.device_id);
      expect(mem?.device_label).toBe("ipad");
    } finally {
      await close();
    }
  });

  it("returns 410 if called twice on the same slot", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startAndClaim(app, repo);
      await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "a" },
      });
      const res2 = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "b" },
      });
      expect(res2.statusCode).toBe(410);
    } finally {
      await close();
    }
  });

  it("returns 403 with a wrong secret_proof", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: randomToken(), label: "x" },
      });
      expect(res.statusCode).toBe(403);
    } finally {
      await close();
    }
  });

  it("returns 409 if the slot was never claimed", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const secret = randomToken();
      const start = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: sha256Hex(secret) },
      });
      const pair_id = start.json().pair_id as string;
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id, secret_proof: secret, label: "x" },
      });
      expect(res.statusCode).toBe(409);
    } finally {
      await close();
    }
  });
});

describe("DELETE /devices/:id", () => {
  it("revokes a sibling device of the same user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "alice2");
      // Provision a sibling for user a
      const sibling = await provisionDevice(repo);
      // Adjust user_id of sibling to match a:
      // (We'll just create memberships directly via the helper, so we need a same-user sibling.)
      // Use a fresh helper: create a second membership under a.user_id.
      const secondToken = randomToken();
      const secondId = (await import("node:crypto")).randomUUID();
      const { hashToken } = await import("../../src/crypto.js");
      repo.memberships.create({
        user_id: a.user_id,
        device_id: secondId,
        device_token_hash: await hashToken(secondToken),
        device_label: "second",
        created_at: Date.now(),
        revoked_at: null,
      });

      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${secondId}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);

      const mem = repo.memberships.findByDeviceId(a.user_id, secondId);
      expect(mem?.revoked_at).not.toBeNull();
    } finally {
      await close();
    }
  });

  it("a revoked token can no longer be used", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${a.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      const res2 = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res2.statusCode).toBe(401);
    } finally {
      await close();
    }
  });

  it("cannot revoke a device of a different user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const res = await app.inject({
        method: "DELETE",
        url: `/devices/${b.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(404);
      expect(repo.memberships.findByDeviceId(b.user_id, b.device_id)?.revoked_at).toBeNull();
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/devices.test.ts
```

Expected: FAIL — routes don't exist.

- [ ] **Step 3: Implement `devices.ts`**

`sharepaste/src/server/routes/devices.ts`:
```ts
import type { FastifyInstance } from "fastify";
import {
  hashToken,
  randomId,
  randomToken,
  sha256Hex,
  timingSafeEqualHex,
} from "../../crypto.js";
import { verifyBearer } from "../auth.js";

export const registerDeviceRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { pair_id: string; secret_proof: string; label: string } }>(
    "/devices",
    {
      schema: {
        body: {
          type: "object",
          required: ["pair_id", "secret_proof", "label"],
          additionalProperties: false,
          properties: {
            pair_id: { type: "string", format: "uuid" },
            secret_proof: { type: "string", minLength: 16, maxLength: 256 },
            label: { type: "string", minLength: 1, maxLength: 128 },
          },
        },
      },
    },
    async (req, reply) => {
      const { pair_id, secret_proof, label } = req.body;
      const pairing = app.deps.repo.pairings.find(pair_id);
      if (!pairing) throw app.httpErrors.notFound("pair not found");
      const now = Date.now();
      if (pairing.consumed_at !== null) throw app.httpErrors.gone("pair slot consumed");
      if (pairing.expires_at < now) throw app.httpErrors.gone("pair slot expired");
      if (pairing.claimed_by === null) throw app.httpErrors.conflict("pair slot not claimed");
      if (!timingSafeEqualHex(sha256Hex(secret_proof), pairing.secret_hash))
        throw app.httpErrors.forbidden("wrong secret");

      const deviceId = randomId();
      const token = randomToken();
      const tokenHash = await hashToken(token);

      const tx = app.deps.repo.db.transaction(() => {
        app.deps.repo.memberships.create({
          user_id: pairing.user_id,
          device_id: deviceId,
          device_token_hash: tokenHash,
          device_label: label,
          created_at: now,
          revoked_at: null,
        });
        app.deps.repo.pairings.markConsumed(pair_id, now);
      });
      tx();

      return reply.send({ device_token: token, device_id: deviceId, user_id: pairing.user_id });
    }
  );

  app.delete<{ Params: { id: string } }>(
    "/devices/:id",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const target = app.deps.repo.memberships.findByDeviceId(auth.user_id, req.params.id);
      if (!target) throw app.httpErrors.notFound("device not found for this user");
      app.deps.repo.memberships.revoke(auth.user_id, req.params.id, Date.now());
      return reply.send({ ok: true });
    }
  );
};
```

- [ ] **Step 4: Wire route into app**

```ts
import { registerDeviceRoutes } from "./routes/devices.js";
// after registerPairingRoutes:
registerDeviceRoutes(app);
```

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/integration/devices.test.ts
```

Expected: PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(devices): POST /devices issues token + consumes slot; DELETE /devices/:id revokes"
```

---

## Task 14: `POST /entries` with inline retention pruning

**Files:**
- Create: `sharepaste/src/server/routes/entries.ts`
- Create: `sharepaste/tests/integration/entries.test.ts`
- Modify: `sharepaste/src/server/app.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/integration/entries.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";

const cipherB64 = (s: string) => Buffer.from(s).toString("base64");

describe("POST /entries", () => {
  it("stores ciphertext and returns id + created_at", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("opaque") },
      });
      expect(res.statusCode).toBe(200);
      const body = res.json() as { id: number; created_at: number };
      expect(typeof body.id).toBe("number");
      expect(body.created_at).toBeLessThanOrEqual(Date.now());
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    } finally {
      await close();
    }
  });

  it("rejects oversized ciphertext", async () => {
    const { app, repo, close } = await buildTestApp({ maxEntryBytes: 16 });
    try {
      const a = await provisionDevice(repo);
      const big = cipherB64("x".repeat(64));
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: big },
      });
      expect(res.statusCode).toBe(413);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });

  it("prunes the oldest entry past the count cap (inline)", async () => {
    const { app, repo, close } = await buildTestApp({ maxEntries: 2 });
    try {
      const a = await provisionDevice(repo);
      for (let i = 0; i < 5; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
      }
      expect(repo.entries.countForUser(a.user_id)).toBe(2);
    } finally {
      await close();
    }
  });

  it("rejects without auth", async () => {
    const { app, close } = await buildTestApp();
    try {
      const res = await app.inject({
        method: "POST",
        url: "/entries",
        payload: { ciphertext: cipherB64("hi") },
      });
      expect(res.statusCode).toBe(401);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/entries.test.ts
```

Expected: FAIL — entries route missing.

- [ ] **Step 3: Implement `entries.ts`**

`sharepaste/src/server/routes/entries.ts`:
```ts
import type { FastifyInstance } from "fastify";
import { verifyBearer } from "../auth.js";

export const registerEntryRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { ciphertext: string } }>(
    "/entries",
    {
      schema: {
        body: {
          type: "object",
          required: ["ciphertext"],
          additionalProperties: false,
          properties: { ciphertext: { type: "string", minLength: 1 } },
        },
      },
    },
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const buf = Buffer.from(req.body.ciphertext, "base64");
      if (buf.length === 0) throw app.httpErrors.badRequest("empty ciphertext");
      if (buf.length > app.deps.maxEntryBytes)
        throw app.httpErrors.payloadTooLarge("ciphertext exceeds maxEntryBytes");
      const now = Date.now();
      const row = app.deps.repo.entries.insertAndPrune(
        {
          user_id: auth.user_id,
          device_id: auth.device_id,
          ciphertext: buf,
          size: buf.length,
          created_at: now,
        },
        app.deps.maxEntries,
        app.deps.maxEntryAgeMs
      );
      app.deps.hub.publish(auth.user_id, {
        type: "entry",
        id: row.id,
        ciphertext: buf.toString("base64"),
        created_at: row.created_at,
        device_id: auth.device_id,
      });
      return reply.send({ id: row.id, created_at: row.created_at });
    }
  );
};
```

- [ ] **Step 4: Wire into app**

```ts
import { registerEntryRoutes } from "./routes/entries.js";
// after registerDeviceRoutes:
registerEntryRoutes(app);
```

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/integration/entries.test.ts
```

Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(entries): POST /entries with inline retention and SSE publish"
```

---

## Task 15: `GET /entries`, `DELETE /entries/:id`, `DELETE /entries`

**Files:**
- Modify: `sharepaste/src/server/routes/entries.ts`
- Modify: `sharepaste/tests/integration/entries.test.ts`

- [ ] **Step 1: Append failing tests**

```ts
describe("GET /entries", () => {
  it("returns entries since a given id, scoped to caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("a" + i) },
        });
      }
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: cipherB64("b") },
      });

      const res = await app.inject({
        method: "GET",
        url: "/entries?since=0&limit=100",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number; ciphertext: string }>;
      expect(list).toHaveLength(3);
      expect(list[0].id).toBeLessThan(list[2].id);
    } finally {
      await close();
    }
  });

  it("respects since pagination", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const ids: number[] = [];
      for (let i = 0; i < 5; i++) {
        const r = await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64("x" + i) },
        });
        ids.push((r.json() as { id: number }).id);
      }
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=${ids[2]}&limit=100`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      const list = res.json() as Array<{ id: number }>;
      expect(list.map((e) => e.id)).toEqual(ids.slice(3));
    } finally {
      await close();
    }
  });
});

describe("DELETE /entries/:id and DELETE /entries", () => {
  it("removes a single entry for the caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const id = (r.json() as { id: number }).id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });

  it("cannot delete another user's entry", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");
      const r = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: cipherB64("hi") },
      });
      const id = (r.json() as { id: number }).id;
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${id}`,
        headers: { authorization: `Bearer ${b.device_token}` },
      });
      expect(del.statusCode).toBe(404);
      expect(repo.entries.countForUser(a.user_id)).toBe(1);
    } finally {
      await close();
    }
  });

  it("DELETE /entries purges all of caller's user", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      for (let i = 0; i < 3; i++) {
        await app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipherB64(String(i)) },
        });
      }
      const del = await app.inject({
        method: "DELETE",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(200);
      expect(repo.entries.countForUser(a.user_id)).toBe(0);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/entries.test.ts
```

Expected: FAIL on the new cases — list/delete routes missing.

- [ ] **Step 3: Implement listing and deletion**

Append to `entries.ts` inside `registerEntryRoutes`:
```ts
app.get<{ Querystring: { since?: string; limit?: string } }>(
  "/entries",
  async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const since = Number(req.query.since ?? 0) || 0;
    const limit = Math.min(Number(req.query.limit ?? 100) || 100, 500);
    const rows = app.deps.repo.entries.listSince(auth.user_id, since, limit);
    return reply.send(
      rows.map((r) => ({
        id: r.id,
        ciphertext: r.ciphertext.toString("base64"),
        created_at: r.created_at,
        device_id: r.device_id,
      }))
    );
  }
);

app.delete<{ Params: { id: string } }>(
  "/entries/:id",
  async (req, reply) => {
    const auth = await verifyBearer(app, req);
    const id = Number(req.params.id);
    if (!Number.isInteger(id) || id <= 0)
      throw app.httpErrors.badRequest("bad id");
    const changed = app.deps.repo.entries.delete(auth.user_id, id);
    if (changed === 0) throw app.httpErrors.notFound("entry not found");
    app.deps.hub.publish(auth.user_id, { type: "delete", id });
    return reply.send({ ok: true });
  }
);

app.delete("/entries", async (req, reply) => {
  const auth = await verifyBearer(app, req);
  const removed = app.deps.repo.entries.deleteAll(auth.user_id);
  return reply.send({ removed });
});
```

- [ ] **Step 4: Run test and verify it passes**

```bash
npm test -- tests/integration/entries.test.ts
```

Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(entries): GET listing with since/limit and per-id/all delete endpoints"
```

---

## Task 16: `GET /events` (SSE)

**Files:**
- Create: `sharepaste/src/server/routes/events.ts`
- Create: `sharepaste/tests/integration/events-sse.test.ts`
- Modify: `sharepaste/src/server/app.ts`
- Modify: `sharepaste/tests/helpers.ts` to optionally start a real listening server (SSE under `.inject` is awkward).

- [ ] **Step 1: Augment helpers.ts**

Replace the `buildTestApp` definition with:
```ts
export interface TestApp {
  app: FastifyInstance;
  repo: Repository;
  hub: SseHub;
  baseUrl: string;
  close: () => Promise<void>;
}

export const buildTestApp = async (
  overrides: Partial<AppDeps> = {},
  opts: { listen?: boolean } = {}
): Promise<TestApp> => {
  const dir = mkdtempSync(path.join(tmpdir(), "sp-test-"));
  const db = openDb(path.join(dir, "t.sqlite"));
  migrate(db);
  const repo = new Repository(db);
  const hub = new SseHub();
  const deps: AppDeps = {
    repo,
    hub,
    pairingTtlMs: 2 * 60 * 1000,
    maxEntries: 100,
    maxEntryAgeMs: 30 * 24 * 60 * 60 * 1000,
    maxEntryBytes: 64 * 1024,
    maxPairingFailures: 3,
    logger: false,
    ...overrides,
  };
  const app = await buildApp(deps);
  await app.ready();
  let baseUrl = "http://inject";
  if (opts.listen) {
    const address = await app.listen({ port: 0, host: "127.0.0.1" });
    baseUrl = address;
  }
  return {
    app,
    repo,
    hub,
    baseUrl,
    close: async () => {
      await app.close();
      db.close();
      rmSync(dir, { recursive: true, force: true });
    },
  };
};
```

- [ ] **Step 2: Write failing SSE test**

`sharepaste/tests/integration/events-sse.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";

describe("GET /events (SSE)", () => {
  it("streams entry events for the caller's user only", async () => {
    const { app, repo, baseUrl, close } = await buildTestApp({}, { listen: true });
    try {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");

      const ctrl = new AbortController();
      const resPromise = fetch(`${baseUrl}/events`, {
        headers: { authorization: `Bearer ${a.device_token}`, accept: "text/event-stream" },
        signal: ctrl.signal,
      });
      const res = await resPromise;
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toMatch(/text\/event-stream/);

      const reader = res.body!.getReader();
      const decoder = new TextDecoder();
      const received: string[] = [];

      const readSome = async (until: (chunks: string) => boolean, timeoutMs = 2000) => {
        const start = Date.now();
        let buf = "";
        while (Date.now() - start < timeoutMs) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value);
          received.push(buf);
          if (until(buf)) return buf;
        }
        return buf;
      };

      // post one entry as user b — should NOT arrive
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: Buffer.from("nope").toString("base64") },
      });

      // post one entry as user a — should arrive
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: Buffer.from("hi").toString("base64") },
      });

      const buf = await readSome((b) => b.includes("event: entry"));
      expect(buf).toMatch(/event: entry/);
      expect(buf).not.toMatch(/nope/);

      ctrl.abort();
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 3: Run test and verify it fails**

```bash
npm test -- tests/integration/events-sse.test.ts
```

Expected: FAIL — `/events` returns 404.

- [ ] **Step 4: Implement SSE route**

`sharepaste/src/server/routes/events.ts`:
```ts
import type { FastifyInstance } from "fastify";
import { verifyBearer } from "../auth.js";

export const registerEventRoutes = (app: FastifyInstance): void => {
  app.get("/events", async (req, reply) => {
    const auth = await verifyBearer(app, req);
    reply.raw.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
      "X-Accel-Buffering": "no",
    });
    reply.raw.write(`: connected\n\n`);

    const heartbeat = setInterval(() => {
      reply.raw.write(`: heartbeat\n\n`);
    }, 15_000);

    const unsub = app.deps.hub.subscribe(auth.user_id, (event) => {
      reply.raw.write(`event: ${event.type}\n`);
      reply.raw.write(`data: ${JSON.stringify(event)}\n\n`);
    });

    req.raw.on("close", () => {
      clearInterval(heartbeat);
      unsub();
      try { reply.raw.end(); } catch {}
    });

    return reply;
  });
};
```

- [ ] **Step 5: Wire into app**

```ts
import { registerEventRoutes } from "./routes/events.js";
// after registerEntryRoutes:
registerEventRoutes(app);
```

- [ ] **Step 6: Run test and verify it passes**

```bash
npm test -- tests/integration/events-sse.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(events): GET /events SSE stream scoped to authed user"
```

---

## Task 17: Multi-tenancy isolation integration test

**Files:**
- Create: `sharepaste/tests/integration/isolation.test.ts`

- [ ] **Step 1: Write the integration test**

`sharepaste/tests/integration/isolation.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { buildTestApp, provisionDevice } from "../helpers.js";

const cipherB64 = (s: string) => Buffer.from(s).toString("base64");

describe("multi-user isolation", () => {
  it("user A cannot see, list, or delete user B entries", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo, "alice");
      const b = await provisionDevice(repo, "bob");

      const post = await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: cipherB64("bob-secret") },
      });
      const bId = (post.json() as { id: number }).id;

      // GET as A: empty
      const list = await app.inject({
        method: "GET",
        url: "/entries?since=0",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(list.json()).toEqual([]);

      // DELETE B's entry as A: 404
      const del = await app.inject({
        method: "DELETE",
        url: `/entries/${bId}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(del.statusCode).toBe(404);

      // DELETE all as A: B's row survives
      await app.inject({
        method: "DELETE",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(repo.entries.countForUser(b.user_id)).toBe(1);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it passes (no impl change needed if Tasks 14–15 are correct)**

```bash
npm test -- tests/integration/isolation.test.ts
```

Expected: PASS. If it fails, the failure indicates a real isolation bug — fix the route before continuing.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test(isolation): cross-user reads/writes/deletes are forbidden"
```

---

## Task 18: Concurrent uploads test

**Files:**
- Modify: `sharepaste/tests/integration/entries.test.ts`

- [ ] **Step 1: Append failing test**

```ts
describe("concurrent uploads", () => {
  it("two devices uploading at once each get a distinct id", async () => {
    const { app, repo, close } = await buildTestApp();
    try {
      const a = await provisionDevice(repo);
      // second device for same user
      const { hashToken, randomId, randomToken } = await import("../../src/crypto.js");
      const t2 = randomToken();
      const d2 = randomId();
      repo.memberships.create({
        user_id: a.user_id,
        device_id: d2,
        device_token_hash: await hashToken(t2),
        device_label: "two",
        created_at: Date.now(),
        revoked_at: null,
      });

      const cipher = Buffer.from("x").toString("base64");
      const [r1, r2] = await Promise.all([
        app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${a.device_token}` },
          payload: { ciphertext: cipher },
        }),
        app.inject({
          method: "POST",
          url: "/entries",
          headers: { authorization: `Bearer ${t2}` },
          payload: { ciphertext: cipher },
        }),
      ]);
      const id1 = (r1.json() as { id: number }).id;
      const id2 = (r2.json() as { id: number }).id;
      expect(id1).not.toBe(id2);
    } finally {
      await close();
    }
  });
});
```

- [ ] **Step 2: Run test and verify it passes (AUTOINCREMENT guarantees this)**

```bash
npm test -- tests/integration/entries.test.ts
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test(entries): concurrent uploads get distinct ids"
```

---

## Task 19: CLI — `user create | list | delete`

**Files:**
- Create: `sharepaste/src/cli/user.ts`
- Modify: `sharepaste/src/index.ts`
- Create: `sharepaste/tests/integration/cli.test.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/integration/cli.test.ts`:
```ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { openDb } from "../../src/db/index.js";
import { migrate } from "../../src/db/migrate.js";
import { Repository } from "../../src/db/repository.js";
import { runUserCreate, runUserList, runUserDelete } from "../../src/cli/user.js";
import { sha256Hex } from "../../src/crypto.js";

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
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/cli.test.ts
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `cli/user.ts`**

`sharepaste/src/cli/user.ts`:
```ts
import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";
import { randomId, randomToken, sha256Hex } from "../crypto.js";

export interface CreateUserArgs {
  dbPath: string;
  username: string;
  ttlSeconds: number;
}

export interface CreateUserResult {
  user_id: string;
  invite_token: string;
}

const open = (dbPath: string) => {
  const db = openDb(dbPath);
  migrate(db);
  return new Repository(db);
};

export const runUserCreate = (args: CreateUserArgs): CreateUserResult => {
  const repo = open(args.dbPath);
  const id = randomId();
  repo.users.create({ id, username: args.username });
  const token = randomToken();
  repo.invites.create({
    token_hash: sha256Hex(token),
    user_id: id,
    expires_at: Date.now() + args.ttlSeconds * 1000,
    claimed_at: null,
  });
  return { user_id: id, invite_token: token };
};

export const runUserList = (args: { dbPath: string }) => open(args.dbPath).users.list();

export const runUserDelete = (args: { dbPath: string; userId: string }) => {
  const repo = open(args.dbPath);
  repo.users.delete(args.userId);
};
```

- [ ] **Step 4: Wire CLI subcommands in `index.ts`**

Replace the contents of `sharepaste/src/index.ts`:
```ts
#!/usr/bin/env node
import { Command } from "commander";
import { runUserCreate, runUserList, runUserDelete } from "./cli/user.js";

const program = new Command()
  .name("sharepaste")
  .description("Sharepaste server and operator CLI")
  .version("0.1.0");

const dbPathOption = (cmd: Command) =>
  cmd.option(
    "--db <path>",
    "path to SQLite database file",
    process.env.DB_PATH ?? "/var/lib/sharepaste/sharepaste.sqlite"
  );

const user = program.command("user").description("User management");

dbPathOption(user.command("create"))
  .argument("<username>")
  .option("--ttl <seconds>", "invite TTL", "604800")
  .action(async (username: string, opts: { db: string; ttl: string }) => {
    const result = runUserCreate({
      dbPath: opts.db,
      username,
      ttlSeconds: Number(opts.ttl),
    });
    console.log(JSON.stringify(result, null, 2));
  });

dbPathOption(user.command("list")).action((opts: { db: string }) => {
  const rows = runUserList({ dbPath: opts.db });
  console.log(JSON.stringify(rows, null, 2));
});

dbPathOption(user.command("delete"))
  .argument("<user_id>")
  .action((userId: string, opts: { db: string }) => {
    runUserDelete({ dbPath: opts.db, userId });
  });

program.command("serve").description("Run HTTP server").action(() => {
  console.log("serve: not implemented yet");
});

program.parseAsync(process.argv);
```

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/integration/cli.test.ts
```

Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(cli): user create/list/delete subcommands"
```

---

## Task 20: CLI — `device list | revoke` and `entry purge`

**Files:**
- Create: `sharepaste/src/cli/device.ts`
- Create: `sharepaste/src/cli/entry.ts`
- Modify: `sharepaste/src/index.ts`
- Modify: `sharepaste/tests/integration/cli.test.ts`

- [ ] **Step 1: Append failing tests**

```ts
import {
  runDeviceList,
  runDeviceRevoke,
} from "../../src/cli/device.js";
import { runEntryPurge } from "../../src/cli/entry.js";
import { hashToken, randomToken, randomId } from "../../src/crypto.js";

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
    const b = runUserCreate({ dbPath, username: "bob", ttlSeconds: 60 });
    const da = await seedMembership(a.user_id);
    const db = await seedMembership(b.user_id);
    repo.entries.insertAndPrune(
      { user_id: a.user_id, device_id: da, ciphertext: Buffer.from("a"), size: 1, created_at: Date.now() },
      100,
      30 * 24 * 3600 * 1000
    );
    repo.entries.insertAndPrune(
      { user_id: b.user_id, device_id: db, ciphertext: Buffer.from("b"), size: 1, created_at: Date.now() },
      100,
      30 * 24 * 3600 * 1000
    );
    runEntryPurge({ dbPath, userId: a.user_id });
    expect(repo.entries.countForUser(a.user_id)).toBe(0);
    expect(repo.entries.countForUser(b.user_id)).toBe(1);
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/cli.test.ts
```

Expected: FAIL — modules missing.

- [ ] **Step 3: Implement device and entry CLI**

`sharepaste/src/cli/device.ts`:
```ts
import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";

const open = (dbPath: string) => {
  const db = openDb(dbPath);
  migrate(db);
  return new Repository(db);
};

export const runDeviceList = (args: { dbPath: string }) =>
  open(args.dbPath).memberships.listAll();

export const runDeviceRevoke = (args: { dbPath: string; deviceId: string }): void => {
  const repo = open(args.dbPath);
  const all = repo.memberships.listAll().filter((m) => m.device_id === args.deviceId);
  if (all.length === 0) throw new Error(`device ${args.deviceId} not found`);
  for (const m of all) {
    repo.memberships.revoke(m.user_id, m.device_id, Date.now());
  }
};
```

`sharepaste/src/cli/entry.ts`:
```ts
import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";

export const runEntryPurge = (args: { dbPath: string; userId: string }): number => {
  const db = openDb(args.dbPath);
  migrate(db);
  const repo = new Repository(db);
  return repo.entries.deleteAll(args.userId);
};
```

- [ ] **Step 4: Wire into `src/index.ts`**

Add after the `user` command block:
```ts
import { runDeviceList, runDeviceRevoke } from "./cli/device.js";
import { runEntryPurge } from "./cli/entry.js";

const device = program.command("device").description("Device management");
dbPathOption(device.command("list")).action((opts: { db: string }) => {
  console.log(JSON.stringify(runDeviceList({ dbPath: opts.db }), null, 2));
});
dbPathOption(device.command("revoke"))
  .argument("<device_id>")
  .action((deviceId: string, opts: { db: string }) => {
    runDeviceRevoke({ dbPath: opts.db, deviceId });
  });

const entry = program.command("entry").description("Entry administration");
dbPathOption(entry.command("purge"))
  .requiredOption("--user <user_id>", "user id")
  .action((opts: { db: string; user: string }) => {
    const removed = runEntryPurge({ dbPath: opts.db, userId: opts.user });
    console.log(JSON.stringify({ removed }, null, 2));
  });
```

- [ ] **Step 5: Run test and verify it passes**

```bash
npm test -- tests/integration/cli.test.ts
```

Expected: PASS (6 tests total).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(cli): device list/revoke and entry purge subcommands"
```

---

## Task 21: `sharepaste serve` and config

**Files:**
- Create: `sharepaste/src/config.ts`
- Modify: `sharepaste/src/index.ts`
- Create: `sharepaste/tests/integration/serve.test.ts`

- [ ] **Step 1: Write failing test**

`sharepaste/tests/integration/serve.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { startServer } from "../../src/cli/serve.js";

describe("startServer", () => {
  it("listens, serves /healthz, and shuts down cleanly", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "sp-serve-"));
    const handle = await startServer({
      dbPath: path.join(dir, "t.sqlite"),
      port: 0,
      host: "127.0.0.1",
    });
    try {
      const res = await fetch(`${handle.url}/healthz`);
      expect(res.status).toBe(200);
      expect(await res.json()).toEqual({ ok: true });
    } finally {
      await handle.close();
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
```

- [ ] **Step 2: Run test and verify it fails**

```bash
npm test -- tests/integration/serve.test.ts
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `config.ts`**

`sharepaste/src/config.ts`:
```ts
export interface ServeConfig {
  dbPath: string;
  port: number;
  host: string;
  tlsCertPath: string | null;
  tlsKeyPath: string | null;
}

export const loadServeConfig = (env: NodeJS.ProcessEnv = process.env): ServeConfig => ({
  dbPath: env.DB_PATH ?? "/var/lib/sharepaste/sharepaste.sqlite",
  port: Number(env.PORT ?? 8443),
  host: env.HOST ?? "0.0.0.0",
  tlsCertPath: env.TLS_CERT ?? null,
  tlsKeyPath: env.TLS_KEY ?? null,
});
```

- [ ] **Step 4: Implement `cli/serve.ts`**

`sharepaste/src/cli/serve.ts`:
```ts
import fs from "node:fs";
import { openDb } from "../db/index.js";
import { migrate } from "../db/migrate.js";
import { Repository } from "../db/repository.js";
import { buildApp, type AppDeps } from "../server/app.js";
import { SseHub } from "../server/sse-hub.js";

export interface ServeOptions {
  dbPath: string;
  port: number;
  host: string;
  tlsCertPath?: string | null;
  tlsKeyPath?: string | null;
}

export interface ServerHandle {
  url: string;
  close: () => Promise<void>;
}

export const startServer = async (opts: ServeOptions): Promise<ServerHandle> => {
  const db = openDb(opts.dbPath);
  migrate(db);
  const repo = new Repository(db);
  const hub = new SseHub();
  const deps: AppDeps = {
    repo,
    hub,
    pairingTtlMs: 2 * 60 * 1000,
    maxEntries: 100,
    maxEntryAgeMs: 30 * 24 * 60 * 60 * 1000,
    maxEntryBytes: 64 * 1024,
    maxPairingFailures: 3,
    logger: { level: process.env.LOG_LEVEL ?? "info" },
  };
  const https =
    opts.tlsCertPath && opts.tlsKeyPath
      ? {
          key: fs.readFileSync(opts.tlsKeyPath),
          cert: fs.readFileSync(opts.tlsCertPath),
        }
      : undefined;
  const app = await buildApp(deps);
  if (https) {
    // Reconfigure: Fastify needs https in constructor; rebuild with https set on logger options
    // Simpler: throw a clear error if TLS is requested but not yet supported here.
    throw new Error("TLS not supported in this build; terminate TLS at a reverse proxy");
  }
  const url = await app.listen({ port: opts.port, host: opts.host });
  return {
    url,
    close: async () => {
      await app.close();
      db.close();
    },
  };
};
```

(TLS in-process is intentionally deferred. See `Open work` at the end of this plan; operators run TLS termination via Caddy/nginx in front of the container.)

- [ ] **Step 5: Wire into CLI**

Replace the `serve` command in `index.ts` with:
```ts
import { startServer } from "./cli/serve.js";
import { loadServeConfig } from "./config.js";

program
  .command("serve")
  .description("Run HTTP server")
  .action(async () => {
    const cfg = loadServeConfig();
    const handle = await startServer({
      dbPath: cfg.dbPath,
      port: cfg.port,
      host: cfg.host,
    });
    console.log(`sharepaste serving on ${handle.url}`);
    const onSignal = async () => {
      await handle.close();
      process.exit(0);
    };
    process.on("SIGINT", onSignal);
    process.on("SIGTERM", onSignal);
  });
```

- [ ] **Step 6: Run test and verify it passes**

```bash
npm test -- tests/integration/serve.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(serve): startServer and sharepaste serve command bound to env config"
```

---

## Task 22: Dockerfile

**Files:**
- Create: `sharepaste/Dockerfile`

- [ ] **Step 1: Write Dockerfile**

`sharepaste/Dockerfile`:
```dockerfile
# syntax=docker/dockerfile:1
FROM node:20-bookworm-slim AS build
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 build-essential ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY package.json package-lock.json* ./
RUN npm ci
COPY tsconfig.json ./
COPY src ./src
RUN npm run build
RUN npm prune --omit=dev

FROM node:20-bookworm-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
ENV DB_PATH=/var/lib/sharepaste/sharepaste.sqlite
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && mkdir -p /var/lib/sharepaste \
  && chown -R node:node /var/lib/sharepaste
USER node
COPY --from=build --chown=node:node /app/node_modules ./node_modules
COPY --from=build --chown=node:node /app/dist ./dist
COPY --from=build --chown=node:node /app/package.json ./package.json
VOLUME /var/lib/sharepaste
EXPOSE 8443
ENTRYPOINT ["node", "dist/src/index.js"]
CMD ["serve"]
```

- [ ] **Step 2: Verify the build locally**

```bash
cd /Users/poalrom/private/sharepaste && docker build -t sharepaste:dev .
```

Expected: build succeeds.

- [ ] **Step 3: Smoke test the running container**

```bash
docker run --rm -d --name sp-smoke -p 18443:8443 sharepaste:dev
sleep 1
curl -s http://localhost:18443/healthz
docker stop sp-smoke
```

Expected: `{"ok":true}`.

- [ ] **Step 4: Verify the CLI works in-container**

```bash
docker run --rm -v sp-data:/var/lib/sharepaste sharepaste:dev user create alice --ttl 3600
```

Expected: JSON output with `user_id` and `invite_token`.

Then clean up the volume:
```bash
docker volume rm sp-data
```

- [ ] **Step 5: Commit**

```bash
git add Dockerfile && git commit -m "build: multi-stage Docker image with non-root runtime and writable volume"
```

---

## Task 23: README and operator notes

**Files:**
- Create: `sharepaste/README.md`

- [ ] **Step 1: Write README**

`sharepaste/README.md`:
```markdown
# Sharepaste — Server + CLI

Self-hosted, end-to-end encrypted clipboard sync. Server only sees ciphertext.

## Quick start

```bash
docker run -d --name sharepaste \
  -v sp-data:/var/lib/sharepaste \
  -p 8443:8443 \
  sharepaste:latest
```

Operate behind a reverse proxy that terminates TLS (Caddy, nginx).

## Operator CLI

Run inside the container:

```bash
# Create a user, get a one-time invite token
docker exec sharepaste sharepaste user create alice

# List users
docker exec sharepaste sharepaste user list

# Revoke a stolen device
docker exec sharepaste sharepaste device revoke <device_id>

# Purge a user's history
docker exec sharepaste sharepaste entry purge --user <user_id>
```

The `--db` flag overrides the DB path; default is `/var/lib/sharepaste/sharepaste.sqlite`.

## Wire protocol

See `docs/superpowers/specs/2026-05-01-sharepaste-design.md`. Endpoints:

- `POST /claim-invite`
- `POST /pair/start`, `POST /pair/claim`, `POST /pair/payload`, `GET /pair/payload`, `GET /pair/poll`
- `POST /devices`, `DELETE /devices/:id`
- `POST /entries`, `GET /entries`, `DELETE /entries/:id`, `DELETE /entries`
- `GET /events` (SSE)

All authenticated endpoints take `Authorization: Bearer <device_token>`.

## Tests

```bash
npm test
```

Real Fastify + real SQLite tempfiles. No HTTP mocks.

## Threat model assumptions

- Operator runs HTTPS in front of the container (no in-process TLS in this build).
- Devices use OS disk encryption (FileVault, BitLocker, etc).
- Device-token revocation 401s further requests but does not retroactively un-encrypt entries on a stolen device. Key rotation is out of scope.
```

- [ ] **Step 2: Commit**

```bash
git add README.md && git commit -m "docs: README with quick start, CLI usage, and threat-model notes"
```

---

## Task 24: Final full-suite run and tag

**Files:** none

- [ ] **Step 1: Run the entire test suite**

```bash
cd /Users/poalrom/private/sharepaste && npm test
```

Expected: all suites green. Sample expected counts:
- `tests/unit/*` — db (1), crypto (5), retention-sql (3), sse-hub (2), repository (2)
- `tests/integration/*` — health (1), claim-invite (5), pairing (14), devices (7), entries (8), events-sse (1), isolation (1), cli (6), serve (1)

If any test is red, fix it before proceeding.

- [ ] **Step 2: Build the Docker image one more time as a release smoke**

```bash
docker build -t sharepaste:0.1.0 .
```

- [ ] **Step 3: Tag the release**

```bash
git tag -a v0.1.0 -m "sharepaste server + CLI 0.1.0"
```

- [ ] **Step 4: Final commit-log review**

```bash
git log --oneline
```

Confirm each task above produced one or more commits with conventional prefixes (`feat`, `test`, `docs`, `build`, `chore`).

---

## Out of scope for this plan

- macOS, Windows, Android, iPadOS clients — separate plans.
- In-process TLS termination — operators front the container with a reverse proxy.
- Token-cache / faster auth — argon2 verify per request is fine at self-hosted scale.
- Backups, snapshots, log shipping — operator concern.

---

## Self-review checklist

- **Spec coverage** — every endpoint in the spec API table has a task and integration tests; every table in the data model is created and exercised; inline retention pruning has both a unit test (Task 5) and an end-to-end test (Task 14); brute-force lockout has a dedicated test (Task 11); SSE per-user scoping has a dedicated test (Task 16); multi-tenant isolation has a dedicated test (Task 17); CLI surface has a dedicated test file (Tasks 19–20).
- **Placeholder scan** — no TBD/TODO; every code step shows the actual code; every test step shows the actual assertions.
- **Type consistency** — `Repository`, `AppDeps`, `SseHub`, `provisionDevice`, `verifyBearer`, `startServer` are referenced consistently across tasks; method names (e.g. `entries.insertAndPrune`, `pairings.markConsumed`, `memberships.revoke`) match between repository, routes, and tests.
