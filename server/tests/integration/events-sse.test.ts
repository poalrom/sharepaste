import { describe, it, expect } from "vitest";
import { provisionDevice, withApp } from "../helpers.js";

describe("GET /events (SSE)", () => {
  it("streams entry events for the caller's user only", () =>
    withApp(async ({ app, repo, baseUrl }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");

      const ctrl = new AbortController();
      const resPromise = fetch(`${baseUrl}/events`, {
        headers: { authorization: `Bearer ${a.device_token}`, accept: "text/event-stream" },
        signal: ctrl.signal,
      });
      const res = await resPromise;
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toMatch(/text\/event-stream/);

      const reader = res.body!.getReader();
      const decoder = new TextDecoder();
      const received: string[] = [];

      const readSome = async (until: (chunks: string) => boolean, timeoutMs = 2000) => {
        const start = Date.now();
        let buf = "";
        while (Date.now() - start < timeoutMs) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value);
          received.push(buf);
          if (until(buf)) return buf;
        }
        return buf;
      };

      // post one entry as user b — should NOT arrive
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${b.device_token}` },
        payload: { ciphertext: Buffer.from("nope").toString("base64") },
      });

      // post one entry as user a — should arrive
      await app.inject({
        method: "POST",
        url: "/entries",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { ciphertext: Buffer.from("hi").toString("base64") },
      });

      const buf = await readSome((b) => b.includes("event: entry"));
      expect(buf).toMatch(/event: entry/);
      expect(buf).not.toMatch(/nope/);

      const frame = JSON.parse(/^data: (.*)$/m.exec(buf)![1]!) as Record<string, unknown>;
      expect(frame).toMatchObject({
        type: "entry",
        ciphertext: Buffer.from("hi").toString("base64"),
        device_id: a.device_id,
        seq: 1,
      });
      expect(frame.last_use).toBe(frame.created_at);

      ctrl.abort();
    }, {}, { listen: true }));
});
