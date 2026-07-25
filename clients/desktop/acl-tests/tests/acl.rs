//! ACL regression tests that do not mock the IPC bridge.
//!
//! # Why this exists
//!
//! `capabilities/default.json` once granted permissions to
//! `["popover", "modal-*"]` while the app builds windows labelled `main` and
//! `popover`. Tauri denies by default, so **every `listen()` from the main
//! window was rejected at runtime** — pairing never confirmed, tray navigation
//! never switched tabs, the account list never updated. `invoke()` kept working
//! (app commands are not `plugin:`-prefixed, so they skip the guard), which is
//! why the window looked healthy. 133 tests were green throughout, because
//! every UI test stubs `invoke`/`listen` and never touches the real bridge.
//!
//! [`tauri::test::get_ipc_response`] dispatches through the real
//! `Webview::on_message`, so these assertions are subject to the real ACL
//! compiled from `capabilities/*.json` by `generate_context!`. Nothing in the
//! asserted path is mocked except the windowing backend.
//!
//! # Why `harness = false`
//!
//! Rust's default test harness runs each `#[test]` on a spawned thread. The
//! app's config declares a tray icon, so building the context creates one, and
//! on macOS that is main-thread-only — under the normal harness it fails with
//! `Tray(NotMainThread)`. Running as a plain `main` puts the work on the
//! process main thread, which is the AppKit main thread. Caught by CI on the
//! macOS leg; Windows does not care either way.
//!
//! # Why a separate crate
//!
//! `MockRuntime` and the real `wry` runtime cannot be linked into one test
//! binary on Windows — the process dies at load with
//! `STATUS_ENTRYPOINT_NOT_FOUND` before any test runs, reproduced in a clean
//! scratch crate. Cargo unifies features across a build graph, so the app crate
//! cannot host this.
//!
//! # Relationship to `capability_guard`
//!
//! `src-tauri/src/lib.rs` has a `capability_guard` module that parses the
//! capability JSON and checks it covers `WINDOW_LABELS`. That is cheap and
//! catches config drift. This is the counterpart that exercises Tauri's *actual*
//! glob matching, permission-set resolution and rejection behaviour, so it still
//! holds if those semantics change under a Tauri upgrade.

use std::panic::{catch_unwind, AssertUnwindSafe};

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Must match `WINDOW_LABELS` in `src-tauri/src/lib.rs`. Kept honest by
/// [`window_label_list_matches_the_app`], which parses the real declaration.
const WINDOW_LABELS: [&str; 2] = ["main", "popover"];

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

fn every_window_the_app_builds_can_register_event_listeners(app: &App<MockRuntime>) {
    for label in WINDOW_LABELS {
        let win = window(app, label);
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

fn an_undeclared_window_is_denied(app: &App<MockRuntime>) {
    // Guards the check above. If Tauri ever stops enforcing deny-by-default,
    // this fails and tells us the other one has silently gone hollow — which is
    // precisely the failure mode that let S1 ship.
    let win = window(app, "window-with-no-capability");
    let res = get_ipc_response(&win, listen_request());

    assert!(
        res.is_err(),
        "an undeclared window was ALLOWED to listen, so deny-by-default is not in \
         force and every_window_the_app_builds_can_register_event_listeners no \
         longer proves anything"
    );
}

fn window_label_list_matches_the_app() {
    // This crate cannot import the app, so WINDOW_LABELS is duplicated above.
    // Parse the real declaration and fail if the copy drifts, rather than
    // letting the checks above quietly stop covering a window.
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
         WINDOW_LABELS in acl-tests/tests/acl.rs so the ACL checks cover every window."
    );
}

fn main() {
    let mut failed = Vec::new();

    let mut check = |name: &str, f: &mut dyn FnMut()| {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(()) => println!("ok   {name}"),
            Err(_) => {
                println!("FAIL {name}");
                failed.push(name.to_owned());
            }
        }
    };

    check("window_label_list_matches_the_app", &mut || {
        window_label_list_matches_the_app()
    });

    // One app for the window checks: building it is the expensive part, and the
    // labels are distinct so they cannot interfere.
    let app = test_app();
    check(
        "every_window_the_app_builds_can_register_event_listeners",
        &mut || every_window_the_app_builds_can_register_event_listeners(&app),
    );
    check("an_undeclared_window_is_denied", &mut || {
        an_undeclared_window_is_denied(&app)
    });

    if failed.is_empty() {
        println!("\nacl: 3 checks passed");
    } else {
        eprintln!("\nacl: {} check(s) FAILED: {}", failed.len(), failed.join(", "));
        std::process::exit(1);
    }
}
