use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct Paths {
    pub(crate) data_dir: PathBuf,
    pub(crate) log_dir: PathBuf,
    pub(crate) cache_dir: PathBuf,
    pub(crate) db_path: PathBuf,
}

impl Paths {
    pub(crate) fn resolve() -> Self {
        Self::resolve_with_env(std::env::var_os("SHAREPASTE_DATA_DIR"))
    }

    pub(crate) fn resolve_with_env(override_data_dir: Option<std::ffi::OsString>) -> Self {
        let data_dir = match override_data_dir {
            Some(p) => PathBuf::from(p),
            None => default_data_dir(),
        };
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
    use std::ffi::OsString;

    #[test]
    fn override_data_dir_is_honoured() {
        let p = Paths::resolve_with_env(Some(OsString::from("/tmp/sp-test-1")));
        assert_eq!(p.data_dir, PathBuf::from("/tmp/sp-test-1"));
        assert_eq!(p.db_path, PathBuf::from("/tmp/sp-test-1/state.sqlite"));
    }

    #[test]
    fn ensure_dirs_creates_all_three() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::resolve_with_env(Some(tmp.path().join("data").into_os_string()));
        p.ensure_dirs().unwrap();
        assert!(p.data_dir.is_dir());
        assert!(p.log_dir.is_dir());
        assert!(p.cache_dir.is_dir());
    }
}
