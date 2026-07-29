//! The renderings both shells would otherwise each invent.
//!
//! Everything here turns protocol data into the exact string a list, a footer
//! or a row shows, and every one of them was written twice before it was
//! written here — once in TypeScript and once in Kotlin, at two different
//! limits or with two different URL parsers. ADR 0006 reuses the Rust precisely
//! so a rule both shells need has one owner and one test; a rendering a shell
//! can get *wrong* is such a rule.
//!
//! What is **not** here is layout. How many characters of the result fit a
//! popover row, whether the origin is shown at all, what typeface it is in —
//! those differ per surface and belong to the surface. The line is: if two
//! shells could disagree about the *answer*, it is here; if they can only
//! disagree about the *space*, it is theirs.

/// How long a [`preview`] may be, in characters.
///
/// Long enough to tell two entries apart in a list, short enough that a
/// hundred of them are a hundred short strings rather than a hundred documents:
/// the history query returns up to a hundred rows and every one of them carries
/// its own plaintext alongside.
const PREVIEW_CHARS: usize = 80;

/// An entry's **Preview**: its plaintext as a list shows it.
///
/// One line, control characters flattened to spaces, trimmed, and capped. The
/// trim is the part that is easy to leave out and impossible to miss once it is
/// missing — an indented entry whose first characters are a newline and two
/// tabs renders as a blank row, which looks exactly like a bug in the sync.
///
/// Truncation counts characters rather than bytes, so a cap can never split a
/// multi-byte character and produce a broken glyph.
pub fn preview(plaintext: &str) -> String {
    plaintext
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(PREVIEW_CHARS)
        .collect()
}

/// How many characters of a Device id stand in for a missing Device Label.
///
/// Enough to tell two devices apart in one list, and deliberately not enough to
/// be mistaken for something a person chose.
const ORIGIN_ID_CHARS: usize = 4;

/// An entry's **Origin**, as a row names it.
///
/// The Device Label when the mirror has one, and a short slice of the Device id
/// when it does not. The fallback is not defensive: ADR 0001 permits a pairing
/// with no label, and a device paired since the last `GET /me` has none yet, so
/// a row that assumed one would render nothing at all for a real case.
///
/// A label that is present but blank counts as absent — it names no device
/// better than the empty string does.
pub fn origin_label(device_label: Option<&str>, device_id: &str) -> String {
    match device_label.map(str::trim) {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => device_id.chars().take(ORIGIN_ID_CHARS).collect(),
    }
}

/// A **Relay** as a person reads it: host and port, no scheme, no path, and no
/// credentials.
///
/// A Pairing is identified by User-on-Relay and that pair has to fit a footer,
/// so the scheme and trailing slash go — they carry nothing a reader uses to
/// tell two relays apart. The port stays only when it is not the scheme's
/// default, which is what makes two relays on one host distinguishable without
/// making every ordinary one read `:443`.
///
/// Dropping any userinfo is the part worth stating: a `server_url` may legally
/// carry `user:password@`, and a shell that sliced the string between `://` and
/// `/` would print that password into a footer, a pairing card and a
/// confirmation prompt.
///
/// An address typed with no scheme is not a URL and does not parse, so the same
/// slice is taken by hand rather than left to a second rule in a shell.
pub fn relay_host(server_url: &str) -> String {
    let trimmed = server_url.trim();
    if let Ok(u) = url::Url::parse(trimmed) {
        if let Some(host) = u.host_str() {
            return match u.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
        }
    }
    authority_of(trimmed)
}

/// The authority of something that did not parse as a URL with a host.
///
/// `relay.example:8443` reads as a scheme and an opaque path to a URL parser,
/// not as a host and a port, and it is what a person types.
fn authority_of(s: &str) -> String {
    let after_scheme = s.split_once("://").map_or(s, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if host.is_empty() {
        s.to_string()
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_one_trimmed_line() {
        assert_eq!(preview("hello"), "hello");
        assert_eq!(preview("a\nb\tc"), "a b c");
    }

    /*
     * The bug this whole function exists for. An entry that begins with a
     * newline and indentation has to render as its first *words*, not as the
     * whitespace in front of them, or the row is blank and the person reads it
     * as a sync failure.
     */
    #[test]
    fn preview_of_an_indented_entry_starts_at_its_first_word() {
        let p = preview("\n\t  cafe on the corner\n\n  8pm\n");
        assert_eq!(p, "cafe on the corner    8pm");
        assert!(!p.starts_with(char::is_whitespace));
    }

    #[test]
    fn preview_caps_at_eighty_characters_without_splitting_one() {
        let ascii: String = std::iter::repeat('z').take(200).collect();
        assert_eq!(preview(&ascii).chars().count(), PREVIEW_CHARS);

        // Every character three bytes wide: a byte-counting cap would slice one
        // in half and produce a replacement glyph.
        let wide: String = std::iter::repeat('あ').take(200).collect();
        let capped = preview(&wide);
        assert_eq!(capped.chars().count(), PREVIEW_CHARS);
        assert_eq!(capped.len(), PREVIEW_CHARS * 3);
    }

    #[test]
    fn preview_of_nothing_is_nothing() {
        assert_eq!(preview(""), "");
        assert_eq!(preview("   \n\t "), "");
    }

    #[test]
    fn origin_label_prefers_the_mirrored_device_label() {
        assert_eq!(origin_label(Some("iphone-15"), "abcdef123456"), "iphone-15");
    }

    #[test]
    fn origin_label_falls_back_to_a_short_device_id_slice() {
        assert_eq!(origin_label(None, "abcdef123456"), "abcd");
        assert_eq!(origin_label(Some("   "), "abcdef123456"), "abcd");
    }

    #[test]
    fn relay_host_drops_the_scheme_and_the_path() {
        assert_eq!(relay_host("https://relay.example/"), "relay.example");
        assert_eq!(relay_host("https://relay.example/sync?x=1"), "relay.example");
        assert_eq!(relay_host("  https://relay.example  "), "relay.example");
    }

    #[test]
    fn relay_host_keeps_a_non_default_port_and_drops_a_default_one() {
        assert_eq!(relay_host("https://relay.example:8443"), "relay.example:8443");
        assert_eq!(relay_host("https://relay.example:443"), "relay.example");
        assert_eq!(relay_host("http://10.0.2.2:8443"), "10.0.2.2:8443");
    }

    /*
     * The divergence that put this in the core. Kotlin sliced between `://`
     * and `/`, which prints the password; the desktop used `new URL().host`,
     * which does not. One answer now, and it is the one that does not.
     */
    #[test]
    fn relay_host_never_prints_credentials() {
        assert_eq!(relay_host("https://alice:hunter2@relay.example/"), "relay.example");
        assert_eq!(relay_host("alice:hunter2@relay.example:8443/x"), "relay.example:8443");
    }

    /*
     * The other half of that divergence: an address typed with no scheme is not
     * a URL, and the desktop's fallback kept the path while Kotlin's did not.
     */
    #[test]
    fn relay_host_of_a_scheme_less_address_is_still_the_authority() {
        assert_eq!(relay_host("relay.example:8443/sync"), "relay.example:8443");
        assert_eq!(relay_host("relay.example"), "relay.example");
    }

    #[test]
    fn relay_host_of_nothing_is_nothing() {
        assert_eq!(relay_host(""), "");
        assert_eq!(relay_host("   "), "");
    }
}
