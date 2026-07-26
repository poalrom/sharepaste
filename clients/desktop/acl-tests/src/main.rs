//! Placeholder binary.
//!
//! `tauri_build::build()` emits `cargo:rustc-link-arg-bins`, which Cargo rejects
//! unless the crate has a bin target. This crate exists only for its tests; the
//! binary is never run.
fn main() {
    eprintln!("sharepaste-acl-tests is a test-only crate; run `cargo test`.");
}
