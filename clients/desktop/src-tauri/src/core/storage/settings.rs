use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Settings {
    pub(crate) capture_enabled: bool,
    pub(crate) deny_list: Vec<String>,
    pub(crate) autostart: bool,
    pub(crate) hotkey: Option<String>,
    #[serde(default)]
    pub(crate) last_active_user_id: Option<String>,
    /// Whether the app asks the Update Source for a newer release at launch.
    ///
    /// Explicitly defaulted rather than left to `#[serde(default)]`: a bool's
    /// serde default is `false`, which would read every settings row written
    /// before the updater shipped as an opt-out the user never made.
    #[serde(default = "update_check_default")]
    pub(crate) update_check_enabled: bool,
}

fn update_check_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Settings {
            capture_enabled: true,
            deny_list: Vec::new(),
            autostart: false,
            hotkey: Some(DEFAULT_HOTKEY.to_string()),
            last_active_user_id: None,
            update_check_enabled: update_check_default(),
        };
        append_builtin_deny_list_entries(&mut settings);
        settings
    }
}

const KEY: &str = "settings";
/// Shipped so the popover has a keyboard entry point on a fresh profile; the
/// user can rebind or clear it from Settings.
pub(crate) const DEFAULT_HOTKEY: &str = "CmdOrCtrl+Shift+V";
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

pub(crate) fn load(conn: &Connection) -> Result<Settings, AppError> {
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

pub(crate) fn save(conn: &Connection, s: &Settings) -> Result<(), AppError> {
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

    fn store(conn: &Connection, json: &str) {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            params![KEY, json],
        )
        .unwrap();
    }

    #[test]
    fn defaults_enable_capture_bind_a_hotkey_and_deny_password_managers() {
        let c = open_in_memory().unwrap();
        let s = load(&c).unwrap();
        assert_eq!(s, Settings::default(), "load() on an empty table must yield the defaults");

        assert!(s.capture_enabled);
        assert!(!s.autostart);
        // The popover must be reachable by keyboard on a fresh profile, without
        // the user first discovering the Settings field.
        assert_eq!(s.hotkey.as_deref(), Some(DEFAULT_HOTKEY));
        assert!(s.last_active_user_id.is_none());
        assert_eq!(s.deny_list.as_slice(), BUILTIN_DENY_LIST_ENTRIES);
    }

    #[test]
    fn every_persisted_deny_list_gains_the_builtin_entries_exactly_once() {
        fn from_legacy_row() -> Vec<String> {
            let c = open_in_memory().unwrap();
            store(
                &c,
                r#"{"capture_enabled":true,"deny_list":["com.1password.1password","com.bitwarden.desktop","CustomApp.exe"],"autostart":true,"hotkey":"Ctrl+Shift+V"}"#,
            );
            load(&c).unwrap().deny_list
        }

        fn from_mixed_case_row() -> Vec<String> {
            let c = open_in_memory().unwrap();
            store(
                &c,
                r#"{"capture_enabled":true,"deny_list":["com.1password.1password","com.bitwarden.desktop","1password.exe","bitwarden.exe"],"autostart":false,"hotkey":null}"#,
            );
            load(&c).unwrap().deny_list
        }

        fn from_save_of_custom_only_list() -> Vec<String> {
            let c = open_in_memory().unwrap();
            save(
                &c,
                &Settings {
                    deny_list: vec!["CustomApp.exe".into()],
                    ..Settings::default()
                },
            )
            .unwrap();
            load(&c).unwrap().deny_list
        }

        // (label, how the stored list was produced, custom entries that must survive)
        let cases: &[(&str, fn() -> Vec<String>, &[&str])] = &[
            (
                "row written before the Windows managers existed",
                from_legacy_row,
                &["CustomApp.exe"],
            ),
            (
                "row whose builtin entries differ from ours only by case",
                from_mixed_case_row,
                &[],
            ),
            (
                "list that went through save() carrying only a custom entry",
                from_save_of_custom_only_list,
                &["CustomApp.exe"],
            ),
        ];

        for (label, produce, customs) in cases {
            let list = produce();
            for builtin in BUILTIN_DENY_LIST_ENTRIES {
                assert_eq!(
                    list.iter()
                        .filter(|e| e.eq_ignore_ascii_case(builtin))
                        .count(),
                    1,
                    "{label}: expected exactly one {builtin} in {list:?}"
                );
            }
            for custom in *customs {
                assert!(
                    list.iter().any(|e| e == custom),
                    "{label}: dropped custom entry {custom} from {list:?}"
                );
            }
            assert_eq!(
                list.len(),
                BUILTIN_DENY_LIST_ENTRIES.len() + customs.len(),
                "{label}: unexpected deny list {list:?}"
            );
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let c = open_in_memory().unwrap();
        let mut s = Settings::default();
        s.capture_enabled = false;
        s.autostart = true;
        s.hotkey = Some("Cmd+Shift+V".into());
        save(&c, &s).unwrap();
        assert_eq!(load(&c).unwrap(), s);
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
        store(
            &c,
            r#"{"capture_enabled":true,"deny_list":[],"autostart":false,"hotkey":null}"#,
        );
        let s = load(&c).unwrap();
        assert!(s.last_active_user_id.is_none());
        // A persisted null hotkey means the user cleared it; the default must not
        // silently rebind it.
        assert!(s.hotkey.is_none());
    }

    #[test]
    fn the_automatic_update_check_defaults_on_for_fresh_and_pre_existing_profiles() {
        let fresh = open_in_memory().unwrap();
        assert!(load(&fresh).unwrap().update_check_enabled);

        // Rows written before the updater existed carry no such field. Plain
        // `#[serde(default)]` reads a missing bool as `false`, which would opt
        // every install that predates this release out of ever hearing about
        // the next one — silently, and with the toggle claiming otherwise.
        let legacy = open_in_memory().unwrap();
        store(
            &legacy,
            r#"{"capture_enabled":true,"deny_list":[],"autostart":false,"hotkey":null}"#,
        );
        assert!(load(&legacy).unwrap().update_check_enabled);

        let opted_out = open_in_memory().unwrap();
        save(
            &opted_out,
            &Settings { update_check_enabled: false, ..Settings::default() },
        )
        .unwrap();
        assert!(!load(&opted_out).unwrap().update_check_enabled);
    }
}
