import SwiftUI

/// A quiet pop-up: the current value and a small chevron on a soft fill, opening a menu of
/// choices. Replaces the default bordered pop-up button, which looks heavy on the panel.
struct ValueMenu<Value: Hashable, Item: Identifiable>: View {
    let items: [Item]
    @Binding var selection: Value
    let value: (Item) -> Value
    let title: (Item) -> String
    var placeholder = "Choose"

    var body: some View {
        Menu {
            ForEach(items) { item in
                Button {
                    selection = value(item)
                } label: {
                    if value(item) == selection {
                        Label(title(item), systemImage: "checkmark")
                    } else {
                        Text(title(item))
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Text(currentTitle).font(.system(size: 13))
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(Theme.selection, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .contentShape(Rectangle())
        }
        // A plain button-style menu keeps the custom label; the borderless style would rebuild
        // it as a native pop-up (chevron first, no fill).
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var currentTitle: String {
        items.first { value($0) == selection }.map(title) ?? placeholder
    }
}

extension ValueMenu where Item: CaseIterable, Item.AllCases == [Item], Value == Item {
    init(_ selection: Binding<Item>, title: @escaping (Item) -> String) {
        self.init(items: Item.allCases, selection: selection, value: { $0 }, title: title)
    }
}

/// A segmented control in the app's style: a soft track with the selected option raised.
struct ChipPicker<Item: Hashable & Identifiable>: View {
    let items: [Item]
    @Binding var selection: Item
    let title: (Item) -> String

    var body: some View {
        HStack(spacing: 2) {
            ForEach(items) { item in
                let selected = item == selection
                Button { selection = item } label: {
                    Text(title(item))
                        .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                        .lineLimit(1)
                        .fixedSize()
                        .foregroundStyle(selected ? .primary : .secondary)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 5)
                        .frame(maxWidth: .infinity)
                        .background {
                            if selected {
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .fill(Theme.panel)
                                    .shadow(color: .black.opacity(0.12), radius: 1.5, y: 1)
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(2)
        .background(Theme.selection, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .animation(.easeOut(duration: 0.15), value: selection)
    }
}

/// A soft, borderless button that sits quietly on the panel.
struct QuietButtonStyle: ButtonStyle {
    var destructive = false
    var prominent = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12.5, weight: .medium))
            .foregroundStyle(prominent ? Color.white : destructive ? Color.red : Color.primary)
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .background(
                prominent ? AnyShapeStyle(Theme.accent) : AnyShapeStyle(Theme.selection),
                in: RoundedRectangle(cornerRadius: 7, style: .continuous)
            )
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// A keyboard key, for shortcut hints: "fn", "⌃", "⌘".
struct KeyCap: View {
    let label: String

    var body: some View {
        Text(label)
            .font(.system(size: 11.5, weight: .semibold, design: .rounded))
            .padding(.horizontal, 7)
            .frame(minWidth: 22, minHeight: 22)
            .background(Theme.panel, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 5, style: .continuous).strokeBorder(Color.primary.opacity(0.14)))
            .shadow(color: .black.opacity(0.08), radius: 0, y: 1)
    }
}

/// A shortcut written as keycaps: ["fn", "⌃"].
struct Shortcut: View {
    let keys: [String]
    var prefix: String?

    var body: some View {
        HStack(spacing: 4) {
            if let prefix { Text(prefix).font(.system(size: 12)).foregroundStyle(.secondary) }
            ForEach(Array(keys.enumerated()), id: \.offset) { _, key in KeyCap(label: key) }
        }
    }
}
