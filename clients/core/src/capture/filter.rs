use std::time::{Duration, Instant};

pub(crate) const MAX_BYTES: usize = 64 * 1024;
pub(crate) const SELF_WRITE_WINDOW: Duration = Duration::from_secs(1);

/// Why the capture filter refused text.
///
/// There is deliberately no `Duplicate`. A repeat copy is not a refusal at all
/// now: it is a **Use** of the entry the device already holds, decided against
/// the cache in `Sharepaste::capture_or_use` rather than against the one
/// previous capture this filter used to remember. Two mechanisms with
/// different answers to "what is a repeat copy" would be worse than either —
/// see ADR 0012.
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

/// What is on the pasteboard, asked in two steps.
///
/// A trait and not a snapshot struct: `evaluate` reads [`types`](Self::types)
/// first and calls [`read_text`](Self::read_text) only if nothing transient or
/// concealed is present. Handing the filter a pre-read snapshot would pull a
/// concealed password's plaintext into memory before it could be rejected.
///
/// `Send + Sync` because `Sharepaste::capture_watched` is `async` and holds the
/// reference across an await, so it has to be able to travel between the
/// runtime's worker threads. Every implementation is stateless or plain data, so
/// this costs nothing.
pub trait PasteboardSniff: Send + Sync {
    fn types(&self) -> Vec<String>;
    fn read_text(&self) -> Option<String>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum FilterDecision {
    Capture(String),
    Skip(SkipReason),
}

/// Watched Capture's decision: what the pasteboard is holding, and whether to
/// take it.
///
/// The `capture_enabled` check is here rather than in [`evaluate_text`] because
/// it must precede the sniff. A disabled watcher has to decide *without* reading
/// the pasteboard, and the type check that follows exists for the same reason —
/// see [`PasteboardSniff`].
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
    evaluate_text(ctx, text, now)
}

/// Every rule that only needs the text, so an Offered Capture runs the same ones
/// a Watched Capture does.
///
/// There is exactly one filter, not two: an Offered Capture reaches this directly
/// with the inputs it has no way to supply written out as inert — see
/// `Sharepaste::offer`. Forking a phone-shaped copy of these four rules is how
/// the size cap would come to differ between the two paths.
///
/// Recognising a repeat copy happens *after* this returns `Capture`, not in it:
/// it needs the database, and this function is a pure decision over what is in
/// front of it.
pub fn evaluate_text(ctx: &CaptureContext<'_>, text: String, now: Instant) -> FilterDecision {
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
    use crate::testing::FakePasteboard;
    use SkipReason::*;

    fn sniff(types: &[&str], text: Option<&str>) -> FakePasteboard {
        FakePasteboard::holding(types, text)
    }

    fn ctx<'a>() -> CaptureContext<'a> {
        CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: None }
    }

    fn kept(text: &str) -> FilterDecision { FilterDecision::Capture(text.to_string()) }
    fn dropped(reason: SkipReason) -> FilterDecision { FilterDecision::Skip(reason) }

    #[test]
    fn evaluate_decision_table() {
        let deny = ["com.1Password.1Password".to_string()];
        let oversized = "a".repeat(MAX_BYTES + 1);

        // (label, context, what the pasteboard holds, expected decision)
        let cases: &[(&str, CaptureContext<'_>, FakePasteboard, FilterDecision)] = &[
            ("plain text is captured", ctx(), sniff(&["public.utf8-plain-text"], Some("hi")), kept("hi")),
            ("capture disabled in settings", CaptureContext { capture_enabled: false, ..ctx() }, sniff(&[], Some("hi")), dropped(Disabled)),
            ("transient type org.nspasteboard.ConcealedType", ctx(), sniff(&["org.nspasteboard.ConcealedType"], Some("hi")), dropped(Transient)),
            ("transient type org.nspasteboard.TransientType", ctx(), sniff(&["org.nspasteboard.TransientType"], Some("hi")), dropped(Transient)),
            ("transient type Concealed", ctx(), sniff(&["Concealed"], Some("hi")), dropped(Transient)),
            ("transient type transient", ctx(), sniff(&["transient"], Some("hi")), dropped(Transient)),
            ("pasteboard holds no text at all", ctx(), sniff(&[], None), dropped(NonText)),
            ("pasteboard holds an empty string", ctx(), sniff(&[], Some("")), dropped(NonText)),
            ("text one byte over the size cap", ctx(), sniff(&[], Some(&oversized)), dropped(TooLarge)),
            ("frontmost app deny-listed, matched case-insensitively", CaptureContext { deny_list: &deny, frontmost_bundle_id: Some("com.1password.1password"), ..ctx() }, sniff(&[], Some("hi")), dropped(DenyList)),
            ("a repeat copy reaches the caller as a capture, because recognising one needs the cache", ctx(), sniff(&[], Some("hi")), kept("hi")),
        ];

        for (label, ctx, sniff, expected) in cases {
            let got = evaluate(ctx, sniff as &dyn PasteboardSniff, Instant::now());
            assert_eq!(&got, expected, "case: {label}");
        }
    }

    #[test]
    fn self_write_is_skipped_only_inside_the_window() {
        let s = sniff(&[], Some("hi"));
        let now = Instant::now();

        let fresh = CaptureContext { last_self_write: Some((now, "hi")), ..ctx() };
        assert_eq!(evaluate(&fresh, &s, now), dropped(SelfWrite));

        let stale = CaptureContext { last_self_write: Some((now - SELF_WRITE_WINDOW * 2, "hi")), ..ctx() };
        assert_eq!(evaluate(&stale, &s, now), kept("hi"));
    }
}
