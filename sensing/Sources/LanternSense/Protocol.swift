// Lantern sensing layer — wire protocol.
//
// The sensing layer speaks newline-delimited JSON:
//   stdin  <- commands from the Rust core
//   stdout -> observation messages to the Rust core
//   stderr -> human-readable diagnostics only
//
// Design rule: this layer reports OBSERVATIONS and MEASUREMENTS only.
// It never emits a conclusion (e.g. "this is the owner", "the user is working").
// Classification against thresholds happens in the Rust core.

import Foundation

let kProtocolVersion = 1

/// File descriptor carrying the JSON protocol stream.
///
/// AVFoundation and Vision occasionally write diagnostics straight to fd 1
/// (observed: "VTEST: error: ..."), which would corrupt a JSON-lines protocol.
/// main.swift takes a private duplicate of the real stdout into this variable
/// and then points fd 1 at stderr, so framework noise can never reach the core.
var gProtocolFd: Int32 = 1

// MARK: - Outbound

struct BBox: Codable {
    let x: Double, y: Double, w: Double, h: Double
    /// Normalized to the frame, origin at the TOP-LEFT (converted from Vision's lower-left).
    let origin = "top-left"
    enum CodingKeys: String, CodingKey { case x, y, w, h, origin }
}

/// Per-face measurements. Every field is either measured or explicitly null.
struct FaceMeasurement: Codable {
    let trackId: String
    let framesTracked: Int
    let trackAgeSeconds: Double
    let detectionConfidence: Float
    let boundingBox: BBox
    let rollDeg: Double?
    let yawDeg: Double?
    let pitchDeg: Double?
    /// Vision's own face capture quality, nil when unavailable.
    let captureQuality: Float?
    let landmarksAvailable: Bool
    /// Distance to the closest enrolled geometry descriptor. nil when not computable.
    let geometryDistance: Double?
    /// Vision FeaturePrint distance to the closest enrolled print. nil when not computable.
    let featurePrintDistance: Double?
    /// OK | NO_ENROLLMENT | LANDMARKS_UNAVAILABLE | FEATUREPRINT_FAILED | INCOMPATIBLE_REVISION
    let descriptorStatus: String
}

struct CameraStatus: Codable {
    /// NOT_STARTED | STARTING | RUNNING | STOPPED | PERMISSION_DENIED | NO_CAMERA | DISCONNECTED | ERROR
    let state: String
    /// NOT_DETERMINED | RESTRICTED | DENIED | AUTHORIZED
    let permission: String
    let deviceName: String?
    let deviceId: String?
    let detail: String?
}

struct EnrollmentStats: Codable {
    let sampleCount: Int
    let createdAt: Double?
    let featurePrintRevision: Int?
    /// Leave-one-out intra-enrollment distances. These are MEASURED from the user's own
    /// samples and are what the core uses to calibrate thresholds. nil when < 2 samples.
    let geometrySelfMean: Double?
    let geometrySelfMax: Double?
    let geometrySelfP95: Double?
    let featurePrintSelfMean: Double?
    let featurePrintSelfMax: Double?
    let featurePrintSelfP95: Double?
    /// Spread of head pose across enrolled samples — wider is better for robustness.
    let yawSpreadDeg: Double?
    let pitchSpreadDeg: Double?
}

struct FrontmostApp: Codable {
    let name: String?
    let bundleId: String?
}

enum Outbound {
    case hello(protocolVersion: Int, pid: Int32, build: String, supportsFeaturePrint: Bool)
    case status(camera: CameraStatus, sensingActive: Bool, computerSamplingActive: Bool, enrollment: EnrollmentStats)
    case perception(ts: Double, faceCount: Int, faces: [FaceMeasurement], frameSeq: Int, processedFps: Double)
    case computer(ts: Double, frontmost: FrontmostApp, idleSeconds: Double)
    case enrollCaptured(accepted: Bool, reason: String, stats: EnrollmentStats)
    case devices([[String: String]])
    case probeCaptured(accepted: Bool, reason: String, probeCount: Int)
    case separationReport(SeparationReport)
    case analysisFailed(ts: Double, stage: String, message: String)
    case pose(ts: Double, bodies: [BodyMeasurement], hands: [HandMeasurement], brightness: Double?, error: String?)
    case scene(ts: Double, labels: [SceneLabel], animals: [AnimalMeasurement], error: String?)
    case health(ts: Double, cpuSeconds: Double, footprintBytes: UInt64?, thermalState: String,
                lowPowerMode: Bool, batteryPercent: Double?, powerSource: String?)
    /// The perception pipeline's heartbeat: how the measuring is going, sent on
    /// a fixed cadence whether or not a frame was produced. Aggregates only —
    /// times, counts and durations, never an image, a crop or a descriptor.
    case senseHealth(ts: Double, captureRunning: Bool, snapshot: PipelineMonitor.Snapshot)
    case voice(ts: Double, state: String, session: Int, levelDb: Double?, voiceActive: Bool, detail: String?)
    case transcript(session: Int, text: String, isFinal: Bool)
    /// The wake boundary's only output. `heard` carries what was said after the
    /// invocation, and ONLY when the invocation was heard.
    case wake(ts: Double, state: String, phrase: String, confidence: Double?, heard: String?, detail: String?)
    case error(code: String, message: String)
    case pong(id: Int)
}

// MARK: - JSON emission

final class Emitter {
    private let q = DispatchQueue(label: "dev.lantern.sense.out")
    private let enc: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.withoutEscapingSlashes]
        return e
    }()

    private func write(_ dict: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: dict, options: [.withoutEscapingSlashes]) else { return }
        q.async {
            var out = data
            out.append(0x0A)
            out.withUnsafeBytes { raw in
                guard var p = raw.baseAddress else { return }
                var n = raw.count
                while n > 0 {
                    let w = Darwin.write(gProtocolFd, p, n)
                    if w <= 0 { break }
                    p = p.advanced(by: w)
                    n -= w
                }
            }
        }
    }

    private func encodeToAny<T: Encodable>(_ v: T) -> Any? {
        guard let d = try? enc.encode(v) else { return nil }
        return try? JSONSerialization.jsonObject(with: d)
    }

    func emit(_ m: Outbound) {
        var d: [String: Any] = ["v": kProtocolVersion]
        switch m {
        case let .hello(pv, pid, build, sfp):
            d["type"] = "hello"; d["protocolVersion"] = pv; d["pid"] = Int(pid)
            d["build"] = build; d["supportsFeaturePrint"] = sfp
        case let .status(cam, active, sampling, enr):
            d["type"] = "status"
            d["camera"] = encodeToAny(cam) ?? [:]
            d["sensingActive"] = active
            d["computerSamplingActive"] = sampling
            d["enrollment"] = encodeToAny(enr) ?? [:]
            d["microphonePermission"] = VoiceEngine.permission()
        case let .perception(ts, n, faces, seq, fps):
            d["type"] = "perception"; d["ts"] = ts; d["faceCount"] = n
            d["faces"] = encodeToAny(faces) ?? []
            d["frameSeq"] = seq; d["processedFps"] = fps
        case let .computer(ts, app, idle):
            d["type"] = "computer"; d["ts"] = ts
            d["frontmostApp"] = encodeToAny(app) ?? [:]
            d["idleSeconds"] = idle
        case let .enrollCaptured(ok, reason, stats):
            d["type"] = "enrollCaptured"; d["accepted"] = ok; d["reason"] = reason
            d["enrollment"] = encodeToAny(stats) ?? [:]
        case let .devices(list):
            d["type"] = "devices"; d["devices"] = list
        case let .probeCaptured(ok, reason, n):
            d["type"] = "probeCaptured"; d["accepted"] = ok; d["reason"] = reason; d["probeCount"] = n
        case let .separationReport(r):
            d["type"] = "separationReport"; d["report"] = encodeToAny(r) ?? [:]
        case let .health(ts, cpu, footprint, thermal, lowPower, battery, source):
            d["type"] = "health"; d["ts"] = ts; d["cpuSeconds"] = cpu
            d["thermalState"] = thermal; d["lowPowerMode"] = lowPower
            if let f = footprint { d["footprintBytes"] = f }
            if let b = battery { d["batteryPercent"] = b }
            if let s = source { d["powerSource"] = s }
        case let .senseHealth(ts, running, h):
            d["type"] = "senseHealth"; d["ts"] = ts
            d["captureRunning"] = running
            d["visionBusy"] = h.visionBusy
            d["loopAlive"] = h.loopAlive
            if let c = h.lastCaptureAt { d["lastCaptureAt"] = c }
            if let a = h.lastAnalyzedAt { d["lastAnalyzedAt"] = a }
            d["analyzeMsLast"] = h.analyzeMsLast
            d["analyzeMsP50"] = h.analyzeMsP50
            d["analyzeMsMax"] = h.analyzeMsMax
            d["captureGapMsMax"] = h.captureGapMsMax
            d["framesCaptured"] = h.framesCaptured
            d["framesAnalyzed"] = h.framesAnalyzed
            d["framesDropped"] = h.framesDropped
        case let .pose(ts, bodies, hands, brightness, err):
            d["type"] = "pose"; d["ts"] = ts
            d["bodies"] = encodeToAny(bodies) ?? []
            d["hands"] = encodeToAny(hands) ?? []
            if let b = brightness { d["brightness"] = b }
            if let e = err { d["error"] = e }
        case let .scene(ts, labels, animals, err):
            d["type"] = "scene"; d["ts"] = ts
            d["labels"] = encodeToAny(labels) ?? []
            d["animals"] = encodeToAny(animals) ?? []
            if let e = err { d["error"] = e }
        case let .analysisFailed(ts, stage, msg):
            d["type"] = "analysisFailed"; d["ts"] = ts; d["stage"] = stage; d["message"] = msg
        case let .voice(ts, state, session, level, active, detail):
            d["type"] = "voice"; d["ts"] = ts; d["state"] = state; d["session"] = session
            d["voiceActive"] = active; d["microphonePermission"] = VoiceEngine.permission()
            if let l = level { d["levelDb"] = l }
            if let x = detail { d["detail"] = x }
        case let .transcript(session, text, isFinal):
            d["type"] = "transcript"; d["session"] = session; d["text"] = text; d["isFinal"] = isFinal
        case let .wake(ts, state, phrase, confidence, heard, detail):
            d["type"] = "wake"; d["ts"] = ts; d["state"] = state; d["phrase"] = phrase
            d["microphonePermission"] = VoiceEngine.permission()
            if let c = confidence { d["confidence"] = c }
            if let h = heard { d["heard"] = h }
            if let x = detail { d["detail"] = x }
        case let .error(code, msg):
            d["type"] = "error"; d["code"] = code; d["message"] = msg
        case let .pong(id):
            d["type"] = "pong"; d["id"] = id
        }
        write(d)
    }
}

func logErr(_ s: String) {
    FileHandle.standardError.write(("[sense] " + s + "\n").data(using: .utf8)!)
}

func nowTs() -> Double { Date().timeIntervalSince1970 }
