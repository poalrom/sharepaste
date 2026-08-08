import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { FastifyInstance } from "fastify";
import { buildApp, type AppDeps } from "../src/server/app.js";
import { randomId, randomToken, sha256Hex } from "../src/crypto.js";
import { openDb, type Db } from "../src/db/index.js";
import { migrate } from "../src/db/migrate.js";
import { Repository } from "../src/db/repository.js";
import { DeviceCredentials, type IssuedCredential } from "../src/server/device-credentials.js";
import { entryRules } from "../src/server/refusal.js";
import { SseHub } from "../src/server/sse-hub.js";

export interface TempDb {
  dbPath: string;
  db: Db;
  repo: Repository;
  /** Closes the handle *before* removing the directory — Windows refuses otherwise. */
  close: () => void;
}

/** A temp database with no schema at all — for tests that seed a pre-migration shape. */
export const openRawTempDb = (): TempDb => {
  const dir = mkdtempSync(path.join(tmpdir(), "sp-test-"));
  const dbPath = path.join(dir, "t.sqlite");
  const db = openDb(dbPath);
  return {
    dbPath,
    db,
    repo: new Repository(db),
    close: () => {
      db.close();
      rmSync(dir, { recursive: true, force: true });
    },
  };
};

export const openTempDb = (): TempDb => {
  const t = openRawTempDb();
  migrate(t.db);
  return t;
};

export interface TestApp {
  app: FastifyInstance;
  repo: Repository;
  baseUrl: string;
  close: () => Promise<void>;
}

export const buildTestApp = async (
  overrides: Partial<AppDeps> = {},
  opts: { listen?: boolean } = {}
): Promise<TestApp> => {
  const temp = openTempDb();
  const deps: AppDeps = {
    repo: temp.repo,
    hub: new SseHub(),
    pairingTtlMs: 2 * 60 * 1000,
    maxEntries: 100,
    maxEntryAgeMs: 30 * 24 * 60 * 60 * 1000,
    entryRules: entryRules({ maxEntryBytes: 64 * 1024 }),
    maxPairingFailures: 3,
    logger: false,
    ...overrides,
  };
  const app = await buildApp(deps);
  await app.ready();
  const baseUrl = opts.listen
    ? await app.listen({ port: 0, host: "127.0.0.1" })
    : "http://inject";
  return {
    app,
    repo: temp.repo,
    baseUrl,
    close: async () => {
      await app.close();
      temp.close();
    },
  };
};

/** Builds an app, runs the body, and always tears the app down afterwards. */
export const withApp = async <T>(
  fn: (t: TestApp) => Promise<T>,
  overrides: Partial<AppDeps> = {},
  opts: { listen?: boolean } = {}
): Promise<T> => {
  const t = await buildTestApp(overrides, opts);
  try {
    return await fn(t);
  } finally {
    await t.close();
  }
};

export const cipherB64 = (s: string): string => Buffer.from(s).toString("base64");

export type ProvisionedDevice = IssuedCredential;

/** Adds a Device to an existing user, minted the one way the relay mints one. */
export const addDevice = (
  repo: Repository,
  user_id: string,
  device_label = "test"
): Promise<ProvisionedDevice> =>
  DeviceCredentials.issue(repo, { userId: user_id, deviceLabel: device_label, now: Date.now() });

export const provisionDevice = (
  repo: Repository,
  username = "alice"
): Promise<ProvisionedDevice> =>
  addDevice(repo, repo.users.create({ id: randomId(), username }).id);

/**
 * A membership as it existed before the sha256 index: argon2 hash only. The same
 * mint, stripped of its index — so the fixture cannot drift from what the relay
 * actually issued back then. Exercises the scan and its backfill.
 */
export const provisionLegacyDevice = (
  repo: Repository,
  username = "legacy"
): Promise<ProvisionedDevice> =>
  DeviceCredentials.issueUnindexed(repo, {
    userId: repo.users.create({ id: randomId(), username }).id,
    deviceLabel: "legacy",
    now: Date.now(),
  });

export const seedInvite = (repo: Repository, userId = "u1", username = "alice"): string => {
  repo.users.create({ id: userId, username });
  const token = randomToken();
  repo.invites.create({
    token_hash: sha256Hex(token),
    user_id: userId,
    expires_at: Date.now() + 60_000,
    claimed_at: null,
  });
  return token;
};

export interface PairContext extends ProvisionedDevice {
  secret: string;
  pair_id: string;
}

/** Provisions an inviter device and opens a Pair Slot. */
export const startPair = async (
  app: FastifyInstance,
  repo: Repository
): Promise<PairContext> => {
  const inviter = await provisionDevice(repo);
  const secret = randomToken();
  const res = await app.inject({
    method: "POST",
    url: "/pair/start",
    headers: { authorization: `Bearer ${inviter.device_token}` },
    payload: { secret_hash: sha256Hex(secret) },
  });
  const body = res.json() as { pair_id: string };
  return { ...inviter, secret, pair_id: body.pair_id };
};

/** `startPair`, then claims the Pair Slot with the correct proof. */
export const startAndClaim = async (
  app: FastifyInstance,
  repo: Repository
): Promise<PairContext> => {
  const ctx = await startPair(app, repo);
  await app.inject({
    method: "POST",
    url: "/pair/claim",
    payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret },
  });
  return ctx;
};
