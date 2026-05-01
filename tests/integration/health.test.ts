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
