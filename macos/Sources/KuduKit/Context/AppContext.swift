import AppKit
import KuduCore

/// Classifies the frontmost app so text can be formatted for it.
public enum AppContext {
    private static let code: Set<String> = [
        "com.apple.dt.Xcode", "com.microsoft.VSCode", "com.todesktop.230313mzl4w4u92", "dev.zed.Zed",
        "com.jetbrains.intellij", "com.jetbrains.pycharm", "com.sublimetext.4", "com.panic.Nova",
        "com.apple.Terminal", "com.googlecode.iterm2", "dev.warp.Warp-Stable", "com.mitchellh.ghostty",
    ]
    private static let chat: Set<String> = [
        "com.tinyspeck.slackmacgap", "com.apple.MobileSMS", "com.hnc.Discord", "net.whatsapp.WhatsApp",
        "ru.keepcoder.Telegram", "com.microsoft.teams2",
    ]
    private static let email: Set<String> = [
        "com.apple.mail", "com.microsoft.Outlook", "com.readdle.smartemail-Mac", "com.superhuman.electron",
    ]

    public static func category(for bundleID: String?) -> AppCategory {
        guard let bundleID else { return .general }
        if code.contains(bundleID) || bundleID.hasPrefix("com.jetbrains.") { return .code }
        if chat.contains(bundleID) { return .chat }
        if email.contains(bundleID) { return .email }
        return .general
    }

    @MainActor
    public static func frontmostCategory() -> AppCategory {
        category(for: NSWorkspace.shared.frontmostApplication?.bundleIdentifier)
    }
}
