# Sharepaste Pairing Show-Code UI Design

**Date:** 2026-05-03
**Status:** Approved

## Summary

Complete the existing desktop pairing modal by adding the missing "show a pair code" path for already-paired devices. The app already has the server protocol, Rust command, event listeners, and code display step; this change wires that capability into the React chooser.

## Goals

- Let an already-paired desktop device generate and display a short pair code from the existing `PairingModal`.
- Keep pairing as a single user-facing modal with three choices: claim invite, enter pair code, or show a pair code.
- Make the show-code option visible even before the first account exists, but disabled until an active account is available.
- Reuse the existing Tauri `pair_start` command and existing `show-code` view.

## Non-Goals

- No server API changes.
- No Rust pairing protocol changes.
- No QR-code generation.
- No account-management changes outside the pairing modal.
- No redesign of invite claim or enter-code flows.

## Current State

The server already exposes `/pair/start`, `/pair/claim`, `/pair/payload`, `/pair/poll`, and `/devices`. The desktop Rust side already exposes `pair_start({ user_id })`, emits `pair-shortcode`, polls for claim status, uploads the encrypted pair payload, and emits `pair-claimed` or `pair-expired`.

The React `PairingModal` currently supports:

- Claiming an operator invite token.
- Entering a pair code from another device.
- Rendering a `show-code` step when `pair-shortcode` is received.

The missing piece is a chooser action that calls `cmd.pairStart` for the active account.

## User Flow

1. User opens `PairingModal`.
2. The chooser renders three options:
   - "I have an invite token"
   - "I have a pair code"
   - "I want to pair another device"
3. The modal reads `active` from `useAccountsStore`.
4. If no active account exists, "I want to pair another device" remains visible but disabled, with helper text explaining that this device needs an account first.
5. If an active account exists, clicking the third option clears any previous error, enters a busy state, and calls `cmd.pairStart({ user_id: active })`.
6. On success, the modal uses the command response `{ code, expires_at }` to set `shortcode`, set `expiresAt`, and transition to `show-code`.
7. The existing `pair-shortcode` event listener remains as a fallback for the emitted event.
8. The existing `pair-claimed` listener closes the modal.
9. The existing `pair-expired` listener shows the expiry error.

## UI Behavior

The chooser remains the main entry point. The third option should match the existing chooser button style and be disabled while unavailable or while `pairStart` is in progress. Disabled styling should make it visibly inactive without hiding it.

The disabled helper copy should be short and specific: this device must claim an invite or enter a pair code before it can show a code for another device.

The `show-code` screen keeps the existing heading, shortcode display, and countdown behavior.

## Error Handling

Errors stay local to `PairingModal`.

- Missing active account: button disabled; no command call is made.
- `pairStart` rejection: keep the chooser visible, stop the busy state, and render the existing error area.
- Duplicate clicks: prevent by disabling the third option while busy.
- Pair slot expiry: keep existing `pair-expired` handling.
- Pair claim success: keep existing `pair-claimed` close behavior.

## Implementation Scope

Expected file changes:

- `clients/desktop/ui/src/modals/PairingModal.tsx`
  - Read `active` from `useAccountsStore`.
  - Add the third chooser option.
  - Add a `startShowCode` handler that calls `cmd.pairStart`.
  - Use the returned code and expiry immediately.
  - Disable the option when there is no active account or when busy.

- `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`
  - Add coverage for the new chooser option, disabled state, successful `pair_start`, and error handling.

No server or Rust files should need changes for this scope.

## Testing

Add focused Vitest and Testing Library coverage:

- Chooser always renders "I want to pair another device".
- With no active account, the option is disabled and does not invoke `pair_start`.
- With an active account, clicking the option invokes `pair_start` with the active `user_id`.
- Successful `pair_start` displays the returned shortcode and countdown.
- Failed `pair_start` renders an error and keeps the chooser visible.

Existing Rust and server integration tests continue to cover the pairing protocol itself.

## Acceptance Criteria

- A paired desktop device can open `PairingModal`, choose "I want to pair another device", and see a short code.
- An unpaired device sees the same option disabled instead of hidden.
- Existing invite-token and enter-code flows still work.
- UI tests cover the new behavior.
