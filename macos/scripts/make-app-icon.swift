#!/usr/bin/env swift
// Generates macos/Resources/AppIcon.icns from the master logo (public/kudu-icon.png):
// the white mark on a midnight-gradient macOS squircle, at every size macOS asks for.
//
//   swift macos/scripts/make-app-icon.swift          (run from the repository root)
//
// The .icns is committed, so this only needs re-running when the artwork changes.
import AppKit

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let logoURL = root.appendingPathComponent("public/kudu-icon.png")
let resources = root.appendingPathComponent("macos/Resources")
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("AppIcon.iconset")

guard let logo = NSImage(contentsOf: logoURL) else {
    fatalError("Run from the repository root; couldn't read \(logoURL.path)")
}

func color(_ hex: UInt32) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

/// Draws the icon on a 1024-unit canvas scaled to `pixels`.
func render(pixels: Int) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let scale = CGFloat(pixels) / 1024
    NSGraphicsContext.current!.cgContext.scaleBy(x: scale, y: scale)

    // macOS icon grid: an 824 × 824 squircle centered on the 1024 canvas.
    let body = NSRect(x: 100, y: 100, width: 824, height: 824)
    let squircle = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)

    // Soft drop shadow under the tile.
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.35)
    shadow.shadowOffset = NSSize(width: 0, height: -10)
    shadow.shadowBlurRadius = 24
    shadow.set()
    color(0x07060F).setFill()
    squircle.fill()
    NSGraphicsContext.restoreGraphicsState()

    // Midnight gradient, lighter at the top.
    NSGradient(colors: [color(0x2A2160), color(0x14112E), color(0x07060F)],
               atLocations: [0, 0.55, 1], colorSpace: .sRGB)!
        .draw(in: squircle, angle: -90)

    // A faint glow behind the mark and a hairline highlight on the top edge.
    NSGraphicsContext.saveGraphicsState()
    squircle.addClip()
    NSGradient(colors: [NSColor(srgbRed: 0.55, green: 0.5, blue: 1, alpha: 0.22), .clear])!
        .draw(fromCenter: NSPoint(x: 512, y: 560), radius: 0, toCenter: NSPoint(x: 512, y: 560), radius: 420, options: [])
    NSColor.white.withAlphaComponent(0.10).setStroke()
    let rim = NSBezierPath(roundedRect: body.insetBy(dx: 1.5, dy: 1.5), xRadius: 184, yRadius: 184)
    rim.lineWidth = 3
    rim.stroke()
    NSGraphicsContext.restoreGraphicsState()

    // The white mark: the artwork's own canvas mapped onto the tile.
    logo.draw(in: body, from: .zero, operation: .sourceOver, fraction: 1)

    NSGraphicsContext.restoreGraphicsState()
    return rep
}

let fileManager = FileManager.default
try? fileManager.removeItem(at: iconset)
try fileManager.createDirectory(at: iconset, withIntermediateDirectories: true)
try fileManager.createDirectory(at: resources, withIntermediateDirectories: true)

for points in [16, 32, 128, 256, 512] {
    for factor in [1, 2] {
        let name = factor == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        let data = render(pixels: points * factor).representation(using: .png, properties: [:])!
        try data.write(to: iconset.appendingPathComponent(name))
    }
}
// A full-size preview for docs and quick checks.
try render(pixels: 1024).representation(using: .png, properties: [:])!
    .write(to: resources.appendingPathComponent("AppIcon-1024.png"))

let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", resources.appendingPathComponent("AppIcon.icns").path]
try iconutil.run()
iconutil.waitUntilExit()
guard iconutil.terminationStatus == 0 else { fatalError("iconutil failed") }
print("Wrote macos/Resources/AppIcon.icns and AppIcon-1024.png")
