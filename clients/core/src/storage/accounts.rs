use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};

/// A Pairing as stored locally.
///
/// `username` and `last_contact_at` are mirrored and derived respectively —
/// neither is supplied by the pairing path, so [`upsert`] leaves both alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub user_id: String,
    pub device_id: String,
    pub device_label: String,
    pub server_url: String,
    pub last_seen_id: i64,
    pub created_at: i64,
    pub username: Option<String>,
    pub last_contact_at: Option<i64>,
}

const COLUMNS: &str =
    "user_id, device_id, device_label, server_url, last_seen_id, created_at, username, last_contact_at";

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        user_id: r.get(0)?, device_id: r.get(1)?, device_label: r.get(2)?,
        server_url: r.get(3)?, last_seen_id: r.get(4)?, created_at: r.get(5)?,
        username: r.get(6)?, last_contact_at: r.get(7)?,
    })
}

pub fn upsert(conn: &Connection, a: &Account) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO accounts (user_id, device_id, device_label, server_url, last_seen_id, created_at, username, last_contact_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (user_id) DO UPDATE SET
            device_id    = excluded.device_id,
            device_label = excluded.device_label,
            server_url   = excluded.server_url",
        params![a.user_id, a.device_id, a.device_label, a.server_url, a.last_seen_id, a.created_at,
                a.username, a.last_contact_at],
    )?;
    Ok(())
}

pub(crate) fn list(conn: &Connection) -> Result<Vec<Account>, AppError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM accounts ORDER BY created_at ASC"))?;
    let rows = stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find(conn: &Connection, user_id: &str) -> Result<Option<Account>, AppError> {
    let row = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM accounts WHERE user_id = ?1"),
            params![user_id],
            map_row,
        )
        .optional()?;
    Ok(row)
}

pub fn set_last_seen(conn: &Connection, user_id: &str, last_seen_id: i64) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE accounts SET last_seen_id = ?2 WHERE user_id = ?1",
        params![user_id, last_seen_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("account {user_id}")));
    }
    Ok(())
}

/// Mirror the username the relay reports for this user.
pub fn set_username(conn: &Connection, user_id: &str, username: &str) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE accounts SET username = ?2 WHERE user_id = ?1",
        params![user_id, username],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("account {user_id}")));
    }
    Ok(())
}

/// Persist Contact — the last moment the relay was heard from.
pub fn set_last_contact(conn: &Connection, user_id: &str, at: i64) -> Result<(), AppError> {
    let n = conn.execute(
        "UPDATE accounts SET last_contact_at = ?2 WHERE user_id = ?1",
        params![user_id, at],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("account {user_id}")));
    }
    Ok(())
}

pub(crate) fn delete(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM accounts WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_in_memory;

    fn acct(uid: &str) -> Account {
        Account {
            user_id: uid.into(), device_id: "d1".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
            username: None, last_contact_at: None,
        }
    }

    #[test]
    fn upsert_then_find_returns_row() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.device_id, "d1");
    }

    #[test]
    fn upsert_updates_label_and_url_but_keeps_last_seen() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        set_last_seen(&c, "u", 42).unwrap();
        let mut a = acct("u");
        a.device_label = "renamed".into();
        a.server_url = "https://other".into();
        a.last_seen_id = 0;
        upsert(&c, &a).unwrap();
        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.device_label, "renamed");
        assert_eq!(got.server_url, "https://other");
        assert_eq!(got.last_seen_id, 42);
    }

    #[test]
    fn mirrored_username_and_contact_round_trip() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        assert_eq!(find(&c, "u").unwrap().unwrap().username, None);

        set_username(&c, "u", "alice").unwrap();
        set_last_contact(&c, "u", 1_700_000_000_000).unwrap();

        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.username.as_deref(), Some("alice"));
        assert_eq!(got.last_contact_at, Some(1_700_000_000_000));
        assert_eq!(list(&c).unwrap()[0], got);
    }

    #[test]
    fn upsert_does_not_clobber_mirrored_username_or_contact() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        set_username(&c, "u", "alice").unwrap();
        set_last_contact(&c, "u", 99).unwrap();

        // The pairing path re-upserts with neither field populated.
        upsert(&c, &acct("u")).unwrap();

        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.username.as_deref(), Some("alice"));
        assert_eq!(got.last_contact_at, Some(99));
    }

    #[test]
    fn mirror_setters_return_not_found_when_missing() {
        let c = open_in_memory().unwrap();
        assert!(matches!(set_username(&c, "ghost", "x").unwrap_err(), AppError::NotFound(_)));
        assert!(matches!(set_last_contact(&c, "ghost", 1).unwrap_err(), AppError::NotFound(_)));
    }

    #[test]
    fn list_orders_by_created_at() {
        let c = open_in_memory().unwrap();
        let mut a = acct("a"); a.created_at = 2;
        let mut b = acct("b"); b.created_at = 1;
        upsert(&c, &a).unwrap();
        upsert(&c, &b).unwrap();
        let ids: Vec<_> = list(&c).unwrap().iter().map(|x| x.user_id.clone()).collect();
        assert_eq!(ids, vec!["b", "a"]);
    }

    #[test]
    fn delete_returns_row_count() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        assert_eq!(delete(&c, "u").unwrap(), 1);
        assert_eq!(delete(&c, "u").unwrap(), 0);
    }

    #[test]
    fn set_last_seen_returns_not_found_when_missing() {
        let c = open_in_memory().unwrap();
        let err = set_last_seen(&c, "ghost", 7).unwrap_err();
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("ghost"), "got: {msg}"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
