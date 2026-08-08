import { describe, it, expect } from "vitest";
import { provisionDevice, provisionLegacyDevice, withApp } from "../helpers.js";
import { DeviceCredentials } from "../../src/server/device-credentials.js";

describe("bearer authentication", () => {
  it("rejects without a token", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    }));

  it("rejects an invalid token", () =>
    withApp(async ({ app }) => {
      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: "Bearer not-a-real-token" },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    }));

  it("a revoked token can no longer be used", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const revoke = await app.inject({
        method: "DELETE",
        url: `/devices/${a.device_id}`,
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(revoke.statusCode).toBe(200);

      const res = await app.inject({
        method: "POST",
        url: "/pair/start",
        headers: { authorization: `Bearer ${a.device_token}` },
        payload: { secret_hash: "00".repeat(32) },
      });
      expect(res.statusCode).toBe(401);
    }));

  it("authenticates a device via the sha256 index", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      // Nothing awaits the index, so the argon2 scan has no row to walk.
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);

      const res = await app.inject({
        method: "GET",
        url: "/entries?since=0",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);
    }));

  it("authenticates a legacy argon2-only device and backfills the index", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionLegacyDevice(repo);
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(1);

      const res = await app.inject({
        method: "GET",
        url: "/entries?since=0",
        headers: { authorization: `Bearer ${a.device_token}` },
      });
      expect(res.statusCode).toBe(200);
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);
    }));

  it("rejects a token passed as a query parameter", () =>
    withApp(async ({ app, repo }) => {
      const a = await provisionDevice(repo);
      const res = await app.inject({
        method: "GET",
        url: `/entries?since=0&token=${a.device_token}`,
      });
      expect(res.statusCode).toBe(401);
    }));
});
