# Sharepaste — Design

**Date:** 2026-05-01
**Status:** Draft, pending user review

## Summary

Sharepaste is a self-hosted, end-to-end encrypted clipboard-sync service for a small set of users with multiple devices each. A single Node.js server acts as a relay and ciphertext store; it sees no plaintext at any point. Clients run on macOS and Windows (Tauri), Android (Kotlin), and iPadOS (Swift).

## Implementation decomposition

This design covers five deliverables (one server + four clients) that are too large to plan and build as a single unit. Each is its own implementation track with its own plan, sharing only this design document and a frozen wire protocol:

1. **Server + CLI** — independently testable against `curl`/automated integration tests. Must ship first; defines the contract.
2. **macOS Tauri client** — first end-user-facing milestone. Confirms protocol works end to end on real OS clipboard plumbing.
3. **Windows Tauri client** — should be a near-trivial follow-on once macOS is working (shared Rust core).
4. **Android client** — independent from desktop; builds on the same wire protocol.
5. **iPadOS client** — independent from Android; mirrors its surface.

The remaining sections describe the full system. Each track will get its own implementation plan derived from the relevant subset of this document.

## Goals

- Sync text-only clipboard entries across a user's paired devices.
- Maintain a per-user history of recent entries, fetched and searchable on every device.
- Guarantee server cannot read clipboard contents (E2E encryption).
- Support multiple isolated users on a single server, with the option for one physical device to host more than one user.
- Self-hosted only: a single operator runs one server instance for themselves and any invited users.

## Non-goals (MVP)

- Image, file, or rich-clipboard support.
- Background sync on mobile (iOS / Android).
- Open signup, billing, or hosted SaaS operation.
- Mobile push notifications via APNs/FCM.
- CRDT-based conflict resolution.
- Full key rotation or per-device key wrapping (server-side device revocation only).
- Recovery for users who lose all paired devices (admin re-invite only).

## Top-level decisions

| Area | Choice |
|---|---|
| Content type | Text only |
| Sync model | History sync — devices share a rolling window of past entries |
| Deployment | Self-hosted only; Docker image |
| Pairing | QR code (with paste-code fallback for cameraless desktops) |
| Live delivery | REST + Server-Sent Events; foreground-only on mobile in MVP |
| History retention | 100 entries OR 30 days, whichever comes first; pruned inline on insert |
| Auth | Per-device bearer token, issued at pairing |
| Provisioning | Invite-only via server-side CLI |
| Tenancy | Multi-user; a device may hold memberships in more than one user |
| Capture | Auto-capture clipboard with deny-list / transient-flag respect |
| Device revocation | Token revoke only (acceptable assuming disk encryption on devices) |
| Per-entry size cap | 64 KB |
| Server storage | SQLite (single file, single process) |
| Crypto | XChaCha20-Poly1305-IETF (libsodium) on every platform |

## Architecture

```
┌─────────────────────┐    ┌─────────────────────┐
│  macOS Tauri app    │    │  Windows Tauri app  │
│  tray + clipboard   │    │  tray + clipboard   │
│  watcher            │    │  watcher            │
└──────────┬──────────┘    └──────────┬──────────┘
           │ HTTPS (POST/GET/SSE)     │
           │ Bearer <device_token>    │
           │ ciphertext only          │
           ▼                          ▼
        ┌─────────────────────────────────┐
        │  Node.js server (self-hosted)   │
        │  - Fastify HTTP                 │
        │  - better-sqlite3               │
        │  - in-memory SSE hub            │
        │  - sees zero plaintext          │
        └──────────────┬──────────────────┘
                       │
        ┌──────────────┴──────────────┐
        ▼                             ▼
┌─────────────────┐          ┌─────────────────┐
│ Android (Kotlin)│          │ iPadOS (Swift)  │
│ foreground only │          │ foreground only │
└─────────────────┘          └─────────────────┘
```

The server is one Node binary with two modes: `sharepaste serve` runs the HTTP server; `sharepaste user|device|entry …` are CLI subcommands that operate directly on the SQLite database. Both modes are packaged into a single Docker image; CLI subcommands are intended to be invoked via `docker exec` against the running container (or `docker run --rm` against the same volume).

## Multi-tenancy model

- A `user` represents an isolated identity with its own clipboard history.
- A `device` is one running app instance. Devices are identified by an opaque server-issued ID.
- A `membership` joins a `user` and a `device`. Each membership owns its own bearer token. A single device may hold multiple memberships (one per user it is signed into); the active membership is selected by the user via the tray switcher and determines which encryption key, token, and history are in use.
- Encryption keys are per-user (`user_key`) and shared by every membership belonging to that user.
- All data plane endpoints are scoped to the membership's user; the server enforces this and clients never send a `user_id`.

## Cryptography

- **Cipher:** XChaCha20-Poly1305-IETF (libsodium), 256-bit key, 192-bit (24-byte) random nonce per entry. AEAD with `aad = user_id` to bind ciphertext to its user.
- **`user_key`:** 256 random bits, generated on the first device that claims an invite for a given user. Stored in OS-secure storage on every paired device. Server never sees it.
- **`pairing_secret`:** 256 random bits, generated by the inviting device per pair operation. Encoded into the QR (and a typeable short code as a desktop fallback). Server only ever sees `hash(pairing_secret)`.
- **Pair-payload encryption:** `XChaCha20-Poly1305(pairing_secret, …, plaintext = {user_id, user_key, server_url})`. Server stores opaque ciphertext, hands it to the claiming device, who decrypts.
- **`device_token` issuance:** authorization is by **channel binding**, not by proof-of-knowledge of `user_key` (which the server cannot verify because it never holds `user_key`). The inviting device's authenticated `POST /pair/start` opens a slot tied to a specific user; only a caller that proves knowledge of `pairing_secret` (`secret_proof`) gets a `device_token` from `POST /devices`. After consumption, the server zeroes `encrypted_payload` and marks the slot consumed; subsequent calls 410.
- **Token storage on server:** stored as `argon2id(device_token)`; raw token only seen at issuance.
- **Token storage on client:** raw token + `user_key` in OS keychain (macOS Keychain, Windows Credential Manager, Android Keystore + EncryptedSharedPreferences, iPadOS Keychain Services).

## Pairing flows

### Flow 1 — first device for a new user

1. Operator runs `sharepaste user create alice` on the server. CLI generates a random invite token, stores its hash in `invites`, prints the raw token once.
2. Operator delivers the invite token out of band (e.g. Signal).
3. User pastes the token into Device A. App calls `POST /claim-invite { token, device_label }`.
4. Server verifies the hash, marks the invite claimed, creates membership, issues `device_token`. Returns `{ device_token, user_id }`.
5. Device A generates a fresh random `user_key` and stores `(user_key, device_token)` in its keychain.

### Flow 2 — adding device B to an existing user

1. Device A: tap "Add device". Generates `pairing_secret`. Calls `POST /pair/start { hash(pairing_secret) }` (auth: own `device_token`). Server stores a pairing slot with a 2-minute expiry and returns `pair_id`.
2. Device A displays a QR encoding `{ server_url, pair_id, pairing_secret }`. (Desktop without camera: the same payload is shown as a base32 short code that can be typed into Device B.)
3. Device B scans/pastes the payload. Calls `POST /pair/claim { pair_id, secret_proof }`. Server verifies the hash, marks the slot claimed.
4. Device A long-polls `GET /pair/poll?id=<pair_id>` and learns the slot was claimed.
5. Device A encrypts `{ user_id, user_key, server_url }` with `pairing_secret` and uploads via `POST /pair/payload { pair_id, encrypted_payload }`.
6. Device B fetches via `GET /pair/payload?id=<pair_id>`, decrypts with `pairing_secret`.
7. Device B calls `POST /devices { pair_id, secret_proof, label }`. Server verifies the slot is claimed-by-this-caller and `secret_proof` matches `secret_hash`, then issues `device_token` for the user the slot was bound to and zeroes `encrypted_payload`. Device B stores `(user_key, device_token)`.

### Brute-force resistance on pairing

- Slot expires after 2 minutes.
- Three wrong `secret_proof` attempts burn the slot.
- `pairing_secret` is 256 bits — guessing is infeasible; attacks rely on observing the QR/short-code, which is the inherent risk model.

### Dual-user device

To add a second user to an already-paired device, repeat Flow 1 (claim a fresh invite for the second user) or Flow 2 (QR-pair from another of that user's devices). The app stores a second membership row alongside the first. The tray account switcher exposes both; only one is active at a time.

## API

All endpoints are JSON over HTTPS. Auth (where required) is `Authorization: Bearer <device_token>`.

### Pairing / membership

| Method | Path | Body | Auth |
|---|---|---|---|
| `POST` | `/claim-invite` | `{ token, device_label }` → `{ device_token, user_id }` | none |
| `POST` | `/pair/start` | `{ secret_hash }` → `{ pair_id }` | device_token |
| `POST` | `/pair/claim` | `{ pair_id, secret_proof }` → `200` | none |
| `POST` | `/pair/payload` | `{ pair_id, encrypted_payload }` → `200` | device_token (inviter) |
| `GET` | `/pair/poll?id=<pair_id>` | long-poll → `{ status }` | device_token |
| `GET` | `/pair/payload?id=<pair_id>&proof=<secret_proof>` | → `{ encrypted_payload }` | secret_proof |
| `POST` | `/devices` | `{ pair_id, secret_proof, label }` → `{ device_token }` | gated by `pair_id` + `secret_proof` |
| `DELETE` | `/devices/:id` | revoke target device of same user | device_token |

### Entries

| Method | Path | Body | Auth |
|---|---|---|---|
| `POST` | `/entries` | `{ ciphertext (base64) }` → `{ id, created_at }` | device_token |
| `GET` | `/entries?since=<id>&limit=<n>` | → `[{ id, ciphertext, created_at, device_id }]` | device_token |
| `DELETE` | `/entries/:id` | → `200` | device_token (same user) |
| `DELETE` | `/entries` | purge all current-user entries | device_token |
| `GET` | `/events` | SSE stream: `entry`, `delete` | device_token (header or `?token=`) |

## Data model

```sql
CREATE TABLE users (
  id          TEXT PRIMARY KEY,
  username    TEXT UNIQUE NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE invites (
  token_hash  TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at  INTEGER NOT NULL,
  claimed_at  INTEGER
);

CREATE TABLE memberships (
  user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id          TEXT NOT NULL,
  device_token_hash  TEXT NOT NULL,
  device_label       TEXT,
  created_at         INTEGER NOT NULL,
  revoked_at         INTEGER,
  PRIMARY KEY (user_id, device_id)
);

CREATE TABLE pairings (
  id                  TEXT PRIMARY KEY,
  user_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  secret_hash         TEXT NOT NULL,
  encrypted_payload   BLOB,
  claimed_by          TEXT,
  failed_attempts     INTEGER NOT NULL DEFAULT 0,
  expires_at          INTEGER NOT NULL
);

CREATE TABLE entries (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id   TEXT NOT NULL,
  ciphertext  BLOB NOT NULL,
  size        INTEGER NOT NULL,
  created_at  INTEGER NOT NULL
);
CREATE INDEX entries_user_id_id ON entries (user_id, id);

```

### Inline retention pruning

`POST /entries` runs in a single transaction:

```sql
INSERT INTO entries (user_id, device_id, ciphertext, size, created_at) VALUES (?, ?, ?, ?, ?);
DELETE FROM entries
  WHERE user_id = ?
    AND (
      created_at < (? - 30*24*60*60*1000)   -- older than 30 days
      OR id NOT IN (
        SELECT id FROM entries
        WHERE user_id = ?
        ORDER BY id DESC
        LIMIT 100
      )
    );
```

No background worker. A user with no inserts retains zero entries trivially.

## Components per platform

### Server (Node.js + TypeScript)

- **Framework:** Fastify
- **DB:** `better-sqlite3`
- **SSE hub:** in-memory `Map<user_id, Set<reply>>`. New entry → broadcast to that user's subscribers.
- **Crypto:** `node:crypto` for HMAC verification of `key_proof`. Server never invokes AEAD.
- **CLI:** same binary, subcommand dispatch (`sharepaste serve | user | device | entry`). CLI subcommands open SQLite directly.
- **Config:** env vars (`PORT`, `DB_PATH`, `TLS_CERT`, `TLS_KEY`).
- **Deploy:** Docker image. Volume mount for SQLite file.

### Tauri desktop (macOS, Windows)

- **Frontend:** React + Tailwind (history list, settings, pair-device modal, account switcher).
- **Rust backend (in-process):**
  - Clipboard watcher: `arboard` crate; macOS uses `NSPasteboardDidChange` notifications, Windows polls `OpenClipboard`/`GetClipboardData` at ~500ms.
  - HTTP client: `reqwest`. SSE: `reqwest-eventsource`.
  - Crypto: libsodium via `sodiumoxide`.
  - Secure storage: `keyring` crate.
  - Tray: `tauri-plugin-tray`.
  - QR generation: `qrcode` crate. Scan: not implemented on desktop; short-code paste fallback used instead.

### Android (Kotlin)

- **UI:** Jetpack Compose.
- **Clipboard:** `ClipboardManager.OnPrimaryClipChangedListener` (foreground only).
- **HTTP/SSE:** OkHttp + okhttp-sse.
- **Crypto:** libsodium-jni.
- **Secure storage:** Android Keystore + EncryptedSharedPreferences.
- **QR:** ML Kit barcode scanner; ZXing for generation.

### iPadOS (Swift / SwiftUI)

- **UI:** SwiftUI.
- **Clipboard:** `UIPasteboard.general` polled on app foreground events.
- **HTTP/SSE:** `URLSession` + `LDSwiftEventSource`.
- **Crypto:** libsodium via SwiftPM.
- **Secure storage:** Keychain Services.
- **QR:** `AVFoundation` for scan, `CoreImage` `CIQRCodeGenerator` for display.

## Capture rules (all desktop platforms)

- Auto-capture every clipboard change by default.
- Skip if the clipboard advertises any of:
  - macOS: `org.nspasteboard.ConcealedType`, `org.nspasteboard.TransientType`, `Concealed`, `transient`.
  - Windows: `CF_TRANSIENT` / app-specific transient flags where surfaced.
  - Android: `ClipDescription.EXTRA_IS_SENSITIVE` (Android 13+) and equivalent legacy flags.
  - iPadOS: `UIPasteboard` items with `UTType.transient` or hosted by apps that mark themselves sensitive.
- Skip if size > 64 KB.
- User can configure an additional source-app deny-list in settings (e.g. block all clipboard activity while 1Password / Bitwarden is frontmost).

## Error handling

| Failure | Behavior |
|---|---|
| Server unreachable | Exponential backoff (1s, 2s, 4s, 8s, 30s cap). Tray "offline" indicator. Local capture continues into a pending queue. |
| SSE drops | Auto-reconnect, then `GET /entries?since=<last_id>` for backfill. |
| `POST /entries` 5xx / network error | Queue locally (SQLite), retry. Tray badge "N pending". No silent drop. |
| `POST /entries` 401 (revoked) | Banner: "Device revoked". Sync stops. Local `user_key` and history retained — wipe only on explicit user action ("Re-pair device" or "Forget account"). |
| Decryption failure on a fetched entry | Log the entry id, skip it, surface "1 undecryptable entry" notification. Don't crash. |
| Invite already claimed | Server 409. App: "Invite already used; ask admin for a new one." |
| Pairing slot expired | Server 410. App restarts pair flow. |
| Pairing claim with wrong secret | Server 403; slot survives until expiry or 3 wrong attempts (then burned). |
| Clipboard item is concealed/transient | Skip. |
| Entry > 64 KB | Skip locally, surface "skipped large clipboard". |
| SQLite locked / disk full | 503. Client retries. |
| Concurrent uploads from two devices | Both succeed, distinct ids. Both broadcast. No conflict resolution needed. |
| Clock skew between client and server | `created_at` is server-side only; clients trust server. Order is monotonic per server. |
| Entry deleted on device A | `DELETE /entries/:id` → server emits `event: delete { id }` over SSE. Other devices remove from local view. |

## Testing

### Server

- **Unit:** crypto helpers (HMAC verification of `key_proof`), inline-prune SQL on synthetic data, invite/pairing state machine.
- **Integration (bulk of suite, no mocks):** real Fastify against a real SQLite tempfile. Cases include:
  - Full first-device claim and second-device QR pairing (end to end).
  - Multi-user isolation: a token from user A cannot read or write user B's entries.
  - Device revocation: revoked device 401s; sibling devices keep working.
  - Inline pruning: insert >100 entries, verify oldest are dropped; insert old-`created_at` entries, verify time-based pruning.
  - SSE delivery: subscribe, post, receive event for the right user only.
  - Concurrent posts get distinct ids.
  - Pairing brute force: 3 wrong proofs burns the slot.
- **Runner:** `vitest`.

### Tauri desktop

- **Rust core unit tests:** crypto wrappers, capture filter logic (deny-list, transient detection, size cap), retry queue.
- **Integration:** Rust core against a real local server in CI (Docker compose). Webview not exercised in CI.
- **Manual smoke:** pair → copy on macOS → see on Windows. Documented in README.

### Mobile

- **Android:** unit tests for crypto and storage adapters (JVM Robolectric). Optional Espresso instrumented test for pairing flow.
- **iPadOS:** XCTest unit tests for crypto and Keychain adapters. Manual smoke for pairing.

### End-to-end

- One scripted harness driving Tauri desktop via `tauri-driver` for desktop pair → copy → receive. Mobile e2e remains manual.

### Security hygiene

- `npm audit`, `cargo audit`, `gradle dependencyCheck` on every CI run.
- Manual review of every change to crypto code paths.

## Threat model summary

| Adversary capability | Outcome |
|---|---|
| Reads server disk and memory | Sees ciphertext, hashes (token, invite, pairing secret), nonces, AAD `user_id`s. Cannot decrypt entries. Cannot mint tokens for an existing user without `user_key`. |
| MITMs HTTPS | Blocked by TLS. Out of scope: operator runs server with valid certificate. |
| Steals a device with disk encryption disabled | Recovers `user_key` and history. Mitigation: rely on FileVault / BitLocker / Android FBE / iOS data protection; document this clearly. |
| Steals a paired device | Operator revokes via `sharepaste device revoke <id>` → 401s for that token. Old ciphertext on the stolen device remains decryptable; key rotation is a non-goal. |
| Observes a QR or short code | Can race the legitimate device to claim the pairing slot. Slot expiry (2 min) and 3-attempt lockout limit damage. Recommended practice: pair only when both devices are physically together. |
| Loses all paired devices | Admin re-invites; old ciphertext is unrecoverable. Acceptable trade for true E2E with no server-side recovery. |

## Open questions for v2

- Mobile background sync via APNs/FCM, with user-supplied or shared-relay credentials.
- Image and small-file clipboard support.
- Global hotkey overlay on desktop (Raycast-style).
- PAKE-based pairing (e.g. SPAKE2) with short numeric code, replacing QR-PSK.
- Per-device key wrapping for true forward secrecy on revocation.
- Recovery codes for last-device loss.
