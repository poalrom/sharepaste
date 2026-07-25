//! ACL regression tests that do not mock the IPC bridge.
//!
//! # Why this crate exists
//!
//! `capabilities/default.json` once granted permissions to
//! `["popover", "modal-*"]` while the app builds windows labelled `main` and
//! `popover`. Tauri denies by default, so **every `listen()` from the main
//! window was rejected at runtime** — pairing never confirmed, tray navigation
//! never switched tabs, the account list never updated. `invoke()` kept working
//! (app commands are not `plugin:`-prefixed, so they skip the guard), which is
//! why the window looked healthy.
//!
//! 133 tests were green throughout, because every UI test stubs `invoke` and
//! `listen` with `vi.fn()` and never touches the real bridge.
//!
//! These tests call [`tauri::test::get_ipc_response`], which dispatches through
//! the real `Webview::on_message` — the same entry point a webview hits — and
//! is therefore subject to the real ACL compiled from `capabilities/*.json` by
//! `generate_context!`. Nothing in the asserted path is mocked except the
//! windowing backend.
//!
//! # Why it is a separate crate
//!
//! `MockRuntime` and the real `wry` runtime cannot be linked into one test
//! binary on Windows — see the comment in `Cargo.toml`. Cargo unifies features
//! across a build graph, so this cannot live in the app crate. Run it with its
//! own invocation:
//!
//! ```text
//! cargo test --manifest-path clients/desktop/acl-tests/Cargo.toml
//! ```
//!
//! # Relationship to `capability_guard`
//!
//! `src-tauri/src/lib.rs` has a `capability_guard` module that parses the
//! capability JSON and checks it covers `WINDOW_LABELS`. That is cheap and
//! catches config drift. This crate is the counterpart that exercises Tauri's
//! *actual* glob matching, permission-set resolution and rejection behaviour,
//! so it still holds if those semantics change under a Tauri upgrade.

#![cfg(test)]

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Must match `WINDOW_LABELS` in `src-tauri/src/lib.rs`.
///
/// Kept in sync by [`window_label_list_matches_the_app`] below, which reads the
/// app's source rather than trusting this copy.
const WINDOW_LABELS: [&str; 2] = ["main", "popover"];

/// Builds a real `App` on the mock runtime from the **real** generated context,
/// so the ACL under test is the one the shipped binary enforces.
fn test_app() -> App<MockRuntime> {
    mock_builder()
        .build(tauri::generate_context!("../src-tauri/tauri.conf.json"))
        .expect("mock app builds from the real generated context")
}

fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("main.html".into()))
        .build()
        .unwrap_or_else(|e| panic!("failed to build window {label:?}: {e}"))
}

/// A well-formed `plugin:event|listen` call — the exact command S1 denied.
fn listen_request() -> InvokeRequest {
    InvokeRequest {
        cmd: "plugin:event|listen".into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "http://tauri.localhost".parse().unwrap(),
        body: InvokeBody::Json(serde_json::json!({
            "event": "sharepaste-acl-probe",
            "target": { "kind": "Any" },
            "handler": 1,
        })),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

#[test]
fn every_window_the_app_builds_can_register_event_listeners() {
    let app = test_app();

    for label in WINDOW_LABELS {
        let win = window(&app, label);
        let res = get_ipc_response(&win, listen_request());

        assert!(
            res.is_ok(),
            "window {label:?} was denied `plugin:event|listen` by the real ACL: {:?}\n\
             \n\
             That window will still render and its commands will still work, but \
             it cannot receive a single event — no pairing confirmation, no live \
             account updates, no tray navigation, and no error anywhere. Add \
             {label:?} to the `windows` array in src-tauri/capabilities/default.json.",
            res.unwrap_err()
        );
    }
}

#[test]
fn an_undeclared_window_is_denied() {
    // Guards the test above. If Tauri ever stops enforcing deny-by-default, this
    // fails and tells us `every_window_...` has silently gone hollow — which is
    // precisely the failure mode that let S1 ship.
    let app = test_app();
    let win = window(&app, "window-with-no-capability");

    let res = get_ipc_response(&win, listen_request());

    assert!(
        res.is_err(),
        "an undeclared window was ALLOWED to listen, so deny-by-default is not in \
         force and every_window_the_app_builds_can_register_event_listeners no \
         longer proves anything"
    );
}

#[test]
fn window_label_list_matches_the_app() {
    // This crate cannot import the app (linking MockRuntime beside wry breaks the
    // test binary), so WINDOW_LABELS is duplicated above. Parse the real
    // declaration and fail if the copy drifts, rather than letting the tests
    // above quietly stop covering a window.
    let src = include_str!("../../src-tauri/src/lib.rs");
    let decl = src
        .lines()
        .find(|l| l.trim_start().starts_with("const WINDOW_LABELS"))
        .expect("const WINDOW_LABELS must exist in src-tauri/src/lib.rs");

    let literal = decl
        .split_once("= [")
        .and_then(|(_, rest)| rest.rsplit_once(']'))
        .map(|(inner, _)| inner)
        .expect("WINDOW_LABELS must be assigned an array literal");

    let actual: Vec<String> = literal
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    assert_eq!(
        actual,
        WINDOW_LABELS.to_vec(),
        "the app builds {actual:?} but this crate tests {WINDOW_LABELS:?}. Update \
         WINDOW_LABELS in acl-tests/src/lib.rs so the ACL tests cover every window."
    );
}
