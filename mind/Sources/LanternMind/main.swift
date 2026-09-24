// Lantern mind — the on-device language model, and nothing else.
//
// A separate process from the sensing layer on purpose. It holds no camera, no
// microphone and no screen handle; it can only answer the text it is given.
// The Rust core decides what that text is: a compact rendering of the context
// object, never a frame, a crop, an image or audio.
//
// The model is Apple's on-device foundation model (FoundationModels,
// SystemLanguageModel.default). It runs on this Mac.
//
// Protocol: newline-delimited JSON.
//   stdin  <- {"cmd":"status"} | {"cmd":"ask","id":..,"instructions":..,"prompt":..}
//             {"cmd":"cancel","id":..} | {"cmd":"shutdown"}
//   stdout -> hello | status | partial | answer | failed

import Foundation
import FoundationModels

let buildStamp = "lantern-mind/0.1.0"

// Frameworks may write diagnostics to fd 1; keep a private copy for the protocol.
let protocolFd = dup(1)
dup2(2, 1)

let outQueue = DispatchQueue(label: "dev.lantern.mind.out")

func emit(_ dict: [String: Any]) {
    var d = dict
    d["v"] = 1
    guard var data = try? JSONSerialization.data(withJSONObject: d, options: [.withoutEscapingSlashes]) else { return }
    data.append(0x0A)
    outQueue.async {
        data.withUnsafeBytes { raw in
            guard var p = raw.baseAddress else { return }
            var n = raw.count
            while n > 0 {
                let w = write(protocolFd, p, n)
                if w <= 0 { break }
                p = p.advanced(by: w); n -= w
            }
        }
    }
}

func logErr(_ s: String) {
    FileHandle.standardError.write(("[mind] " + s + "\n").data(using: .utf8)!)
}

/// AVAILABLE, or the reason macOS gives for the model being unavailable.
func availability() -> (String, String?) {
    switch SystemLanguageModel.default.availability {
    case .available:
        return ("AVAILABLE", nil)
    case .unavailable(let reason):
        switch reason {
        case .deviceNotEligible:
            return ("UNAVAILABLE", "This Mac is not eligible for Apple Intelligence.")
        case .appleIntelligenceNotEnabled:
            return ("UNAVAILABLE", "Apple Intelligence is turned off in System Settings.")
        case .modelNotReady:
            return ("UNAVAILABLE", "The on-device model is still downloading or preparing.")
        @unknown default:
            return ("UNAVAILABLE", "macOS reports the model as unavailable (\(reason)).")
        }
    }
}

func emitStatus(_ type: String = "status") {
    let (state, reason) = availability()
    var d: [String: Any] = ["type": type, "build": buildStamp, "model": "Apple on-device foundation model",
                            "availability": state, "pid": Int(getpid())]
    if let r = reason { d["reason"] = r }
    emit(d)
}

actor Answering {
    private var tasks: [String: Task<Void, Never>] = [:]
    /// Whether a session was warmed before this answer. Reported so a
    /// measurement can never be attributed to the wrong condition.
    private(set) var prewarmed = false
    /// A session built while idle and used for the NEXT question, so the cost of
    /// building one is paid off the critical path.
    ///
    /// Deliberately one-use: a LanguageModelSession accumulates the turns it has
    /// served, and KUE decides what history a question may see (bounded, cleared
    /// by the privacy firewall). Reusing a session across questions would carry
    /// earlier context into a later answer behind KUE's back, so each question
    /// gets a fresh session — this only moves WHEN it is built.
    private var spare: LanguageModelSession? = nil
    private var spareInstructions: String? = nil

    /// Builds the next session now, while nothing is waiting on it.
    func warm(instructions: String) {
        guard SystemLanguageModel.default.availability == .available else { return }
        let s = LanguageModelSession(instructions: instructions)
        s.prewarm()
        spare = s
        spareInstructions = instructions
    }

    private func takeSpare(for instructions: String) -> LanguageModelSession? {
        guard spareInstructions == instructions, let s = spare else { return nil }
        spare = nil
        spareInstructions = nil
        return s
    }

    func ask(id: String, instructions: String, prompt: String) {
        tasks[id]?.cancel()
        tasks[id] = Task {
            let started = Date()
            let (state, reason) = availability()
            guard state == "AVAILABLE" else {
                emit(["type": "failed", "id": id, "reason": "MODEL_UNAVAILABLE", "message": reason ?? "unavailable"])
                await self.finish(id)
                return
            }
            // Where the seconds go. Measured because an answer to "what is the
            // capital of France?" took 11.5 s and prompt size was ruled out as
            // the cause (2026-09-20). Times only — no prompt, no answer text.
            let beforeSession = Date()
            let warmed = self.takeSpare(for: instructions)
            self.prewarmed = warmed != nil
            let session = warmed ?? LanguageModelSession(instructions: instructions)
            let sessionCreatedAt = Date()
            var firstTokenAt: Date? = nil
            do {
                var last = ""
                var lastSent = Date.distantPast
                for try await snapshot in session.streamResponse(to: prompt) {
                    if Task.isCancelled { break }
                    if firstTokenAt == nil { firstTokenAt = Date() }
                    last = snapshot.content
                    // Partial text at most ~6 times a second: the interface only
                    // needs to show progress, not every token.
                    if Date().timeIntervalSince(lastSent) > 0.16 {
                        emit(["type": "partial", "id": id, "text": last])
                        lastSent = Date()
                    }
                }
                if Task.isCancelled {
                    emit(["type": "failed", "id": id, "reason": "CANCELLED", "message": "The answer was cancelled."])
                } else {
                    let done = Date()
                    let first = firstTokenAt ?? done
                    emit(["type": "answer", "id": id, "text": last,
                          "seconds": done.timeIntervalSince(started),
                          // The parts, in milliseconds:
                          //   sessionCreateMs — constructing LanguageModelSession
                          //   prefillMs       — session ready → first token
                          //   generationMs    — first token → last token
                          "sessionCreateMs": sessionCreatedAt.timeIntervalSince(beforeSession) * 1000,
                          "prefillMs": first.timeIntervalSince(sessionCreatedAt) * 1000,
                          "generationMs": done.timeIntervalSince(first) * 1000,
                          "prewarmed": self.prewarmed])
                }
            } catch let e as LanguageModelSession.GenerationError {
                let code: String
                switch e {
                case .exceededContextWindowSize: code = "CONTEXT_TOO_LARGE"
                case .guardrailViolation: code = "GUARDRAIL"
                case .refusal: code = "REFUSED"
                case .unsupportedLanguageOrLocale: code = "UNSUPPORTED_LANGUAGE"
                case .assetsUnavailable: code = "MODEL_UNAVAILABLE"
                case .rateLimited: code = "RATE_LIMITED"
                case .concurrentRequests: code = "BUSY"
                default: code = "GENERATION_FAILED"
                }
                emit(["type": "failed", "id": id, "reason": code, "message": String(describing: e)])
            } catch {
                emit(["type": "failed", "id": id, "reason": "GENERATION_FAILED", "message": String(describing: error)])
            }
            await self.finish(id)
        }
    }

    func cancel(_ id: String) { tasks[id]?.cancel() }

    private func finish(_ id: String) { tasks[id] = nil }
}

let answering = Answering()

func handle(_ line: String) {
    guard let data = line.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let cmd = obj["cmd"] as? String else {
        emit(["type": "error", "code": "BAD_COMMAND", "message": "could not parse command line"])
        return
    }
    switch cmd {
    case "status":
        emitStatus()
    case "ask":
        guard let id = obj["id"] as? String, let prompt = obj["prompt"] as? String else {
            emit(["type": "error", "code": "BAD_COMMAND", "message": "ask needs id and prompt"])
            return
        }
        let instructions = obj["instructions"] as? String ?? ""
        Task { await answering.ask(id: id, instructions: instructions, prompt: prompt) }
    case "warm":
        // Build the next session now, while nothing waits on it. Optional: the
        // helper answers identically without it.
        let instructions = obj["instructions"] as? String ?? ""
        Task { await answering.warm(instructions: instructions) }
    case "cancel":
        if let id = obj["id"] as? String { Task { await answering.cancel(id) } }
    case "shutdown":
        exit(0)
    default:
        emit(["type": "error", "code": "UNKNOWN_COMMAND", "message": cmd])
    }
}

emitStatus("hello")

Thread.detachNewThread {
    var buffer = Data()
    while true {
        let chunk = FileHandle.standardInput.availableData
        if chunk.isEmpty { logErr("stdin closed; exiting"); exit(0) }
        buffer.append(chunk)
        while let nl = buffer.firstIndex(of: 0x0A) {
            let lineData = buffer[buffer.startIndex..<nl]
            buffer = buffer[buffer.index(after: nl)...]
            if let s = String(data: lineData, encoding: .utf8), !s.trimmingCharacters(in: .whitespaces).isEmpty {
                DispatchQueue.main.async { handle(s) }
            }
        }
    }
}

RunLoop.main.run()
