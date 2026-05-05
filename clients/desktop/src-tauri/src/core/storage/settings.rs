use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub capture_enabled: bool,
    pub deny_list: Vec<String>,
    pub autostart: bool,
    pub hotkey: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            capture_enabled: true,
            deny_list: vec![
                "com.1password.1password".into(),
                "com.bitwarden.desktop".into(),
                "1Password.exe".into(),
                "Bitwarden.exe".into(),
            ],
            autostart: false,
            hotkey: None,
        }
    }
}

const KEY: &str = "settings";

pub fn load(conn: &Connection) -> Result<Settings, AppError> {
    let json: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![KEY], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    match json {
        Some(j) => serde_json::from_str(&j).map_err(|e| AppError::Storage(e.to_string())),
        None => Ok(Settings::default()),
    }
}

pub fn save(conn: &Connection, s: &Settings) -> Result<(), AppError> {
    let j = serde_json::to_string(s).map_err(|e| AppError::Storage(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![KEY, j],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    #[test]
    fn load_returns_default_when_unset() {
        let c = open_in_memory().unwrap();
        let s = load(&c).unwrap();
        assert!(s.capture_enabled);
        assert!(!s.autostart);
        assert!(s.hotkey.is_none());
    }

    #[test]
    fn default_deny_list_includes_macos_and_windows_password_managers() {
        let s = Settings::default();

        for app_id in [
            "com.1password.1password",
            "com.bitwarden.desktop",
            "1Password.exe",
            "Bitwarden.exe",
        ] {
            assert!(
                s.deny_list.iter().any(|entry| entry == app_id),
                "missing default deny-list entry: {app_id}"
            );
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let c = open_in_memory().unwrap();
        let mut s = Settings::default();
        s.capture_enabled = false;
        s.hotkey = Some("Cmd+Shift+V".into());
        save(&c, &s).unwrap();
        assert_eq!(load(&c).unwrap(), s);
    }
}
