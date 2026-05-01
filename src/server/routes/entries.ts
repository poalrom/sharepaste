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
};
