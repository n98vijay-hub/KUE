// Lantern sensing layer — daemon entry point.
//
// Owns: camera, Vision perception, face enrollment, computer-activity sampling.
// Speaks newline-delimited JSON on stdin/stdout (see Protocol.swift).
//
// This process holds the ONLY camera handle in the system. The UI cannot reach
// the camera; it can only ask the Rust core, which asks this process.

import Foundation
import AppKit
import CoreGraphics

let buildStamp = "lantern-sense/0.1.0"

// MARK: - Argument parsing

var args = Array(CommandLine.arguments.dropFirst())
func flagValue(_ name: String) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    return args[i + 1]
}
let selfTest = args.contains("--selftest")
// Measuring the wake boundary without a microphone: runs one line of text
// through the same matcher the live path uses, and prints the decision.
if let heard = flagValue("--wake-check") {
    let phrase = WakePhrase.parse(flagValue("--wake-phrase") ?? "hello kue")
    let d = WakeMatcher.check(heard, phrase: phrase)
    let out: [String: Any] = ["woke": d.woke, "confidence": d.confidence, "rest": d.rest, "why": d.why]
    if let j = try? JSONSerialization.data(withJSONObject: out), let s = String(data: j, encoding: .utf8) { print(s) }
    exit(0)
}

// The wake gate's own properties, with synthetic audio: pre-roll length, no
// silence reaching the recogniser, and the end of an utterance reported once.
if args.contains("--wake-gate-check") {
    let failures = WakeGate.check() + WakeUtterance.check()
    if let j = try? JSONSerialization.data(withJSONObject: ["ok": failures.isEmpty, "failures": failures]),
       let s = String(data: j, encoding: .utf8) { print(s) }
    exit(failures.isEmpty ? 0 : 1)
}

// The wake boundary end to end, without a microphone: an audio file goes
// through the same transcriber and the same matcher the live path uses.
if let audio = flagValue("--wake-file") {
    let phrase = flagValue("--wake-phrase") ?? "computer"
    Task {
        let r = await WakeEngine.wakeFile(audio, phrase: phrase)
        var out: [String: Any] = ["woke": r.decision.woke, "confidence": r.decision.confidence,
                                  "rest": r.decision.rest, "why": r.decision.why, "phrase": phrase]
        if let e = r.error { out["error"] = e }
        if let j = try? JSONSerialization.data(withJSONObject: out), let s = String(data: j, encoding: .utf8) { print(s) }
        exit(r.error == nil ? 0 : 1)
    }
    RunLoop.main.run()
}

// The LIVE listener's path from a file: same gate, same recogniser options,
// same decision on every result, audio at microphone pace, no end of input.
// Measurement only — prints what the recogniser returned for the clip.
if let audio = flagValue("--wake-stream") {
    let phrase = flagValue("--wake-phrase") ?? "computer"
    Task {
        let out = await WakeEngine.wakeStream(audio, phrase: phrase, realTime: !args.contains("--fast"),
                                              stopAtWake: !args.contains("--whole-clip"),
                                              leadingQuiet: Double(flagValue("--lead") ?? "") ?? 1.0)
        if let j = try? JSONSerialization.data(withJSONObject: out), let s = String(data: j, encoding: .utf8) { print(s) }
        exit(out["error"] == nil ? 0 : 1)
    }
    RunLoop.main.run()
}

if let audio = flagValue("--transcribe-file") {
    // Speech recognition self-test from a file you supply: no microphone, no
    // camera. One JSON line on stdout; the transcript never goes to stderr.
    let volatile = args.contains("--volatile")
    // Measuring only: biases the recogniser toward phrases, to find out whether
    // an invocation word can be heard reliably at all.
    let bias = (flagValue("--bias") ?? "").split(separator: ",").map(String.init).filter { !$0.isEmpty }
    Task {
        let r = await VoiceEngine.transcribeFile(audio, volatile: volatile, bias: bias)
        var out: [String: Any] = ["volatile": volatile, "finalSegments": r.accumulator.finalSegments,
                                  "volatileResults": r.accumulator.volatileResults,
                                  "transcript": r.accumulator.finalized]
        if let e = r.error { out["error"] = e }
        if let d = try? JSONSerialization.data(withJSONObject: out), let s = String(data: d, encoding: .utf8) { print(s) }
        exit(r.error == nil ? 0 : 1)
    }
    RunLoop.main.run()
}
let enrollCount = Int(flagValue("--enroll") ?? "") ?? 0
let resetEnroll = args.contains("--reset-enrollment")
let selfTestDuration = Double(flagValue("--duration") ?? "") ?? 8.0
let redirectOut = flagValue("--out")

// --selftest writes the same JSON stream to a file so the sensing layer can be
// exercised standalone via `open LanternSense.app --args --selftest ...`,
// where stdout is not connected to a terminal.
if let path = redirectOut {
    FileManager.default.createFile(atPath: path, contents: nil)
    if let fh = FileHandle(forWritingAtPath: path) {
        dup2(fh.fileDescriptor, FileHandle.standardOutput.fileDescriptor)
    }
}

// Claim a private copy of the real stdout for the protocol, then redirect fd 1 to
// stderr. After this point nothing except Emitter can write to the core's channel.
gProtocolFd = dup(1)
dup2(2, 1)

// MARK: - Support paths

func supportDirectory() -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
    let dir = base.appendingPathComponent("Lantern", isDirectory: true)
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}

// MARK: - Computer activity (no content, ever)

enum ComputerActivity {
    /// Seconds since the last HID input event of ANY kind.
    /// This is a single scalar from the window server. It is structurally
    /// incapable of revealing which keys were pressed or what was typed.
    static func idleSeconds() -> Double {
        if let any = CGEventType(rawValue: ~0) {
            return CGEventSource.secondsSinceLastEventType(.hidSystemState, eventType: any)
        }
        let types: [CGEventType] = [.mouseMoved, .keyDown, .leftMouseDown, .rightMouseDown,
                                    .scrollWheel, .flagsChanged]
        return types.map { CGEventSource.secondsSinceLastEventType(.hidSystemState, eventType: $0) }.min() ?? -1
    }

    /// Application-level identity only. No window titles, no document names,
    /// no URLs — those would leak content, so they are deliberately not read.
    static func frontmost() -> FrontmostApp {
        let a = NSWorkspace.shared.frontmostApplication
        return FrontmostApp(name: a?.localizedName, bundleId: a?.bundleIdentifier)
    }
}

// MARK: - Daemon

final class Daemon {
    let emitter = Emitter()
    let enrollment = EnrollmentStore(directory: supportDirectory())
    let camera = CameraEngine()
    lazy var voice = VoiceEngine(emitter: emitter)
    /// Waiting to hear the invocation. It reports the wake and then lets go of
    /// the microphone; the core decides what happens next, the same way it does
    /// for a request that was typed.
    lazy var wake = WakeEngine(emitter: emitter, onWake: { _, _ in })
    let probes = ProbeStore(directory: supportDirectory())
    let tracker = FaceTracker()
    private let q = DispatchQueue(label: "dev.lantern.sense.daemon")

    private var pendingEnroll = false
    private var pendingProbe = false
    /// PAUSE: no camera frame and no computer-activity sample is taken while set.
    /// Written through `q`, the queue the sampling timer runs on, so a tick can
    /// never race a pause.
    private var paused = false
    /// Whether the core has asked for the camera. `resume` restores exactly what
    /// was running before the pause and never starts a camera nobody asked for.
    private var cameraWanted = false
    private var minEnrollQuality: Float = 0.25
    private var preferredDeviceId: String? = nil
    private var computerTimer: DispatchSourceTimer?
    private var healthTimer: DispatchSourceTimer?
    private var pipelineTimer: DispatchSourceTimer?

    /// Applies analysis rates carried on a command. Rate policy lives in the core.
    private func applyRates(_ obj: [String: Any]) {
        if let fps = obj["targetFps"] as? Double { camera.targetFps = fps }
        if let s = obj["poseInterval"] as? Double { camera.poseInterval = s }
        if let s = obj["sceneInterval"] as? Double { camera.sceneInterval = s }
    }

    init() {
        camera.onStateChange = { [weak self] in self?.emitStatus() }
        camera.onFaces = { [weak self] faces, seq, fps in self?.handleFaces(faces, seq, fps) }
        camera.onPose = { [weak self] r in
            guard let self = self, !self.q.sync(execute: { self.paused }) else { return }
            self.emitter.emit(.pose(ts: nowTs(), bodies: r.bodies, hands: r.hands, brightness: r.brightness, error: r.error))
        }
        camera.onScene = { [weak self] r in
            guard let self = self, !self.q.sync(execute: { self.paused }) else { return }
            self.emitter.emit(.scene(ts: nowTs(), labels: r.labels, animals: r.animals, error: r.error))
        }
        camera.onAnalysisFailed = { [weak self] stage, message in
            guard let self = self, !self.q.sync(execute: { self.paused }) else { return }
            self.emitter.emit(.analysisFailed(ts: nowTs(), stage: stage, message: message))
        }
    }

    func begin() {
        emitter.emit(.hello(protocolVersion: kProtocolVersion, pid: ProcessInfo.processInfo.processIdentifier,
                            build: buildStamp, supportsFeaturePrint: true))
        startHealthTimer()
        startPipelineTimer()
        startComputerTimer()
        emitStatus()
        emitReport()
    }

    /// The identity-check report compares probes against the CURRENT enrollment,
    /// so it is re-emitted whenever either side changes — otherwise the interface
    /// would show a verdict about a profile that no longer exists.
    func emitReport() {
        emitter.emit(.separationReport(probes.report(against: enrollment)))
    }

    func emitStatus() {
        let sampling = q.sync { computerTimer != nil }
        emitter.emit(.status(camera: camera.status, sensingActive: camera.state == .running,
                             computerSamplingActive: sampling, enrollment: enrollment.stats()))
    }

    private func startComputerTimer() {
        q.sync {
            guard computerTimer == nil else { return }
            let t = DispatchSource.makeTimerSource(queue: q)
            t.schedule(deadline: .now(), repeating: 1.0)
            t.setEventHandler { [weak self] in
                guard let self = self, !self.paused else { return }
                self.emitter.emit(.computer(ts: nowTs(), frontmost: ComputerActivity.frontmost(),
                                            idleSeconds: ComputerActivity.idleSeconds()))
            }
            t.resume()
            computerTimer = t
        }
    }

    /// Self-measurement every 5 seconds. Deliberately NOT stopped by pause: it
    /// reads this process and the machine, not the person, and it is how the
    /// cost of pause itself is measured.
    /// The perception pipeline's heartbeat, once a second.
    ///
    /// Deliberately NOT stopped by pause, and deliberately sent even when no
    /// frame was produced: a pipeline that is alive but late is indistinguishable
    /// from a dead one unless something keeps saying "still here". That
    /// distinction is the whole point (see PipelineMonitor.swift). It carries
    /// counts and durations only.
    private func startPipelineTimer() {
        let t = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "dev.lantern.sense.pipeline"))
        t.schedule(deadline: .now() + 0.5, repeating: 1.0)
        t.setEventHandler { [weak self] in
            guard let self = self else { return }
            self.emitter.emit(.senseHealth(ts: nowTs(),
                                           captureRunning: self.camera.state == .running,
                                           snapshot: self.camera.monitor.snapshot()))
        }
        t.resume()
        pipelineTimer = t
    }

    private func startHealthTimer() {
        let t = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "dev.lantern.sense.health"))
        t.schedule(deadline: .now() + 1.0, repeating: 5.0)
        t.setEventHandler { [weak self] in
            let power = Health.power()
            self?.emitter.emit(.health(ts: nowTs(), cpuSeconds: Health.cpuSeconds(),
                                       footprintBytes: Health.footprintBytes(),
                                       thermalState: Health.thermalState(),
                                       lowPowerMode: Health.lowPowerMode(),
                                       batteryPercent: power.percent, powerSource: power.source))
        }
        t.resume()
        healthTimer = t
    }

    private func stopComputerTimer() {
        q.sync {
            computerTimer?.cancel()
            computerTimer = nil
        }
    }

    // MARK: Face handling

    private func handleFaces(_ faces: [AnalyzedFace], _ seq: Int, _ fps: Double) {
        // A frame whose analysis was already in flight when pause arrived is
        // dropped here, not emitted.
        guard !q.sync(execute: { paused }) else { return }
        let ts = nowTs()
        let boxes = faces.map { $0.obs.boundingBox.cgRect }
        let tracks = tracker.update(boxes, ts: ts)

        // Enrollment and probe capture consume a frame before measurements go out.
        if pendingEnroll { tryEnroll(faces, ts: ts) }
        if pendingProbe { tryProbe(faces, ts: ts) }

        var msgs: [FaceMeasurement] = []
        for (i, f) in faces.enumerated() {
            let t = i < tracks.count ? tracks[i] : nil
            var geoDist: Double? = nil
            var fpDist: Double? = nil
            var status = "OK"

            if !enrollment.hasEnrollment {
                status = "NO_ENROLLMENT"
            } else {
                if let g = f.geometry { geoDist = enrollment.minGeometryDistance(g) }
                else { status = "LANDMARKS_UNAVAILABLE" }
                if let p = f.featurePrint {
                    let (d, s) = enrollment.minFeaturePrintDistance(p)
                    fpDist = d
                    if s != "OK" && status == "OK" { status = s }
                } else if f.featurePrintFailed && status == "OK" {
                    status = "FEATUREPRINT_FAILED"
                }
            }

            // Vision's normalized rect is lower-left origin; convert to top-left for the UI.
            let r = f.obs.boundingBox.cgRect
            let box = BBox(x: Double(r.minX), y: Double(1 - r.maxY), w: Double(r.width), h: Double(r.height))

            msgs.append(FaceMeasurement(
                trackId: t?.id ?? "untracked",
                framesTracked: t?.frames ?? 0,
                trackAgeSeconds: t.map { ts - $0.firstSeen } ?? 0,
                detectionConfidence: f.obs.confidence,
                boundingBox: box,
                rollDeg: f.obs.roll.converted(to: .degrees).value,
                yawDeg: f.obs.yaw.converted(to: .degrees).value,
                pitchDeg: f.obs.pitch.converted(to: .degrees).value,
                captureQuality: f.quality,
                landmarksAvailable: f.geometry != nil,
                geometryDistance: geoDist,
                featurePrintDistance: fpDist,
                descriptorStatus: status))
        }
        emitter.emit(.perception(ts: ts, faceCount: faces.count, faces: msgs, frameSeq: seq, processedFps: fps))
    }

    private func tryEnroll(_ faces: [AnalyzedFace], ts: Double) {
        func finish(_ ok: Bool, _ reason: String) {
            pendingEnroll = false
            emitter.emit(.enrollCaptured(accepted: ok, reason: reason, stats: enrollment.stats()))
            if ok { emitReport() }
        }
        guard faces.count != 0 else { return finish(false, "NO_FACE") }
        guard faces.count == 1 else { return finish(false, "MULTIPLE_FACES") }
        let f = faces[0]
        guard let geom = f.geometry else { return finish(false, "LANDMARKS_UNAVAILABLE") }
        if let q = f.quality, q < minEnrollQuality { return finish(false, "LOW_CAPTURE_QUALITY") }

        var fpData: Data? = nil
        if let p = f.featurePrint { fpData = try? JSONEncoder().encode(p) }

        let sample = EnrollSample(
            ts: ts, geometry: geom, featurePrint: fpData,
            featurePrintRevision: fpData != nil ? 1 : nil,
            captureQuality: f.quality,
            yawDeg: f.obs.yaw.converted(to: .degrees).value,
            pitchDeg: f.obs.pitch.converted(to: .degrees).value,
            rollDeg: f.obs.roll.converted(to: .degrees).value)
        enrollment.add(sample)
        finish(true, fpData == nil ? "STORED_GEOMETRY_ONLY" : "STORED")
    }

    /// Captures a sample of a face that is NOT the owner, into a separate store.
    /// Never touches enrollment — that separation is the whole point.
    private func tryProbe(_ faces: [AnalyzedFace], ts: Double) {
        func finish(_ ok: Bool, _ reason: String) {
            pendingProbe = false
            emitter.emit(.probeCaptured(accepted: ok, reason: reason, probeCount: probes.sampleCount))
            if ok { emitReport() }
        }
        guard faces.count != 0 else { return finish(false, "NO_FACE") }
        guard faces.count == 1 else { return finish(false, "MULTIPLE_FACES") }
        let f = faces[0]
        guard let geom = f.geometry else { return finish(false, "LANDMARKS_UNAVAILABLE") }
        if let q = f.quality, q < minEnrollQuality { return finish(false, "LOW_CAPTURE_QUALITY") }
        var fpData: Data? = nil
        if let p = f.featurePrint { fpData = try? JSONEncoder().encode(p) }
        probes.add(EnrollSample(
            ts: ts, geometry: geom, featurePrint: fpData,
            featurePrintRevision: fpData != nil ? 1 : nil, captureQuality: f.quality,
            yawDeg: f.obs.yaw.converted(to: .degrees).value,
            pitchDeg: f.obs.pitch.converted(to: .degrees).value,
            rollDeg: f.obs.roll.converted(to: .degrees).value))
        finish(true, "STORED")
    }

    // MARK: Commands

    func handle(_ line: String) {
        guard let data = line.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cmd = obj["cmd"] as? String else {
            emitter.emit(.error(code: "BAD_COMMAND", message: "could not parse command line"))
            return
        }
        switch cmd {
        case "start":
            preferredDeviceId = obj["deviceId"] as? String ?? preferredDeviceId
            applyRates(obj)
            cameraWanted = true
            tracker.reset()
            camera.start(preferredDeviceId: preferredDeviceId)
        case "stop":
            cameraWanted = false
            camera.stop()
            tracker.reset()
        case "pause":
            // Everything stops: the capture session is torn down, computer-activity
            // sampling is cancelled, and pending captures are abandoned.
            q.sync { paused = true }
            voice.cancel()
            wake.stop(reason: "PAUSED")
            pendingEnroll = false
            pendingProbe = false
            stopComputerTimer()
            camera.stop()
            tracker.reset()
            emitStatus()
        case "resume":
            q.sync { paused = false }
            applyRates(obj)
            startComputerTimer()
            if cameraWanted {
                tracker.reset()
                camera.start(preferredDeviceId: preferredDeviceId)
            }
            emitStatus()
        case "set_fps":
            // Rate policy is decided in the core; this only applies it.
            applyRates(obj)
        case "status":
            emitStatus()
            emitReport()
        case "devices":
            emitter.emit(.devices(CameraEngine.listDevices()))
        case "enroll_capture":
            guard camera.state == .running else {
                emitter.emit(.enrollCaptured(accepted: false, reason: "CAMERA_NOT_RUNNING", stats: enrollment.stats()))
                return
            }
            pendingEnroll = true
        case "enroll_undo":
            let n = obj["count"] as? Int ?? 1
            let removed = enrollment.removeLast(n)
            emitter.emit(.enrollCaptured(accepted: false, reason: "REMOVED_\(removed)", stats: enrollment.stats()))
            emitReport()
        case "probe_capture":
            guard camera.state == .running else {
                emitter.emit(.probeCaptured(accepted: false, reason: "CAMERA_NOT_RUNNING", probeCount: probes.sampleCount))
                return
            }
            pendingProbe = true
        case "probe_reset":
            probes.reset()
            emitter.emit(.probeCaptured(accepted: false, reason: "RESET", probeCount: 0))
            emitReport()
        case "probe_report":
            emitReport()
        case "enroll_reset":
            enrollment.reset()
            emitter.emit(.enrollCaptured(accepted: false, reason: "RESET", stats: enrollment.stats()))
            emitReport()
        case "wake_start":
            if q.sync(execute: { paused }) {
                emitter.emit(.wake(ts: nowTs(), state: "OFF", phrase: "", confidence: nil, heard: nil, detail: "PAUSED"))
                return
            }
            wake.start(phrase: obj["phrase"] as? String ?? "computer")
        case "wake_stop":
            wake.stop(reason: obj["reason"] as? String ?? "STOPPED")
        case "listen_start":
            // One microphone: the ordinary listening session takes it from the
            // wake listener, which the core restarts afterwards.
            wake.stop(reason: "LISTENING")
            if q.sync(execute: { paused }) {
                emitter.emit(.voice(ts: nowTs(), state: "IDLE", session: 0, levelDb: nil, voiceActive: false, detail: "PAUSED"))
                return
            }
            voice.start(maxSeconds: obj["maxSeconds"] as? Double ?? 30, silenceSeconds: obj["silenceSeconds"] as? Double ?? 1.8)
        case "listen_stop":
            voice.finish(reason: "STOPPED")
        case "listen_cancel":
            voice.cancel()
        case "ping":
            emitter.emit(.pong(id: obj["id"] as? Int ?? 0))
        case "shutdown":
            voice.cancel()
            wake.stop(reason: "SHUTDOWN")
            camera.stop()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { exit(0) }
        default:
            emitter.emit(.error(code: "UNKNOWN_COMMAND", message: cmd))
        }
    }
}

// MARK: - Boot

let daemon = Daemon()
daemon.begin()

if selfTest {
    // Standalone exercise: start the camera, run for a fixed window, then exit cleanly.
    logErr("selftest: starting camera for \(selfTestDuration)s")
    if resetEnroll { daemon.handle("{\"cmd\":\"enroll_reset\"}") }
    daemon.handle("{\"cmd\":\"devices\"}")
    daemon.handle("{\"cmd\":\"start\"}")
    // Space enrollment captures out so the operator can vary pose between them.
    for i in 0..<enrollCount {
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.0 + Double(i) * 2.0) {
            logErr("selftest: enroll capture \(i + 1)/\(enrollCount)")
            daemon.handle("{\"cmd\":\"enroll_capture\"}")
        }
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + selfTestDuration) {
        daemon.handle("{\"cmd\":\"stop\"}")
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            logErr("selftest: done")
            exit(0)
        }
    }
} else {
    // Read newline-delimited commands from the core. EOF means the parent is gone.
    Thread.detachNewThread {
        var buffer = Data()
        while true {
            let chunk = FileHandle.standardInput.availableData
            if chunk.isEmpty {
                logErr("stdin closed; shutting down")
                daemon.camera.stop()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { exit(0) }
                return
            }
            buffer.append(chunk)
            while let nl = buffer.firstIndex(of: 0x0A) {
                let lineData = buffer[buffer.startIndex..<nl]
                buffer = buffer[buffer.index(after: nl)...]
                if let s = String(data: lineData, encoding: .utf8),
                   !s.trimmingCharacters(in: .whitespaces).isEmpty {
                    DispatchQueue.main.async { daemon.handle(s) }
                }
            }
        }
    }
}

RunLoop.main.run()
