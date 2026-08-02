import Foundation
import SharepasteCore
import SharepasteKit
import SQLite3
import XCTest

/// The database opens and the migrations run, on the simulator, in the app's own
/// container.
///
/// The path is handed *in*. The core never asks iOS where an application's data
/// lives — that is the whole reason `dbPath` is a parameter rather than a lookup
/// — so this is also the check that the shell hands over the right place:
/// ``AppContainer/databaseDirectory()``, which is what puts the cache behind
/// Data Protection and outside every backup without a plaintext-at-rest toggle
/// of our own.
final class DatabaseTest: XCTestCase {

    func testMigrationsRunAgainstAFileInTheAppsOwnContainer() throws {
        let directory = try freshDatabase(named: "migration-proof.db")
        let file = directory.appendingPathComponent("migration-proof.db")

        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        XCTAssertTrue(
            file.path.hasPrefix(support.path),
            "the database must live in the app's own Application Support, not \(file.path)"
        )

        // Reads a row the migrations created; a schema that had not been built
        // would fail here rather than at `open`.
        let core = try open(at: file)
        _ = try core.getSettings()
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))

        // `sqlite3` directly, because the claim is about the schema rather than
        // about anything the facade would tell us: a facade that answered from
        // an empty database it had just created would pass a test written
        // through it.
        let tables = try query(file, "SELECT name FROM sqlite_master WHERE type='table' "
            + "AND name NOT LIKE 'sqlite_%' ORDER BY name")
        XCTAssertEqual(
            tables,
            ["accounts", "devices", "entries_cache", "pending_uploads", "settings"]
        )

        let columns = try query(file, "SELECT name FROM pragma_table_info('accounts')")
        // `username` and `last_contact_at` are the two columns added by the
        // `PRAGMA table_info` + `ALTER TABLE` path, which is the half
        // `CREATE TABLE IF NOT EXISTS` cannot reach.
        for column in ["user_id", "device_id", "username", "last_contact_at"] {
            XCTAssertTrue(columns.contains(column), "accounts has no \(column): \(columns)")
        }
    }

    func testOpeningTheSameDatabaseTwiceIsIdempotent() throws {
        let directory = try freshDatabase(named: "idempotence-proof.db")
        let file = directory.appendingPathComponent("idempotence-proof.db")

        // The first facade is released at the end of this scope, which is what
        // closes it: the generated binding destroys the Rust object in `deinit`
        // and there is no `close` to call.
        try autoreleasepool {
            _ = try open(at: file).getSettings()
        }
        let second = try open(at: file)
        _ = try second.getSettings()
        XCTAssertTrue(
            FileManager.default.fileExists(atPath: file.path),
            "a second open must not fail on a non-idempotent migration"
        )
    }

    private func open(at file: URL) throws -> Sharepaste {
        try Sharepaste.open(
            dbPath: file.path,
            keychain: InMemoryKeychain(),
            clipboard: NoClipboard(),
            events: SilentSink(),
            // This test never opens a socket, so the transport policy is beside
            // the point — but it has to say something, which is the point of
            // making it a parameter. The shipped value is what
            // `TransportPolicyTest` proves.
            requireHttps: false
        )
    }

    /// One column of one query, as strings.
    private func query(_ file: URL, _ sql: String) throws -> [String] {
        var handle: OpaquePointer?
        guard sqlite3_open_v2(file.path, &handle, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else {
            throw DatabaseFailure.cannotOpen(String(cString: sqlite3_errmsg(handle)))
        }
        defer { sqlite3_close(handle) }

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &statement, nil) == SQLITE_OK else {
            throw DatabaseFailure.cannotOpen(String(cString: sqlite3_errmsg(handle)))
        }
        defer { sqlite3_finalize(statement) }

        var rows: [String] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            if let text = sqlite3_column_text(statement, 0) {
                rows.append(String(cString: text))
            }
        }
        return rows
    }
}

enum DatabaseFailure: Error {
    case cannotOpen(String)
}
