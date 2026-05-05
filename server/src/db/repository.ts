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
  device_label: string | null;
  created_at: number;
  revoked_at: number | null;
}

export interface PairingRow {
  id: string;
  user_id: string;
  secret_hash: string;
  encrypted_payload: Buffer | null;
  claimed_by: string | null;
  paired_device_label: string | null;
  failed_attempts: number;
  consumed_at: number | null;
  expires_at: number;
}

export interface EntryRow {
  id: number;
  user_id: string;
  device_id: string;
  ciphertext: Buffer;
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
    findById: (id: string): UserRow | undefined =>
      this.db.prepare("SELECT * FROM users WHERE id = ?").get(id) as UserRow | undefined,
    findByUsername: (username: string): UserRow | undefined =>
      this.db.prepare("SELECT * FROM users WHERE username = ?").get(username) as UserRow | undefined,
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
      this.db.prepare("SELECT * FROM invites WHERE token_hash = ?").get(hash) as InviteRow | undefined,
    markClaimed: (hash: string, at: number): void => {
      this.db.prepare("UPDATE invites SET claimed_at = ? WHERE token_hash = ?").run(at, hash);
    },
  };

  readonly memberships = {
    create: (row: MembershipRow): void => {
      this.db
        .prepare(
          `INSERT INTO memberships
           (user_id, device_id, device_token_hash, device_label, created_at, revoked_at)
           VALUES (?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.user_id,
          row.device_id,
          row.device_token_hash,
          row.device_label,
          row.created_at,
          row.revoked_at
        );
    },
    findByDeviceId: (user_id: string, device_id: string): MembershipRow | undefined =>
      this.db
        .prepare("SELECT * FROM memberships WHERE user_id = ? AND device_id = ?")
        .get(user_id, device_id) as MembershipRow | undefined,
    listActive: (): MembershipRow[] =>
      this.db.prepare("SELECT * FROM memberships WHERE revoked_at IS NULL").all() as MembershipRow[],
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
           (id, user_id, secret_hash, encrypted_payload, claimed_by, paired_device_label, failed_attempts, consumed_at, expires_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`
        )
        .run(
          row.id,
          row.user_id,
          row.secret_hash,
          row.encrypted_payload,
          row.claimed_by,
          row.paired_device_label,
          row.failed_attempts,
          row.consumed_at,
          row.expires_at
        );
    },
    find: (id: string): PairingRow | undefined =>
      this.db.prepare("SELECT * FROM pairings WHERE id = ?").get(id) as PairingRow | undefined,
    incrementFailed: (id: string): number =>
      this.db
        .prepare("UPDATE pairings SET failed_attempts = failed_attempts + 1 WHERE id = ?")
        .run(id).changes,
    setClaimedBy: (id: string, claimed_by: string): number =>
      this.db
        .prepare("UPDATE pairings SET claimed_by = ? WHERE id = ? AND claimed_by IS NULL")
        .run(claimed_by, id).changes,
    setPayload: (id: string, payload: Buffer): void => {
      this.db.prepare("UPDATE pairings SET encrypted_payload = ? WHERE id = ?").run(payload, id);
    },
    markConsumed: (id: string, at: number, pairedDeviceLabel: string | null = null): void => {
      this.db
        .prepare("UPDATE pairings SET consumed_at = ?, paired_device_label = ?, encrypted_payload = NULL WHERE id = ?")
        .run(at, pairedDeviceLabel, id);
    },
  };

  readonly entries = {
    insertAndPrune: (
      row: Omit<EntryRow, "id">,
      maxCount: number,
      maxAgeMs: number
    ): EntryRow => {
      const tx = this.db.transaction(() => {
        const result = this.db
          .prepare(
            "INSERT INTO entries (user_id, device_id, ciphertext, size, created_at) VALUES (?, ?, ?, ?, ?)"
          )
          .run(row.user_id, row.device_id, row.ciphertext, row.size, row.created_at);
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
        .prepare(
          "SELECT * FROM entries WHERE user_id = ? AND id > ? ORDER BY id ASC LIMIT ?"
        )
        .all(user_id, sinceId, limit) as EntryRow[],
    delete: (user_id: string, id: number): number =>
      this.db
        .prepare("DELETE FROM entries WHERE user_id = ? AND id = ?")
        .run(user_id, id).changes,
    deleteAll: (user_id: string): number =>
      this.db.prepare("DELETE FROM entries WHERE user_id = ?").run(user_id).changes,
    countForUser: (user_id: string): number =>
      (this.db.prepare("SELECT COUNT(*) AS c FROM entries WHERE user_id = ?").get(user_id) as { c: number }).c,
  };
}
