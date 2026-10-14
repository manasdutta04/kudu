import Foundation
import KuduCore
import os
#if canImport(FoundationModels)
import FoundationModels
#endif

/// Command mode: a spoken instruction applied with Apple's on-device model.
/// - With selected text it edits it: "make this more formal", "translate to Hindi", "fix grammar".
/// - With nothing selected it drafts new text: "write a polite reply saying I'll be late".
@MainActor
public final class CommandEngine {
    public enum Failure: Error, LocalizedError {
        case unavailable, empty, timedOut

        public var errorDescription: String? {
            switch self {
            case .unavailable: "Command mode needs Apple Intelligence"
            case .empty: "No result"
            case .timedOut: "That took too long"
            }
        }
    }

    private let log = Logger(subsystem: "app.kudu", category: "command")

    public init() {}

    public var isAvailable: Bool {
        #if canImport(FoundationModels)
        if #available(macOS 26.0, *), case .available = SystemLanguageModel.default.availability { return true }
        #endif
        return false
    }

    public func run(instruction: String, selection: String?, category: AppCategory) async throws -> String {
        #if canImport(FoundationModels)
        guard #available(macOS 26.0, *), isAvailable else { throw Failure.unavailable }
        let start = ContinuousClock.now
        let session = LanguageModelSession(instructions: Self.instructions)
        let prompt: String
        if let selection, !selection.isEmpty {
            prompt = "Instruction: \(instruction)\n\nText:\n\(selection)"
        } else {
            prompt = ["Request: \(instruction)", Self.destination(category)].filter { !$0.isEmpty }.joined(separator: "\n")
        }
        let output: String? = try await withThrowingTaskGroup(of: String?.self) { group in
            group.addTask { try await session.respond(to: prompt).content }
            group.addTask {
                try await Task.sleep(for: .seconds(12))
                return nil
            }
            let first = try await group.next() ?? nil
            group.cancelAll()
            return first
        }
        guard let output else { throw Failure.timedOut }
        let text = CommandOutput.clean(output, instruction: instruction)
        guard !text.isEmpty else { throw Failure.empty }
        log.info("Command done in \(Int((ContinuousClock.now - start) / .milliseconds(1))) ms (selection: \(selection?.isEmpty == false))")
        return text
        #else
        throw Failure.unavailable
        #endif
    }

    private static let instructions = """
        You are a writing tool inside the user's text field. The user speaks an instruction.
        If text is given, apply the instruction to it (rewrite, change tone, translate, shorten, fix, reformat) \
        and return only the resulting text. Keep the meaning unless asked to change it.
        If only a request is given, write the requested text in the user's voice, ready to send.
        Don't add a greeting, salutation or sign-off unless asked.
        Never explain, never add a preamble or quotes, never ask questions. Output only the text.
        """

    private static func destination(_ category: AppCategory) -> String {
        switch category {
        case .email: "It will be inserted into an email."
        case .personalMessage, .workMessage: "It will be sent as a chat message: keep it short and natural."
        case .code: "It will be inserted into a code editor."
        case .general: ""
        }
    }
}
