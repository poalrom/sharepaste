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
