import Foundation

/// A byte-for-byte TCP forwarder in front of the relay, so a test can take the
/// relay away.
///
/// One criterion needs "no route to the Relay" to be a *fact* rather than a
/// mock: an Offer queueing instead of uploading, on a Pairing this phone has
/// since switched away from. The simulator's own switches are no help — it
/// shares the host's network stack, so there is nothing to turn off that would
/// not also take the relay away from every other test in the run.
///
/// A Pairing made against a port this test owns is the same failure through a
/// blast radius of one. ``close()`` shuts the listener and kills every live
/// connection, so the next request gets a real connection refusal from the real
/// network stack.
///
/// Forwarding is unbuffered: the session runs on SSE, and a frame held in a
/// buffer here is a frame the phone never sees, which would look like a broken
/// session loop.
///
/// Sockets rather than `NWListener`: the pumps want to block, which is what a
/// thread is for, and `Network.framework`'s callback choreography would be three
/// times the code for a forwarder that exists to be switched off.
final class RelayProxy: @unchecked Sendable {

    /// What to pair against. Fixed for the lifetime of this object.
    let url: String

    private let relayHost: String
    private let relayPort: UInt16
    private let port: UInt16

    private let lock = NSLock()
    private var listener: Int32
    private var connections: [Int32] = []
    private var open = true

    /// A proxy in front of whatever ``Suite/relayURL`` points at, on a port the
    /// OS picks.
    ///
    /// The port is taken from a real bind rather than guessed, so two runs on
    /// one machine cannot collide.
    static func inFrontOfTheTestRelay() throws -> RelayProxy {
        Suite.assertRelayIsReachable()
        let (host, port) = Tcp.split(url: Suite.relayURL)
        return try RelayProxy(relayHost: host, relayPort: port)
    }

    private init(relayHost: String, relayPort: UInt16) throws {
        self.relayHost = relayHost
        self.relayPort = relayPort

        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { throw ProxyFailure.cannotListen(errno) }
        var reuse: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0 // Whatever is free.
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddress in
                bind(fd, sockaddress, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 32) == 0 else {
            Darwin.close(fd)
            throw ProxyFailure.cannotListen(errno)
        }

        var assigned = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &assigned) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddress in
                getsockname(fd, sockaddress, &length)
            }
        }

        self.listener = fd
        self.port = UInt16(bigEndian: assigned.sin_port)
        self.url = "http://127.0.0.1:\(UInt16(bigEndian: assigned.sin_port))"

        Thread.detachNewThread { [weak self] in self?.accept() }
    }

    /// Take the relay away: no listener, and every open connection dropped.
    ///
    /// Closing the listener alone is not enough, and the race it leaves is the
    /// kind that shows up as one flaky test in a long run: a connection accepted
    /// a moment before this call is registered a moment after the sweep, leaving
    /// a live tunnel through a proxy that is supposed to be gone. Registration
    /// and the sweep share `open` under this lock, so a connection either dies
    /// in the sweep or is refused outright.
    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard open else { return }
        open = false
        Darwin.close(listener)
        listener = -1
        for fd in connections { Darwin.close(fd) }
        connections.removeAll()
    }

    /// Fail unless the port really does refuse a connection.
    ///
    /// Asserted rather than assumed: a test that thinks it has taken the network
    /// away and has not would quietly prove the opposite of what it claims — an
    /// Offer that uploaded — and pass for the wrong reason.
    var isUnreachable: Bool {
        !Tcp.canConnect(host: "127.0.0.1", port: port, timeout: 2)
    }

    private func accept() {
        while true {
            let listening = lock.withLock { open ? listener : -1 }
            guard listening >= 0 else { return }

            let downstream = Darwin.accept(listening, nil, nil)
            guard downstream >= 0 else { return } // The listener was closed. That is this class's job.

            guard let upstream = connectUpstream() else {
                Darwin.close(downstream)
                continue
            }
            let registered = lock.withLock { () -> Bool in
                guard open else { return false }
                connections.append(contentsOf: [downstream, upstream])
                return true
            }
            guard registered else {
                Darwin.close(downstream)
                Darwin.close(upstream)
                continue
            }
            Thread.detachNewThread { RelayProxy.pump(from: downstream, to: upstream) }
            Thread.detachNewThread { RelayProxy.pump(from: upstream, to: downstream) }
        }
    }

    private func connectUpstream() -> Int32? {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = relayPort.bigEndian
        address.sin_addr.s_addr = inet_addr(relayHost)
        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddress in
                Darwin.connect(fd, sockaddress, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard connected == 0 else {
            Darwin.close(fd)
            return nil
        }
        return fd
    }

    private static func pump(from source: Int32, to destination: Int32) {
        var buffer = [UInt8](repeating: 0, count: 8 * 1024)
        while true {
            let read = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(source, bytes.baseAddress, bytes.count)
            }
            guard read > 0 else { break }
            var written = 0
            while written < read {
                let sent = buffer.withUnsafeBytes { bytes in
                    Darwin.write(destination, bytes.baseAddress! + written, read - written)
                }
                // A dropped connection is the point of this class, not a
                // failure: `close()` shuts both ends under the caller's feet.
                guard sent > 0 else { return }
                written += sent
            }
        }
    }
}

enum ProxyFailure: Error {
    case cannotListen(Int32)
}
