//! Pure-Rust core for sharepaste-desktop. No `tauri::*` imports allowed in
//! this module tree — everything below is testable without a Tauri runtime.

pub mod crypto;
