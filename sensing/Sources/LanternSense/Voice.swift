// Lantern sensing layer — voice.
//
// Three separate stages, never merged:
//   VOICE_ACTIVITY      audio level from the microphone buffer (energy, in dBFS)
//   SPEECH_RECOGNITION  Apple SpeechAnalyzer + SpeechTranscriber, on this Mac
//   SPEAKER_IDENTITY    NOT IMPLEMENTED. Nothing here says who is speaking, and
//                       a transcript is never treated as authentication.
//
// Push-to-talk only: the microphone runs for one listening session started by an
// explicit command, ends on stop, silence, a time limit, pause, or shutdown.
// Audio buffers are analysed in memory and released. Nothing is recorded.

import Foundation
import AVFoundation
import Speech

/// Builds one session's transcript from SpeechTranscriber results.
///
/// The transcriber finalizes per segment (measured: a three-sentence clip
/// produced three final results), and with volatile results it also sends
/// provisional text for the segment in progress. The running transcript is
/// every finalized segment plus the current one. Shared by the microphone path
/// and the file self-test, so the self-test exercises the same logic.
struct TranscriptAccumulator {
    private(set) var finalized = ""
    private(set) var finalSegments = 0
    private(set) var volatileResults = 0

    /// Adds one result and returns the running transcript.
    mutating func add(_ text: String, isFinal: Bool) -> String {
        let running = VoiceEngine.join(finalized, text)
        if isFinal { finalized = running; finalSegments += 1 } else { volatileResults += 1 }
        return running
    }
}

final class VoiceEngine {
    enum State: String {
        case idle = "IDLE", starting = "STARTING", listening = "LISTENING", finishing = "FINISHING"
        case permissionDenied = "PERMISSION_DENIED", noMicrophone = "NO_MICROPHONE"
        case unavailable = "UNAVAILABLE", error = "ERROR"
    }

    private let emitter: Emitter
    private let q = DispatchQueue(label: "dev.lantern.sense.voice")
    private var engine: AVAudioEngine?
    private var analyzer: SpeechAnalyzer?
    private var inputContinuation: AsyncStream<AnalyzerInput>.Continuation?
    private var resultsTask: Task<Void, Never>?
    private var session = 0
    private(set) var state: State = .idle
    private var startedAt = Date()
    private var lastVoiceAt: Date?
    private var heardSpeech = false
    private var lastLevelEmit = Date.distantPast
    private var maxSeconds = 30.0
    private var silenceSeconds = 1.8
    /// Energy threshold for "voice activity". A level, not a speech classifier.
    private let activityThresholdDb: Float = -42
    private var watchdog: Timer?
    /// This session's transcript so far. Emitted as final ONCE, when the session
    /// ends, so a multi-sentence request is never delivered as fragments.
    private var transcript = TranscriptAccumulator()

    init(emitter: Emitter) { self.emitter = emitter }

    private func emitVoice(_ s: State, level: Float? = nil, active: Bool = false, detail: String? = nil) {
        state = s
        emitter.emit(.voice(ts: nowTs(), state: s.rawValue, session: session, levelDb: level.map(Double.init),
                            voiceActive: active, detail: detail))
    }

    static func permission() -> String {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return "AUTHORIZED"
        case .denied: return "DENIED"
        case .restricted: return "RESTRICTED"
        case .notDetermined: return "NOT_DETERMINED"
        @unknown default: return "UNKNOWN"
        }
    }

    func start(maxSeconds: Double, silenceSeconds: Double) {
        guard state == .idle || state == .permissionDenied || state == .error || state == .unavailable || state == .noMicrophone else { return }
        self.maxSeconds = max(3, min(maxSeconds, 60))
        self.silenceSeconds = max(0.8, min(silenceSeconds, 5))
        session += 1
        emitVoice(.starting)
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: begin()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .audio) { ok in
                DispatchQueue.main.async { ok ? self.begin() : self.emitVoice(.permissionDenied, detail: "Microphone access was not granted.") }
            }
        default:
            emitVoice(.permissionDenied, detail: "macOS is blocking microphone access for KUE.")
        }
    }

    private func begin() {
        guard SpeechTranscriber.isAvailable else {
            emitVoice(.unavailable, detail: "On-device speech recognition is not available on this Mac.")
            return
        }
        let mySession = session
        Task { @MainActor in
            do {
                let locale = Locale(identifier: "en-US")
                let installed = await SpeechTranscriber.installedLocales
                guard installed.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) else {
                    // Installing would download Apple's speech model. Lantern does not start downloads on its own.
                    self.emitVoice(.unavailable, detail: "The en-US on-device speech model is not installed.")
                    return
                }
                let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [],
                                                    reportingOptions: [.volatileResults], attributeOptions: [])
                let analyzer = SpeechAnalyzer(modules: [transcriber])
                guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
                    self.emitVoice(.error, detail: "No audio format is compatible with the speech model.")
                    return
                }
                let (stream, continuation) = AsyncStream<AnalyzerInput>.makeStream()
                self.inputContinuation = continuation
                self.analyzer = analyzer

                let engine = AVAudioEngine()
                let input = engine.inputNode
                let hw = input.outputFormat(forBus: 0)
                guard hw.channelCount > 0, hw.sampleRate > 0 else {
                    self.emitVoice(.noMicrophone, detail: "No microphone input is available.")
                    return
                }
                guard let converter = AVAudioConverter(from: hw, to: format) else {
                    self.emitVoice(.error, detail: "Could not convert microphone audio for the speech model.")
                    return
                }
                input.installTap(onBus: 0, bufferSize: 2048, format: hw) { [weak self] buffer, _ in
                    guard let self else { return }
                    self.measure(buffer, session: mySession)
                    let ratio = format.sampleRate / hw.sampleRate
                    let cap = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 32
                    guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: cap) else { return }
                    var fed = false
                    var err: NSError?
                    converter.convert(to: out, error: &err) { _, status in
                        if fed { status.pointee = .noDataNow; return nil }
                        fed = true; status.pointee = .haveData; return buffer
                    }
                    if err == nil, out.frameLength > 0 { continuation.yield(AnalyzerInput(buffer: out)) }
                }
                engine.prepare()
                try engine.start()
                self.engine = engine
                self.startedAt = Date()
                self.heardSpeech = false
                self.lastVoiceAt = nil

                self.transcript = TranscriptAccumulator()
                self.resultsTask = Task {
                    do {
                        for try await r in transcriber.results {
                            let running = self.transcript.add(String(r.text.characters), isFinal: r.isFinal)
                            self.emitter.emit(.transcript(session: mySession, text: running, isFinal: false))
                        }
                    } catch {
                        self.emitter.emit(.voice(ts: nowTs(), state: State.error.rawValue, session: mySession, levelDb: nil,
                                                 voiceActive: false, detail: "Speech recognition failed: \(error.localizedDescription)"))
                    }
                }
                try await analyzer.start(inputSequence: stream)
                self.emitVoice(.listening)
                self.watchdog = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in self?.checkLimits() }
            } catch {
                self.teardown()
                self.emitVoice(.error, detail: "Could not start listening: \(error.localizedDescription)")
            }
        }
    }

    /// Voice activity from buffer energy. The buffer is not kept.
    private func measure(_ buffer: AVAudioPCMBuffer, session mySession: Int) {
        guard let ch = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return }
        var sum: Float = 0
        for i in 0..<Int(buffer.frameLength) { sum += ch[i] * ch[i] }
        let rms = sqrt(sum / Float(buffer.frameLength))
        let db = 20 * log10(max(rms, 1e-7))
        let active = db > activityThresholdDb
        DispatchQueue.main.async {
            guard mySession == self.session, self.state == .listening else { return }
            if active { self.lastVoiceAt = Date(); self.heardSpeech = true }
            if Date().timeIntervalSince(self.lastLevelEmit) > 0.2 {
                self.lastLevelEmit = Date()
                self.emitter.emit(.voice(ts: nowTs(), state: State.listening.rawValue, session: mySession,
                                         levelDb: Double(db), voiceActive: active, detail: nil))
            }
        }
    }

    private func checkLimits() {
        guard state == .listening else { return }
        let elapsed = Date().timeIntervalSince(startedAt)
        if elapsed >= maxSeconds { finish(reason: "TIME_LIMIT"); return }
        if heardSpeech, let last = lastVoiceAt, Date().timeIntervalSince(last) >= silenceSeconds {
            finish(reason: "SILENCE")
        } else if !heardSpeech && elapsed >= 8 {
            finish(reason: "NO_SPEECH")
        }
    }

    /// Stops the microphone and lets the recogniser finish what it heard.
    func finish(reason: String) {
        guard state == .listening || state == .starting else { return }
        emitVoice(.finishing, detail: reason)
        stopMicrophone()
        inputContinuation?.finish()
        let analyzer = self.analyzer
        let results = resultsTask
        let mySession = session
        Task { @MainActor in
            try? await analyzer?.finalizeAndFinishThroughEndOfInput()
            _ = await results?.value
            let heard = self.transcript.finalized.trimmingCharacters(in: .whitespacesAndNewlines)
            if !heard.isEmpty { self.emitter.emit(.transcript(session: mySession, text: heard, isFinal: true)) }
            self.transcript = TranscriptAccumulator()
            self.teardown()
            self.emitVoice(.idle, detail: reason)
        }
    }

    /// Stops immediately and discards anything not yet transcribed.
    func cancel() {
        guard state != .idle else { return }
        stopMicrophone()
        inputContinuation?.finish()
        resultsTask?.cancel()
        let analyzer = self.analyzer
        Task { await analyzer?.cancelAndFinishNow() }
        teardown()
        emitVoice(.idle, detail: "CANCELLED")
    }

    static func join(_ a: String, _ b: String) -> String {
        let (x, y) = (a.trimmingCharacters(in: .whitespaces), b.trimmingCharacters(in: .whitespaces))
        if x.isEmpty { return y }
        if y.isEmpty { return x }
        return x + " " + y
    }

    /// Self-test without a microphone: transcribes an audio file through the same
    /// transcriber settings and the same `TranscriptAccumulator` as the live path.
    /// Only reachable through `--transcribe-file`; nothing is written to stderr.
    static func transcribeFile(_ path: String, volatile: Bool, bias: [String] = []) async -> (accumulator: TranscriptAccumulator, error: String?) {
        var acc = TranscriptAccumulator()
        do {
            let locale = Locale(identifier: "en-US")
            let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [],
                                                reportingOptions: volatile ? [.volatileResults] : [], attributeOptions: [])
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            if !bias.isEmpty {
                let context = AnalysisContext()
                context.contextualStrings = [.general: bias]
                try await analyzer.setContext(context)
            }
            let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
            let collect = Task { () -> TranscriptAccumulator in
                var a = TranscriptAccumulator()
                for try await r in transcriber.results { _ = a.add(String(r.text.characters), isFinal: r.isFinal) }
                return a
            }
            if let last = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            acc = try await collect.value
            return (acc, nil)
        } catch {
            return (acc, error.localizedDescription)
        }
    }

    private func stopMicrophone() {
        watchdog?.invalidate(); watchdog = nil
        if let e = engine {
            e.inputNode.removeTap(onBus: 0)
            e.stop()
        }
        engine = nil
    }

    private func teardown() {
        stopMicrophone()
        inputContinuation = nil
        analyzer = nil
        resultsTask = nil
    }
}
