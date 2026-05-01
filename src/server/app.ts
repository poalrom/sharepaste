import Fastify, { type FastifyInstance } from "fastify";
import sensible from "@fastify/sensible";
import type { Repository } from "../db/repository.js";
import type { SseHub } from "./sse-hub.js";
import { registerClaimInviteRoute } from "./routes/claim-invite.js";
import { registerDeviceRoutes } from "./routes/devices.js";
import { registerEntryRoutes } from "./routes/entries.js";
import { registerEventRoutes } from "./routes/events.js";
import { registerPairingRoutes } from "./routes/pairing.js";

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
  registerClaimInviteRoute(app);
  registerPairingRoutes(app);
  registerDeviceRoutes(app);
  registerEntryRoutes(app);
  registerEventRoutes(app);

  return app;
};

declare module "fastify" {
  interface FastifyInstance {
    deps: AppDeps;
  }
}
