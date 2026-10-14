import AppKit
import SwiftUI

/// Colors and type shared by the main window. Each color adapts to light and dark mode.
enum Theme {
    /// Window and sidebar background.
    static let canvas = dynamic(light: 0xF2F1EE, dark: 0x141416)
    /// The rounded content panel.
    static let panel = dynamic(light: 0xFFFFFF, dark: 0x1D1D20)
    /// Cards inside the panel.
    static let card = dynamic(light: 0xF8F7F4, dark: 0x252529)
    /// Selected sidebar row.
    static let selection = dynamic(light: 0xE4E2DD, dark: 0x2A2A2F)
    static let hairline = Color.primary.opacity(0.08)
    static let accent = Color(red: 0.42, green: 0.33, blue: 0.95)

    static let midnight = LinearGradient(
        colors: [Color(red: 0.18, green: 0.14, blue: 0.40), Color(red: 0.06, green: 0.05, blue: 0.13)],
        startPoint: .topLeading, endPoint: .bottomTrailing
    )

    /// Large serif display type, for page titles and headline numbers.
    static func display(_ size: CGFloat) -> Font { .system(size: size, weight: .regular, design: .serif) }

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let hex = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        })
    }
}

/// A rounded card with the shared fill and hairline border.
struct CardBackground: ViewModifier {
    var radius: CGFloat = 16

    func body(content: Content) -> some View {
        content
            .background(Theme.card, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Theme.hairline))
    }
}

extension View {
    func card(radius: CGFloat = 16) -> some View { modifier(CardBackground(radius: radius)) }
}
