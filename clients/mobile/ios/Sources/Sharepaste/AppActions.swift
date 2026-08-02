import SharepasteCore
import SwiftUI

/// Everything a screen can ask the app to do.
///
/// A bag of closures rather than the state holder itself, so no view can reach
/// past it into the repository or the core, and so a screen can be rendered in a
/// preview without a facade behind it. The App Intents do not appear here: they
/// call ``SharepasteRepository`` directly, because an intent has no composition
/// and no screen, which is the whole of what ticket 07 is about.
@MainActor
struct AppActions {
    var setDeviceLabel: (String) -> Void
    /// The pairing code field, as somebody types in it.
    var setPairingCode: (String) -> Void
    /// A code the camera read.
    ///
    /// Separate from ``setPairingCode`` because it means more: it also stands the
    /// viewfinder down, and it is the one write to that field that is not a
    /// keystroke. It stops at the field instead of pairing — see
    /// ``SharepasteViewModel/codeScanned(_:)``.
    var codeScanned: (String) -> Void
    /// Pair with the code the field holds, whichever of the two put it there.
    var pairWithCode: () -> Void
    var setCameraProblem: (CameraProblem?) -> Void
    /// Reset the whole attempt: the spent code, the scan latch and the failure.
    /// The Device Label survives.
    var dismissPairFailure: () -> Void
    var offerPasteboard: () -> Void
    var recallLatest: () -> Void
    var recall: (Entry) -> Void
    var deleteEntry: (Entry) -> Void
    var dismissNotice: () -> Void
    // -- the Settings Screen ----------------------------------------------------
    var openSettings: () -> Void
    var openHistory: () -> Void
    var openAddPairing: () -> Void
    /// Look at this Pairing's History. Transient; changes no syncing.
    var viewPairing: (String) -> Void
    /// Sync this Pairing from now on. Persistent; changes what the phone does.
    var activatePairing: (String) -> Void
    /// Ask for a destructive action, or take the question back.
    var confirm: (Confirmation?) -> Void
    var clearHistory: (String) -> Void
    var forgetPairing: (String) -> Void
    // -- what this phone remembers about its own chrome -------------------------
    /// Whether a Recall says what it put on the pasteboard. See ADR 0009.
    var setShowRecalled: (Bool) -> Void
    /// Close the foreground-only band for good. Only `▴ CLOSE` may call it.
    var dismissForegroundNote: () -> Void
}

/// The app's own wiring, in one place.
///
/// The scene builds its bag from here. That is the point: a preview or a test
/// that assembled its own closures could pass while the app was wired to
/// something else entirely, and the wiring is exactly the part nobody re-reads.
@MainActor
func appActions(_ model: SharepasteViewModel) -> AppActions {
    AppActions(
        setDeviceLabel: { model.setDeviceLabel($0) },
        setPairingCode: { model.setPairingCode($0) },
        codeScanned: { model.codeScanned($0) },
        pairWithCode: { model.pairWithCode() },
        setCameraProblem: { model.setCameraProblem($0) },
        dismissPairFailure: { model.dismissPairFailure() },
        offerPasteboard: { model.offerPasteboard() },
        recallLatest: { model.recallLatest() },
        recall: { model.recall($0) },
        deleteEntry: { model.deleteEntry($0) },
        dismissNotice: { model.dismissNotice() },
        openSettings: { model.openSettings() },
        openHistory: { model.openHistory() },
        openAddPairing: { model.openAddPairing() },
        viewPairing: { model.viewPairing($0) },
        activatePairing: { model.activatePairing($0) },
        confirm: { model.confirm($0) },
        clearHistory: { model.clearHistory($0) },
        forgetPairing: { model.forgetPairing($0) },
        setShowRecalled: { model.setShowRecalled($0) },
        dismissForegroundNote: { model.dismissForegroundNote() }
    )
}
