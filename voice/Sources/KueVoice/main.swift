// KUE voice — speech output, and nothing else.
//
// A separate process on purpose. It holds no camera, no microphone and no
// screen handle, and it decides nothing: it speaks the text KUE Core already
// cleared, in the voice KUE Core chose, and reports when speech started,
// finished or was cut off. It is terminated by the kill switch.
//
// Apple's AVSpeechSynthesizer, on this Mac. No network. Nothing is recorded:
// audio goes to the output device and is not written anywhere. Text arrives on
// standard input, never as an argument, so it never appears in the process table.
//
// Protocol: newline-delimited JSON.
//   stdin  <- {"cmd":"speak","id":..,"text":..,"voice":..,"rate":..,"volume":..,"interrupt":bool}
//             {"cmd":"stop"} | {"cmd":"shutdown"}
//   stdout -> hello (with the installed voices) | started | finished | cancelled | failed
//
// Self-test without playing sound (text on stdin):
//   kue-voice --synthesize <voice identifier>   → one JSON line with frame counts

import Foundation
import AVFoundation

let buildStamp = "kue-voice/0.1.0"

// Frameworks may write diagnostics to fd 1; keep a private copy for the protocol.
let protocolFd = dup(1)
dup2(2, 1)

let outQueue = DispatchQueue(label: "dev.lantern.voice.out")

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

/// The longest text one utterance may carry. KUE Core caps below this.
let maxCharacters = 4000

func describe(_ v: AVSpeechSynthesisVoice) -> [String: Any] {
    let quality: String
    switch v.quality {
    case .premium: quality = "PREMIUM"
    case .enhanced: quality = "ENHANCED"
    default: quality = "DEFAULT"
    }
    let gender: String
    switch v.gender {
    case .female: gender = "FEMALE"
    case .male: gender = "MALE"
    default: gender = "UNSPECIFIED"
    }
    return [
        "identifier": v.identifier, "name": v.name, "language": v.language,
        "quality": quality, "gender": gender,
        "novelty": v.voiceTraits.contains(.isNoveltyVoice),
        "personal": v.voiceTraits.contains(.isPersonalVoice),
    ]
}

/// A voice by identifier. A Personal Voice — a synthetic copy of a real
/// person's voice — is never used, whoever asks for it.
func voice(_ identifier: String) -> AVSpeechSynthesisVoice? {
    guard let v = AVSpeechSynthesisVoice(identifier: identifier) else { return nil }
    return v.voiceTraits.contains(.isPersonalVoice) ? nil : v
}

func utterance(_ text: String, _ v: AVSpeechSynthesisVoice, rate: Float, volume: Float) -> AVSpeechUtterance {
    let u = AVSpeechUtterance(string: text)
    u.voice = v
    u.rate = max(AVSpeechUtteranceMinimumSpeechRate, min(rate, AVSpeechUtteranceMaximumSpeechRate))
    u.volume = max(0, min(volume, 1))
    return u
}

// MARK: - Self-test: synthesize to memory, play nothing

if CommandLine.arguments.count == 3 && CommandLine.arguments[1] == "--synthesize" {
    let text = String(data: FileHandle.standardInput.readDataToEndOfFile(), encoding: .utf8)?
        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard let v = voice(CommandLine.arguments[2]), !text.isEmpty else {
        emit(["type": "synthesized", "frames": 0, "error": "NO_VOICE_OR_TEXT"])
        outQueue.sync {}
        exit(1)
    }
    let synth = AVSpeechSynthesizer()
    var frames = 0
    var sampleRate = 0.0
    var finished = false
    // Buffers are delivered on the main run loop, so it has to keep running.
    synth.write(utterance(text, v, rate: AVSpeechUtteranceDefaultSpeechRate, volume: 1)) { buffer in
        guard let pcm = buffer as? AVAudioPCMBuffer else { return }
        if pcm.frameLength == 0 { finished = true; return }
        frames += Int(pcm.frameLength)
        sampleRate = pcm.format.sampleRate
    }
    let deadline = Date().addingTimeInterval(30)
    while !finished && Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
    emit(["type": "synthesized", "frames": frames, "sample_rate": sampleRate,
          "seconds": sampleRate > 0 ? Double(frames) / sampleRate : 0, "completed": finished])
    outQueue.sync {}
    exit(frames > 0 ? 0 : 1)
}

// MARK: - The speaking process

/// Used only on the main queue: commands are dispatched there, and the
/// synthesizer calls its delegate there.
final class Speaker: NSObject, AVSpeechSynthesizerDelegate, @unchecked Sendable {
    let synth = AVSpeechSynthesizer()
    private var ids: [ObjectIdentifier: String] = [:]
    /// Utterances cut off by stop or interrupt. Measured on this Mac: the
    /// synthesizer reports those through didFinish, not didCancel, so KUE keeps
    /// its own record rather than report a cut-off sentence as finished.
    private var cutOff: Set<ObjectIdentifier> = []
    /// The utterance being spoken now, between didStart and didFinish.
    private var current: ObjectIdentifier?

    override init() {
        super.init()
        synth.delegate = self
    }

    func speak(id: String, text: String, voiceId: String, rate: Float, volume: Float, interrupt: Bool) {
        guard !text.isEmpty, text.count <= maxCharacters else { emit(["type": "failed", "id": id, "reason": "BAD_TEXT"]); return }
        guard let v = voice(voiceId) else { emit(["type": "failed", "id": id, "reason": "VOICE_NOT_AVAILABLE"]); return }
        if interrupt { stop() }
        let u = utterance(text, v, rate: rate, volume: volume)
        ids[ObjectIdentifier(u)] = id
        synth.speak(u)
    }

    /// Stops the sentence being spoken and drops everything queued behind it.
    /// Measured: queued utterances dropped by stopSpeaking get no delegate call
    /// at all, so they are reported cancelled here.
    func stop() {
        guard synth.isSpeaking || !ids.isEmpty else { return }
        for (key, id) in ids where key != current {
            emit(["type": "cancelled", "id": id])
            ids.removeValue(forKey: key)
        }
        if let c = current { cutOff.insert(c) }
        synth.stopSpeaking(at: .immediate)
    }

    private func id(_ u: AVSpeechUtterance, remove: Bool) -> String {
        let key = ObjectIdentifier(u)
        let id = ids[key] ?? ""
        if remove { ids.removeValue(forKey: key) }
        return id
    }

    func speechSynthesizer(_ s: AVSpeechSynthesizer, didStart u: AVSpeechUtterance) {
        current = ObjectIdentifier(u)
        emit(["type": "started", "id": id(u, remove: false)])
    }
    func speechSynthesizer(_ s: AVSpeechSynthesizer, didFinish u: AVSpeechUtterance) {
        if current == ObjectIdentifier(u) { current = nil }
        let wasCutOff = cutOff.remove(ObjectIdentifier(u)) != nil
        emit(["type": wasCutOff ? "cancelled" : "finished", "id": id(u, remove: true)])
    }
    func speechSynthesizer(_ s: AVSpeechSynthesizer, didCancel u: AVSpeechUtterance) {
        if current == ObjectIdentifier(u) { current = nil }
        cutOff.remove(ObjectIdentifier(u))
        emit(["type": "cancelled", "id": id(u, remove: true)])
    }
}

let speaker = Speaker()

emit(["type": "hello", "build": buildStamp,
      "voices": AVSpeechSynthesisVoice.speechVoices().map(describe)])

Thread.detachNewThread {
    while let line = readLine(strippingNewline: true) {
        guard let data = line.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cmd = obj["cmd"] as? String else { continue }
        DispatchQueue.main.async {
            switch cmd {
            case "speak":
                speaker.speak(id: obj["id"] as? String ?? "", text: obj["text"] as? String ?? "",
                              voiceId: obj["voice"] as? String ?? "",
                              rate: Float(obj["rate"] as? Double ?? Double(AVSpeechUtteranceDefaultSpeechRate)),
                              volume: Float(obj["volume"] as? Double ?? 1), interrupt: obj["interrupt"] as? Bool ?? false)
            case "stop":
                speaker.stop()
            case "shutdown":
                speaker.stop()
                outQueue.sync {}
                exit(0)
            default:
                break
            }
        }
    }
    // Standard input closed: the shell is gone, so speech stops too.
    DispatchQueue.main.async { speaker.stop(); outQueue.sync {}; exit(0) }
}

dispatchMain()
