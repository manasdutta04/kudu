import KuduKit
import SwiftUI

enum Page: String, CaseIterable, Identifiable {
    case home, dictionary, snippets, style, settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .home: "Home"
        case .dictionary: "Dictionary"
        case .snippets: "Snippets"
        case .style: "Style"
        case .settings: "Settings"
        }
    }

    var symbol: String {
        switch self {
        case .home: "house"
        case .dictionary: "character.book.closed"
        case .snippets: "text.badge.plus"
        case .style: "wand.and.stars"
        case .settings: "gearshape"
        }
    }
}

/// Which page the sidebar shows. An ObservableObject rather than @State: @State is a macro on
/// newer SDKs and the Command Line Tools ship without the SwiftUI macro plugin.
@MainActor
final class Navigation: ObservableObject {
    @Published var page: Page? = .home
}

struct MainWindow: View {
    @ObservedObject var controller: DictationController
    @StateObject private var navigation = Navigation()

    var body: some View {
        NavigationSplitView {
            List(selection: $navigation.page) {
                ForEach(Page.allCases) { page in
                    Label(page.title, systemImage: page.symbol).tag(page)
                }
            }
            .safeAreaInset(edge: .top) { Brand().padding(.horizontal, 18).padding(.vertical, 10) }
            .safeAreaInset(edge: .bottom) { SidebarStatus(controller: controller).padding(12) }
            .navigationSplitViewColumnWidth(min: 200, ideal: 220, max: 260)
        } detail: {
            detail
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(Color(nsColor: .windowBackgroundColor))
        }
        .frame(minWidth: 860, minHeight: 580)
    }

    @ViewBuilder private var detail: some View {
        switch navigation.page ?? .home {
        case .home: HomeView(controller: controller, history: controller.history)
        case .dictionary: DictionaryPage(store: controller.store)
        case .snippets: SnippetsPage(store: controller.store)
        case .style: StylePage(controller: controller, store: controller.store)
        case .settings: SettingsPage(controller: controller, history: controller.history)
        }
    }
}

private struct Brand: View {
    var body: some View {
        HStack(spacing: 10) {
            Image(nsImage: KuduMark.image(size: NSSize(width: 26, height: 26), template: true))
                .foregroundStyle(.primary)
            Text("Kudu").font(.system(size: 20, weight: .semibold, design: .rounded))
            Spacer()
        }
    }
}

/// Bottom of the sidebar: is Kudu ready to dictate, and what to do if not.
private struct SidebarStatus: View {
    @ObservedObject var controller: DictationController
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack(spacing: 8) {
            Circle().fill(color).frame(width: 7, height: 7)
            Text(message).font(.caption).foregroundStyle(.secondary).lineLimit(2)
            Spacer(minLength: 0)
            if !isReady {
                Button("Fix") {
                    NSApp.activate(ignoringOtherApps: true)
                    openWindow(id: WindowID.onboarding)
                }
                .controlSize(.small)
            }
        }
        .padding(10)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
    }

    private var isReady: Bool { controller.modelState == .ready && controller.hotkeyActive }

    private var color: Color {
        if isReady { return .green }
        if case .failed = controller.modelState { return .red }
        return .orange
    }

    private var message: String {
        switch controller.modelState {
        case .ready where controller.hotkeyActive: "Ready · hold \(controller.trigger.displayName)"
        case .ready: "Needs Accessibility permission"
        case .downloading(let fraction): "Downloading model \(Int(fraction * 100))%"
        case .loading: "Loading speech model…"
        case .missing: "Speech model not downloaded"
        case .failed: "Speech model error"
        }
    }
}

/// Title block shared by the pages.
struct PageHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 26, weight: .semibold))
            Text(subtitle).font(.callout).foregroundStyle(.secondary)
        }
    }
}
