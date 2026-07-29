use crate::errors::AppError;
use data_encoding::BASE32_NOPAD;
use uuid::Uuid;

const VERSION: u8 = 1;
const SECRET_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcodePayload {
    pub server_url: String,
    pub pair_id: Uuid,
    pub pairing_secret: [u8; SECRET_LEN],
}

pub(crate) fn encode(p: &ShortcodePayload) -> Result<String, AppError> {
    let url_bytes = p.server_url.as_bytes();
    let url_len: u16 = url_bytes
        .len()
        .try_into()
        .map_err(|_| AppError::BadInput("server_url too long".into()))?;
    let mut buf = Vec::with_capacity(1 + 2 + url_bytes.len() + 16 + SECRET_LEN);
    buf.push(VERSION);
    buf.extend_from_slice(&url_len.to_be_bytes());
    buf.extend_from_slice(url_bytes);
    buf.extend_from_slice(p.pair_id.as_bytes());
    buf.extend_from_slice(&p.pairing_secret);
    Ok(BASE32_NOPAD.encode(&buf))
}

pub fn decode(s: &str) -> Result<ShortcodePayload, AppError> {
    let cleaned: String = s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let bytes = BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map_err(|e| AppError::BadInput(format!("base32 decode: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::BadInput("empty payload".into()));
    }
    if bytes[0] != VERSION {
        return Err(AppError::BadInput(format!("unknown version {}", bytes[0])));
    }
    if bytes.len() < 3 {
        return Err(AppError::BadInput("payload truncated".into()));
    }
    let url_len = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    let url_start = 3;
    let url_end = url_start + url_len;
    if bytes.len() < url_end + 16 + SECRET_LEN {
        return Err(AppError::BadInput("payload truncated".into()));
    }
    let server_url = std::str::from_utf8(&bytes[url_start..url_end])
        .map_err(|_| AppError::BadInput("server_url not utf-8".into()))?
        .to_string();
    let mut id_bytes = [0u8; 16];
    id_bytes.copy_from_slice(&bytes[url_end..url_end + 16]);
    let pair_id = Uuid::from_bytes(id_bytes);
    let mut secret = [0u8; SECRET_LEN];
    secret.copy_from_slice(&bytes[url_end + 16..url_end + 16 + SECRET_LEN]);
    Ok(ShortcodePayload { server_url, pair_id, pairing_secret: secret })
}

pub fn group_for_display(code: &str) -> String {
    code.chars()
        .collect::<Vec<_>>()
        .chunks(5)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ShortcodePayload {
        ShortcodePayload {
            server_url: "https://srv.example".into(),
            pair_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
            pairing_secret: [7u8; SECRET_LEN],
        }
    }

    #[test]
    fn round_trip() {
        let p = sample();
        let code = encode(&p).unwrap();
        assert_eq!(decode(&code).unwrap(), p);
    }

    #[test]
    fn round_trip_with_whitespace_and_dashes_and_lowercase() {
        let p = sample();
        let code = encode(&p).unwrap();
        let formatted = format!("  {}  ", group_for_display(&code).to_lowercase().replace(' ', "-"));
        assert_eq!(decode(&formatted).unwrap(), p);
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(decode(""), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(decode("not a real code"), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut bytes = vec![99u8, 0, 0];
        bytes.extend_from_slice(Uuid::nil().as_bytes());
        bytes.extend_from_slice(&[0u8; SECRET_LEN]);
        let s = BASE32_NOPAD.encode(&bytes);
        assert!(matches!(decode(&s), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_truncated() {
        let p = sample();
        let mut code = encode(&p).unwrap();
        code.truncate(code.len() - 4);
        assert!(matches!(decode(&code), Err(AppError::BadInput(_))));
    }

    #[test]
    fn group_for_display_groups_in_fives() {
        let s = "ABCDEFGHIJKLMN";
        assert_eq!(group_for_display(s), "ABCDE FGHIJ KLMN");
    }
}
