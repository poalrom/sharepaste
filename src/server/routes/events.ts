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
