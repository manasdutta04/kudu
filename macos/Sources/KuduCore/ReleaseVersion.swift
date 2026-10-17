/// Release versions ("0.1.3") and tags ("v0.1.3"), compared numerically.
public enum ReleaseVersion {
    /// "v0.1.4" → "0.1.4"; nil if it isn't a three-part version.
    public static func fromTag(_ tag: String) -> String? {
        let trimmed = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
        return parts(trimmed) == nil ? nil : trimmed
    }

    /// Whether `candidate` is a later release than `current` (0.1.10 is later than 0.1.9).
    public static func isNewer(_ candidate: String, than current: String) -> Bool {
        guard let a = parts(candidate), let b = parts(current) else { return false }
        return b.lexicographicallyPrecedes(a)
    }

    private static func parts(_ version: String) -> [Int]? {
        let numbers = version.split(separator: ".").map { Int($0) }
        guard numbers.count == 3, numbers.allSatisfy({ $0 != nil }) else { return nil }
        return numbers.compactMap { $0 }
    }
}
