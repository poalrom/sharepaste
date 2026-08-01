import Foundation


/// Where this app keeps its database, and the two things that have to be true
/// about that directory before anything is written into it.
///
/// Both defaults on this platform are permissive, so omitting either ships the
/// exposure with nothing at runtime looking wrong. That is the same shape as
/// Android's `allowBackup` and `dataExtractionRules`, which is why the controls
/// land with the shell rather than later.
public enum AppContainer {

    /// The directory the facade's SQLite lives in, prepared for it.
    ///
    /// `Application Support` rather than `Documents`: `Documents` is what the
    /// Files app and iTunes file sharing expose, and a cache of decrypted
    /// Previews is not a document the person is meant to browse.
    public static func databaseDirectory() throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = base.appendingPathComponent("Sharepaste", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try excludeFromBackup(directory)
        try protect(directory)
        return directory
    }

    /// Keep the cache out of iCloud and out of an encrypted local backup.
    ///
    /// The iOS analogue of Android's backup-off. Without it the database — the
    /// decrypted Previews and the cached plaintexts — is copied into whatever
    /// backup the person's iCloud account holds, which is a copy of their
    /// Entries somewhere the pairing was never extended to.
    ///
    /// It is set on the directory, and it is set **again on every launch**
    /// rather than once at creation: restoring a device or moving a container
    /// does not carry the flag, and a control that only ever ran on a
    /// first launch is a control that silently stops being true.
    private static func excludeFromBackup(_ url: URL) throws {
        var url = url
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try url.setResourceValues(values)
    }

    /// Refuse reads while the device is locked and has not been unlocked since
    /// boot.
    ///
    /// `completeUntilFirstUserAuthentication` and not `complete`, for the reason
    /// ``IosKeychain`` files its items `AfterFirstUnlock`: a shortcut bound to
    /// the Action Button runs from a locked device (ticket 07), and a database
    /// that could not be opened then would report a paired phone as unpaired.
    /// The protection that survives is the one that matters — a device seized
    /// powered off holds nothing readable.
    private static func protect(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: url.path
        )
    }
}

/// The transport policy the shipped app hands the core, in one place.
///
/// A constant in the shell rather than a value in the facade, because the facade
/// must keep working for desktops paired to cleartext relays — the refusal is
/// this client's, not the protocol's. A release build compiles it `true` with
/// the platform's bundled public roots, no pinning and no private-CA path, so a
/// relay reached over plain `http://` is refused before a byte leaves the device
/// and the refusal names the reason.
public enum TransportPolicy {
    /// `true` in every variant, debug included. There is no build that quietly
    /// stops enforcing TLS: a facade test that needs the cleartext test relay
    /// passes `false` at the one call that needs it and says so there.
    public static let requireHttps = true
}
