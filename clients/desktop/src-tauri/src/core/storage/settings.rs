use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub capture_enabled: bool,
    pub deny_list: Vec<String>,
    pub autostart: bool,
    pub hotkey: Option<String>,
    #[serde(default)]
    pub last_active_user_id: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Settings {
            capture_enabled: true,
            deny_list: Vec::new(),
            autostart: false,
            hotkey: None,
            last_active_user_id: None,
        };
        append_builtin_deny_list_entries(&mut settings);
        settings
    }
}

const KEY: &str = "settings";
const BUILTIN_DENY_LIST_ENTRIES: &[&str] = &[
    "com.1password.1password",
    "com.bitwarden.desktop",
    "1Password.exe",
    "Bitwarden.exe",
];

fn append_builtin_deny_list_entries(settings: &mut Settings) {
    for entry in BUILTIN_DENY_LIST_ENTRIES {
        if !settings
            .deny_list
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(entry))
        {
            settings.deny_list.push((*entry).into());
        }
    }
}

pub fn load(conn: &Connection) -> Result<Settings, AppError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    match json {
        Some(j) => {
            let mut settings: Settings =
                serde_json::from_str(&j).map_err(|e| AppError::Storage(e.to_string()))?;
            append_builtin_deny_list_entries(&mut settings);
            Ok(settings)
        }
        None => Ok(Settings::default()),
    }
}

pub fn save(conn: &Connection, s: &Settings) -> Result<(), AppError> {
    let mut settings = s.clone();
    append_builtin_deny_list_entries(&mut settings);
    let j = serde_json::to_string(&settings).map_err(|e| AppError::Storage(e.to_string()))?;
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

    #[test]
    fn load_upgrades_existing_persisted_deny_list_with_windows_password_managers() {
        let c = open_in_memory().unwrap();
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params![
                KEY,
                r#"{"capture_enabled":true,"deny_list":["com.1password.1password","com.bitwarden.desktop","CustomApp.exe"],"autostart":true,"hotkey":"Ctrl+Shift+V"}"#
            ],
        )
        .unwrap();

        let s = load(&c).unwrap();

        assert_eq!(s.capture_enabled, true);
        assert_eq!(s.autostart, true);
        assert_eq!(s.hotkey, Some("Ctrl+Shift+V".into()));
        assert!(s.deny_list.contains(&"CustomApp.exe".into()));
        assert!(s.deny_list.contains(&"1Password.exe".into()));
        assert!(s.deny_list.contains(&"Bitwarden.exe".into()));
    }

    #[test]
    fn save_persists_normalized_deny_list_and_preserves_custom_entries() {
        let c = open_in_memory().unwrap();
        let s = Settings {
            capture_enabled: false,
            deny_list: vec!["CustomApp.exe".into()],
            autostart: true,
            hotkey: Some("Ctrl+Shift+V".into()),
            last_active_user_id: None,
        };

        save(&c, &s).unwrap();

        let loaded = load(&c).unwrap();
        assert_eq!(loaded.capture_enabled, false);
        assert_eq!(loaded.autostart, true);
        assert_eq!(loaded.hotkey, Some("Ctrl+Shift+V".into()));
        assert!(loaded.deny_list.contains(&"CustomApp.exe".into()));
        assert!(loaded.deny_list.contains(&"com.1password.1password".into()));
        assert!(loaded.deny_list.contains(&"com.bitwarden.desktop".into()));
        assert!(loaded.deny_list.contains(&"1Password.exe".into()));
        assert!(loaded.deny_list.contains(&"Bitwarden.exe".into()));
    }

    #[test]
    fn save_then_load_round_trips_last_active_user_id() {
        let c = open_in_memory().unwrap();
        let mut s = Settings::default();
        s.last_active_user_id = Some("user-1".into());
        save(&c, &s).unwrap();
        let loaded = load(&c).unwrap();
        assert_eq!(loaded.last_active_user_id, Some("user-1".into()));
    }

    #[test]
    fn load_returns_none_for_last_active_user_id_when_field_missing() {
        let c = open_in_memory().unwrap();
        let legacy = r#"{"capture_enabled":true,"deny_list":[],"autostart":false,"hotkey":null}"#;
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params![KEY, legacy],
        ).unwrap();
        let s = load(&c).unwrap();
        assert!(s.last_active_user_id.is_none());
    }

    #[test]
    fn load_does_not_add_case_insensitive_duplicate_deny_list_entries() {
        let c = open_in_memory().unwrap();
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params![
                KEY,
                r#"{"capture_enabled":true,"deny_list":["com.1password.1password","com.bitwarden.desktop","1password.exe","bitwarden.exe"],"autostart":false,"hotkey":null}"#
            ],
        )
        .unwrap();

        let s = load(&c).unwrap();

        assert_eq!(
            s.deny_list
                .iter()
                .filter(|entry| entry.eq_ignore_ascii_case("1Password.exe"))
                .count(),
            1
        );
        assert_eq!(
            s.deny_list
                .iter()
                .filter(|entry| entry.eq_ignore_ascii_case("Bitwarden.exe"))
                .count(),
            1
        );
    }
}
