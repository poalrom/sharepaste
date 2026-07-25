import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { FastifyInstance } from "fastify";
import { buildApp, type AppDeps } from "../src/server/app.js";
import { hashToken, randomId, randomToken, sha256Hex } from "../src/crypto.js";
import { openDb, type Db } from "../src/db/index.js";
import { migrate } from "../src/db/migrate.js";
import { Repository } from "../src/db/repository.js";
import { SseHub } from "../src/server/sse-hub.js";

export interface TempDb {
  dbPath: string;
  db: Db;
  repo: Repository;
  /** Closes the handle *before* removing the directory — Windows refuses otherwise. */
  close: () => void;
}

export const openTempDb = (): TempDb => {
  const dir = mkdtempSync(path.join(tmpdir(), "sp-test-"));
  const dbPath = path.join(dir, "t.sqlite");
  const db = openDb(dbPath);
  migrate(db);
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
    maxEntryBytes: 64 * 1024,
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

export interface ProvisionedDevice {
  user_id: string;
  device_id: string;
  device_token: string;
}

/** Adds a device to an existing user, indexed the way the server now issues them. */
export const addDevice = async (
  repo: Repository,
  user_id: string,
  device_label = "test"
): Promise<ProvisionedDevice> => {
  const device_token = randomToken();
  const device_id = randomId();
  repo.memberships.create({
    user_id,
    device_id,
    device_token_hash: await hashToken(device_token),
    token_sha256: sha256Hex(device_token),
    device_label,
    created_at: Date.now(),
    revoked_at: null,
  });
  return { user_id, device_id, device_token };
};

export const provisionDevice = async (
  repo: Repository,
  username = "alice"
): Promise<ProvisionedDevice> => {
  const user = repo.users.create({ id: randomId(), username });
  return addDevice(repo, user.id);
};

/**
 * A membership as it existed before the sha256 index: argon2 hash only.
 * Exercises the legacy authentication path and its backfill.
 */
export const provisionLegacyDevice = async (
  repo: Repository,
  username = "legacy"
): Promise<ProvisionedDevice> => {
  const user = repo.users.create({ id: randomId(), username });
  const device_token = randomToken();
  const device_id = randomId();
  repo.memberships.create({
    user_id: user.id,
    device_id,
    device_token_hash: await hashToken(device_token),
    token_sha256: null,
    device_label: "legacy",
    created_at: Date.now(),
    revoked_at: null,
  });
  return { user_id: user.id, device_id, device_token };
};

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

/** Provisions an inviter device and opens a pairing slot. */
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

/** `startPair`, then claims the slot with the correct proof. */
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
