# Server Simplification & Test Consolidation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **STATUS: EXECUTED 2026-07-25.** Phases 1, 2, 4 and Tasks 3.1–3.3 are complete and verified. Task 3.4 (drop `argon2`) is intentionally deferred — it is gated on the backfill completing in the target deployment. See the Execution Record at the end of this document.

**Goal:** Remove dead and redundant code from `server/`, fix the O(N-devices) authentication scan, and cut the test suite from 60 tests to ~38 without losing a single covered contract.

**Scope:** `server/src`, `server/tests`, `server/tsconfig.json`, `server/package.json`, `.github/workflows`. No client (`clients/desktop`) changes except where a wire contract genuinely changes — none is proposed.

**Baseline measured 2026-07-25:** `src` = 1016 lines / 18 files. `tests` = 1508 lines / 60 tests / 14 files. `npx vitest run` on Windows: **12 failed / 48 passed**.

---

## Findings

Severity-ordered. Each is evidence-backed; line references are to the files as of this date.

### S1 — Authentication is O(active devices) argon2 hashes per request

`src/server/auth.ts:24-29` loads **every** active membership and runs `argon2.verify` against each until one matches.

- Measured `argon2id` verify at the configured cost (`m=19456, t=2, p=1`): **20.2 ms**.
- Live `db/db.sqlite` holds **71 memberships**.
- Therefore a single authenticated request costs up to **~1.4 s of CPU**, on the libuv threadpool (default 4 threads). Every `POST /entries`, every `GET /events` connect, and every `/pair/poll` long-poll pays it.

Root cause: argon2 is a *password* KDF. Device tokens are `randomBytes(32)` (`crypto.ts:7`) — 256 bits of entropy. Key-stretching buys nothing against an offline attacker who already has 2^256 to search, and it forces a linear scan because argon2 hashes are unqueryable. Note the codebase already does the right thing for invites: `invites.token_hash` is a plain `sha256Hex` (`cli/user.ts:29`, `routes/claim-invite.ts:22`) with a PK index. Device tokens are the inconsistent case.

**Fix:** store `sha256Hex(token)` in `memberships.device_token_hash`, add a unique index, and authenticate with one indexed lookup. Deletes `hashToken`/`verifyToken` and the `argon2` dependency.

**Migration constraint:** existing argon2 hashes cannot be converted — the plaintext token is gone. Two options:

- **(a) Recommended — dual-path backfill.** Add nullable `token_sha256` column. Lookup by sha256 first; on miss, fall back to the argon2 scan and backfill `token_sha256` for the row that matched. Old devices pay the slow path exactly once; new devices never do. Drop the fallback (and `argon2`) in a follow-up release once `SELECT COUNT(*) FROM memberships WHERE revoked_at IS NULL AND token_sha256 IS NULL` is 0.
- **(b) Hard cutover.** Replace the column outright; every existing device must re-pair. Acceptable only if the deployment is personal/small.

### S2 — 12 of 60 tests fail: leaked SQLite handles in teardown

Every test that opens a DB without `db.close()` fails on Windows with `EPERM` at `rmSync`:

- `tests/unit/db.test.ts` (1), `tests/unit/repository.test.ts` (2), `tests/unit/retention-sql.test.ts` (3), `tests/integration/cli.test.ts` (6).

`tests/helpers.ts:53-55` gets this right (`app.close()` then `db.close()`); the unit/CLI fixtures do not. `cli.test.ts` is doubly affected because `src/cli/user.ts:18-22` `open()` creates a **second** handle per CLI call and never closes it — the CLI helpers leak a `better-sqlite3` connection on every invocation. Harmless for a one-shot process, fatal for tests.

This went unnoticed because **there is no CI for the server** (see S6) — `.github/workflows/` contains only `desktop-build.yml`, and POSIX `unlink` tolerates open handles.

### S3 — TLS support is dead code that reads files then throws

`config.ts:5-6,13-14` exposes `tlsCertPath`/`tlsKeyPath`; `cli/serve.ts:12-13,35-45` reads both files into memory and then unconditionally `throw new Error("TLS not supported in this build")`. `index.ts` never passes them. README:72 already states TLS terminates at a reverse proxy. Pure liability: ~15 lines whose only reachable behaviour is a crash.

### S4 — `pairings.claimed_by` stores a value already in `secret_hash`

`routes/pairing.ts:79` writes `setClaimedBy(pair_id, sha256Hex(secret_proof))`. The branch above it (line 71) has just proven `sha256Hex(secret_proof) === pairing.secret_hash`. So `claimed_by` is a duplicate of `secret_hash` used only for `!== null` tests (`pairing.ts:163`, `devices.ts:37`). It is a boolean wearing a hash costume.

### S5 — Ciphertext is base64-decoded and re-encoded on every read and write

The server never inspects entry bytes; it only needs the length. Yet:

- `routes/entries.ts:19` decodes to a Buffer, `:35-40` re-encodes the *same bytes* back to base64 to publish over SSE — a decode+encode round trip producing a string equal to the request body field.
- `routes/entries.ts:53` re-encodes every row on every `GET /entries`.

Storing `ciphertext` as base64 `TEXT` removes both round trips. Byte length for the `maxEntryBytes` check is exact arithmetic on the base64 string length, no allocation required.

### S6 — No CI for the server

Only `.github/workflows/desktop-build.yml` exists. Nothing builds, type-checks, or tests `server/` on any push. Direct cause of S2 surviving in `main`.

### S7 — The UUID regex is copy-pasted three times

`routes/devices.ts:11` (`UUID_PATTERN`) and `routes/pairing.ts:48-49` and `:96` (two inline copies). `HEX_64` (`pairing.ts:6`) is a fourth ad-hoc schema fragment. One shared `schemas.ts` covers all four.

### S8 — `pairings` and `invites` grow without bound

`entries` is pruned on insert (`repository.ts:161-186`). Nothing ever deletes expired pairings or claimed/expired invites. Live DB: 14 pairings, 57 invites, none reclaimable. Small in absolute terms, unbounded in principle, and expired pairing rows retain `encrypted_payload` blobs unless the slot was consumed (`markConsumed` nulls it; expiry does not).

### S9 — `maxPairingFailures` is enforced on one of three proof-checking endpoints

`POST /pair/claim` counts failures (`pairing.ts:72-77`). `GET /pair/payload` (`pairing.ts:128`) and `POST /devices` (`devices.ts:38`) verify the same `secret_proof` and count nothing. An attacker simply brute-forces via the uncounted endpoints.

**Severity: low, not exploitable.** The pairing secret is 32 random bytes (`clients/desktop/src-tauri/src/core/pairing/qr.rs:52-53`), carried whole in the shortcode (`shortcode.rs:11-13`, `SECRET_LEN = 32`). 2^256 is not brute-forceable. But the control as written provides no security while costing a column, a config knob, and a test — either apply it uniformly or delete it and say the secret carries the weight.

### S10 — `tsconfig.json` compiles tests into the shipped build

`include` covers `tests/**/*` and `rootDir` is `"."`, so `npm run build` emits `dist/tests/` alongside `dist/src/`, and `package.json:bin` must point at the awkward `dist/src/index.js`. Splitting into a build config (`src` only, `rootDir: "src"`) and a typecheck config makes `bin` = `dist/index.js` and keeps test code out of the image.

### S11 — Three different default DB paths, and the README documents a fourth

| Source | Default |
|---|---|
| `config.ts:10` | `/var/lib/sharepaste/sharepaste.sqlite` |
| `index.ts:20` | `/var/lib/sharepaste/sharepaste.sqlite` (duplicated literal) |
| `docker-compose.yml` / `Dockerfile` | `/var/lib/sharepaste/db.sqlite` vs `.../sharepaste.sqlite` — **these two disagree** |
| `README.md:27` | "defaults to `../db/db.sqlite`" — true of no code path |

One exported constant, one correct README line.

### S12 — Unused repository surface

Reachable from no production code path (verified by grep across `server/src`):

- `users.findById`, `users.findByUsername` — tests only.
- `entries.countForUser` — tests only (legitimate as a test assertion helper; keep, but it is not production API).
- `TestApp.hub` (`tests/helpers.ts:15,31,50`) — returned by the fixture, destructured by zero tests.

Also dead and worth deleting on security grounds: `auth.ts:14-15` accepts the device token from a **query string** (`?token=`). No client uses it (grepped `clients/desktop`: zero matches) and it puts bearer tokens into reverse-proxy access logs.

---

## Plan

### Phase 1 — Make the suite green and guarded

Nothing else should land on a red suite.

- [ ] **Task 1.1: Close DB handles in test teardown.**
  Files: `tests/unit/db.test.ts`, `tests/unit/repository.test.ts`, `tests/unit/retention-sql.test.ts`, `tests/integration/cli.test.ts`.
  Introduce a single `tests/helpers.ts` export — `withTempDb(): { dbPath, repo, cleanup }` — that owns `mkdtempSync`, `openDb`, `migrate`, `db.close()`, `rmSync`, in that order. Replace all four ad-hoc `beforeEach`/`afterEach` blocks with it.
  Acceptance: `npx vitest run` reports 0 failures on Windows.

- [ ] **Task 1.2: Stop the CLI leaking a connection per call.**
  File: `src/cli/user.ts`, `src/cli/device.ts`, `src/cli/entry.ts`.
  Collapse the three copies of `open(dbPath)` into one `src/cli/db.ts` helper exposing `withRepo(dbPath, fn)` that closes the handle in a `finally`. Each `run*` function becomes a `withRepo` body.
  Acceptance: `cli.test.ts` passes with the shared temp-DB fixture; no `openDb` call site outside `withRepo`/`startServer`/tests.

- [ ] **Task 1.3: Add a server CI workflow.**
  File: create `.github/workflows/server-ci.yml`. Ubuntu + Windows matrix (Windows is what caught S2), Node 25, `npm ci`, `npm run typecheck`, `npm test`. Trigger on push to `main` and on PR touching `server/**`.
  Acceptance: workflow is valid YAML and both matrix legs pass.

### Phase 2 — Delete dead code (no behaviour change)

- [ ] **Task 2.1: Remove the TLS stub (S3).**
  Drop `tlsCertPath`/`tlsKeyPath` from `ServeConfig` and `ServeOptions`, delete the `fs.readFileSync` block and the `throw` in `cli/serve.ts`, drop the now-unused `node:fs` import.

- [ ] **Task 2.2: Remove the query-string token fallback (S12).**
  `auth.ts:14-15` → header-only extraction. Add a one-line note to README's auth section.

- [ ] **Task 2.3: Remove unused repository methods (S12).**
  Delete `users.findById` and `users.findByUsername`. Update `cli.test.ts` (currently the only `findById` consumer) to assert through `users.list()`. Keep `entries.countForUser` — it is the assertion primitive the retention tests need.

- [ ] **Task 2.4: Remove `TestApp.hub` (S12).**
  `tests/helpers.ts`: drop the field from the interface and the return object; the local `hub` const stays because `AppDeps` needs it.

- [ ] **Task 2.5: Unify the DB path default (S11).**
  Export `DEFAULT_DB_PATH` from `config.ts`; consume it in `index.ts:20`. Align `docker-compose.yml` with the `Dockerfile` `ENV DB_PATH`. Correct `README.md:27`.

- [ ] **Task 2.6: Extract shared JSON-schema fragments (S7).**
  New `src/server/routes/schemas.ts` exporting `UUID` and `HEX_64`. Replace the four inline copies.

- [ ] **Task 2.7: Split tsconfig (S10).**
  `tsconfig.json` → `include: ["src/**/*"]`, `rootDir: "src"`. New `tsconfig.test.json` extending it with `include: ["src/**/*","tests/**/*"]` and `noEmit`. `package.json`: `build` unchanged, add `"typecheck": "tsc -p tsconfig.test.json"`, change `bin` to `dist/index.js`, update `Dockerfile` `ENTRYPOINT` to `dist/index.js`.
  Acceptance: `npm run build && node dist/index.js --help` prints CLI usage; `dist/tests` does not exist.

### Phase 3 — Fix authentication (the load-bearing change)

- [ ] **Task 3.1: Add the sha256 token index.**
  `db/migrate.ts`: `ALTER TABLE memberships ADD COLUMN token_sha256 TEXT` (guarded by the same `PRAGMA table_info` check already used for `paired_device_label`), plus `CREATE UNIQUE INDEX IF NOT EXISTS memberships_token_sha256 ON memberships (token_sha256) WHERE token_sha256 IS NOT NULL`.
  `repository.ts`: add `memberships.findByTokenSha256(hash)` (indexed, `revoked_at IS NULL`) and `memberships.setTokenSha256(user_id, device_id, hash)`.

- [ ] **Task 3.2: Write sha256 on every new membership.**
  `routes/claim-invite.ts` and `routes/devices.ts` populate `token_sha256 = sha256Hex(token)` in the same transaction that creates the membership. Keep writing the argon2 hash for now so a rollback is possible.

- [ ] **Task 3.3: Fast-path `verifyBearer` with backfill.**
  `auth.ts`: look up `findByTokenSha256(sha256Hex(token))` first and return on hit. On miss, run the existing argon2 scan; on a scan hit, call `setTokenSha256` before returning. Behaviour is identical; cost drops from O(N)×20 ms to one indexed `SELECT` after the first request per legacy device.
  Acceptance: a new test asserts (i) a freshly paired device authenticates without any argon2 call, and (ii) a membership seeded with only an argon2 hash authenticates and has `token_sha256` populated afterwards.

- [ ] **Task 3.4 (follow-up release, gated): drop argon2.**
  Only once `SELECT COUNT(*) FROM memberships WHERE revoked_at IS NULL AND token_sha256 IS NULL` returns 0 in the target deployment. Delete `hashToken`/`verifyToken`, `memberships.listActive`, the `device_token_hash` column, and the `argon2` dependency.
  If the deployment is personal and re-pairing every device is acceptable, collapse 3.1–3.4 into a single hard cutover instead.

### Phase 4 — Simplify the data model and hot paths

- [ ] **Task 4.1: Replace `pairings.claimed_by` with `claimed_at INTEGER` (S4).**
  Migration adds `claimed_at`, backfills `claimed_at = expires_at` (any non-null `claimed_by` implies claimed; the exact instant is unrecoverable and unused), drops the read of `claimed_by`. `pairing.ts:79` → `markClaimed(pair_id, now)`. `pairing.ts:163` and `devices.ts:37` test `claimed_at !== null`.

- [ ] **Task 4.2: Store ciphertext as base64 TEXT (S5).**
  Migration: new `entries.ciphertext_b64 TEXT`, backfill from the BLOB, drop the BLOB. `routes/entries.ts` stores `req.body.ciphertext` verbatim; the size check becomes arithmetic on the base64 length (`floor(len/4)*3 - padding`) with no `Buffer.from`. `GET /entries` and `hub.publish` return the stored string directly. Wire format is unchanged — no client change.
  Acceptance: `entries.test.ts` and `events-sse.test.ts` pass unmodified apart from fixture field names.

- [ ] **Task 4.3: Decide `maxPairingFailures` (S9).**
  Recommended: apply `incrementFailed` + the burn check uniformly in a shared `assertPairingUsable(pairing, proof)` helper consumed by `/pair/claim`, `GET /pair/payload`, and `POST /devices`. Alternative if the 256-bit secret is deemed sufficient: delete `failed_attempts`, `maxPairingFailures`, and the two tests that cover them. Do not leave it as-is.

- [ ] **Task 4.4: Sweep expired pairings and dead invites (S8).**
  Add `repository.maintenance.sweep(now)` deleting `pairings WHERE expires_at < ?` and `invites WHERE expires_at < ? OR claimed_at IS NOT NULL`. Call it from `startServer` at boot and on a `setInterval` (unref'd) once an hour.
  Acceptance: one unit test seeding an expired pairing, a claimed invite, and a live invite, asserting only the live invite survives.

---

## Test Consolidation

Current: **60 tests / 14 files / 1508 lines**. Target: **~38 tests / 11 files**, no contract dropped.

### Task T1: Kill the fixture boilerplate

`try { … } finally { await close() }` is repeated in ~40 tests. `cipherB64` is defined twice (`entries.test.ts:4`, `isolation.test.ts:4`). `startPair` (`pairing.test.ts:59`) and `startAndClaim` (`devices.test.ts:8`) are the same helper, both typed `(app: any, repo: any)`. `devices.test.ts:112-114` and `entries.test.ts:216` use inline `await import(...)` to reach helpers that belong in `helpers.ts`.

- [ ] Add to `tests/helpers.ts`, all properly typed: `withApp(overrides?, opts?)` as a `beforeEach`/`afterEach` pair or an around-wrapper; `cipherB64`; `startPair(app, repo)`; `startAndClaim(app, repo)`; `addDevice(repo, user_id, label)`; `seedInvite(repo, userId)`.
- [ ] Delete every inline `await import(...)` from test bodies.

### Task T2: Delete duplicated tests

| Delete | Reason | Contract preserved by |
|---|---|---|
| `integration/health.test.ts` (whole file, 1 test) | Asserts `/healthz` → `{ok:true}` via `inject` | `serve.test.ts` asserts the same over a real socket through `startServer` — strictly stronger |
| `integration/isolation.test.ts` (whole file, 1 test) | Its three assertions are cross-user list scoping, cross-user delete 404, and delete-all scoping | First two already in `entries.test.ts` ("scoped to caller's user", "cannot delete another user's entry"). **Move the third** (`countForUser(b)` survives A's `DELETE /entries`) into `entries.test.ts`'s delete-all test as one extra assertion |
| `entries.test.ts` "prunes the oldest entry past the count cap (inline)" | Same invariant as `retention-sql.test.ts` "keeps only the most recent MAX_COUNT", via 5 HTTP round trips instead of direct calls | `retention-sql.test.ts`, which also covers age-based pruning and cross-user isolation |
| `entries.test.ts` "two devices uploading at once each get a distinct id" | Asserts SQLite `AUTOINCREMENT`, not our logic; no plausible bug in our code makes it fail | Nothing needed |
| `unit/repository.test.ts` (whole file, 2 tests) | "creates and looks up users" is trivial CRUD exercised by all 58 other tests; "rejects duplicate usernames" is verbatim duplicated by `cli.test.ts` "rejects duplicate usernames" | `cli.test.ts` |
| `unit/db.test.ts` (whole file, 1 test) | Table existence is implied by every other test; `PRAGMA foreign_keys` tests `openDb`, not `migrate`; cascade behaviour is proven end-to-end | `cli.test.ts` "deletes a user and cascades…". Keep migrate-idempotency as a two-line assertion inside `withTempDb` (call `migrate` twice) |
| `crypto.test.ts` "hashToken / verifyToken round-trips" | Deleted together with argon2 in Task 3.4 | n/a — the primitive is gone |

### Task T3: Give each contract exactly one owner

- [ ] **Token/ID format regexes belong to `crypto.test.ts`.** Remove `/^[A-Za-z0-9_-]{43}$/` from `claim-invite.test.ts:30` and `devices.test.ts:38`, and `/^[0-9a-f-]{36}$/` from `claim-invite.test.ts:31` and `pairing.test.ts:21`. Those tests should assert *a token was issued and works*, not its alphabet.
- [ ] **Create `integration/auth.test.ts`** owning `verifyBearer`, and remove the three scattered cases: `pairing.test.ts` "rejects without a token", `pairing.test.ts` "rejects an invalid token", `devices.test.ts` "a revoked token can no longer be used". None of them is about pairing or devices; all three probe `/pair/start` purely because it is a convenient authed route. Add the two Task 3.3 cases here.
- [ ] **Clean up `sse-hub.test.ts`.** The four-line comment justifying `as unknown as SseEvent` is longer than the fix: build the literals with the real `SseEvent` shape (or add a `entryEvent(id)` factory in `helpers.ts`). Keep both tests — unsubscribe and multi-subscriber are not covered by `events-sse.test.ts`.

### Resulting suite

| File | Before | After |
|---|---|---|
| `unit/crypto.test.ts` | 5 | 4 |
| `unit/db.test.ts` | 1 | 0 (deleted) |
| `unit/repository.test.ts` | 2 | 0 (deleted) |
| `unit/retention-sql.test.ts` | 3 | 3 |
| `unit/sse-hub.test.ts` | 2 | 2 |
| `unit/maintenance.test.ts` | — | 1 (Task 4.4) |
| `integration/health.test.ts` | 1 | 0 (deleted) |
| `integration/serve.test.ts` | 1 | 1 |
| `integration/auth.test.ts` | — | 5 (3 moved + 2 new) |
| `integration/claim-invite.test.ts` | 5 | 5 |
| `integration/pairing.test.ts` | 15 | 13 |
| `integration/devices.test.ts` | 7 | 6 |
| `integration/entries.test.ts` | 10 | 8 |
| `integration/events-sse.test.ts` | 1 | 1 |
| `integration/isolation.test.ts` | 1 | 0 (deleted) |
| `integration/cli.test.ts` | 6 | 6 |
| **Total** | **60** | **55 → 38 after Phase 3.4 lands and argon2 tests retire** |

Net: 4 files deleted, 1 added, ~350 lines of boilerplate removed, zero contracts lost.

---

## Verification

Run after each phase, not once at the end:

```bash
cd server && npm run typecheck && npm test
```

Phase 3 additionally requires a before/after latency measurement, since the whole point is the 20 ms × N scan:

```bash
# seed N memberships, then time 20 sequential authenticated GET /entries
cd server && npx tsx scripts/bench-auth.ts   # write as part of Task 3.3, delete before merge
```

Expected: p50 request latency independent of membership count, versus ~20 ms × N today.

---

## Execution Record — 2026-07-25

Implemented via six parallel subagents over a foundation (`db/migrate.ts`, `db/repository.ts`, `tests/helpers.ts`) written first, since those three files are the contract every slice consumes.

### Result

| | Before | After |
|---|---|---|
| `npx vitest run` (Windows) | 12 failed / 48 passed | **0 failed / 59 passed** |
| Test files | 14 | 12 |
| `tsc -p tsconfig.test.json` | n/a (no typecheck script) | clean |
| Server CI | none | 2-OS matrix, typecheck + test |

### Verified, not assumed

- **Auth cost (the headline fix).** Benchmarked against 70 argon2-only memberships plus indexed ones, same code, same data:
  - legacy token, 1st request (full argon2 scan — the old behaviour): **1114.2 ms**
  - same token, 2nd request (backfilled → indexed): **0.5 ms**
  - token indexed at issue: **0.2 ms**; 50 sequential indexed auths: 5.7 ms total
- **Migration against a copy of the real `db/db.sqlite`** (57 users, 71 memberships, 31 entries, 14 claimed pairings): `claimed_by` → `claimed_at` backfilled 14/14; `token_sha256` added with 57 active rows correctly reported unindexed; all 31 entries converted BLOB → base64 TEXT with decoded byte length exactly matching the pre-existing `size` column; second `migrate()` is a no-op; `sweep` then reclaimed 14 pairings and 57 invites.
- **End-to-end against the built `dist/index.js`**, not a test harness: CLI `user create`/`list`/`device revoke`; `claim-invite`; SSE connect; `POST`/`GET /entries` with a byte-exact base64 round trip and the same ciphertext arriving over SSE; the full pairing handshake (`start → poll waiting → claim → poll claimed → payload up/down → devices → poll consumed` with the device label); new device authenticates, CLI revoke then 401, inviter unaffected.
- **S9 bypass is closed.** Four wrong proofs sent *only* through `GET /pair/payload` — the endpoint that previously counted nothing — now return `403, 403, 403, 410`, and the slot is burnt for the correct proof and for `POST /devices` thereafter.
- **S12.** `GET /entries?token=<valid>` with no `Authorization` header returns 401.
- Every device issued through both provisioning paths lands with `token_sha256` populated: zero unindexed active memberships in the smoke database.

### Deviations from the plan as written

- **59 tests, not the projected 55.** All deliberate: `auth.test.ts` got 3 new cases rather than 2 (added the query-string rejection); `maintenance.test.ts` got a second test absorbing `db.test.ts`'s unique migrate-idempotency assertion; `entries.test.ts` gained "rejects malformed base64" for the new 400; and `crypto.test.ts` keeps its 5th test because argon2 is still live (see below).
- **`crypto.test.ts` left untouched.** The plan retired its `hashToken`/`verifyToken` test alongside argon2. Since Task 3.4 is deferred, both functions remain production code on the legacy authentication path and keep their coverage.
- **Two extra modules beyond the plan's file list.** `src/server/pairing-slot.ts` holds `loadUsableSlot` + `verifySlotProof` so all three proof-checking endpoints share one implementation (Task 4.3 specified the behaviour but not a home for it). `routes/schemas.ts` also exports `SECRET_PROOF` and `DEVICE_LABEL`, which were duplicated across the same route files as the UUID pattern.
- **`pairings.incrementFailed` now returns the post-increment count** instead of `changes`, which removed the re-read the old claim handler performed.

### Follow-up

Task 3.4 is the only outstanding item. Gate:

```sql
SELECT COUNT(*) FROM memberships WHERE revoked_at IS NULL AND token_sha256 IS NULL;
```

Once that is 0 in the deployment, delete `hashToken`/`verifyToken` from `crypto.ts`, the backfill loop in `auth.ts`, `memberships.listUnindexed`, the `device_token_hash` column, the `argon2` dependency, and the `hashToken`/`verifyToken` test — roughly 40 further lines, and it drops a native build dependency.
