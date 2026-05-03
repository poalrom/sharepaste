import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import type { FastifyInstance } from "fastify";
import { buildApp, type AppDeps } from "../src/server/app.js";
import { hashToken, randomId, randomToken } from "../src/crypto.js";
import { openDb } from "../src/db/index.js";
import { migrate } from "../src/db/migrate.js";
import { Repository } from "../src/db/repository.js";
import { SseHub } from "../src/server/sse-hub.js";

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
