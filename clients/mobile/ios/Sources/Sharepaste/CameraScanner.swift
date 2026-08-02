import AVFoundation
import SwiftUI
import UIKit

/// The camera permission, watched from outside the thing it decides.
///
/// **This shape is the fix for a bug worth naming**, and it is easy to write
/// twice. Android's first version remembered the permission inside a composable
/// that existed only while the permission was granted: it reported "refused",
/// the screen swapped the viewfinder for the refusal, the reporter left the
/// composition — and the grant it had just asked for arrived at a `remember`
/// nothing was reading any more. Granting camera access left the refusal on
/// screen until the app was closed and reopened.
///
/// So this is held by the pairing flow itself, as a `@StateObject` at the
/// screen's root, and not by the viewfinder or by the refusal that replaces it.
/// A `@StateObject` belongs to the screen's *identity*, not to whichever branch
/// its `body` produced last time, which is the property the Android bug needed
/// and did not have. Android puts it one level further out again, in
/// `SharepasteApp.kt`, because its `PairingScreen` takes the scanner as a
/// parameter; here the shell's call is fixed at
/// `PairingScreen(state:actions:onBack:)`, so the screen's root is as far out as
/// it goes — and it is far enough.
///
/// **It publishes nothing, on purpose.** The answer goes to the state holder
/// through ``start(report:)`` and comes back as ``PairingState/camera``, so the
/// screen renders a camera problem it was *handed*. A screen that read the
/// answer off this object could not be previewed, and the previews are the only
/// way anybody sees the refusal without refusing a permission on a real phone.
/// `ObservableObject` is here for `@StateObject`'s lazy initialiser and for
/// nothing else: `@State` would build a holder — two `AVCaptureDevice` queries —
/// on every render and throw it away.
///
/// Two things move it, and Android's third does not port. The request's own
/// answer, for the dialog put up on first sight. And the app coming back to the
/// front, for the person who went to Settings — every route to granting a
/// permission that is not the dialog ends there. Android also polls once a
/// second while the answer is still no; that belt has nothing to watch here,
/// because iOS has no third route: a change made in Settings usually terminates
/// the app outright, and when it does not, the return to the front is the same
/// edge the scene phase already reports. A timer could only re-read a value that
/// nothing running is able to change.
@MainActor
final class CameraAccess: ObservableObject {

    /// Whether there is a camera at all, asked once.
    ///
    /// Hardware does not appear mid-session, and unlike the permission this
    /// answer cannot change while the screen is up. `default(for:)` answers for
    /// a device whose permission was refused too — discovery is not gated — so
    /// the two facts stay independent, which is what
    /// ``cameraProblem(hasCamera:permissionGranted:)`` needs in order to tell
    /// them apart.
    private let hasCamera = AVCaptureDevice.default(for: .video) != nil

    private var granted = AVCaptureDevice.authorizationStatus(for: .video) == .authorized

    /// Where the answer goes: ``AppActions/setCameraProblem``.
    private var report: ((CameraProblem?) -> Void)?

    /// Asking twice in one visit is how an app ends up permanently denied.
    private var asked = false

    /// Report what is true now, then ask if nobody has ever been asked.
    ///
    /// Called from `.task`, which runs once per appearance and carries the
    /// action bag of the render that started it. The bag is stored rather than
    /// read again later, which is this file's version of Android's
    /// `rememberUpdatedState`: a bag rebuilt during a render must not re-run the
    /// ask.
    func start(report: @escaping (CameraProblem?) -> Void) async {
        self.report = report
        publish()
        await askOnce()
    }

    /// The app came back to the front. The one route that has to work.
    func refresh() {
        granted = AVCaptureDevice.authorizationStatus(for: .video) == .authorized
        publish()
    }

    /// The control beside the refusal, for the person who would rather press
    /// something than trust that the app noticed.
    ///
    /// It re-reads first and asks second, and the ask is almost always a no-op:
    /// once refused, `requestAccess` answers `false` without showing anything.
    /// That is why the refusal offers ``openSettings()`` beside this, and why
    /// ``Strings/cameraPermissionRefused`` names Settings rather than promising
    /// a second dialog.
    func recheck() {
        refresh()
        Task { await askOnce() }
    }

    /// Take the person to this app's own Settings page.
    ///
    /// iOS's only deep link into Settings lands on the calling app's own page,
    /// which is exactly where the camera switch is. Android has no such
    /// guarantee and so offers no such button; this is one of the few places
    /// where the iOS shell can do more than Android rather than less.
    func openSettings() {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        UIApplication.shared.open(url)
    }

    private func askOnce() async {
        // `notDetermined` rather than `!granted`: a refusal already given is not
        // a question worth re-asking, and the platform would not put the dialog
        // up for it anyway.
        guard hasCamera, !asked,
              AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined
        else { return }
        asked = true
        granted = await AVCaptureDevice.requestAccess(for: .video)
        publish()
    }

    private func publish() {
        report?(cameraProblem(hasCamera: hasCamera, permissionGranted: granted))
    }
}

/// A live viewfinder that reads one pairing code and stops.
///
/// `AVCaptureMetadataOutput` with `.qr`, which is the whole decoder: iOS reads
/// the square in this process, offline, with nothing bundled and nothing
/// downloaded. Android needed an argument here — ML Kit's unbundled variant
/// fetches its models from Google on first use, so `QrCodeAnalyser` uses ZXing
/// instead — and on iOS the same requirement costs nothing, because the platform
/// decoder is already the one that makes no request. A vendored decoder would
/// be a second copy of a capability the OS ships, and ticket 13 has to be able
/// to prove this app talks to nothing but the Relay.
///
/// Rotation is not corrected, for the reason `QrCodeAnalyser` gives: a QR code
/// is located by its three finder patterns, so a phone held sideways reads the
/// same code as a phone held upright. That also keeps this file off
/// `videoOrientation`, which is deprecated, and off its iOS 17 replacement,
/// which is above the floor.
///
/// The scanner is *armed* rather than started: it exists only while
/// ``PairingState/scanned`` is false, so a code arriving stands it down by
/// taking it out of the hierarchy. Emptying the code field puts it back, and
/// that is the only way to read a second code.
struct CameraScanner: UIViewRepresentable {

    /// Where a decoded code goes: ``AppActions/codeScanned``, which fills the
    /// field and does **not** pair.
    let onCode: (String) -> Void

    func makeCoordinator() -> ScannerSession { ScannerSession(onCode: onCode) }

    func makeUIView(context: Context) -> CameraPreviewView {
        let view = CameraPreviewView()
        context.coordinator.attach(to: view.previewLayer)
        return view
    }

    /// Re-bind the callback rather than the session.
    ///
    /// The action bag is rebuilt whenever the screen renders, so the closure
    /// captured at `makeCoordinator` goes stale. Android solves the same problem
    /// with `rememberUpdatedState`; here the coordinator outlives the struct and
    /// this is where it is told about the new one. Rebuilding the session for a
    /// new closure would restart the camera on every keystroke in the field
    /// underneath it.
    func updateUIView(_ uiView: CameraPreviewView, context: Context) {
        context.coordinator.onCode = onCode
    }

    /// Stop the sensor when the viewfinder leaves.
    ///
    /// Load-bearing rather than tidy, and the same point Android's `unbindAll`
    /// makes: a scan takes the viewfinder off a screen that stays, and a session
    /// nobody stopped keeps the camera warm behind a panel that has already
    /// stood down — with the indicator lit and the battery going.
    static func dismantleUIView(_ uiView: CameraPreviewView, coordinator: ScannerSession) {
        coordinator.stop()
    }
}

/// The session, its output, and the one code it is allowed to report.
///
/// `@unchecked Sendable` with the confinement written down, because this object
/// genuinely lives on two threads and no annotation states that precisely:
///
///   * the session is configured and started only on ``queue``, a serial queue
///     of its own. `AVCaptureSession.startRunning()` blocks — long enough to
///     drop frames off whichever thread calls it — which is why none of that is
///     done where the layer is.
///   * ``onCode`` and the report latch are touched only on the main queue: the
///     one writer is `updateUIView`, and the one reader is the delegate
///     callback, which is delivered to `.main` by our own choice below.
///
/// The alternative was `@MainActor` with the session work hopped off it, which
/// is the same two threads with the compiler able to check neither — plus a
/// `Task` per teardown, racing the deallocation it was supposed to precede.
final class ScannerSession: NSObject, AVCaptureMetadataOutputObjectsDelegate, @unchecked Sendable {

    /// Main queue only. See the type's note.
    var onCode: (String) -> Void

    private let session = AVCaptureSession()
    private let queue = DispatchQueue(label: "net.sharepaste.scanner.session")

    /// **One code per arming.** The state holder latches as well —
    /// ``SharepasteViewModel/codeScanned(_:)`` ignores everything after the
    /// first — and this is not a duplicate of that guard: the delegate fires for
    /// every frame the square stays in view, so without this, each of those
    /// frames reaches the state holder to be thrown away. The holder's latch is
    /// about the field; this one is about the thirty calls a second.
    private var reported = false

    init(onCode: @escaping (String) -> Void) {
        self.onCode = onCode
    }

    /// Hand the layer its session, then bring the session up off this thread.
    func attach(to layer: AVCaptureVideoPreviewLayer) {
        // Main thread, because the layer is a view's. Handing over a session
        // that is not running yet is the supported order: the preview lights up
        // when the queue below reaches `startRunning`.
        layer.session = session
        // `resizeAspectFill` plus a clip at the call site, which together are
        // Android's `FILL_CENTER` inside `clipToBounds`. A letterboxed preview
        // in a frame someone is aiming a square at would leave them aiming at
        // the letterbox.
        layer.videoGravity = .resizeAspectFill
        queue.async { [self] in configureAndStart() }
    }

    func stop() {
        queue.async { [self] in
            guard session.isRunning else { return }
            session.stopRunning()
        }
    }

    private func configureAndStart() {
        guard let device = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: device)
        else {
            // Nothing to report and nothing to draw. The two states worth a
            // sentence — no camera, no permission — are ``CameraAccess``'s to
            // find and the screen's to say; a failure here is a device that
            // answered `default(for:)` and then would not open, which is not
            // worth a third message the person cannot act on.
            return
        }
        session.beginConfiguration()
        if session.canAddInput(input) { session.addInput(input) }

        let output = AVCaptureMetadataOutput()
        if session.canAddOutput(output) {
            session.addOutput(output)
            // Both lines only after `addOutput`, and the order is not a style
            // choice: an output with no session publishes no available types,
            // and assigning a type it has not published raises an Objective-C
            // exception no Swift `catch` can take.
            if output.availableMetadataObjectTypes.contains(.qr) {
                output.metadataObjectTypes = [.qr]
            }
            // `.main`, which is what lets the callback below touch main-queue
            // state without a hop. The decode itself happens inside the capture
            // pipeline rather than here, so this queue carries a decoded string
            // a few times a second and nothing else.
            output.setMetadataObjectsDelegate(self, queue: .main)
        }
        session.commitConfiguration()
        session.startRunning()
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard !reported,
              let code = metadataObjects
                  .lazy
                  .compactMap({ $0 as? AVMetadataMachineReadableCodeObject })
                  .compactMap(\.stringValue)
                  .first(where: { !$0.isEmpty })
        else { return }
        reported = true
        // Delivered on `.main` by the queue this delegate was registered with,
        // in `configureAndStart` above. `assumeIsolated` states that as a
        // checked fact rather than hopping through a `Task`, which would put the
        // report a turn of the run loop after the frame it came from.
        MainActor.assumeIsolated { onCode(code) }
    }
}

/// A view that *is* the preview layer.
///
/// `layerClass` rather than a sublayer added in `makeUIView`: a sublayer does
/// not resize with its view, so every rotation and every keyboard appearance
/// would need the frame copied across by hand, and the one that gets missed
/// leaves a stale rectangle of camera over the panel.
final class CameraPreviewView: UIView {

    override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

    var previewLayer: AVCaptureVideoPreviewLayer {
        // Guaranteed by `layerClass` above. A failure here is UIKit having
        // ignored it, which is not a state worth carrying an optional for.
        layer as! AVCaptureVideoPreviewLayer
    }
}
