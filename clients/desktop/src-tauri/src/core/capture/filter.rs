use std::time::{Duration, Instant};

pub const MAX_BYTES: usize = 64 * 1024;
pub const SELF_WRITE_WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    Transient,
    NonText,
    TooLarge,
    DenyList,
    SelfWrite,
}

pub struct CaptureContext<'a> {
    pub capture_enabled: bool,
    pub deny_list: &'a [String],
    pub frontmost_bundle_id: Option<&'a str>,
    pub last_self_write: Option<(Instant, &'a str)>,
}

pub trait PasteboardSniff {
    fn types(&self) -> Vec<String>;
    fn read_text(&self) -> Option<String>;
}

#[derive(Debug)]
pub enum FilterDecision {
    Capture(String),
    Skip(SkipReason),
}

pub fn evaluate(ctx: &CaptureContext<'_>, sniff: &dyn PasteboardSniff, now: Instant) -> FilterDecision {
    if !ctx.capture_enabled {
        return FilterDecision::Skip(SkipReason::Disabled);
    }
    let types = sniff.types();
    if types.iter().any(|t| is_transient_type(t)) {
        return FilterDecision::Skip(SkipReason::Transient);
    }
    let Some(text) = sniff.read_text() else {
        return FilterDecision::Skip(SkipReason::NonText);
    };
    if text.is_empty() {
        return FilterDecision::Skip(SkipReason::NonText);
    }
    if text.as_bytes().len() > MAX_BYTES {
        return FilterDecision::Skip(SkipReason::TooLarge);
    }
    if let Some(frontmost) = ctx.frontmost_bundle_id {
        if ctx.deny_list.iter().any(|d| d.eq_ignore_ascii_case(frontmost)) {
            return FilterDecision::Skip(SkipReason::DenyList);
        }
    }
    if let Some((ts, last_text)) = ctx.last_self_write {
        if now.saturating_duration_since(ts) <= SELF_WRITE_WINDOW && last_text == text {
            return FilterDecision::Skip(SkipReason::SelfWrite);
        }
    }
    FilterDecision::Capture(text)
}

fn is_transient_type(t: &str) -> bool {
    matches!(
        t,
        "org.nspasteboard.ConcealedType"
            | "org.nspasteboard.TransientType"
            | "Concealed"
            | "transient"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake { types: Vec<String>, text: Option<String> }
    impl PasteboardSniff for Fake {
        fn types(&self) -> Vec<String> { self.types.clone() }
        fn read_text(&self) -> Option<String> { self.text.clone() }
    }

    fn ctx_default<'a>() -> CaptureContext<'a> {
        CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: None }
    }

    #[test]
    fn captures_plain_text() {
        let s = Fake { types: vec!["public.utf8-plain-text".into()], text: Some("hi".into()) };
        match evaluate(&ctx_default(), &s, Instant::now()) {
            FilterDecision::Capture(t) => assert_eq!(t, "hi"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn skips_when_disabled() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let ctx = CaptureContext { capture_enabled: false, ..ctx_default() };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Skip(SkipReason::Disabled)));
    }

    #[test]
    fn skips_each_transient_type() {
        for t in ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType", "Concealed", "transient"] {
            let s = Fake { types: vec![t.into()], text: Some("hi".into()) };
            assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::Transient)));
        }
    }

    #[test]
    fn skips_non_text() {
        let s = Fake { types: vec![], text: None };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::NonText)));
    }

    #[test]
    fn skips_empty_text() {
        let s = Fake { types: vec![], text: Some(String::new()) };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::NonText)));
    }

    #[test]
    fn skips_too_large() {
        let big = "a".repeat(MAX_BYTES + 1);
        let s = Fake { types: vec![], text: Some(big) };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::TooLarge)));
    }

    #[test]
    fn skips_deny_listed_app_case_insensitive() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let deny = vec!["com.1Password.1Password".to_string()];
        let ctx = CaptureContext { capture_enabled: true, deny_list: &deny, frontmost_bundle_id: Some("com.1password.1password"), last_self_write: None };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Skip(SkipReason::DenyList)));
    }

    #[test]
    fn skips_self_write_within_window() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let now = Instant::now();
        let ctx = CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: Some((now, "hi")) };
        assert!(matches!(evaluate(&ctx, &s, now), FilterDecision::Skip(SkipReason::SelfWrite)));
    }

    #[test]
    fn does_not_skip_self_write_after_window() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let earlier = Instant::now() - SELF_WRITE_WINDOW * 2;
        let ctx = CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: Some((earlier, "hi")) };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Capture(_)));
    }
}
