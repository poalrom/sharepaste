# One language, three copies, checked at publish

The FUI palette now exists three times — `clients/desktop/ui/src/styles.css`,
`clients/mobile/android/.../ui/Fui.kt`, and a SwiftUI `Fui.swift` to come. We are keeping
all three as hand-written source rather than generating them from a canonical token file,
and adding a `check-tokens` step that parses all three and refuses to publish when they
disagree. A shell also reproduces another shell's *outcome* where a platform-specific
reason for it has gone away, and records that the reason was platform-specific.

## Considered Options

**One canonical `tokens.json`, generating all three.** Drift becomes impossible, which is
the whole attraction. Rejected on two counts. It needs a generation step in three build
systems — Vite, Gradle and SwiftPM — and it would strip the hand-written contrast-ratio
comments beside every token, which are the record of a real WCAG audit:
`docs/popover-redesign.md` §1 measured the mock's ramp as failing (`--text-muted` 4.39:1,
`--text-dim` 2.45:1) and raised it, and `docs/android-redesign.md` §1 explains why
`Fui.kt` ports the corrected CSS rather than the design file. Generated files don't carry
arguments. It also cuts against the pattern every other cross-file agreement in this repo
follows: `check-versions.mjs` asserts six hand-written versions rather than generating
any of them.

**Accept the drift.** `docs/android-redesign.md` §8 already records it as a standing risk
with two copies — *"Nothing checks that they agree… a token changed on one client and not
the other is a silent divergence."* Rejected because a third copy is where 45 tokens stop
being reviewable by eye, and because the risk was written down precisely so it would be
answered rather than inherited.

**Let each shell re-derive from the mock.** Rejected: it produces two phones that visibly
differ. Where Android chose against the mock for an Android-specific reason, iOS keeps
Android's result — `Fui.kt:234-238` substitutes **↓** for the mock's **⤓** (U+2913)
because no bundled Android face carries it, and iOS reproduces **↓** even though it can
render **⤓**.

## Consequences

**The check goes where divergence becomes visible: at release.** `check-tokens` follows
`check-versions.mjs` — parse heterogeneous files, compare against an authority, block
`publish`. The declarations are regular enough to make this small:
`--text-body: #b3d0d9;` against `val TextBody = Color(0xFFB3D0D9)`.

**It checks tokens, not geometry.** A `FuiPanel` with a different corner radius, or a
`ChromeBand` a few points taller, passes. That is the accepted limit: geometry drift is
caught by looking at the device, a wrong hex digit never is.

**Sameness is impossible for type.** `docs/android-redesign.md` decision 11 refuses
vendored fonts, so each platform uses its own monospace — Android's is not iOS's. The
*rule* ports; the glyphs cannot. This is the one place the three shells are knowingly
allowed to differ.

**Reproduced outcomes must carry their reason.** An iOS file drawing **↓** with no
explanation reads as an oversight and invites a future "fix". `Fui.kt` already sets the
standard here: every token carries its contrast ratio and the glyph list says *"check a
new glyph on the emulator before adding it here."* Porting that habit is the difference
between a decision and an accident.
