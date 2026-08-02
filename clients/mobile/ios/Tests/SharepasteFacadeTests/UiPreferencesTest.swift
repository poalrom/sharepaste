import Foundation
import SharepasteKit
import XCTest

/// Both UI preferences leave memory, and an instance that did not write them
/// reads them back.
///
/// Ticket 02 asks that both preferences survive process death and ticket 04 says
/// the same of the switch. This test cannot literally show that — the suite runs
/// inside one process, and killing it kills the run — so, rather than take a
/// name for something it does not do, it proves the property the claim rests on
/// and stops exactly there:
///
///  * **The values are in a file.** After a write there is a non-empty plist in
///    the app container's `Library/Preferences`, which is the thing a restart
///    has to find. Before that file exists there is nothing for durability to be
///    a property of.
///  * **A reader that did not write sees the write.** A ``UiPreferences``
///    constructed after the fact, over the same store, reads what an earlier one
///    put there rather than only the writer's own copy.
///  * **An empty store answers with the defaults**, so the first launch after an
///    install behaves as the tables in ticket 02 say.
///
/// What is taken on trust is the last link only: `UserDefaults`' own contract
/// that what it has written to that file comes back after the process that wrote
/// it is gone. Nothing in this app influences that link.
///
/// **A named suite, not `.standard`.** Android's port ran against the process's
/// one DataStore and had to clear it on both ends of every test so as not to
/// silence the Receipt for whatever ran next. A suite of its own is the same
/// isolation for free, and it gives the durability half a *deterministic* path
/// to look at: `.standard` writes to a plist named after the test runner's
/// bundle identifier, which is generated and not ours to predict.
///
/// One thing does not port. Android has two readings of the same preference — a
/// flow for the open app, and `showRecalledNow()` for a Standing Action on a
/// closed phone — and a test that they never disagree. Here there is one
/// reading: ``UiPreferences/values`` is synchronous, and an App Intent calls the
/// same property the screen does. There is no second path to disagree with.
final class UiPreferencesTest: XCTestCase {

    private static let suiteName = "com.sharepaste.ios.tests.preferences"

    private var defaults: UserDefaults!

    override func setUpWithError() throws {
        defaults = try XCTUnwrap(
            UserDefaults(suiteName: Self.suiteName),
            "the test suite's own defaults could not be opened"
        )
        // Cleared on both ends. Clearing beforehand as well means a previous run
        // that crashed between a write and its teardown cannot poison this one.
        UiPreferences(defaults: defaults).resetToDefaults()
    }

    override func tearDown() {
        UiPreferences(defaults: defaults).resetToDefaults()
        defaults.synchronize()
    }

    /// A store nobody has written answers `true` and `false`, in that order.
    ///
    /// Both defaults are load-bearing and in opposite directions. A fresh
    /// install must behave as the app did before these preferences existed, plus
    /// the new note: the Receipt drawn, the band shown. A store that failed open
    /// to `showRecalled = false` would disable the Recall Receipt on every fresh
    /// install and look like nothing at all was wrong — the Entry still reaches
    /// the pasteboard, so the only symptom is silence.
    func testAnUnwrittenStoreReadsTheReceiptOnAndTheNoteUndismissed() {
        let values = UiPreferences(defaults: defaults).values
        XCTAssertTrue(
            values.showRecalled,
            "a fresh install must draw the Recall Receipt; failing open to off is silent"
        )
        XCTAssertFalse(
            values.foregroundNoteDismissed,
            "a fresh install must be shown the foreground-only note at least once"
        )
    }

    /// Turning the Receipt off reaches a file, and reaches a reader that did not
    /// turn it off.
    ///
    /// The switch is on the Settings Screen and the value is read in two other
    /// places — the state holder that folds it into `UiState`, and the Standing
    /// Action path. Both construct their own ``UiPreferences``. A write only the
    /// writing instance could see would leave the switch looking like it worked
    /// and doing nothing anywhere else.
    func testTurningTheReceiptOffIsVisibleToAnInstanceThatDidNotWriteIt() throws {
        UiPreferences(defaults: defaults).setShowRecalled(false)

        // The durability half: bytes in the app's own container, which is what a
        // restarted process reads. `synchronize()` is what makes the timing
        // deterministic — without it the write is flushed at a moment of the
        // system's choosing, which is fine for the app and useless for an
        // assertion.
        defaults.synchronize()
        let library = try FileManager.default.url(
            for: .libraryDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: false
        )
        let file = library
            .appendingPathComponent("Preferences", isDirectory: true)
            .appendingPathComponent("\(Self.suiteName).plist")
        let size = try FileManager.default.attributesOfItem(atPath: file.path)[.size] as? Int
        XCTAssertNotNil(
            size,
            "the switch position is not in a file, so there is nothing for a restart to find: \(file.path)"
        )
        XCTAssertGreaterThan(size ?? 0, 0)

        // The reader half: an instance built after the write, holding none of
        // the writer's state.
        XCTAssertFalse(
            UiPreferences(defaults: defaults).values.showRecalled,
            """
            a second UiPreferences did not see the switch go off; the Settings switch would \
            reset itself and the Standing Action would keep reporting Recalls
            """
        )

        // And back on again, so the value is being read rather than a `false`
        // being produced by something that only ever writes once.
        UiPreferences(defaults: defaults).setShowRecalled(true)
        XCTAssertTrue(
            UiPreferences(defaults: defaults).values.showRecalled,
            "the switch does not come back on; a one-way switch is not a switch"
        )
    }

    /// Dismissing the foreground note reaches a new reader, and nothing brings
    /// it back.
    ///
    /// The dismissal is the one preference with no undo on any path a User can
    /// take, on purpose: the note stays readable at full length on the Settings
    /// Screen, so a dismissal is an acknowledgement rather than a hidden
    /// disclosure. That makes an accidental un-dismissal the failure worth
    /// defending against — the band would return on the History Screen after the
    /// User closed it for good, which is exactly the nagging the persisted flag
    /// exists to stop. Writing the *other* preference is the realistic way that
    /// would happen, so that is what this drives.
    func testDismissingTheForegroundNoteIsVisibleToANewInstanceAndIsOneWay() {
        UiPreferences(defaults: defaults).dismissForegroundNote()

        XCTAssertTrue(
            UiPreferences(defaults: defaults).values.foregroundNoteDismissed,
            """
            a second UiPreferences did not see the dismissal; the band would come back on the \
            next launch and the flag would be persisting nothing
            """
        )

        // An unrelated write through an unrelated instance, twice over, and a
        // repeat of the dismissal itself: none of them is an un-dismiss.
        UiPreferences(defaults: defaults).setShowRecalled(false)
        UiPreferences(defaults: defaults).setShowRecalled(true)
        UiPreferences(defaults: defaults).dismissForegroundNote()

        let values = UiPreferences(defaults: defaults).values
        XCTAssertTrue(
            values.foregroundNoteDismissed,
            """
            the dismissal was undone by writing the other preference; the two keys share one \
            store and must not share a value
            """
        )
        XCTAssertTrue(
            values.showRecalled,
            "and the switch it was written beside must be readable at its own last value"
        )
    }

    /// `resetToDefaults` puts both keys back to absent, which is what the
    /// defaults are made of.
    ///
    /// It is the only method on ``UiPreferences`` the shipped app never calls,
    /// so it gets the one test that would notice it clearing the wrong thing —
    /// every other test here depends on it in `setUp`, where a silent failure
    /// would show up as an unrelated test being wrong.
    func testResettingPutsBothKeysBackToAbsent() {
        let preferences = UiPreferences(defaults: defaults)
        preferences.setShowRecalled(false)
        preferences.dismissForegroundNote()

        preferences.resetToDefaults()

        let values = UiPreferences(defaults: defaults).values
        XCTAssertTrue(values.showRecalled)
        XCTAssertFalse(values.foregroundNoteDismissed)
    }
}
