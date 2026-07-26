pub(crate) mod filter;

#[cfg(target_os = "macos")]
pub(crate) mod macos;

#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) mod watcher;
