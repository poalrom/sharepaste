import type { FastifyInstance } from "fastify";
import { verifyBearer } from "../auth.js";

export const registerEntryRoutes = (app: FastifyInstance): void => {
  app.post<{ Body: { ciphertext: string } }>(
    "/entries",
    { schema: { body: app.deps.entryRules.bodySchema } },
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const { ciphertext } = req.body;
      const size = app.deps.entryRules.sizeOf(ciphertext);
      const now = Date.now();
      const row = app.deps.repo.entries.insertAndPrune(
        {
          user_id: auth.user_id,
          device_id: auth.device_id,
          ciphertext_b64: ciphertext,
          size,
          created_at: now,
        },
        app.deps.maxEntries,
        app.deps.maxEntryAgeMs
      );
      app.deps.hub.publish(auth.user_id, {
        type: "entry",
        id: row.id,
        ciphertext: row.ciphertext_b64,
        created_at: row.created_at,
        device_id: auth.device_id,
        seq: row.seq,
        last_use: row.last_use,
      });
      return reply.send({
        id: row.id,
        created_at: row.created_at,
        seq: row.seq,
        last_use: row.last_use,
      });
    }
  );

  app.get<{ Querystring: { since?: string; limit?: string } }>(
    "/entries",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      // `since` is a sequence, not an id: a used entry keeps its id and gets a
      // fresh sequence, which is how it comes back past a watermark.
      const since = Number(req.query.since ?? 0) || 0;
      const limit = Math.min(Number(req.query.limit ?? 100) || 100, 500);
      const rows = app.deps.repo.entries.listSince(auth.user_id, since, limit);
      return reply.send(
        rows.map((r) => ({
          id: r.id,
          ciphertext: r.ciphertext_b64,
          created_at: r.created_at,
          device_id: r.device_id,
          seq: r.seq,
          last_use: r.last_use,
        }))
      );
    }
  );

  /**
   * Records a use, which moves the entry to the head of the history everywhere.
   *
   * The fan-out is the ordinary `entry` frame carrying the whole row again: a use
   * is an entry row that changed, so re-sending it says so without a second frame
   * type or a second event kind for every client to learn. `device_id` is the
   * stored Origin, not whoever asked — the relay deliberately keeps no record of
   * which device used what.
   */
  app.post<{ Params: { id: string } }>(
    "/entries/:id/use",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const id = app.deps.entryRules.idFrom(req.params.id);
      const row = app.deps.repo.entries.recordUse(auth.user_id, id, Date.now());
      if (!row) throw app.httpErrors.notFound("entry not found");
      app.deps.hub.publish(auth.user_id, {
        type: "entry",
        id: row.id,
        ciphertext: row.ciphertext_b64,
        created_at: row.created_at,
        device_id: row.device_id,
        seq: row.seq,
        last_use: row.last_use,
      });
      return reply.send({ seq: row.seq, last_use: row.last_use });
    }
  );

  app.delete<{ Params: { id: string } }>(
    "/entries/:id",
    async (req, reply) => {
      const auth = await verifyBearer(app, req);
      const id = app.deps.entryRules.idFrom(req.params.id);
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
};
