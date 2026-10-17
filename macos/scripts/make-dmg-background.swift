#!/usr/bin/env swift
// Generates macos/packaging/dmg-background.tiff: the picture behind the Kudu disk image's
// Finder window (a title, an arrow from the app to Applications, and the one-line fix for
// "Kudu can't be opened"). It holds a 1x and a 2x version so it's sharp on Retina screens.
//
//   swift macos/scripts/make-dmg-background.swift          (run from the repository root)
//
// The .tiff is committed, so this only needs re-running when the design changes. The icon
// positions must match macos/packaging/dmg-settings.py.
import AppKit

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let packaging = root.appendingPathComponent("macos/packaging")

/// The Finder window's content size, in points. It's 20 points taller than the visible area
/// (dmg-settings.py) so a taller or shorter title bar never shows an edge.
let width: CGFloat = 660
let height: CGFloat = 440
/// Icon centres (from the top-left, as Finder measures them), shared with dmg-settings.py.
let appCenter = NSPoint(x: 170, y: 196)
let applicationsCenter = NSPoint(x: 490, y: 196)

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
}

let canvas = color(0xF4F2EE)
let ink = color(0x17151F)
let muted = color(0x6F6A7A)
let violet = color(0x6B54F2)
let lilac = color(0xB8A8FF)

func serif(_ size: CGFloat) -> NSFont {
    let base = NSFont.systemFont(ofSize: size, weight: .regular)
    return NSFont(descriptor: base.fontDescriptor.withDesign(.serif) ?? base.fontDescriptor, size: size) ?? base
}

/// Draws `text` centred on `x`, with its top at `top` (measured from the top of the window).
func drawCentered(_ text: String, font: NSFont, color: NSColor, x: CGFloat, top: CGFloat, kern: CGFloat = 0) {
    let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .kern: kern]
    let string = NSAttributedString(string: text, attributes: attributes)
    let size = string.size()
    string.draw(at: NSPoint(x: x - size.width / 2, y: height - top - size.height))
}

/// A point given from the top-left, converted to AppKit's bottom-left coordinates.
func p(_ x: CGFloat, _ y: CGFloat) -> NSPoint { NSPoint(x: x, y: height - y) }

func render(scale: CGFloat) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: width, height: height)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSGraphicsContext.current!.imageInterpolation = .high
    let bounds = NSRect(x: 0, y: 0, width: width, height: height)

    // Warm paper canvas with a soft violet glow behind the icons.
    canvas.setFill()
    bounds.fill()
    NSGradient(colors: [color(0x6B54F2, 0.13), color(0x6B54F2, 0)])!
        .draw(fromCenter: p(330, 200), radius: 0, toCenter: p(330, 200), radius: 300, options: [])
    NSGradient(colors: [color(0xFF6B4A, 0.05), color(0xFF6B4A, 0)])!
        .draw(fromCenter: p(560, 40), radius: 0, toCenter: p(560, 40), radius: 220, options: [])

    // Title and instruction.
    drawCentered("Install Kudu", font: serif(30), color: ink, x: width / 2, top: 30)
    drawCentered("Drag Kudu onto the Applications folder.", font: .systemFont(ofSize: 13.5, weight: .regular),
                 color: muted, x: width / 2, top: 72)

    // Soft tiles under each icon so they sit on something, and a violet plate under each name.
    // Finder draws the names black in Light mode and white in Dark mode; this mid violet keeps
    // both readable.
    for center in [appCenter, applicationsCenter] {
        let tile = NSBezierPath(roundedRect: NSRect(x: center.x - 82, y: height - center.y - 104, width: 164, height: 176),
                                xRadius: 26, yRadius: 26)
        color(0xFFFFFF, 0.55).setFill()
        tile.fill()
        color(0x17151F, 0.06).setStroke()
        tile.lineWidth = 1
        tile.stroke()
        let plate = NSBezierPath(roundedRect: NSRect(x: center.x - 58, y: height - center.y - 78, width: 116, height: 24),
                                 xRadius: 12, yRadius: 12)
        color(0x8B78F5).setFill()
        plate.fill()
    }

    // The arrow: a gentle arc from the app to Applications, with a rounded head.
    let start = p(appCenter.x + 96, appCenter.y - 4)
    let end = p(applicationsCenter.x - 104, applicationsCenter.y - 4)
    let arc = NSBezierPath()
    arc.move(to: start)
    arc.curve(to: end, controlPoint1: p(appCenter.x + 140, appCenter.y - 40), controlPoint2: p(applicationsCenter.x - 148, applicationsCenter.y - 40))
    arc.lineWidth = 4
    arc.lineCapStyle = .round
    let dash: [CGFloat] = [0.5, 11]
    arc.setLineDash(dash, count: 2, phase: 0)
    lilac.setStroke()
    arc.stroke()

    let head = NSBezierPath()
    head.move(to: NSPoint(x: end.x - 13, y: end.y + 9))
    head.line(to: end)
    head.line(to: NSPoint(x: end.x - 13, y: end.y - 11))
    head.lineWidth = 4
    head.lineCapStyle = .round
    head.lineJoinStyle = .round
    violet.setStroke()
    head.stroke()

    // Footer: the one-time fix for an app that isn't signed with a paid Apple certificate.
    let footer = NSRect(x: 40, y: height - 398, width: width - 80, height: 58)
    let card = NSBezierPath(roundedRect: footer, xRadius: 14, yRadius: 14)
    color(0xFFFFFF, 0.75).setFill()
    card.fill()
    color(0x17151F, 0.07).setStroke()
    card.lineWidth = 1
    card.stroke()
    drawCentered("macOS says Kudu “is damaged” or can’t be opened? Run this once in Terminal:",
                 font: .systemFont(ofSize: 11.5, weight: .medium), color: muted, x: width / 2, top: height - footer.maxY + 10)
    drawCentered("xattr -cr /Applications/Kudu.app", font: .monospacedSystemFont(ofSize: 12, weight: .medium),
                 color: color(0x4A35C9), x: width / 2, top: height - footer.maxY + 31)

    NSGraphicsContext.restoreGraphicsState()
    return rep
}

let fileManager = FileManager.default
try fileManager.createDirectory(at: packaging, withIntermediateDirectories: true)
let temp = fileManager.temporaryDirectory.appendingPathComponent("kudu-dmg-background")
try? fileManager.removeItem(at: temp)
try fileManager.createDirectory(at: temp, withIntermediateDirectories: true)

let one = temp.appendingPathComponent("background.png")
let two = temp.appendingPathComponent("background@2x.png")
try render(scale: 1).representation(using: .png, properties: [:])!.write(to: one)
try render(scale: 2).representation(using: .png, properties: [:])!.write(to: two)

let tiffutil = Process()
tiffutil.executableURL = URL(fileURLWithPath: "/usr/bin/tiffutil")
tiffutil.arguments = ["-cathidpicheck", one.path, two.path, "-out", packaging.appendingPathComponent("dmg-background.tiff").path]
try tiffutil.run()
tiffutil.waitUntilExit()
guard tiffutil.terminationStatus == 0 else { fatalError("tiffutil failed") }
print("Wrote macos/packaging/dmg-background.tiff")
