import { httpErrors } from "@fastify/sensible";
import { hashToken, randomId, randomToken, sha256Hex, verifyToken } from "../crypto.js";
import type { Repository } from "../db/repository.js";

/** The identity a bearer token resolves to. */
export interface AuthedMembership {
  user_id: string;
  device_id: string;
}

/** Everything the mint needs to know about the Device it is credentialing. */
export interface MintRequest {
  userId: string;
  /** The Device Label, or null for a Device paired before labels existed. */
  deviceLabel: string | null;
  now: number;
}

/**
 * A credential at the one moment its token exists in the clear. The relay keeps
 * an argon2 hash and a sha256 index of it and neither is reversible, so a caller
 * that drops `device_token` has dropped it for good: there is no second read.
 */
export interface IssuedCredential {
  user_id: string;
  device_id: string;
  device_token: string;
}

/** One Device as `GET /me` lists it: ADR 0001's four fields, and no others. */
export interface ListedDevice {
  device_id: string;
  label: string | null;
  created_at: number;
  revoked_at: number | null;
}

/**
 * The whole `memberships` row. Unexported on purpose: `device_token_hash` and
 * `token_sha256` are named in this file and nowhere else, which is what makes
 * `/me`'s redaction hold by construction rather than by review. Every other
 * reader is handed `ListedDevice` or `AuthedMembership`, neither of which can
 * carry credential material however carelessly it is spread.
 */
interface MembershipRow {
  user_id: string;
  device_id: string;
  device_token_hash: string;
  token_sha256: string | null;
  device_label: string | null;
  created_at: number;
  revoked_at: number | null;
}

/**
 * The relay's Device credentials: minted here, recognised here, and listed here
 * already redacted. One mint serves both doors that pair a Device and every
 * fixture that stands one up, so "how a credential is issued" has one editable
 * home and a test can no longer drift from production without failing.
 */
export class DeviceCredentials {
  /**
   * Mints a credential for a Device: an id, a token, the argon2 hash the slow
   * path verifies against and the sha256 index the fast one looks up. The token
   * comes back exactly once — nothing stored can reproduce it.
   *
   * `alongside` is the write that must land with the mint or not at all: the
   * Invite `/claim-invite` spends, the Pair Slot `/devices` consumes. The key
   * derivation runs before the transaction opens, because a better-sqlite3
   * transaction is synchronous and a write lock is the last thing to hold
   * across 19 MiB of argon2.
   */
  static async issue(
    repo: Repository,
    { userId, deviceLabel, now }: MintRequest,
    alongside?: () => void
  ): Promise<IssuedCredential> {
    const device_id = randomId();
    const device_token = randomToken();
    const device_token_hash = await hashToken(device_token);

    const tx = repo.db.transaction(() => {
      repo.db
        .prepare(
          `INSERT INTO memberships
           (user_id, device_id, device_token_hash, token_sha256, device_label, created_at, revoked_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)`
        )
        .run(userId, device_id, device_token_hash, sha256Hex(device_token), deviceLabel, now, null);
      alongside?.();
    });
    tx();

    return { user_id: userId, device_id, device_token };
  }

  /**
   * Recognises a bearer, or refuses it with the one 401 the relay has always
   * answered. Both paths are here: the sha256 index, and — until the backfill
   * at the foot of this file goes — the argon2 scan for a membership issued
   * before that index existed.
   *
   * A revoked membership is not a membership: neither path returns one.
   */
  static async authenticate(
    repo: Repository,
    authorization: string | undefined
  ): Promise<AuthedMembership> {
    if (!authorization?.startsWith("Bearer "))
      throw httpErrors.unauthorized("missing bearer token");
    const token = authorization.slice("Bearer ".length).trim();
    if (!token) throw httpErrors.unauthorized("missing bearer token");

    const indexed = repo.db
      .prepare(
        "SELECT user_id, device_id FROM memberships WHERE token_sha256 = ? AND revoked_at IS NULL"
      )
      .get(sha256Hex(token)) as AuthedMembership | undefined;
    if (indexed) return indexed;

    // ── argon2 backfill: this call goes with the block at the foot of the file.
    const scanned = await scanForUnindexed(repo, token);
    if (scanned) return scanned;
    // ── end argon2 backfill.

    throw httpErrors.unauthorized("invalid token");
  }

  /**
   * A user's Devices, in ADR 0001's shape. Revoked Devices stay in the list:
   * an entry captured on a Device that has since been revoked still needs its
   * Origin resolved, and a client that could not name it would show a bare
   * device-id slice forever.
   */
  static list(repo: Repository, userId: string): ListedDevice[] {
    return repo.db
      .prepare(
        `SELECT device_id, device_label AS label, created_at, revoked_at
         FROM memberships WHERE user_id = ? ORDER BY created_at ASC`
      )
      .all(userId) as ListedDevice[];
  }

  /**
   * Revokes one of this user's Devices, or refuses with 404. A Device already
   * revoked is still this user's Device, so revoking it again is a no-op rather
   * than a refusal — only a Device that is nobody's, or somebody else's, is not
   * found.
   */
  static revoke(repo: Repository, userId: string, deviceId: string, now: number): void {
    const owned = repo.db
      .prepare("SELECT 1 FROM memberships WHERE user_id = ? AND device_id = ?")
      .pluck()
      .get(userId, deviceId);
    if (owned === undefined) throw httpErrors.notFound("device not found for this user");

    repo.db
      .prepare(
        "UPDATE memberships SET revoked_at = ? WHERE user_id = ? AND device_id = ? AND revoked_at IS NULL"
      )
      .run(now, userId, deviceId);
  }

  /**
   * The operator-only listing: the whole row, credential columns and all. It
   * exists for one reason — to keep `sharepaste device list`'s JSON
   * byte-identical, which is output a person can see and so not a refactor's to
   * change (spec decision 1). It is not an escape hatch: `list` is what a route
   * gets, and nothing under `routes/` may call this.
   *
   * When someone rules that the CLI's output shape is not a contract, this
   * method and its one call site in `cli/device.ts` are deleted together.
   */
  static listForOperator(repo: Repository): MembershipRow[] {
    return repo.db.prepare("SELECT * FROM memberships").all() as MembershipRow[];
  }

  /**
   * Revokes every membership naming this Device, whichever user's, and answers
   * how many it found. Zero is how an operator learns the Device does not
   * exist; one already revoked still counts, so revoking twice is not an error.
   */
  static revokeEverywhere(repo: Repository, deviceId: string, now: number): number {
    const found = repo.db
      .prepare("SELECT COUNT(*) FROM memberships WHERE device_id = ?")
      .pluck()
      .get(deviceId) as number;
    if (found === 0) return 0;

    repo.db
      .prepare("UPDATE memberships SET revoked_at = ? WHERE device_id = ? AND revoked_at IS NULL")
      .run(now, deviceId);
    return found;
  }

  // ───────────────────────────────────────────────────────────────────────────
  // The argon2 backfill.
  //
  // A membership issued before the sha256 index carries a null `token_sha256`,
  // so the bearer it holds cannot be looked up — it has to be found by scanning
  // the argon2 hash of every active row still missing the index. Each such
  // membership costs one scan, after which it is indexed and never scanned
  // again.
  //
  // This is a migration, not a feature, and deleting it is deleting one file:
  // these two members, `scanForUnindexed` below the class, and the two marked
  // lines in `authenticate`. `awaitingIndex` reaching zero on a production
  // database is the signal that the day has come.
  // ───────────────────────────────────────────────────────────────────────────

  /**
   * A credential in the pre-index shape, minted the one way credentials are
   * minted and then stripped of its index. Fixtures use it to exercise the scan
   * above; nothing in production issues one.
   */
  static async issueUnindexed(repo: Repository, mint: MintRequest): Promise<IssuedCredential> {
    const issued = await DeviceCredentials.issue(repo, mint);
    repo.db
      .prepare("UPDATE memberships SET token_sha256 = NULL WHERE user_id = ? AND device_id = ?")
      .run(issued.user_id, issued.device_id);
    return issued;
  }

  /** How many active memberships the scan still has to walk. Zero ends the migration. */
  static awaitingIndex(repo: Repository): number {
    return repo.db
      .prepare("SELECT COUNT(*) FROM memberships WHERE revoked_at IS NULL AND token_sha256 IS NULL")
      .pluck()
      .get() as number;
  }
}

const scanForUnindexed = async (
  repo: Repository,
  token: string
): Promise<AuthedMembership | undefined> => {
  const unindexed = repo.db
    .prepare(
      `SELECT user_id, device_id, device_token_hash FROM memberships
       WHERE revoked_at IS NULL AND token_sha256 IS NULL`
    )
    .all() as Pick<MembershipRow, "user_id" | "device_id" | "device_token_hash">[];

  for (const m of unindexed) {
    if (!(await verifyToken(m.device_token_hash, token))) continue;
    repo.db
      .prepare("UPDATE memberships SET token_sha256 = ? WHERE user_id = ? AND device_id = ?")
      .run(sha256Hex(token), m.user_id, m.device_id);
    return { user_id: m.user_id, device_id: m.device_id };
  }
  return undefined;
};
