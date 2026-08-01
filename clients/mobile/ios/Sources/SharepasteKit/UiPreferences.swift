import Foundation

/// Both preferences as the app reads them: never absent, only defaulted.
public struct UiPreferenceValues: Sendable, Equatable {
    /// Whether a Recall says what it put on the pasteboard. See ADR 0009.
    public var showRecalled: Bool

    /// Whether the History Screen's foreground-only band has been closed for
    /// good.
    ///
    /// Not the band's open/closed state, which is view state inside the band:
    /// expanding it is exploration, and only `▴ CLOSE` is acknowledgement. The
    /// note itself does not disappear with a dismissal — it is on the Settings
    /// Screen at full length, because it is the app's most important disclosure
    /// and a dismissal must not be the last time it can be read.
    public var foregroundNoteDismissed: Bool

    public init(showRecalled: Bool = true, foregroundNoteDismissed: Bool = false) {
        self.showRecalled = showRecalled
        self.foregroundNoteDismissed = foregroundNoteDismissed
    }
}

/// What this phone has been told to do about its own chrome.
///
/// Two booleans, and they have nothing in common with the core's key material —
/// which is why they are here and not in ``IosKeychain``. The keychain is
/// guarded by the platform because what it holds would decrypt somebody's
/// Entries; a switch position and a dismissed note would survive being read
/// aloud, and putting them behind the same protection would say otherwise.
/// Android's reasoning ports exactly; its DataStore does not exist here, and
/// `UserDefaults` is the plain file this shell already has.
///
/// **Both keys are declared here**, in the ticket that builds the store, so the
/// History Screen's dismissal and the Settings Screen's switch call it rather
/// than each growing a store of their own.
///
/// A missing or unreadable store reads as the defaults — Receipt on, note
/// showing — which is the fresh-install state and the safe direction for both.
/// `UserDefaults.bool(forKey:)` answers `false` for an absent key, so
/// ``showRecalled`` cannot use it directly: absent has to mean `true` there, and
/// `object(forKey:)` is the only reading that tells absent from `false`.
/// `@unchecked` because `UserDefaults` is not marked `Sendable` and is
/// documented thread-safe. Nothing here adds state of its own.
public final class UiPreferences: @unchecked Sendable {

    private enum Key {
        static let showRecalled = "show_recalled"
        static let foregroundNoteDismissed = "foreground_note_dismissed"
    }

    private let defaults: UserDefaults

    /// A store over the app's own defaults, or over a named suite for a test
    /// that needs one nothing else has written.
    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var values: UiPreferenceValues {
        UiPreferenceValues(
            // Absent means on. See the type's note.
            showRecalled: defaults.object(forKey: Key.showRecalled) as? Bool ?? true,
            foregroundNoteDismissed: defaults.bool(forKey: Key.foregroundNoteDismissed)
        )
    }

    public func setShowRecalled(_ show: Bool) {
        defaults.set(show, forKey: Key.showRecalled)
    }

    /// Close the foreground-only band for good.
    ///
    /// One direction only, and deliberately: there is no un-dismiss on any
    /// surface a screen can reach, because closing the band for good is the whole
    /// of what `▴ CLOSE` promises and a control that could quietly undo it would
    /// make the promise a lie. The disclosure is not lost — the Settings Screen
    /// carries it at full length, which is the only reason the dismissal is
    /// allowed to persist.
    public func dismissForegroundNote() {
        defaults.set(true, forKey: Key.foregroundNoteDismissed)
    }

    /// Put both preferences back to what a fresh install has.
    ///
    /// The shipped app never calls this. A facade test does, because it shares
    /// the process's `UserDefaults` with everything else in the suite and has to
    /// hand the next test the defaults it expected.
    public func resetToDefaults() {
        defaults.removeObject(forKey: Key.showRecalled)
        defaults.removeObject(forKey: Key.foregroundNoteDismissed)
    }
}
