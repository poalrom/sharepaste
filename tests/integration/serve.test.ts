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
