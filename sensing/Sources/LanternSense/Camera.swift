// Lantern sensing layer — AVFoundation capture session + Vision analysis loop.

import Foundation
import AVFoundation
import Vision
import CoreVideo

/// The outcome of analysing one frame. A failed analysis is NOT an empty face
/// list: reporting it as one would tell the core that nobody is there.
enum FrameAnalysis {
    case faces([AnalyzedFace])
    case failed(stage: String, message: String)
}

struct AnalyzedFace {
    let obs: FaceObservation
    let quality: Float?
    let geometry: [Double]?
    let featurePrint: FeaturePrintObservation?
    let featurePrintFailed: Bool
}

enum CamState: String {
    case notStarted = "NOT_STARTED", starting = "STARTING", running = "RUNNING"
    case stopped = "STOPPED", permissionDenied = "PERMISSION_DENIED"
    case noCamera = "NO_CAMERA", disconnected = "DISCONNECTED", error = "ERROR"
}

final class CameraEngine {
    private let session = AVCaptureSession()
    private let slot = FrameSlot()
    /// How the measuring is going, reported on its own cadence by main.swift.
    let monitor = PipelineMonitor()
    private lazy var delegate = CaptureDelegate(slot: slot)
    private let sampleQueue = DispatchQueue(label: "dev.lantern.sense.frames")
    private let sessionQueue = DispatchQueue(label: "dev.lantern.sense.session")
    private var loopTask: Task<Void, Never>?
    private var device: AVCaptureDevice?
    private var observers: [NSObjectProtocol] = []

    private(set) var state: CamState = .notStarted
    private(set) var detail: String?
    private(set) var frameSeq: Int = 0
    private(set) var processedFps: Double = 0

    /// Target analysis rate. Frames arriving faster than this are dropped, never queued.
    var targetFps: Double = 4.0
    /// Padding applied around the face box before computing a FeaturePrint.
    ///
    /// Kept deliberately tight. The FeaturePrint is a general image-similarity
    /// embedding, so every pixel of background it sees becomes part of the
    /// "identity" it encodes. A 0.25 margin was measured drifting 4.5x on the
    /// same face across a lighting change; this trims most of that surface.
    var facePadding: Double = 0.08
    /// Seconds between body/hand pose passes, and between scene passes. Set by
    /// the core; 0 disables the pass.
    var poseInterval: Double = 1.0
    var sceneInterval: Double = 10.0

    var onFaces: (([AnalyzedFace], Int, Double) -> Void)?
    var onPose: ((PoseResult) -> Void)?
    var onScene: ((SceneResult) -> Void)?
    var onAnalysisFailed: ((String, String) -> Void)?
    var onStateChange: (() -> Void)?

    var permissionString: String {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .notDetermined: return "NOT_DETERMINED"
        case .restricted:    return "RESTRICTED"
        case .denied:        return "DENIED"
        case .authorized:    return "AUTHORIZED"
        @unknown default:    return "UNKNOWN"
        }
    }

    var status: CameraStatus {
        CameraStatus(state: state.rawValue, permission: permissionString,
                     deviceName: device?.localizedName, deviceId: device?.uniqueID, detail: detail)
    }

    static func listDevices() -> [[String: String]] {
        AVCaptureDevice.DiscoverySession(
            deviceTypes: [.builtInWideAngleCamera, .external, .continuityCamera],
            mediaType: .video, position: .unspecified
        ).devices.map { ["id": $0.uniqueID, "name": $0.localizedName, "connected": $0.isConnected ? "true" : "false"] }
    }

    private func setState(_ s: CamState, _ d: String? = nil) {
        state = s; detail = d
        onStateChange?()
    }

    // MARK: - Lifecycle

    func start(preferredDeviceId: String?) {
        guard state != .running && state != .starting else { return }
        setState(.starting)

        AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
            guard let self = self else { return }
            guard granted else {
                self.setState(.permissionDenied,
                              "macOS denied camera access. Grant it in System Settings > Privacy & Security > Camera.")
                return
            }
            self.sessionQueue.async { self.configureAndRun(preferredDeviceId) }
        }
    }

    private func configureAndRun(_ preferredDeviceId: String?) {
        let discovered = AVCaptureDevice.DiscoverySession(
            deviceTypes: [.builtInWideAngleCamera, .external, .continuityCamera],
            mediaType: .video, position: .unspecified).devices

        let dev: AVCaptureDevice? = preferredDeviceId.flatMap { id in discovered.first { $0.uniqueID == id } }
            ?? discovered.first { $0.deviceType == .builtInWideAngleCamera }
            ?? discovered.first
            ?? AVCaptureDevice.default(for: .video)

        guard let d = dev else {
            setState(.noCamera, "No video capture device was found on this Mac.")
            return
        }
        device = d

        session.beginConfiguration()
        session.sessionPreset = .medium
        for i in session.inputs { session.removeInput(i) }
        for o in session.outputs { session.removeOutput(o) }
        do {
            let input = try AVCaptureDeviceInput(device: d)
            guard session.canAddInput(input) else {
                session.commitConfiguration()
                setState(.error, "Camera input could not be attached.")
                return
            }
            session.addInput(input)
        } catch {
            session.commitConfiguration()
            setState(.error, "Could not open camera: \(error.localizedDescription)")
            return
        }
        let out = AVCaptureVideoDataOutput()
        out.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        out.alwaysDiscardsLateVideoFrames = true
        out.setSampleBufferDelegate(delegate, queue: sampleQueue)
        guard session.canAddOutput(out) else {
            session.commitConfiguration()
            setState(.error, "Camera output could not be attached.")
            return
        }
        session.addOutput(out)
        session.commitConfiguration()

        installObservers(for: d)
        session.startRunning()
        setState(.running)
        startLoop()
    }

    private func installObservers(for d: AVCaptureDevice) {
        removeObservers()
        let nc = NotificationCenter.default
        observers.append(nc.addObserver(forName: AVCaptureDevice.wasDisconnectedNotification, object: d, queue: nil) { [weak self] _ in
            guard let self = self else { return }
            self.setState(.disconnected, "The camera was disconnected.")
            self.stop(keepState: true)
        })
        observers.append(nc.addObserver(forName: AVCaptureSession.runtimeErrorNotification, object: session, queue: nil) { [weak self] n in
            guard let self = self else { return }
            let e = (n.userInfo?[AVCaptureSessionErrorKey] as? NSError)?.localizedDescription ?? "unknown"
            self.setState(.error, "Capture session error: \(e)")
        })
    }

    private func removeObservers() {
        for o in observers { NotificationCenter.default.removeObserver(o) }
        observers.removeAll()
    }

    /// Full teardown. After this returns the camera indicator light goes out.
    func stop(keepState: Bool = false) {
        loopTask?.cancel(); loopTask = nil
        sessionQueue.async { [weak self] in
            guard let self = self else { return }
            if self.session.isRunning { self.session.stopRunning() }
            self.session.beginConfiguration()
            for i in self.session.inputs { self.session.removeInput(i) }
            for o in self.session.outputs { self.session.removeOutput(o) }
            self.session.commitConfiguration()
            self.removeObservers()
            self.slot.clear()
            self.monitor.sessionEnded()
            self.device = nil
            if !keepState { self.setState(.stopped) } else { self.onStateChange?() }
        }
    }

    // MARK: - Analysis loop

    private func startLoop() {
        loopTask?.cancel()
        loopTask = Task { [weak self] in
            guard let self = self else { return }
            self.slot.monitor = self.monitor
            var lastTick = Date()
            var emaFps = 0.0
            var lastPose = Date.distantPast
            var lastScene = Date.distantPast
            while !Task.isCancelled {
                let t0 = Date()
                // Every pass, frame or no frame: this is what says the loop is
                // alive rather than merely quiet.
                self.monitor.loopTicked()
                if let (pb, seq) = self.slot.take() {
                    self.frameSeq = seq
                    let analyzeStart = Date()
                    self.monitor.analysisBegan()
                    let analysis = await self.analyze(pb)
                    self.monitor.analysisEnded(milliseconds: Date().timeIntervalSince(analyzeStart) * 1000)
                    // Stopped while this frame was being analysed: do not report it.
                    if Task.isCancelled { break }
                    switch analysis {
                    case .faces(let faces):
                        let dt = Date().timeIntervalSince(lastTick)
                        lastTick = Date()
                        if dt > 0 { emaFps = emaFps == 0 ? 1 / dt : (emaFps * 0.8 + (1 / dt) * 0.2) }
                        self.processedFps = emaFps
                        self.onFaces?(faces, seq, emaFps)

                        if self.poseInterval > 0 && Date().timeIntervalSince(lastPose) >= self.poseInterval {
                            lastPose = Date()
                            let pose = await analyzePose(pb)
                            if Task.isCancelled { break }
                            self.onPose?(pose)
                        }
                        if self.sceneInterval > 0 && Date().timeIntervalSince(lastScene) >= self.sceneInterval {
                            lastScene = Date()
                            let scene = await analyzeScene(pb)
                            if Task.isCancelled { break }
                            self.onScene?(scene)
                        }
                    case .failed(let stage, let message):
                        self.onAnalysisFailed?(stage, message)
                    }
                }
                let elapsed = Date().timeIntervalSince(t0)
                let target = 1.0 / max(0.5, self.targetFps)
                if elapsed < target {
                    try? await Task.sleep(nanoseconds: UInt64((target - elapsed) * 1_000_000_000))
                }
            }
        }
    }

    private func analyze(_ pb: CVPixelBuffer) async -> FrameAnalysis {
        let size = CGSize(width: CVPixelBufferGetWidth(pb), height: CVPixelBufferGetHeight(pb))

        var landmarkReq = DetectFaceLandmarksRequest()
        landmarkReq.regionOfInterest = NormalizedRect(x: 0, y: 0, width: 1, height: 1)
        var faces: [FaceObservation] = []
        do {
            faces = try await landmarkReq.perform(on: pb, orientation: .up)
        } catch {
            logErr("face landmark request failed: \(error)")
            return .failed(stage: "face_landmarks", message: String(describing: error))
        }
        guard !faces.isEmpty else { return .faces([]) }

        // Capture quality is best-effort; a failure must not lose the detections.
        var qualities: [UUID: Float] = [:]
        do {
            var qReq = DetectFaceCaptureQualityRequest()
            qReq.inputFaceObservations = faces
            let q = try await qReq.perform(on: pb, orientation: .up)
            for (i, o) in q.enumerated() where i < faces.count {
                if let s = o.captureQuality?.score { qualities[faces[i].uuid] = s }
            }
        } catch {
            // leave qualities empty
        }

        var out: [AnalyzedFace] = []
        for f in faces {
            let geom = geometryDescriptor(f, imageSize: size)
            var fp: FeaturePrintObservation? = nil
            var fpFailed = false
            let r = f.boundingBox.cgRect
            let padX = r.width * facePadding, padY = r.height * facePadding
            let roi = CGRect(x: max(0, r.minX - padX), y: max(0, r.minY - padY),
                             width: min(1, r.width + 2 * padX), height: min(1, r.height + 2 * padY))
            let clamped = CGRect(x: roi.minX, y: roi.minY,
                                 width: min(roi.width, 1 - roi.minX), height: min(roi.height, 1 - roi.minY))
            if clamped.width > 0.02 && clamped.height > 0.02 {
                do {
                    var fpReq = GenerateImageFeaturePrintRequest()
                    fpReq.regionOfInterest = NormalizedRect(normalizedRect: clamped)
                    fpReq.cropAndScaleAction = .scaleToFill
                    fp = try await fpReq.perform(on: pb, orientation: .up)
                } catch {
                    fpFailed = true
                }
            } else {
                fpFailed = true
            }
            out.append(AnalyzedFace(obs: f, quality: qualities[f.uuid], geometry: geom,
                                    featurePrint: fp, featurePrintFailed: fpFailed))
        }
        return .faces(out)
    }
}
