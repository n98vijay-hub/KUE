// Lantern sensing layer — the wake boundary.
//
// THE RULE OF THIS FILE: audio goes in, ONE DECISION comes out.
//
// While KUE is waiting to hear its name, it is listening to the room. Everything
// heard in that state is recognised inside this file and thrown away here. The
// only thing that may leave is whether the invocation was heard, and what
// followed it in that same utterance. No other text, no audio, no transcript of
// anything else — there is no code path from here to `.transcript`, and that is
// the point of it being a separate file with a separate emitter.
//
// After a wake, KUE starts an ordinary listening session (VoiceEngine), which is
// the push-to-talk path that already exists, already tells the core it is
// listening, and already shows in the window.
//
// WHAT THIS IS NOT: it is not an acoustic keyword spotter. It is Apple's
// on-device transcriber applied to short windows of speech, with a string match
// over the result. Its confidence is a string-match confidence, never an
// acoustic or biometric score, and it says nothing about WHO spoke — that is
// identity's job, and a wake grants no authority whatsoever.

import Foundation

/// The invocation, as configured. "hello kue" → lead ["hello"], then the name.
///
/// `kue` in the phrase means "the sound of KUE", because that sound is a
/// homophone of Q, cue and queue and the transcriber returns any of them. Every
/// other word must be heard as itself.
struct WakePhrase {
    /// Words before the name, e.g. ["hello"]. Matched as written.
    let lead: [String]
    /// Whether the KUE sound must follow the lead words.
    let expectsName: Bool
    /// What the owner configured, for display.
    let spoken: String

    static func parse(_ phrase: String) -> WakePhrase {
        let words = WakeMatcher.words(phrase)
        let expectsName = words.contains { WakeMatcher.nameSounds.contains($0) }
        let lead = words.filter { !WakeMatcher.nameSounds.contains($0) }
        return WakePhrase(lead: lead, expectsName: expectsName, spoken: phrase)
    }
}

/// What the boundary is allowed to say.
struct WakeDecision {
    let woke: Bool
    /// 1.0 when the name was heard as written, 0.8 when heard as one of the
    /// sounds it shares with ordinary words. A string match, nothing more.
    let confidence: Double
    /// What was said after the invocation, in the same breath. Empty when the
    /// owner said only the name. Leaves this file ONLY when `woke` is true.
    let rest: String
    /// Why not, when not — for the measurement harness, never for the core.
    let why: String

    static func no(_ why: String) -> WakeDecision { WakeDecision(woke: false, confidence: 0, rest: "", why: why) }
}

enum WakeMatcher {
    /// How the transcriber writes the sound /kjuː/. Measured on this Mac across
    /// three voices: "Q", "queue", "cue", "quay", "koo", "qui", "cool", "coo".
    /// They are homophones — no recogniser can tell them apart, so KUE accepts
    /// the sound and disambiguates by position instead.
    static let nameSounds: Set<String> = ["kue", "q", "cue", "queue", "quay", "koo", "kui", "qui", "coo", "ku", "kew"]

    /// Lowercased words, letters and digits only.
    static func words(_ text: String) -> [String] {
        text.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
    }

    /// Did this utterance begin with the invocation?
    ///
    /// The invocation must come FIRST. "The queue is long" is not KUE being
    /// addressed, and neither is "put it in the queue for KUE" — a name in the
    /// middle of a sentence is someone talking about KUE, not to it. That single
    /// rule removes most of what would otherwise be constant false waking.
    ///
    /// Words are matched over the utterance's opening letters rather than token
    /// by token, because the transcriber sometimes runs the invocation together
    /// ("Helloqui, check my storage") and sometimes splits it ("Hello, Q.").
    static func check(_ text: String, phrase: WakePhrase) -> WakeDecision {
        let spoken = words(text)
        guard !spoken.isEmpty else { return .no("NOTHING_HEARD") }

        var index = 0
        var carry = ""   // letters left over from a token the transcriber ran together
        for lead in phrase.lead {
            if !carry.isEmpty {
                guard carry.hasPrefix(lead) else { return .no("NOT_AT_THE_START") }
                carry = String(carry.dropFirst(lead.count))
                continue
            }
            guard index < spoken.count else { return .no("NOT_AT_THE_START") }
            let token = spoken[index]
            guard token.hasPrefix(lead) else { return .no("NOT_AT_THE_START") }
            carry = String(token.dropFirst(lead.count))
            index += 1
        }

        guard phrase.expectsName else {
            // A lead-only phrase ("computer"): the words themselves are the invocation.
            guard carry.isEmpty else { return .no("NOT_AT_THE_START") }
            return WakeDecision(woke: true, confidence: 1.0, rest: rest(spoken, from: index), why: "HEARD")
        }

        let candidate: String
        if !carry.isEmpty {
            candidate = carry
        } else {
            guard index < spoken.count else { return .no("NAME_NOT_HEARD") }
            candidate = spoken[index]
            index += 1
        }
        guard nameSounds.contains(candidate) else { return .no("NAME_NOT_HEARD") }
        // "kue" written out is the name itself; the rest are sounds it shares.
        let confidence = candidate == "kue" ? 1.0 : 0.8
        return WakeDecision(woke: true, confidence: confidence, rest: rest(spoken, from: index), why: "HEARD")
    }

    private static func rest(_ spoken: [String], from index: Int) -> String {
        index < spoken.count ? spoken[index...].joined(separator: " ") : ""
    }
}

// MARK: - Listening for the invocation

import AVFoundation
import Speech

/// Waits for the invocation and reports NOTHING ELSE.
///
/// The microphone runs, but silence is never recognised: buffers reach the
/// recogniser only while there is voice energy, with half a second of pre-roll
/// so the first word is not clipped. Each utterance gets its own recogniser,
/// finished at the pause that ends it. Words exist in memory for the length of
/// one utterance and are matched here. If the invocation is not heard, they are
/// discarded and nothing is emitted.
///
/// A wake is not authorization and not identity. It says only that the name was
/// heard — by anyone, including a voice on a podcast.
final class WakeEngine {
    enum State: String {
        case off = "OFF", starting = "STARTING", waiting = "WAITING", woke = "WOKE"
        case permissionDenied = "PERMISSION_DENIED", unavailable = "UNAVAILABLE", error = "ERROR"
    }

    private let emitter: Emitter
    /// Called on the main queue when the invocation is heard, with anything the
    /// owner said in the same breath (possibly empty).
    private let onWake: (String, Double) -> Void
    private var engine: AVAudioEngine?
    private var recognizer: WakeRecognizer?
    private var phrase = WakePhrase.parse("computer")
    private(set) var state: State = .off
    private var session = 0
    /// What the recogniser is allowed to see. Touched only by the tap once it runs.
    private var gate = WakeGate()
    private var lastReport = Date.distantPast

    init(emitter: Emitter, onWake: @escaping (String, Double) -> Void) {
        self.emitter = emitter
        self.onWake = onWake
    }

    private func report(_ s: State, detail: String? = nil) {
        state = s
        emitter.emit(.wake(ts: nowTs(), state: s.rawValue, phrase: phrase.spoken, confidence: nil, heard: nil, detail: detail))
    }

    func start(phrase phraseText: String) {
        guard state == .off || state == .error || state == .unavailable || state == .permissionDenied else { return }
        phrase = WakePhrase.parse(phraseText)
        session += 1
        report(.starting)
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: begin()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .audio) { ok in
                DispatchQueue.main.async {
                    ok ? self.begin() : self.report(.permissionDenied, detail: "Microphone access was not granted.")
                }
            }
        default: report(.permissionDenied, detail: "macOS is blocking microphone access.")
        }
    }

    private func begin() {
        guard SpeechTranscriber.isAvailable else {
            report(.unavailable, detail: "On-device speech recognition is not available on this Mac.")
            return
        }
        let mySession = session
        Task { @MainActor in
            do {
                let installed = await SpeechTranscriber.installedLocales
                guard installed.contains(where: { $0.identifier(.bcp47) == WakeRecognizer.locale.identifier(.bcp47) }) else {
                    self.report(.unavailable, detail: "The en-US on-device speech model is not installed.")
                    return
                }
                guard let recognizer = await WakeRecognizer.make() else {
                    self.report(.error, detail: "No audio format is compatible with the speech model.")
                    return
                }
                // Stopped while the model was being checked: do not open a microphone.
                guard mySession == self.session, self.state == .starting else { return }

                let engine = AVAudioEngine()
                let input = engine.inputNode
                let hw = input.outputFormat(forBus: 0)
                guard hw.channelCount > 0, hw.sampleRate > 0 else {
                    self.report(.error, detail: "No microphone input is available.")
                    return
                }
                guard let converter = AVAudioConverter(from: hw, to: recognizer.format) else {
                    self.report(.error, detail: "Could not convert microphone audio for the speech model.")
                    return
                }
                self.gate = WakeGate()
                self.recognizer = recognizer
                input.installTap(onBus: 0, bufferSize: 2048, format: hw) { [weak self] buffer, _ in
                    guard let self, mySession == self.session else { return }
                    self.feed(buffer, converter: converter, recognizer: recognizer, session: mySession)
                }
                engine.prepare()
                try engine.start()
                self.engine = engine
                self.report(.waiting)
            } catch {
                self.stop(reason: "ERROR")
                self.report(.error, detail: "Could not start listening: \(error.localizedDescription)")
            }
        }
    }

    /// Voice energy decides what the recogniser ever sees. Silence is not
    /// transcribed — not as a policy, as a wiring: those buffers are dropped.
    private func feed(_ buffer: AVAudioPCMBuffer, converter: AVAudioConverter,
                      recognizer: WakeRecognizer, session mySession: Int) {
        guard let copy = buffer.copyForWake() else { return }
        let now = Date()
        for b in gate.admit(copy, at: now.timeIntervalSinceReferenceDate) {
            if let out = WakeEngine.convert(b, with: converter, to: recognizer.format) { recognizer.yield(out) }
        }
        // The pause that ends an utterance ends its recogniser, and the one
        // decision is taken over what it heard.
        if gate.segmentEnded, let utterance = recognizer.close() {
            let phrase = self.phrase
            Task { [weak self] in
                do {
                    // The ONE place words are looked at, and the only thing
                    // that may leave is the decision.
                    let finals = try await utterance.finish()
                    guard let decision = WakeUtterance.decide(finals, phrase: phrase) else { return }
                    await self?.heard(decision, session: mySession)
                } catch {
                    await self?.failed(error.localizedDescription, session: mySession)
                }
            }
        }
        heartbeat(now)
    }

    /// Says only "still here", every few seconds, so the window can tell the
    /// owner the microphone is on and stop saying so if this stops.
    ///
    /// Deliberately carries NO voice activity. A message each time someone in
    /// the room speaks would be a record of when people were talking — a
    /// presence archive by another name — and the window does not need one to
    /// say that KUE is waiting for its name.
    private func heartbeat(_ now: Date) {
        guard now.timeIntervalSince(lastReport) > 5.0 else { return }
        lastReport = now
        emitter.emit(.wake(ts: nowTs(), state: State.waiting.rawValue, phrase: phrase.spoken,
                           confidence: nil, heard: nil, detail: nil))
    }

    @MainActor private func failed(_ message: String, session mySession: Int) {
        guard mySession == session else { return }
        stop(reason: "ERROR")
        report(.error, detail: "Listening failed: \(message)")
    }

    @MainActor private func heard(_ decision: WakeDecision, session mySession: Int) {
        guard mySession == session, state == .waiting else { return }
        state = .woke
        emitter.emit(.wake(ts: nowTs(), state: State.woke.rawValue, phrase: phrase.spoken,
                           confidence: decision.confidence, heard: decision.rest, detail: nil))
        // The microphone is handed to the ordinary listening session, which is
        // the path that already tells the core it is listening.
        stop(reason: "WOKE")
        onWake(decision.rest, decision.confidence)
    }

    func stop(reason: String) {
        if let e = engine {
            e.inputNode.removeTap(onBus: 0)
            e.stop()
        }
        engine = nil
        recognizer?.shutdown()
        recognizer = nil
        session += 1
        // A wake has already been reported, and that report IS the end of this
        // session: the core takes WOKE to mean the microphone was handed over,
        // and starts the listener again itself. Any other stop says so.
        if reason != "WOKE" { report(.off, detail: reason) } else { state = .off }
    }

    static func convert(_ b: AVAudioPCMBuffer, with converter: AVAudioConverter, to format: AVAudioFormat) -> AVAudioPCMBuffer? {
        let ratio = format.sampleRate / b.format.sampleRate
        let cap = AVAudioFrameCount(Double(b.frameLength) * ratio) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: cap) else { return nil }
        var fed = false
        var err: NSError?
        converter.convert(to: out, error: &err) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return b
        }
        return err == nil && out.frameLength > 0 ? out : nil
    }

    /// The live listener's path with a file in place of the microphone: the
    /// same gate, the same per-utterance recogniser, the same decision, audio
    /// delivered at the pace a microphone delivers it, and — like a room — no
    /// end of input. A wake that only happens because the file ended is not a
    /// wake the live path would ever see.
    ///
    /// MEASUREMENT ONLY. It returns what the recogniser said about a clip made
    /// to be measured; the live listener has no such path.
    static func wakeStream(_ path: String, phrase phraseText: String, realTime: Bool,
                           stopAtWake: Bool = true, leadingQuiet: Double = 1.0) async -> [String: Any] {
        let phrase = WakePhrase.parse(phraseText)
        var out: [String: Any] = ["phrase": phraseText, "realTime": realTime]
        do {
            let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
            let src = file.processingFormat
            guard let recognizer = await WakeRecognizer.make(),
                  let converter = AVAudioConverter(from: src, to: recognizer.format) else {
                out["error"] = "no compatible audio format"; return out
            }
            let log = WakeStreamLog()
            let started = Date()
            var gate = WakeGate()
            let chunk = AVAudioFrameCount(src.sampleRate / 10)
            var audioTime = 0.0
            var speechEndedAt: Double? = nil
            var pending: [Task<Void, Never>] = []
            func quiet() -> AVAudioPCMBuffer? {
                guard let b = AVAudioPCMBuffer(pcmFormat: src, frameCapacity: chunk) else { return nil }
                b.frameLength = chunk
                if let ch = b.floatChannelData { for c in 0..<Int(src.channelCount) { ch[c].update(repeating: 0, count: Int(chunk)) } }
                return b
            }
            func push(_ b: AVAudioPCMBuffer) async {
                let seconds = Double(b.frameLength) / src.sampleRate
                if let db = WakeGate.level(b), db > WakeGate.activityThresholdDb { speechEndedAt = audioTime + seconds }
                for g in gate.admit(b, at: audioTime) {
                    if let c = convert(g, with: converter, to: recognizer.format) { recognizer.yield(c) }
                }
                if gate.segmentEnded, let utterance = recognizer.close() {
                    let closedAt = Date().timeIntervalSince(started)
                    pending.append(Task {
                        do {
                            let finals = try await utterance.finish()
                            await log.utterance(closedAt: closedAt, decidedAt: Date().timeIntervalSince(started),
                                                finals: finals, decision: WakeUtterance.decide(finals, phrase: phrase))
                        } catch {
                            await log.failed(error.localizedDescription)
                        }
                    })
                }
                audioTime += seconds
                if realTime { try? await Task.sleep(for: .seconds(seconds)) }
            }
            // A room is quiet before anyone speaks, and a clip made by `say` is
            // not: a second of quiet first, as a microphone would deliver.
            for _ in 0..<Int((leadingQuiet * 10).rounded()) { if let b = quiet() { await push(b) } }
            while file.framePosition < file.length {
                if stopAtWake, await log.woke { break }
                guard let b = AVAudioPCMBuffer(pcmFormat: src, frameCapacity: chunk) else { break }
                try file.read(into: b, frameCount: chunk)
                if b.frameLength == 0 { break }
                await push(b)
            }
            // Then a quiet room for a while.
            let quietUntil = audioTime + 4.0
            while audioTime < quietUntil {
                if stopAtWake, await log.woke { break }
                guard let b = quiet() else { break }
                await push(b)
            }
            for t in pending { await t.value }
            recognizer.shutdown()

            let d = await log.decision
            let wokeAt = await log.wokeAt
            out["woke"] = d != nil
            out["rest"] = d?.rest ?? ""
            out["confidence"] = d?.confidence ?? 0
            out["speechEndedAt"] = speechEndedAt as Any
            out["wokeAt"] = wokeAt as Any
            if let w = wokeAt, let e = speechEndedAt { out["wokeAfterSpeechEnded"] = w - e }
            out["resultsError"] = await log.error as Any
            out["utterances"] = await log.utterances.map {
                ["closedAt": $0.closedAt, "decidedAt": $0.decidedAt, "finals": $0.finals, "woke": $0.woke] as [String: Any]
            }
        } catch {
            out["error"] = error.localizedDescription
        }
        return out
    }

    /// The same boundary, without a microphone: transcribes a file and applies
    /// the same matcher. NOTE: this is the FINISHED-FILE path — final results
    /// only, end of input known. It measures the recogniser and the matcher,
    /// not the live listener's timing; `wakeStream` measures that.
    static func wakeFile(_ path: String, phrase phraseText: String) async -> (decision: WakeDecision, error: String?) {
        let phrase = WakePhrase.parse(phraseText)
        let (acc, err) = await VoiceEngine.transcribeFile(path, volatile: false)
        if let e = err { return (WakeDecision.no("TRANSCRIBE_FAILED"), e) }
        return (WakeMatcher.check(acc.finalized, phrase: phrase), nil)
    }
}

/// One recogniser per utterance. The gate says where an utterance ends; this
/// finishes the recogniser there and starts a fresh one for the next.
///
/// Why, as MEASURED with `--wake-stream` over five voices on this Mac:
/// - A recogniser whose input never ends — a microphone's — returned nothing
///   at all for "Computer, check my storage." followed by silence: 0 wakes in
///   15 such clips. The gate drops the silence it would have used to decide the
///   utterance was over, so the gate has to tell it.
/// - Finalizing a long-lived recogniser at each pause fixed that, but the first
///   word of the NEXT utterance came back as "...." for two voices in five, so
///   the invocation was lost. Explicit timestamps did not help. The first
///   utterance a recogniser hears was clean every time.
/// - Provisional results are not requested: a provisional "Computer" arrived
///   ~50 ms before the final "Computer, check my storage." for four voices in
///   five, and waking on it lost the request.
final class WakeRecognizer: @unchecked Sendable {
    static let locale = Locale(identifier: "en-US")
    let format: AVAudioFormat
    private let lock = NSLock()
    private var open: Utterance?
    private var shut = false

    private init(format: AVAudioFormat) { self.format = format }

    private static func transcriber() -> SpeechTranscriber {
        SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [], attributeOptions: [])
    }

    /// nil when no audio format suits the speech model.
    static func make() async -> WakeRecognizer? {
        guard let f = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber()]) else { return nil }
        return WakeRecognizer(format: f)
    }

    /// Audio already in `format`, for the utterance in progress — starting one
    /// if none is.
    func yield(_ buffer: AVAudioPCMBuffer) {
        lock.lock(); defer { lock.unlock() }
        guard !shut else { return }
        if open == nil { open = Utterance(transcriber: WakeRecognizer.transcriber()) }
        open?.continuation.yield(AnalyzerInput(buffer: buffer))
    }

    /// Ends the utterance in progress. Immediate, so audio that follows goes to
    /// a new recogniser; finishing the old one is the caller's `finish()`.
    func close() -> Utterance? {
        lock.lock(); defer { lock.unlock() }
        let u = open
        open = nil
        return u
    }

    /// Nothing more is recognised, and whatever was in progress is abandoned.
    func shutdown() {
        lock.lock()
        shut = true
        let u = open
        open = nil
        lock.unlock()
        u?.cancel()
    }

    final class Utterance: @unchecked Sendable {
        fileprivate let continuation: AsyncStream<AnalyzerInput>.Continuation
        private let analyzer: SpeechAnalyzer
        private let started: Task<Void, Error>
        private let finals: Task<[String], Error>

        fileprivate init(transcriber: SpeechTranscriber) {
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            let (stream, continuation) = AsyncStream<AnalyzerInput>.makeStream()
            self.analyzer = analyzer
            self.continuation = continuation
            self.finals = Task {
                var out: [String] = []
                for try await r in transcriber.results where r.isFinal { out.append(String(r.text.characters)) }
                return out
            }
            self.started = Task { try await analyzer.start(inputSequence: stream) }
        }

        /// What was said, as the recogniser's final results in order.
        func finish() async throws -> [String] {
            continuation.finish()
            try await started.value
            try await analyzer.finalizeAndFinishThroughEndOfInput()
            return try await finals.value
        }

        func cancel() {
            continuation.finish()
            finals.cancel()
            let a = analyzer
            Task { await a.cancelAndFinishNow() }
        }
    }
}

/// The one decision for one utterance, from its final results in order.
///
/// The invocation must begin one of them. What follows it — in that result and
/// in any results after it — is the request, so "Computer." and "Check my
/// storage." recognised as two results are the one request the owner meant.
/// MEASURED: with a 0.7 s pause after the name, deciding on the first result
/// alone lost the request for four voices in five.
enum WakeUtterance {
    static func decide(_ finals: [String], phrase: WakePhrase) -> WakeDecision? {
        for (i, text) in finals.enumerated() {
            let d = WakeMatcher.check(text, phrase: phrase)
            guard d.woke else { continue }
            let following = finals[(i + 1)...].flatMap { WakeMatcher.words($0) }
            let rest = ([d.rest] + following).filter { !$0.isEmpty }.joined(separator: " ")
            return WakeDecision(woke: true, confidence: d.confidence, rest: rest, why: "HEARD")
        }
        return nil
    }

    /// Returns the failures; empty means every check held.
    static func check() -> [String] {
        let phrase = WakePhrase.parse("computer")
        var failures: [String] = []
        func expect(_ finals: [String], _ rest: String?) {
            let d = decide(finals, phrase: phrase)
            if d?.rest != rest { failures.append("\(finals) → \(d.map { "woke, \"\($0.rest)\"" } ?? "no wake"), expected \(rest.map { "woke, \"\($0)\"" } ?? "no wake")") }
        }
        expect(["Computer, check my storage."], "check my storage")
        expect(["Computer.", "Check my storage."], "check my storage")
        expect(["Computer."], "")
        expect(["My computer is slow today."], nil)
        expect(["My computer is slow today.", "Computer, open Safari."], "open safari")
        expect([], nil)
        return failures
    }
}


/// What the recogniser is allowed to see: voice energy decides, half a second
/// of pre-roll is held back so a first word is not clipped, and a segment ends
/// after a pause. Clocked by the caller, so the live listener and the
/// measurement harness run the same gate.
struct WakeGate {
    /// The same energy threshold the listening session uses.
    static let activityThresholdDb: Float = -42
    static let preRollSeconds = 0.5
    static let segmentSilenceSeconds = 1.5

    private var preRoll: [AVAudioPCMBuffer] = []
    private var preRollFrames: AVAudioFrameCount = 0
    private(set) var speaking = false
    private var lastVoiceAt = -Double.infinity
    /// Set by `admit` on the buffer where a spoken segment ended.
    private(set) var segmentEnded = false

    /// RMS level of the first channel, in dB.
    static func level(_ buffer: AVAudioPCMBuffer) -> Float? {
        guard let ch = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return nil }
        var sum: Float = 0
        for i in 0..<Int(buffer.frameLength) { sum += ch[i] * ch[i] }
        return 20 * log10(max(sqrt(sum / Float(buffer.frameLength)), 1e-7))
    }

    /// The buffers to hand the recogniser now, oldest first — none while quiet.
    /// `buffer` must outlive the call (a copy, not a tap's reused buffer).
    mutating func admit(_ buffer: AVAudioPCMBuffer, at t: Double) -> [AVAudioPCMBuffer] {
        segmentEnded = false
        guard let db = WakeGate.level(buffer) else { return [] }
        if db > WakeGate.activityThresholdDb {
            speaking = true
            lastVoiceAt = t
        } else if speaking, t - lastVoiceAt > WakeGate.segmentSilenceSeconds {
            speaking = false
            segmentEnded = true
        }
        if !speaking {
            // Keep the shortest run of recent buffers that still covers the
            // pre-roll. (An earlier version computed the length once and then
            // trimmed against that stale total, keeping as little as one
            // buffer — a first word could be clipped.)
            preRoll.append(buffer)
            preRollFrames += buffer.frameLength
            let keep = AVAudioFrameCount(WakeGate.preRollSeconds * buffer.format.sampleRate)
            while preRoll.count > 1, preRollFrames - preRoll[0].frameLength >= keep {
                preRollFrames -= preRoll.removeFirst().frameLength
            }
            return []
        }
        let out = preRoll + [buffer]
        preRoll.removeAll()
        preRollFrames = 0
        return out
    }

    /// Seconds of audio held back as pre-roll right now. For tests.
    var heldSeconds: Double {
        guard let f = preRoll.first else { return 0 }
        return Double(preRollFrames) / f.format.sampleRate
    }
}

extension WakeGate {
    /// The gate's own properties, checked with synthetic audio — no microphone,
    /// no recogniser. Returns the failures; empty means every check held.
    static func check() -> [String] {
        var failures: [String] = []
        let rate = 48_000.0
        guard let fmt = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1) else { return ["no format"] }
        func buffer(seconds: Double, amplitude: Float) -> AVAudioPCMBuffer {
            let n = AVAudioFrameCount(seconds * rate)
            let b = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: n)!
            b.frameLength = n
            let ch = b.floatChannelData![0]
            for i in 0..<Int(n) { ch[i] = amplitude * (i % 2 == 0 ? 1 : -1) }
            return b
        }
        let quiet: Float = 0.001   // -60 dB
        let voice: Float = 0.1     // -20 dB
        var g = WakeGate()
        var t = 0.0
        var admitted = 0
        // A quiet room, in the ~100 ms buffers a microphone delivers. Once half a
        // second has passed, the pre-roll must hold half a second after EVERY
        // buffer — the defect it replaced held that much only some of the time.
        var short: [String] = []
        for i in 0..<23 {
            admitted += g.admit(buffer(seconds: 0.1, amplitude: quiet), at: t).count; t += 0.1
            if i >= 5, g.heldSeconds < WakeGate.preRollSeconds - 0.01 || g.heldSeconds > WakeGate.preRollSeconds + 0.11 {
                short.append(String(format: "%.1f", g.heldSeconds))
            }
        }
        if admitted != 0 { failures.append("silence reached the recogniser (\(admitted) buffers)") }
        if !short.isEmpty { failures.append("pre-roll held \(short.joined(separator: ", ")) s instead of \(WakeGate.preRollSeconds)") }
        // The first voiced buffer brings the pre-roll with it.
        let first = g.admit(buffer(seconds: 0.1, amplitude: voice), at: t); t += 0.1
        let firstSeconds = first.reduce(0.0) { $0 + Double($1.frameLength) / rate }
        if firstSeconds < 0.5 { failures.append("the first word arrived with only \(firstSeconds) s before it") }
        // A pause shorter than the segment silence keeps the segment open…
        for _ in 0..<10 { _ = g.admit(buffer(seconds: 0.1, amplitude: quiet), at: t); t += 0.1 }
        if !g.speaking { failures.append("a one-second pause ended the utterance") }
        if g.segmentEnded { failures.append("a one-second pause was reported as the end of the utterance") }
        // …and a longer one ends it, once, and says so.
        var ends = 0
        for _ in 0..<10 { _ = g.admit(buffer(seconds: 0.1, amplitude: quiet), at: t); t += 0.1; if g.segmentEnded { ends += 1 } }
        if g.speaking { failures.append("two quiet seconds did not end the utterance") }
        if ends != 1 { failures.append("the end of the utterance was reported \(ends) times") }
        return failures
    }
}

/// What `wakeStream` saw. Measurement only.
actor WakeStreamLog {
    struct Utterance { let closedAt: Double; let decidedAt: Double; let finals: [String]; let woke: Bool }
    private(set) var utterances: [Utterance] = []
    private(set) var decision: WakeDecision?
    private(set) var wokeAt: Double?
    private(set) var error: String?
    var woke: Bool { decision != nil }

    func utterance(closedAt: Double, decidedAt: Double, finals: [String], decision d: WakeDecision?) {
        utterances.append(Utterance(closedAt: closedAt, decidedAt: decidedAt, finals: finals, woke: d != nil))
        if decision == nil, let d { decision = d; wokeAt = decidedAt }
    }

    func failed(_ message: String) { error = message }
}

extension AVAudioPCMBuffer {
    /// A copy that outlives the tap callback.
    func copyForWake() -> AVAudioPCMBuffer? {
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameLength) else { return nil }
        out.frameLength = frameLength
        let channels = Int(format.channelCount)
        if let src = floatChannelData, let dst = out.floatChannelData {
            for c in 0..<channels {
                dst[c].update(from: src[c], count: Int(frameLength))
            }
        }
        return out
    }
}
