import Foundation
import KuduCore
import os

/// Checks whether a newer Kudu has been released, and installs it.
///
/// The check is one request to GitHub for the latest release's address (which ends in its tag,
/// like `v0.1.4`); nothing about the user or their dictations is sent. It runs at launch and
/// then once a day, and can be switched off in Settings.
@MainActor
public final class UpdateChecker: ObservableObject {
    public enum State: Equatable {
        case idle
        case checking
        case upToDate
        /// A newer version is available.
        case available(String)
        /// The installer is running; Kudu will quit and reopen.
        case installing
        case failed(String)
    }

    @Published public private(set) var state: State = .idle
    @Published public var automatic: Bool {
        didSet {
            UserDefaults.standard.set(automatic, forKey: Keys.automatic)
            if automatic { checkIfDue() }
        }
    }

    /// This copy's version ("0.1.3").
    public let current: String

    private static let latestRelease = URL(string: "https://github.com/manasdutta04/kudu/releases/latest")!
    private static let installer = "https://github.com/manasdutta04/kudu/releases/latest/download/install-macos.sh"
    private static let interval: TimeInterval = 24 * 60 * 60
    private let log = Logger(subsystem: "app.kudu", category: "updates")
    private var timer: Timer?

    private enum Keys {
        static let automatic = "checkForUpdates"
        static let lastCheck = "lastUpdateCheck"
    }

    public init(current: String = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0") {
        self.current = current
        self.automatic = UserDefaults.standard.object(forKey: Keys.automatic) as? Bool ?? true
    }

    /// Starts the daily schedule: checks now if a day has passed since the last check.
    public func start() {
        checkIfDue()
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 60 * 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.checkIfDue() }
        }
    }

    public func checkIfDue() {
        guard automatic else { return }
        let last = UserDefaults.standard.double(forKey: Keys.lastCheck)
        if case .available = state { return }
        if Date().timeIntervalSince1970 - last >= Self.interval { Task { await check() } }
    }

    /// Asks GitHub for the latest release and compares it with this version.
    public func check() async {
        if state == .checking || state == .installing { return }
        state = .checking
        var request = URLRequest(url: Self.latestRelease, timeoutInterval: 15)
        request.httpMethod = "HEAD"
        do {
            let (_, response) = try await URLSession.shared.data(for: request)
            UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: Keys.lastCheck)
            guard let tag = response.url?.lastPathComponent, let latest = ReleaseVersion.fromTag(tag) else {
                state = .failed("Couldn't read the latest version")
                return
            }
            state = ReleaseVersion.isNewer(latest, than: current) ? .available(latest) : .upToDate
            log.info("Update check: latest \(latest), this is \(self.current)")
        } catch {
            log.error("Update check failed: \(error.localizedDescription)")
            state = .failed("Couldn't reach GitHub")
        }
    }

    /// Runs the installer: it downloads the new version, quits this one, replaces it in
    /// /Applications and opens the new one.
    public func install() {
        guard case .available = state else { return }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = ["-c", "curl -fsSL \(Self.installer) | bash"]
        let logURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/Kudu-update.log")
        FileManager.default.createFile(atPath: logURL.path, contents: nil)
        if let handle = try? FileHandle(forWritingTo: logURL) {
            process.standardOutput = handle
            process.standardError = handle
        }
        do {
            try process.run()
            state = .installing
        } catch {
            state = .failed("Couldn't start the installer: \(error.localizedDescription)")
        }
    }
}
