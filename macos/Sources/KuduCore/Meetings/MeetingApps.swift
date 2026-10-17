import Foundation

/// An app that holds meetings, recognised by the microphone it's using.
public struct MeetingApp: Sendable, Equatable {
    public enum Kind: Sendable { case meeting, browser }

    public var name: String
    public var kind: Kind

    public init(name: String, kind: Kind) {
        self.name = name
        self.kind = kind
    }
}

public enum MeetingApps {
    /// Bundle identifier prefixes on the Mac. Prefixes, because the process that uses the
    /// microphone is often a helper ("com.google.Chrome.helper", "us.zoom.CptHost").
    static let mac: [(prefix: String, app: MeetingApp)] = [
        ("us.zoom.", MeetingApp(name: "Zoom", kind: .meeting)),
        ("com.microsoft.teams", MeetingApp(name: "Microsoft Teams", kind: .meeting)),
        ("Cisco-Systems.Spark", MeetingApp(name: "Webex", kind: .meeting)),
        ("com.webex.", MeetingApp(name: "Webex", kind: .meeting)),
        ("com.cisco.webex", MeetingApp(name: "Webex", kind: .meeting)),
        ("com.tinyspeck.slackmacgap", MeetingApp(name: "Slack", kind: .meeting)),
        ("com.hnc.Discord", MeetingApp(name: "Discord", kind: .meeting)),
        ("com.apple.FaceTime", MeetingApp(name: "FaceTime", kind: .meeting)),
        ("com.apple.TelephonyUtilities", MeetingApp(name: "FaceTime", kind: .meeting)),
        ("com.skype.", MeetingApp(name: "Skype", kind: .meeting)),
        ("com.logmein.", MeetingApp(name: "GoTo Meeting", kind: .meeting)),
        ("com.bluejeansnet.", MeetingApp(name: "BlueJeans", kind: .meeting)),
        ("com.ringcentral.", MeetingApp(name: "RingCentral", kind: .meeting)),
        ("net.whatsapp.WhatsApp", MeetingApp(name: "WhatsApp", kind: .meeting)),
        ("com.google.Chrome", MeetingApp(name: "Google Chrome", kind: .browser)),
        ("com.apple.Safari", MeetingApp(name: "Safari", kind: .browser)),
        ("com.apple.WebKit", MeetingApp(name: "Safari", kind: .browser)),
        ("com.microsoft.edgemac", MeetingApp(name: "Microsoft Edge", kind: .browser)),
        ("org.mozilla.firefox", MeetingApp(name: "Firefox", kind: .browser)),
        ("company.thebrowser.", MeetingApp(name: "Arc", kind: .browser)),
        ("com.brave.Browser", MeetingApp(name: "Brave", kind: .browser)),
        ("com.operasoftware.", MeetingApp(name: "Opera", kind: .browser)),
        ("com.vivaldi.", MeetingApp(name: "Vivaldi", kind: .browser)),
    ]

    /// The meeting app a Mac bundle identifier belongs to, if any.
    public static func app(bundleID: String) -> MeetingApp? {
        mac.first { bundleID.hasPrefix($0.prefix) }?.app
    }

    /// The app to name when several are using the microphone: a meeting app wins over a
    /// browser.
    public static func preferred(_ apps: [MeetingApp]) -> MeetingApp? {
        apps.first { $0.kind == .meeting } ?? apps.first
    }
}

/// Decides when a meeting has started and ended from which meeting apps are using the
/// microphone, checked every couple of seconds. A start needs a few seconds of use (not a
/// quick mic test); an end needs the microphone to be released for a while (not a brief
/// reconnect).
public struct MeetingDetector: Sendable {
    public enum Change: Sendable, Equatable {
        case started(MeetingApp)
        case ended
    }

    public var startAfter: TimeInterval = 3
    public var endAfter: TimeInterval = 20

    public private(set) var active: MeetingApp?
    private var candidateSince: Date?
    private var lastSeen: Date?

    public init() {}

    public mutating func observe(_ apps: [MeetingApp], at now: Date) -> Change? {
        if let app = MeetingApps.preferred(apps) {
            lastSeen = now
            guard active == nil else { return nil }
            let since = candidateSince ?? now
            candidateSince = since
            if now.timeIntervalSince(since) >= startAfter {
                active = app
                candidateSince = nil
                return .started(app)
            }
            return nil
        }
        candidateSince = nil
        if active != nil, let lastSeen, now.timeIntervalSince(lastSeen) >= endAfter {
            active = nil
            return .ended
        }
        return nil
    }

    /// Forgets the current meeting, so the next one is noticed afresh.
    public mutating func reset() {
        active = nil
        candidateSince = nil
        lastSeen = nil
    }
}
