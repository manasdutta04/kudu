import AppKit

/// The Kudu logo — a "K" whose middle is a small three-bar waveform — drawn as vectors so it
/// is crisp at any size and the waveform bars can move with the voice.
///
/// Geometry is measured from the master artwork (`public/kudu-icon.png`, a 1254 × 1254
/// canvas), so the drawing matches it exactly at rest.
public enum KuduMark {
    /// Bar heights at rest, as in the artwork.
    public static let restLevels: [CGFloat] = [0.5, 0.5, 0.5]

    // Master-canvas geometry (y grows downward).
    private static let stem = CGRect(x: 292, y: 252, width: 144, height: 800)
    private static let bars: [(centerX: CGFloat, width: CGFloat, restHeight: CGFloat)] = [
        (490, 68, 196), (586, 80, 374), (684, 68, 196),
    ]
    private static let barsCenterY: CGFloat = 656
    private static let armThickness: CGFloat = 144
    private static let armLength: CGFloat = 401
    private static let upperArmCenter = CGPoint(x: 851, y: 402)
    private static let lowerArmCenter = CGPoint(x: 843, y: 911)
    /// The part of the master canvas the mark occupies, with a little breathing room.
    private static let contentBox = CGRect(x: 272, y: 218, width: 762, height: 876)

    /// - Parameters:
    ///   - size: point size of the returned image (the mark is centered and aspect-fit).
    ///   - barLevels: 0…1 per waveform bar; `restLevels` reproduces the artwork. 0 shrinks a
    ///     bar to 45% of its rest height, 1 grows it to 120%.
    ///   - template: true for menu-bar use (macOS tints it for light/dark and highlight).
    public static func image(size: CGSize, barLevels: [CGFloat] = restLevels, template: Bool) -> NSImage {
        let image = NSImage(size: size, flipped: true) { rect in
            draw(in: rect, barLevels: barLevels, color: .white)
            return true
        }
        image.isTemplate = template
        return image
    }

    /// Draws the mark aspect-fit inside `rect` (in a flipped, y-down context).
    public static func draw(in rect: CGRect, barLevels: [CGFloat] = restLevels, color: NSColor) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        context.saveGState()
        defer { context.restoreGState() }

        let scale = min(rect.width / contentBox.width, rect.height / contentBox.height)
        let drawn = CGSize(width: contentBox.width * scale, height: contentBox.height * scale)
        context.translateBy(x: rect.midX - drawn.width / 2, y: rect.midY - drawn.height / 2)
        context.scaleBy(x: scale, y: scale)
        context.translateBy(x: -contentBox.minX, y: -contentBox.minY)

        color.setFill()
        capsule(stem).fill()

        for (index, bar) in bars.enumerated() {
            let level = index < barLevels.count ? min(max(barLevels[index], 0), 1) : 0.5
            // 0.5 → rest height; 0 → 45%; 1 → 120%.
            let factor = level < 0.5 ? 0.45 + 1.1 * level : 1.0 + 0.4 * (level - 0.5)
            let height = max(bar.width, bar.restHeight * factor)
            capsule(CGRect(x: bar.centerX - bar.width / 2, y: barsCenterY - height / 2,
                           width: bar.width, height: height)).fill()
        }

        arm(center: upperArmCenter, angle: -.pi / 4).fill()
        arm(center: lowerArmCenter, angle: .pi / 4).fill()
    }

    private static func capsule(_ rect: CGRect) -> NSBezierPath {
        let radius = min(rect.width, rect.height) / 2
        return NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
    }

    private static func arm(center: CGPoint, angle: CGFloat) -> NSBezierPath {
        let path = capsule(CGRect(x: -armLength / 2, y: -armThickness / 2, width: armLength, height: armThickness))
        var transform = AffineTransform(translationByX: center.x, byY: center.y)
        transform.rotate(byRadians: angle)
        path.transform(using: transform)
        return path
    }
}
