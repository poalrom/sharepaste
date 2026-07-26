import type { Db } from "./index.js";

export interface UserRow {
  id: string;
  username: string;
  created_at: number;
}

export interface InviteRow {
  token_hash: string;
  user_id: string;
  expires_at: number;
  claimed_at: number | null;
}

export interface MembershipRow {
  user_id: string;
  device_id: string;
  device_token_hash: string;
  /**
   * sha256 of the device token, indexed for O(1) authentication.
   *
   * Null only for memberships created before the sha256 index existed; those
   * fall back to the argon2 scan in `verifyBearer`, which backfills this column
   * on first successful use.
   */
  token_sha256: string | null;
  device_label: string | null;
  created_at: number;
  revoked_at: number | null;
}

export interface PairingRow {
  id: string;
  user_id: string;
  secret_hash: string;
  encrypted_payload: Buffer | null;
  claimed_at: number | null;
  paired_device_label: string | null;
  failed_attempts: number;
  consumed_at: number | null;
  expires_at: number;
}

export interface EntryRow {
  id: number;
  user_id: string;
  device_id: string;
  /** Client ciphertext, stored verbatim as the base64 the client sent. */
  ciphertext_b64: string;
  size: number;
  created_at: number;
}

export class Repository {
  constructor(public readonly db: Db) {}

  readonly users = {
    create: ({ id, username }: { id: string; username: string }): UserRow => {
      const created_at = Date.now();
      this.db
        .prepare("INSERT INTO users (id, username, created_at) VALUES (?, ?, ?)")
        .run(id, username, created_at);
      return { id, username, created_at };
    },
    find: (id: string): UserRow | undefined =>
      this.db.prepare("SELECT * FROM users WHERE id = ?").get(id) as UserRow | undefined,
    list: (): UserRow[] =>
      this.db.prepare("SELECT * FROM users ORDER BY created_at").all() as UserRow[],
    delete: (id: string): void => {
      this.db.prepare("DELETE FROM users WHERE id = ?").run(id);
    },
  };

  readonly invites = {
    create: (row: InviteRow): void => {
      this.db
        .prepare(
          "INSERT INTO invites (token_hash, user_id, expires_at, claimed_at) VALUES (?, ?, ?, ?)"
        )
        .run(row.token_hash, row.user_id, row.expires_at, row.claimed_at);
    },
    findByHash: (hash: string): InviteRow | undefined =>
      this.db.prepare("SELECT * FROM invites WHERE token_hash = ?").get(hash) as
        | InviteRow
        | undefined,
    markClaimed: (hash: string, at: number): void => {
      this.db.prepare("UPDATE invites SET claimed_at = ? WHERE token_hash = ?").run(at, hash);
    },
  };

  readonly memberships = {
    create: (row: MembershipRow): void => {
      this.db
        .prepare(
          `INSERT INTO memberships
           (user_id, device_id, device_token_hash, token_sha256, device_label, created_at, revoked_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.user_id,
          row.device_id,
          row.device_token_hash,
          row.token_sha256,
          row.device_label,
          row.created_at,
          row.revoked_at
        );
    },
    /** Authentication fast path: one indexed lookup, no key derivation. */
    findActiveByTokenSha256: (token_sha256: string): MembershipRow | undefined =>
      this.db
        .prepare(
          "SELECT * FROM memberships WHERE token_sha256 = ? AND revoked_at IS NULL"
        )
        .get(token_sha256) as MembershipRow | undefined,
    setTokenSha256: (user_id: string, device_id: string, token_sha256: string): void => {
      this.db
        .prepare(
          "UPDATE memberships SET token_sha256 = ? WHERE user_id = ? AND device_id = ?"
        )
        .run(token_sha256, user_id, device_id);
    },
    findByDeviceId: (user_id: string, device_id: string): MembershipRow | undefined =>
      this.db
        .prepare("SELECT * FROM memberships WHERE user_id = ? AND device_id = ?")
        .get(user_id, device_id) as MembershipRow | undefined,
    /** Legacy authentication path: rows still awaiting a `token_sha256` backfill. */
    listUnindexed: (): MembershipRow[] =>
      this.db
        .prepare("SELECT * FROM memberships WHERE revoked_at IS NULL AND token_sha256 IS NULL")
        .all() as MembershipRow[],
    /** Every membership of a user, revoked included: old entries still need their Origin resolved. */
    listByUser: (user_id: string): MembershipRow[] =>
      this.db
        .prepare("SELECT * FROM memberships WHERE user_id = ? ORDER BY created_at ASC")
        .all(user_id) as MembershipRow[],
    listAll: (): MembershipRow[] =>
      this.db.prepare("SELECT * FROM memberships").all() as MembershipRow[],
    revoke: (user_id: string, device_id: string, at: number): number =>
      this.db
        .prepare(
          "UPDATE memberships SET revoked_at = ? WHERE user_id = ? AND device_id = ? AND revoked_at IS NULL"
        )
        .run(at, user_id, device_id).changes,
  };

  readonly pairings = {
    create: (row: PairingRow): void => {
      this.db
        .prepare(
          `INSERT INTO pairings
           (id, user_id, secret_hash, encrypted_payload, claimed_at, paired_device_label, failed_attempts, consumed_at, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.id,
          row.user_id,
          row.secret_hash,
          row.encrypted_payload,
          row.claimed_at,
          row.paired_device_label,
          row.failed_attempts,
          row.consumed_at,
          row.expires_at
        );
    },
    find: (id: string): PairingRow | undefined =>
      this.db.prepare("SELECT * FROM pairings WHERE id = ?").get(id) as PairingRow | undefined,
    /** Returns the slot's failed-attempt count after the increment. */
    incrementFailed: (id: string): number => {
      this.db
        .prepare("UPDATE pairings SET failed_attempts = failed_attempts + 1 WHERE id = ?")
        .run(id);
      // `pluck` yields the bare column, so there is no row shape to assert.
      const failed = this.db
        .prepare("SELECT failed_attempts FROM pairings WHERE id = ?")
        .pluck()
        .get(id) as number | undefined;
      return failed ?? 0;
    },
    markClaimed: (id: string, at: number): number =>
      this.db
        .prepare("UPDATE pairings SET claimed_at = ? WHERE id = ? AND claimed_at IS NULL")
        .run(at, id).changes,
    setPayload: (id: string, payload: Buffer): void => {
      this.db.prepare("UPDATE pairings SET encrypted_payload = ? WHERE id = ?").run(payload, id);
    },
    markConsumed: (id: string, at: number, pairedDeviceLabel: string | null = null): void => {
      this.db
        .prepare(
          "UPDATE pairings SET consumed_at = ?, paired_device_label = ?, encrypted_payload = NULL WHERE id = ?"
        )
        .run(at, pairedDeviceLabel, id);
    },
  };

  readonly entries = {
    insertAndPrune: (row: Omit<EntryRow, "id">, maxCount: number, maxAgeMs: number): EntryRow => {
      const tx = this.db.transaction(() => {
        const result = this.db
          .prepare(
            "INSERT INTO entries (user_id, device_id, ciphertext_b64, size, created_at) VALUES (?, ?, ?, ?, ?)"
          )
          .run(row.user_id, row.device_id, row.ciphertext_b64, row.size, row.created_at);
        const id = Number(result.lastInsertRowid);
        this.db
          .prepare(
            `DELETE FROM entries
             WHERE user_id = ?
               AND (
                 created_at < ?
                 OR id NOT IN (
                   SELECT id FROM entries
                   WHERE user_id = ?
                   ORDER BY id DESC
                   LIMIT ?
                 )
               )`
          )
          .run(row.user_id, row.created_at - maxAgeMs, row.user_id, maxCount);
        return { id, ...row };
      });
      return tx();
    },
    listSince: (user_id: string, sinceId: number, limit: number): EntryRow[] =>
      this.db
        .prepare("SELECT * FROM entries WHERE user_id = ? AND id > ? ORDER BY id ASC LIMIT ?")
        .all(user_id, sinceId, limit) as EntryRow[],
    delete: (user_id: string, id: number): number =>
      this.db.prepare("DELETE FROM entries WHERE user_id = ? AND id = ?").run(user_id, id).changes,
    deleteAll: (user_id: string): number =>
      this.db.prepare("DELETE FROM entries WHERE user_id = ?").run(user_id).changes,
    countForUser: (user_id: string): number =>
      this.db
        .prepare("SELECT COUNT(*) FROM entries WHERE user_id = ?")
        .pluck()
        .get(user_id) as number,
  };

  readonly maintenance = {
    /**
     * Drops pairing slots and invites that can never be used again.
     *
     * Entries prune themselves on insert; these two tables otherwise grow without
     * bound, and expired pairing rows keep their `encrypted_payload` blob unless
     * the slot happened to be consumed.
     */
    sweep: (now: number): { pairings: number; invites: number } => {
      const tx = this.db.transaction(() => ({
        pairings: this.db.prepare("DELETE FROM pairings WHERE expires_at < ?").run(now).changes,
        invites: this.db
          .prepare("DELETE FROM invites WHERE expires_at < ? OR claimed_at IS NOT NULL")
          .run(now).changes,
      }));
      return tx();
    },
  };
}
