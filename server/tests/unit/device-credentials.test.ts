import { describe, it, expect } from "vitest";
import { DeviceCredentials } from "../../src/server/device-credentials.js";
import { provisionDevice, seedInvite, startAndClaim, withApp } from "../helpers.js";

/**
 * The mint is one function or it is four. Every door that hands out a Device
 * credential — `/claim-invite`, `/devices`, and the fixtures every other test
 * provisions with — goes through `issue`, so a credential from any of them is
 * the same credential: the verify accepts it, and it needs no backfill because
 * the mint wrote the sha256 index. A fifth copy that forgot the index would
 * authenticate once and then be counted here forever.
 */
describe("DeviceCredentials: one mint", () => {
  it("mints through /claim-invite a credential that verifies and needs no backfill", () =>
    withApp(async ({ app, repo }) => {
      const invite = seedInvite(repo);
      const res = await app.inject({
        method: "POST",
        url: "/claim-invite",
        payload: { token: invite, device_label: "macbook" },
      });
      const body = res.json() as { device_token: string; device_id: string; user_id: string };

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${body.device_token}`)
      ).resolves.toEqual({ user_id: body.user_id, device_id: body.device_id });
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);
    }));

  it("mints through /devices a credential that verifies and needs no backfill", () =>
    withApp(async ({ app, repo }) => {
      const ctx = await startAndClaim(app, repo);
      const res = await app.inject({
        method: "POST",
        url: "/devices",
        payload: { pair_id: ctx.pair_id, secret_proof: ctx.secret, label: "ipad" },
      });
      const body = res.json() as { device_token: string; device_id: string; user_id: string };

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${body.device_token}`)
      ).resolves.toEqual({ user_id: body.user_id, device_id: body.device_id });
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);
    }));

  it("mints a fixture credential that verifies and needs no backfill", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const minted = await DeviceCredentials.issue(repo, {
        userId: a.user_id,
        deviceLabel: "ipad",
        now: Date.now(),
      });

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${minted.device_token}`)
      ).resolves.toEqual({ user_id: a.user_id, device_id: minted.device_id });
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);
    }));

  it("commits the write a caller hands it in the mint's own transaction", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      await expect(
        DeviceCredentials.issue(
          repo,
          { userId: a.user_id, deviceLabel: "doomed", now: Date.now() },
          () => {
            throw new Error("the caller's write failed");
          }
        )
      ).rejects.toThrow("the caller's write failed");

      // One Device, not two: the mint rolled back with the write it was given.
      const labels = DeviceCredentials.list(repo, a.user_id).map((d) => d.label);
      expect(labels).not.toContain("doomed");
    }));
});

/**
 * Two paths reach the same membership. The fast one is an indexed lookup; the
 * slow one is the argon2 scan that exists only for memberships issued before
 * the index did, and it indexes the row on the way out so it is walked once.
 */
describe("DeviceCredentials and the bearer verify", () => {
  it("resolves an indexed credential with nothing left to scan", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      // Nothing awaits the index, so the scan has no row to walk: the index is
      // the only thing that can have resolved this.
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${a.device_token}`)
      ).resolves.toEqual({ user_id: a.user_id, device_id: a.device_id });
    }));

  it("scans argon2 for an unindexed credential and indexes it on the way out", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const legacy = await DeviceCredentials.issueUnindexed(repo, {
        userId: a.user_id,
        deviceLabel: "legacy",
        now: Date.now(),
      });
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(1);

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${legacy.device_token}`)
      ).resolves.toEqual({ user_id: a.user_id, device_id: legacy.device_id });
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);

      // And now by the fast path, with nothing left for the scan to walk.
      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${legacy.device_token}`)
      ).resolves.toEqual({ user_id: a.user_id, device_id: legacy.device_id });
    }));

  it("refuses a revoked credential by either path", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const legacy = await DeviceCredentials.issueUnindexed(repo, {
        userId: a.user_id,
        deviceLabel: "legacy",
        now: Date.now(),
      });
      DeviceCredentials.revoke(repo, a.user_id, a.device_id, Date.now());
      DeviceCredentials.revoke(repo, a.user_id, legacy.device_id, Date.now());

      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${a.device_token}`)
      ).rejects.toThrow(expect.objectContaining({ statusCode: 401 }));
      await expect(
        DeviceCredentials.authenticate(repo, `Bearer ${legacy.device_token}`)
      ).rejects.toThrow(expect.objectContaining({ statusCode: 401 }));
      // A revoked row is not a row awaiting the index either — nothing to walk.
      expect(DeviceCredentials.awaitingIndex(repo)).toBe(0);
    }));

  it("refuses a missing, malformed or unknown bearer with 401", () =>
    withApp(async ({ repo }) => {
      for (const header of [undefined, "", "Bearer ", "not-a-scheme abc", "Bearer nope"]) {
        await expect(DeviceCredentials.authenticate(repo, header)).rejects.toThrow(
          expect.objectContaining({ statusCode: 401 })
        );
      }
    }));
});

/**
 * Redaction by construction: the listing a route gets back is already ADR 0001's
 * four fields, so there is no wide row for a handler to spread by accident. The
 * operator listing is the only thing that still carries credential material, and
 * these prove the two differ by exactly that.
 */
describe("DeviceCredentials and the Device listing", () => {
  it("lists ADR 0001's four fields and nothing else", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const [device] = DeviceCredentials.list(repo, a.user_id);

      expect(device).toBeDefined();
      expect(Object.keys(device ?? {}).sort()).toEqual([
        "created_at",
        "device_id",
        "label",
        "revoked_at",
      ]);
    }));

  it("carries none of the credential material the operator listing does", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const [row] = DeviceCredentials.listForOperator(repo);
      expect(row?.device_token_hash.length).toBeGreaterThan(0);
      expect(row?.token_sha256?.length).toBeGreaterThan(0);

      const listed = JSON.stringify(DeviceCredentials.list(repo, a.user_id));
      expect(listed).not.toContain(row?.device_token_hash);
      expect(listed).not.toContain(row?.token_sha256);
      expect(listed).not.toContain(a.device_token);
    }));

  it("keeps a revoked Device listed so an Origin still resolves", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const retired = await DeviceCredentials.issue(repo, {
        userId: a.user_id,
        deviceLabel: "retired",
        now: Date.now(),
      });
      const revokedAt = Date.now();
      DeviceCredentials.revoke(repo, a.user_id, retired.device_id, revokedAt);

      const found = DeviceCredentials.list(repo, a.user_id).find(
        (d) => d.device_id === retired.device_id
      );
      expect(found?.revoked_at).toBe(revokedAt);
    }));

  it("lists only that user's Devices", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");

      expect(DeviceCredentials.list(repo, a.user_id).map((d) => d.device_id)).toEqual([
        a.device_id,
      ]);
      expect(DeviceCredentials.list(repo, b.user_id).map((d) => d.device_id)).toEqual([
        b.device_id,
      ]);
    }));

  it("refuses to revoke a Device that is not this user's", () =>
    withApp(async ({ repo }) => {
      const a = await provisionDevice(repo);
      const b = await provisionDevice(repo, "bob");

      expect(() => DeviceCredentials.revoke(repo, a.user_id, b.device_id, Date.now())).toThrow(
        expect.objectContaining({ statusCode: 404 })
      );
      expect(DeviceCredentials.list(repo, b.user_id)[0]?.revoked_at).toBeNull();
    }));
});
