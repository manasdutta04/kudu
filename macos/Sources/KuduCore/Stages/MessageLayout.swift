import Foundation

/// Lays out letter-style greetings and sign-offs without the language model:
/// "Dear Priya thanks for…" → "Dear Priya,\nThanks for…", and
/// "…patience. Best regards Manas." → "…patience.\n\nBest regards,\nManas".
/// "Dear …" and unambiguous sign-offs ("best regards", "sincerely") apply everywhere; the
/// casual ones ("Hi Sam", "Thanks, Manas") only in email, where they are clearly a letter.
public struct MessageLayout: TextStage {
    private static let name = "([A-Z][\\w'’-]+(?: [A-Z][\\w'’-]+)?)"
    // The greeting word is case-insensitive; the name must be capitalized, so "Hey team"
    // or "Dear all of you" are left alone.
    private static let formalGreeting = try! NSRegularExpression(
        pattern: "^((?i:dear)) \(name)[,.]?\\s+(?=\\S)")
    private static let casualGreeting = try! NSRegularExpression(
        pattern: "^((?i:hi|hello|hey)) \(name)[,.]?\\s+(?=\\S)")
    private static let formalSignOff = try! NSRegularExpression(
        pattern: "[,.]?\\s+(Best regards|Kind regards|Warm regards|Regards|Sincerely|Yours sincerely|Best wishes)[,.]?\\s+([A-Za-z][\\w'’-]+)[.!]?$",
        options: [.caseInsensitive])
    private static let casualSignOff = try! NSRegularExpression(
        pattern: "[,.]?\\s+(Thanks|Thank you|Cheers|Best)[,.]?\\s+([A-Za-z][\\w'’-]+)[.!]?$", options: [.caseInsensitive])

    public init() {}

    public func apply(_ text: String, context: TextContext) -> String {
        guard context.category != .code else { return text }
        var result = text
        let isEmail = context.category == .email
        for regex in isEmail ? [Self.formalGreeting, Self.casualGreeting] : [Self.formalGreeting] {
            if let match = regex.firstMatch(in: result, range: NSRange(result.startIndex..., in: result)),
               let greeting = Range(match.range(at: 1), in: result), let who = Range(match.range(at: 2), in: result),
               let whole = Range(match.range, in: result) {
                let line = "\(result[greeting].capitalized) \(result[who]),\n"
                result.replaceSubrange(whole, with: line)
                break
            }
        }
        for regex in isEmail ? [Self.formalSignOff, Self.casualSignOff] : [Self.formalSignOff] {
            if let match = regex.firstMatch(in: result, range: NSRange(result.startIndex..., in: result)),
               let phrase = Range(match.range(at: 1), in: result), let who = Range(match.range(at: 2), in: result),
               let whole = Range(match.range, in: result) {
                let before = result[..<whole.lowerBound]
                let ending = before.last.map { ".?!".contains($0) } == true ? "" : "."
                let signOff = "\(ending)\n\n\(Self.capitalizeFirst(String(result[phrase]))),\n\(Self.capitalizeFirst(String(result[who])))"
                result.replaceSubrange(whole, with: signOff)
                break
            }
        }
        return result
    }

    private static func capitalizeFirst(_ word: String) -> String {
        word.prefix(1).uppercased() + word.dropFirst()
    }
}
