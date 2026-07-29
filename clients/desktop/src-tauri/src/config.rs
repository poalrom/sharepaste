use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct Paths {
    pub(crate) data_dir: PathBuf,
    pub(crate) log_dir: PathBuf,
    pub(crate) cache_dir: PathBuf,
    pub(crate) db_path: PathBuf,
}

impl Paths {
    // The data directory arrives as data, never derived here: a sandboxed platform has
    // one handed to it and must not be able to fall back on a home directory.
    pub(crate) fn for_data_dir(data_dir: PathBuf) -> Self {
        let log_dir = default_log_dir(&data_dir);
        let cache_dir = default_cache_dir(&data_dir);
        let db_path = data_dir.join("state.sqlite");
        Self { data_dir, log_dir, cache_dir, db_path }
    }

    pub(crate) fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.log_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        Ok(())
    }
}

// The desktop shell's own derivation, deliberately separate from the construction above:
// `SHAREPASTE_DATA_DIR` wins so a dev build can be pointed at a scratch directory.
pub(crate) fn desktop_data_dir() -> PathBuf {
    match std::env::var_os("SHAREPASTE_DATA_DIR") {
        Some(p) => PathBuf::from(p),
        None => default_data_dir(),
    }
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Application Support/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-data"))
}

#[cfg(not(target_os = "macos"))]
fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-data"))
}

#[cfg(target_os = "macos")]
fn default_log_dir(_data: &Path) -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Logs/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-logs"))
}

#[cfg(not(target_os = "macos"))]
fn default_log_dir(data: &Path) -> PathBuf {
    data.join("logs")
}

#[cfg(target_os = "macos")]
fn default_cache_dir(_data: &Path) -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Caches/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-cache"))
}

#[cfg(not(target_os = "macos"))]
fn default_cache_dir(data: &Path) -> PathBuf {
    data.join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_path_sits_inside_the_given_data_dir() {
        let p = Paths::for_data_dir(PathBuf::from("/tmp/sp-test-1"));
        assert_eq!(p.data_dir, PathBuf::from("/tmp/sp-test-1"));
        assert_eq!(p.db_path, PathBuf::from("/tmp/sp-test-1/state.sqlite"));
    }

    #[test]
    fn ensure_dirs_creates_all_three() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::for_data_dir(tmp.path().join("data"));
        p.ensure_dirs().unwrap();
        assert!(p.data_dir.is_dir());
        assert!(p.log_dir.is_dir());
        assert!(p.cache_dir.is_dir());
    }
}
