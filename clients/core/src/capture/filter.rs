use std::time::{Duration, Instant};

pub(crate) const MAX_BYTES: usize = 64 * 1024;
pub(crate) const SELF_WRITE_WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    Transient,
    NonText,
    TooLarge,
    DenyList,
    SelfWrite,
    Duplicate,
}

pub struct CaptureContext<'a> {
    pub capture_enabled: bool,
    pub deny_list: &'a [String],
    pub frontmost_bundle_id: Option<&'a str>,
    pub last_self_write: Option<(Instant, &'a str)>,
    /// Plaintext of the most recent capture that made it past this filter.
    pub last_capture: Option<&'a str>,
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
/// `Sharepaste::offer`. Forking a phone-shaped copy of these five rules is how
/// the size cap or the dedupe rule would come to differ between the two paths.
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
    // Dedupe rule: a repeat of the immediately preceding capture is dropped
    // outright. The alternative — bumping the existing entry back to the top —
    // is deliberately not implemented: dropping keeps a repeat at zero cost
    // (no encrypt, no upload, no server row, no cache slot).
    if ctx.last_capture == Some(text.as_str()) {
        return FilterDecision::Skip(SkipReason::Duplicate);
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
        CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: None, last_capture: None }
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
            ("first copy of a string, nothing captured before it", ctx(), sniff(&[], Some("hi")), kept("hi")),
            ("same string copied again immediately is dropped", CaptureContext { last_capture: Some("hi"), ..ctx() }, sniff(&[], Some("hi")), dropped(Duplicate)),
            ("same string copied again after a different capture is kept", CaptureContext { last_capture: Some("something else"), ..ctx() }, sniff(&[], Some("hi")), kept("hi")),
            ("a different string after a capture is kept", CaptureContext { last_capture: Some("hi"), ..ctx() }, sniff(&[], Some("bye")), kept("bye")),
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
