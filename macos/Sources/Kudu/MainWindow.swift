import KuduKit
import SwiftUI

enum Page: String, CaseIterable, Identifiable {
    case home, insights, dictionary, snippets, style, settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .home: "Home"
        case .insights: "Insights"
        case .dictionary: "Dictionary"
        case .snippets: "Snippets"
        case .style: "Style"
        case .settings: "Settings"
        }
    }

    var symbol: String {
        switch self {
        case .home: "house"
        case .insights: "chart.bar.xaxis"
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
    @Published var page: Page

    init(page: Page = .home) {
        self.page = page
    }
}

/// The main window: a quiet sidebar on the window background, and the current page on a
/// rounded panel. The title bar is hidden; the traffic lights sit over the sidebar.
struct MainWindow: View {
    @ObservedObject var controller: DictationController
    /// Shared with the menu bar, so "Settings…" there opens this window on Settings.
    @ObservedObject var navigation: Navigation

    var body: some View {
        HStack(spacing: 0) {
            Sidebar(controller: controller, navigation: navigation)
                .frame(width: 228)
            detail
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(Theme.panel, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Theme.hairline))
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(.vertical, 10)
                .padding(.trailing, 10)
        }
        .background(Theme.canvas)
        .ignoresSafeArea()
        .frame(minWidth: 900, minHeight: 600)
    }

    @ViewBuilder private var detail: some View {
        switch navigation.page {
        case .home: HomeView(controller: controller, history: controller.history)
        case .insights: InsightsPage(ledger: controller.ledger)
        case .dictionary: DictionaryPage(store: controller.store)
        case .snippets: SnippetsPage(store: controller.store)
        case .style: StylePage(controller: controller, store: controller.store)
        case .settings: SettingsPage(controller: controller, history: controller.history)
        }
    }
}

private struct Sidebar: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var navigation: Navigation

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 9) {
                Image(nsImage: KuduMark.image(size: NSSize(width: 24, height: 24), template: true))
                    .foregroundStyle(.primary)
                Text("Kudu").font(.system(size: 19, weight: .semibold, design: .rounded))
            }
            .padding(.horizontal, 10)
            .padding(.top, 50) // room for the traffic lights
            .padding(.bottom, 18)

            ForEach(Page.allCases) { page in
                SidebarItem(page: page, selected: navigation.page == page) { navigation.page = page }
            }

            Spacer()

            SidebarStatus(controller: controller) { navigation.page = .home }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 14)
    }
}

private struct SidebarItem: View {
    let page: Page
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: page.symbol)
                    .font(.system(size: 13.5))
                    .frame(width: 20)
                    .foregroundStyle(selected ? .primary : .secondary)
                Text(page.title)
                    .font(.system(size: 13.5, weight: selected ? .semibold : .regular))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(selected ? Theme.selection : .clear, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Bottom of the sidebar: is Kudu ready to dictate, and what to do if not.
private struct SidebarStatus: View {
    @ObservedObject var controller: DictationController
    let showSetup: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Circle().fill(color).frame(width: 7, height: 7)
            Text(message).font(.system(size: 11.5)).foregroundStyle(.secondary).lineLimit(2)
            Spacer(minLength: 0)
            if !isReady {
                Button("Set up", action: showSetup).controlSize(.small)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Theme.selection.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(Theme.display(30))
            Text(subtitle).font(.system(size: 13.5)).foregroundStyle(.secondary)
        }
    }
}
