use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub user_id: String,
    pub device_id: String,
    pub device_label: String,
    pub server_url: String,
    pub last_seen_id: i64,
    pub created_at: i64,
}

pub fn upsert(conn: &Connection, a: &Account) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO accounts (user_id, device_id, device_label, server_url, last_seen_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (user_id) DO UPDATE SET
            device_id    = excluded.device_id,
            device_label = excluded.device_label,
            server_url   = excluded.server_url",
        params![a.user_id, a.device_id, a.device_label, a.server_url, a.last_seen_id, a.created_at],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Account>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT user_id, device_id, device_label, server_url, last_seen_id, created_at
         FROM accounts ORDER BY created_at ASC"
    )?;
    let rows = stmt
        .query_map([], |r| Ok(Account {
            user_id: r.get(0)?, device_id: r.get(1)?, device_label: r.get(2)?,
            server_url: r.get(3)?, last_seen_id: r.get(4)?, created_at: r.get(5)?,
        }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find(conn: &Connection, user_id: &str) -> Result<Option<Account>, AppError> {
    let row = conn
        .query_row(
            "SELECT user_id, device_id, device_label, server_url, last_seen_id, created_at
             FROM accounts WHERE user_id = ?1",
            params![user_id],
            |r| Ok(Account {
                user_id: r.get(0)?, device_id: r.get(1)?, device_label: r.get(2)?,
                server_url: r.get(3)?, last_seen_id: r.get(4)?, created_at: r.get(5)?,
            }),
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

pub fn delete(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM accounts WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    fn acct(uid: &str) -> Account {
        Account {
            user_id: uid.into(), device_id: "d1".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
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
