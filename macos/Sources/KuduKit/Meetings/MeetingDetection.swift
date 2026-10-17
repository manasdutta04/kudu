import AppKit
import CoreAudio
import KuduCore
import os

/// Notices meetings starting and ending, by checking every two seconds which apps are using
/// the microphone (Zoom, Teams, a browser on a call…). Nothing is recorded by this: it only
/// reads Core Audio's list of processes that have an input running.
@MainActor
final class MeetingDetection {
    var onChange: ((MeetingDetector.Change) -> Void)?
    private var detector = MeetingDetector()
    private var timer: Timer?
    private let log = Logger(subsystem: "app.kudu", category: "meetings")

    var isRunning: Bool { timer != nil }

    /// The meeting app currently detected, if any.
    var active: MeetingApp? { detector.active }

    func start() {
        guard timer == nil else { return }
        detector.reset()
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.check() }
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        detector.reset()
    }

    private func check() {
        if let change = detector.observe(Self.appsUsingMicrophone(), at: .now) {
            log.info("Meeting \(String(describing: change), privacy: .public)")
            onChange?(change)
        }
    }

    /// The meeting apps that have the microphone open right now, Kudu excluded.
    static func appsUsingMicrophone() -> [MeetingApp] {
        let system = AudioObjectID(kAudioObjectSystemObject)
        guard let processes = try? CoreAudioProperty.readArray(system, kAudioHardwarePropertyProcessObjectList) else {
            return fallback()
        }
        let own = getpid()
        return processes.compactMap { process in
            guard let running = try? CoreAudioProperty.read(process, kAudioProcessPropertyIsRunningInput, as: UInt32.self),
                  running != 0,
                  (try? CoreAudioProperty.read(process, kAudioProcessPropertyPID, as: pid_t.self)) != own,
                  let bundleID = try? CoreAudioProperty.readString(process, kAudioProcessPropertyBundleID)
            else { return nil }
            return MeetingApps.app(bundleID: bundleID)
        }
    }

    /// Before macOS 14.2, Core Audio can't say which app uses the microphone: a meeting app
    /// that is running while some app has the default microphone open counts. (Browsers are
    /// always running, so they can't be told apart this way.)
    static func fallback() -> [MeetingApp] {
        let system = AudioObjectID(kAudioObjectSystemObject)
        guard let input = try? CoreAudioProperty.read(system, kAudioHardwarePropertyDefaultInputDevice, as: AudioDeviceID.self),
              let running = try? CoreAudioProperty.read(input, kAudioDevicePropertyDeviceIsRunningSomewhere, as: UInt32.self),
              running != 0 else { return [] }
        return NSWorkspace.shared.runningApplications.compactMap { app in
            app.bundleIdentifier.flatMap(MeetingApps.app(bundleID:)).flatMap { $0.kind == .meeting ? $0 : nil }
        }
    }
}
