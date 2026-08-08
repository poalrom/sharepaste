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

/**
 * The relay's row for a Pair Slot. Internal to `server/pair-slot.ts`, which is
 * the only reader: nothing else handles the nine fields or decides what a
 * combination of them means.
 */
export interface PairSlotRow {
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
  /** When the entry was last captured or used — the key the history orders and prunes on. */
  last_use: number;
  /**
   * Per-user monotonic fetch cursor, re-allocated on every use so a used entry
   * comes back down the `since=` pipe a client's watermark has already passed.
   */
  seq: number;
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

  // There is no `memberships` namespace, and no row type for that table either:
  // `server/device-credentials.ts` owns it whole — the mint, the bearer verify,
  // the redacted Device listing and the operator one. Keeping the two credential
  // columns to a single file is what makes `/me`'s redaction hold by
  // construction, so a shortcut back through here would undo it.

  /**
   * The SQLite table keeps the name `pairings` on purpose: renaming it is a
   * migration against every installed self-hosted database, and the old name
   * never leaves these SQL strings. See ADR 0017.
   */
  readonly pairSlots = {
    create: (row: PairSlotRow): void => {
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
    find: (id: string): PairSlotRow | undefined =>
      this.db.prepare("SELECT * FROM pairings WHERE id = ?").get(id) as PairSlotRow | undefined,
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

  /**
   * Allocate the user's next sequence, and never hand the same one out twice.
   *
   * The counter lives on `users` rather than being derived from
   * `MAX(entries.seq)`, because a maximum over surviving rows *rolls back* when
   * the highest row is deleted — and `DELETE /entries` would reset it to zero
   * outright. A client whose `last_seen_seq` had already passed a reused value
   * would never fetch the row wearing it: `listSince` filters `seq > ?`, so the
   * entry is lost for good. That is the loss the client's own watermark
   * invariant exists to prevent, and it is why `entries.id` is `AUTOINCREMENT`
   * rather than a plain rowid; the sequence needs the same guarantee.
   *
   * Callers must already be inside the transaction that consumes it —
   * `UNIQUE (user_id, seq)` turns a lost race into a constraint failure rather
   * than two entries a client fetches as one.
   */
  private nextSeq(user_id: string): number {
    this.db.prepare("UPDATE users SET next_seq = next_seq + 1 WHERE id = ?").run(user_id);
    // `pluck` yields the bare column, so there is no row shape to assert.
    return this.db
      .prepare("SELECT next_seq FROM users WHERE id = ?")
      .pluck()
      .get(user_id) as number;
  }

  readonly entries = {
    insertAndPrune: (
      row: Omit<EntryRow, "id" | "seq" | "last_use">,
      maxCount: number,
      maxAgeMs: number
    ): EntryRow => {
      const tx = this.db.transaction(() => {
        const seq = this.nextSeq(row.user_id);
        const result = this.db
          .prepare(
            `INSERT INTO entries (user_id, device_id, ciphertext_b64, size, created_at, last_use, seq)
             VALUES (?, ?, ?, ?, ?, ?, ?)`
          )
          .run(
            row.user_id,
            row.device_id,
            row.ciphertext_b64,
            row.size,
            row.created_at,
            row.created_at,
            seq
          );
        const id = Number(result.lastInsertRowid);
        // Both caps read `last_use`, so ciphertext you keep recalling never ages
        // out and the count cap never deletes the row sitting at the top of the
        // list. The `id DESC` tiebreak keeps the order total when two entries
        // share a millisecond.
        this.db
          .prepare(
            `DELETE FROM entries
             WHERE user_id = ?
               AND (
                 last_use < ?
                 OR id NOT IN (
                   SELECT id FROM entries
                   WHERE user_id = ?
                   ORDER BY last_use DESC, id DESC
                   LIMIT ?
                 )
               )`
          )
          .run(row.user_id, row.created_at - maxAgeMs, row.user_id, maxCount);
        return { id, ...row, last_use: row.created_at, seq };
      });
      return tx();
    },
    /**
     * Records a **Use**: stamps `last_use` and allocates a fresh `seq` so the
     * entry comes back down every client's `since=` fetch.
     *
     * It deliberately does not prune. A use only ever raises one entry's
     * `last_use`, so the prunable set can only shrink; there is nothing new to
     * evict and pruning here would only spend a scan per recall.
     */
    recordUse: (user_id: string, id: number, now: number): EntryRow | undefined => {
      const tx = this.db.transaction(() => {
        const row = this.db
          .prepare("SELECT * FROM entries WHERE user_id = ? AND id = ?")
          .get(user_id, id) as EntryRow | undefined;
        if (!row) return undefined;
        const seq = this.nextSeq(user_id);
        this.db
          .prepare("UPDATE entries SET last_use = ?, seq = ? WHERE user_id = ? AND id = ?")
          .run(now, seq, user_id, id);
        return { ...row, last_use: now, seq };
      });
      return tx();
    },
    listSince: (user_id: string, sinceSeq: number, limit: number): EntryRow[] =>
      this.db
        .prepare("SELECT * FROM entries WHERE user_id = ? AND seq > ? ORDER BY seq ASC LIMIT ?")
        .all(user_id, sinceSeq, limit) as EntryRow[],
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
