use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

/// One row of the local mirror of the relay's device list.
///
/// Deliberately not `http::dto::DeviceDto`: the mirror is storage's shape and
/// must not move whenever the wire does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeviceRecord {
    pub(crate) device_id: String,
    pub(crate) label: Option<String>,
    pub(crate) revoked_at: Option<i64>,
}

/// Replace what the mirror knows about `user_id`'s devices, in one transaction.
///
/// Devices the relay no longer lists are left alone rather than deleted: an
/// entry captured on a since-removed device still wants its Origin.
pub(crate) fn upsert_many(
    conn: &Connection,
    user_id: &str,
    devices: &[DeviceRecord],
    now: i64,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO devices (user_id, device_id, label, revoked_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
                label      = excluded.label,
                revoked_at = excluded.revoked_at,
                updated_at = excluded.updated_at",
        )?;
        for d in devices {
            stmt.execute(params![user_id, d.device_id, d.label, d.revoked_at, now])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// `device_id -> label` for one user.
///
/// Devices the relay holds no label for are omitted, so a caller reading a
/// missing key cannot tell "unlabelled" from "unknown" — ask [`is_mirrored`]
/// when that difference matters.
pub(crate) fn map_for(conn: &Connection, user_id: &str) -> Result<HashMap<String, String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT device_id, label FROM devices WHERE user_id = ?1 AND label IS NOT NULL",
    )?;
    let rows = stmt
        .query_map(params![user_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<HashMap<String, String>, _>>()?;
    Ok(rows)
}

/// Whether the mirror has heard of this device at all, label or not.
///
/// The one signal that the mirror is stale: an entry from an unknown device
/// means a `GET /me` is worth making, whereas an entry from a known but
/// unlabelled device is a steady state and must not provoke one.
pub(crate) fn is_mirrored(conn: &Connection, user_id: &str, device_id: &str) -> Result<bool, AppError> {
    let hit: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM devices WHERE user_id = ?1 AND device_id = ?2",
            params![user_id, device_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

/// Drop everything the mirror holds for one user, when the Pairing is forgotten.
pub(crate) fn delete_all(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM devices WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    fn dev(id: &str, label: Option<&str>) -> DeviceRecord {
        DeviceRecord { device_id: id.into(), label: label.map(Into::into), revoked_at: None }
    }

    #[test]
    fn upsert_then_map_returns_labelled_devices() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("mac")), dev("d2", Some("iphone"))], 10).unwrap();
        let m = map_for(&c, "u").unwrap();
        assert_eq!(m.get("d1").map(String::as_str), Some("mac"));
        assert_eq!(m.get("d2").map(String::as_str), Some("iphone"));
    }

    #[test]
    fn map_omits_unlabelled_devices_but_mirror_still_knows_them() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", None)], 10).unwrap();
        assert!(map_for(&c, "u").unwrap().is_empty());
        assert!(is_mirrored(&c, "u", "d1").unwrap());
        assert!(!is_mirrored(&c, "u", "ghost").unwrap());
    }

    #[test]
    fn upsert_updates_a_changed_label_in_place() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("mac"))], 10).unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("macbook"))], 20).unwrap();
        let m = map_for(&c, "u").unwrap();
        assert_eq!(m.len(), 1, "must update, not duplicate: {m:?}");
        assert_eq!(m.get("d1").map(String::as_str), Some("macbook"));
        let updated: i64 = c
            .query_row("SELECT updated_at FROM devices WHERE user_id='u' AND device_id='d1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(updated, 20);
    }

    #[test]
    fn upsert_can_clear_a_label() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("mac"))], 10).unwrap();
        upsert_many(&c, "u", &[dev("d1", None)], 20).unwrap();
        assert!(map_for(&c, "u").unwrap().is_empty());
    }

    #[test]
    fn map_is_scoped_to_one_user() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("mine"))], 10).unwrap();
        upsert_many(&c, "other", &[dev("d1", Some("theirs"))], 10).unwrap();
        assert_eq!(map_for(&c, "u").unwrap().get("d1").map(String::as_str), Some("mine"));
        assert!(!is_mirrored(&c, "nobody", "d1").unwrap());
    }

    #[test]
    fn delete_all_drops_only_that_users_mirror() {
        let c = open_in_memory().unwrap();
        upsert_many(&c, "u", &[dev("d1", Some("mac")), dev("d2", None)], 10).unwrap();
        upsert_many(&c, "other", &[dev("d1", Some("theirs"))], 10).unwrap();
        assert_eq!(delete_all(&c, "u").unwrap(), 2);
        assert!(!is_mirrored(&c, "u", "d1").unwrap());
        assert!(is_mirrored(&c, "other", "d1").unwrap());
    }

    #[test]
    fn revoked_devices_are_mirrored_and_keep_their_label() {
        let c = open_in_memory().unwrap();
        let revoked = DeviceRecord { device_id: "d1".into(), label: Some("old-laptop".into()), revoked_at: Some(99) };
        upsert_many(&c, "u", &[revoked], 10).unwrap();
        assert_eq!(map_for(&c, "u").unwrap().get("d1").map(String::as_str), Some("old-laptop"));
    }
}
