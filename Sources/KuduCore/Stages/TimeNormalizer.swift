import Foundation

/// The speech model writes clock times with a dot ("at 10.30"). Rewrites them as "10:30"
/// when the context makes it clearly a time: a preceding "at/by/until/from/to/around/before/after",
/// or a following "am/pm". Version numbers and decimals ("version 2.10", "3.14") are untouched.
public struct TimeNormalizer: TextStage {
    private let afterPreposition: NSRegularExpression
    private let beforeMeridiem: NSRegularExpression

    public init() {
        afterPreposition = try! NSRegularExpression(
            pattern: "\\b(at|by|until|till|from|to|around|before|after|between|and)(\\s+)([01]?\\d|2[0-3])\\.([0-5]\\d)\\b(?!\\.\\d)",
            options: [.caseInsensitive]
        )
        beforeMeridiem = try! NSRegularExpression(
            pattern: "\\b(1[0-2]|0?[1-9])\\.([0-5]\\d)(\\s*)(am|pm|a\\.m\\.|p\\.m\\.)",
            options: [.caseInsensitive]
        )
    }

    public func apply(_ text: String, context: TextContext) -> String {
        var result = text
        var range = NSRange(result.startIndex..., in: result)
        result = beforeMeridiem.stringByReplacingMatches(in: result, range: range, withTemplate: "$1:$2$3$4")
        range = NSRange(result.startIndex..., in: result)
        result = afterPreposition.stringByReplacingMatches(in: result, range: range, withTemplate: "$1$2$3:$4")
        return result
    }
}
