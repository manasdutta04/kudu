import KuduKit
import SwiftUI

/// Shown at the top of Home when a newer Kudu is out: one click installs it (Kudu quits,
/// updates itself and reopens).
struct UpdateBanner: View {
    @ObservedObject var updates: UpdateChecker

    var body: some View {
        switch updates.state {
        case .available(let version):
            banner(
                icon: "arrow.down.circle.fill",
                title: "Kudu \(version) is available",
                detail: "You're on \(updates.current), an older version. Installing takes a few seconds; Kudu quits and reopens by itself."
            ) {
                Link("What's new", destination: URL(string: "https://github.com/manasdutta04/kudu/releases/tag/v\(version)")!)
                    .font(.callout)
                Button("Install update") { updates.install() }
                    .buttonStyle(QuietButtonStyle(prominent: true))
            }
        case .installing:
            banner(
                icon: "arrow.triangle.2.circlepath",
                title: "Installing the update…",
                detail: "Kudu will quit and reopen in a moment. If macOS asks, allow Accessibility again for the new version."
            ) {
                ProgressView().controlSize(.small)
            }
        default:
            EmptyView()
        }
    }

    private func banner<Actions: View>(icon: String, title: String, detail: String, @ViewBuilder actions: () -> Actions) -> some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 9, style: .continuous).fill(Theme.accent.opacity(0.14)).frame(width: 38, height: 38)
                Image(systemName: icon).font(.system(size: 17, weight: .semibold)).foregroundStyle(Theme.accent)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13.5, weight: .semibold))
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            HStack(spacing: 12, content: actions)
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.accent.opacity(0.06))
        )
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Theme.accent.opacity(0.35), lineWidth: 1))
    }
}
