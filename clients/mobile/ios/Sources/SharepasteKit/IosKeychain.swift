import Foundation
import Security
import SharepasteCore

/// The phone's answer to the desktop's system keychain.
///
/// iOS Keychain Services, one generic-password item per account. This holds the
/// user key and the device token — the two secrets that, together, are the
/// pairing. Android's equivalent is `EncryptedSharedPreferences` with its master
/// key in the Android Keystore; here the platform owns the store outright and
/// there is no master key of ours to manage.
///
/// Called from whatever thread made the FFI call, which the chokepoint has
/// already guaranteed is not the main thread.
/// Checked `Sendable`, not `@unchecked`: this type holds no state at all — the
/// items live in the platform's keychain — so the compiler can prove it, and an
/// `@unchecked` here would go on silencing the day somebody adds a cache.
public final class IosKeychain: Keychain, Sendable {

    /// The service every item is filed under. One string, so a `delete` of an
    /// account cannot miss an item written under a different spelling.
    private static let service = "com.sharepaste.ios.keychain"

    /// When the items are readable.
    ///
    /// `AfterFirstUnlock` rather than `WhenUnlocked`, and the difference is the
    /// whole of what an App Intent can do. A shortcut bound to the Action Button
    /// runs from a locked device (ticket 07), and a key filed `WhenUnlocked`
    /// would be unreadable exactly then — the phone would report itself unpaired
    /// while holding a perfectly good Pairing.
    ///
    /// `ThisDeviceOnly` because the alternative is iCloud Keychain, and a user
    /// key that syncs to Apple is a user key Apple could be compelled to produce.
    /// The whole point of the pairing is that only the person's own devices hold
    /// it.
    /// Computed rather than stored: `CFString` is not `Sendable`, so a `static
    /// let` of one is a concurrency error under Swift 6.
    private static var accessible: CFString { kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly }

    public init() {}

    public func put(account: String, secret: String) throws {
        guard let data = secret.data(using: .utf8) else {
            throw AppError.Keychain(detail: "put: the secret is not UTF-8")
        }
        // Written as one update-or-insert rather than delete-then-add: a crash
        // between the two would leave a pairing whose key is simply gone, and
        // the core treats a returned `put` as durable and writes the pairing row
        // next.
        let query = Self.query(account: account)
        let status = SecItemUpdate(
            query as CFDictionary,
            [kSecValueData: data] as CFDictionary
        )
        if status == errSecSuccess { return }
        if status == errSecItemNotFound {
            var insert = query
            insert[kSecValueData] = data
            insert[kSecAttrAccessible] = Self.accessible
            let added = SecItemAdd(insert as CFDictionary, nil)
            guard added == errSecSuccess else {
                throw Self.failure("put", added)
            }
            return
        }
        throw Self.failure("put", status)
    }

    public func get(account: String) throws -> String? {
        var query = Self.query(account: account)
        query[kSecReturnData] = true
        query[kSecMatchLimit] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw Self.failure("get", status) }
        guard let data = item as? Data, let secret = String(data: data, encoding: .utf8) else {
            throw AppError.Keychain(detail: "get: the stored item is not UTF-8")
        }
        return secret
    }

    public func delete(account: String) throws {
        let status = SecItemDelete(Self.query(account: account) as CFDictionary)
        // Deleting what is not there is the state the caller asked for.
        if status == errSecSuccess || status == errSecItemNotFound { return }
        throw Self.failure("delete", status)
    }

    private static func query(account: String) -> [CFString: Any] {
        [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
        ]
    }

    /// Every Keychain Services failure, as the one error the core understands.
    ///
    /// A locked device, a wiped key, an entitlement the signing tool did not
    /// grant — all of them arrive as an `OSStatus` and all of them mean the same
    /// thing to the core. None of them may cross the FFI boundary as anything
    /// other than an `AppError`, which is what Android's `guard` does for the
    /// dozen exception types the JVM raises for the same set of causes.
    ///
    /// The status number is carried because `SecCopyErrorMessageString` is not
    /// available on iOS, and `-34018` in a report is at least searchable.
    private static func failure(_ operation: String, _ status: OSStatus) -> AppError {
        AppError.Keychain(detail: "\(operation): OSStatus \(status)")
    }
}
