# One protocol, three shells

**Landed, 2026-07-29 — where the extraction actually put things.** The decision held: the
crate exists as `sharepaste-core` in a Cargo workspace at `clients/`, with one
`clients/Cargo.lock` in place of the two, and the desktop runs on it. But every path cited
below was relative to `clients/desktop/src-tauri/src/`, and none of that code lives there
any more:

| cited below | now |
| --- | --- |
| `core/*` | `clients/core/src/*` |
| `core/crypto.rs:88-116` | `clients/core/src/crypto.rs:88-114` (`matches_libsodium_kat`) |
| `core/pairing/payload.rs:135-138` | `clients/core/src/pairing/payload.rs`, `hex_output_matches_wire_golden_values` |
| `core/keychain.rs:9-19` | `clients/core/src/keychain.rs:9-13` |
| `errors.rs:67-69` | `clients/core/src/errors.rs:116-119` |
| `commands.rs:137-138` | `clients/core/src/facade.rs`, the doc comment on `pair_start` |

Four claims below also need correcting rather than re-arguing:

- **The three blockers are closed.** `impl From<keyring::Error>` is gated to macOS and
  Windows; `arboard` is declared by the desktop crate alone; and the core takes its paths
  as data — `dirs` survives only in the desktop's `config.rs`, while Android hands in
  `context.filesDir`.
- **`EventSink` carries twelve events, not seventeen.** They are the `CoreEvent` variants in
  `clients/core/src/event.rs`, and the count moves as the core learns and unlearns one — the
  decryption-failure variant was removed once `Entry` carried `undecryptable` itself. The
  desktop's `events.rs` is the adapter that maps each onto the name `ui/` already listened
  for, and it keeps two shell-only names out of the core: `main://navigate` and
  `update-available`.
- **The seam grew a fourth part: shared renderings.** `clients/core/src/render.rs` owns the
  Preview, the Origin label and the readable Relay host, because each is a rule two shells
  could get *differently* wrong — which is the same argument this record makes about the
  protocol, applied one layer up. Layout stayed in the shells.
- **The third shell does not exist.** Android got Compose; there is no SwiftUI client and
  no Xcode project. See the deferral note in
  [ADR 0008](0008-sideloaded-and-not-self-updating.md).

An Android and iOS client has to come from somewhere, and the honest choice was
between reimplementing the protocol in Kotlin — where Compose Multiplatform would
have given one UI for both phones — and reusing the Rust that already speaks it,
at the cost of writing the UI natively twice. We reused the Rust: `core/*` and the
orchestration in `commands.rs` become a crate, `sharepaste-core`, exposing a
plain-Rust facade over the operations, the sync session and an `EventSink`. The
desktop's `commands.rs` collapses into `#[tauri::command]` shims; the mobile crate
is `#[uniffi::export]` shims; Android and iOS get Compose and SwiftUI.

Both answers duplicate something forever, and the whole decision is *which*.
Protocol duplication fails silently and destroys data — a wrong AAD or a mis-stepped
`last_seen_id` produces **Undecryptable** entries and lost history, discovered
weeks later. UI duplication is merely expensive, and it fails visibly. The
protocol is also frozen where the mobile UI is not: the desktop shipped 0.2.0 and
its wire format has golden vectors — the libsodium known-answer test at
`core/crypto.rs:88-116` exists for exactly this purpose, and
`core/pairing/payload.rs:135-138` pins the sha256-over-the-hex-*string* quirk that
nobody would guess.

## Considered Options

**Kotlin Multiplatform with Compose Multiplatform.** The closest call, and stronger
than it would have been two years ago: Compose for iOS has been production-stable
since 1.8.0, Ktor's client speaks SSE, SQLDelight is KMP-native, and
`kotlin-multiplatform-libsodium` supplies the exact AEAD on both targets — which
matters, because CryptoKit has no XChaCha20 at all, only 12-byte-nonce IETF
ChaChaPoly. Rejected on the duplication axis above. Note what would have been
re-derived by hand: not the primitives, but the sequencing. `commands.rs:137-138`
records *"pre-upload payload before exposing the code, so the claimer's fetch can
never race the inviter's upload"* — invisible from the wire protocol, and the
class of race you see once a month and never reproduce.

**Tauri 2 mobile.** The only option duplicating neither protocol nor UI, and it
would have inherited the React views and their twelve test files. Rejected on the
surfaces: an iOS App Intent and an Android ongoing notification are native code
either way, Tauri's `gen/apple` and `gen/android` projects are generated and hostile
to hand-extension, and every native call would cost three layers — platform code,
Rust bridge, JS binding. A WebView is also a poor substrate for a tool whose entire
value is feeling instant.

**Extract the primitives only, and let each platform sequence them.** Rejected as
precisely inverted: it shares the easy part and duplicates the dangerous part.

**Build the facade for mobile and migrate the desktop later.** Rejected by naming
it honestly — "later" does not happen, and until it does there are two orchestration
implementations, which is the cost the extraction was paid to avoid.

## Consequences

The seam is three platform traits plus paths-as-data. `Keychain` already exists as
a trait (`core/keychain.rs:9-19`) and needs only per-platform implementations;
`Clipboard` is new and finally makes `arboard` a target-gated dependency instead of
an unconditional one; `EventSink` is new and carries the seventeen events in
`events.rs`, which `AppHandle` was doing informally all along — `pair_with_code` is
six protocol steps and exactly two lines of Tauri.

Three things break a third target today and must be fixed first regardless:
`errors.rs:67-69` has an unconditional `impl From<keyring::Error>` while `keyring`
is declared only for macOS and Windows, `arboard` is unconditional, and `config.rs`
derives paths from `dirs`, which is meaningless inside an app sandbox.

A shipped client is being refactored before any mobile code exists, so the desktop
must be re-smoke-tested against both checklists in `clients/desktop/README.md` to
prove no regression. A Cargo workspace at `clients/` also consolidates the two
independent lockfiles that exist today — `src-tauri/Cargo.lock` and
`acl-tests/Cargo.lock` — which moves cache and path configuration in
`desktop-build.yml`. Per **P1**, every mobile ticket touching the core is blocked
behind the extraction, so the parallel frontier is deliberately narrow at the start.
