// Lantern sensing layer — camera capture + Apple Vision face perception.
//
// PRIVACY INVARIANTS enforced here:
//  * Frames live only in memory, only for the duration of one analysis pass.
//  * No frame, crop, or encoded image is ever written to disk or to stdout.
//  * stop() fully tears the AVCaptureSession down so the camera indicator light
//    goes out. There is no "paused but still capturing" state.

import Foundation
import AVFoundation
import Vision
import CoreImage
import CoreVideo

// MARK: - Face tracking (frame-to-frame continuity)

final class TrackedFace {
    let id: String
    var bbox: CGRect            // normalized, lower-left origin (Vision space)
    var frames: Int
    var firstSeen: Double
    var lastSeen: Double
    init(bbox: CGRect, ts: Double) {
        self.id = UUID().uuidString
        self.bbox = bbox; self.frames = 1; self.firstSeen = ts; self.lastSeen = ts
    }
}

/// Greedy IoU tracker. Gives stable track ids so the core can reason about
/// continuity ("same face for N seconds") rather than per-frame flicker.
final class FaceTracker {
    private var tracks: [TrackedFace] = []
    private let iouThreshold: Double
    private let dropAfter: Double
    init(iouThreshold: Double = 0.3, dropAfter: Double = 1.5) {
        self.iouThreshold = iouThreshold; self.dropAfter = dropAfter
    }

    private func iou(_ a: CGRect, _ b: CGRect) -> Double {
        let inter = a.intersection(b)
        if inter.isNull || inter.isEmpty { return 0 }
        let ia = inter.width * inter.height
        let ua = a.width * a.height + b.width * b.height - ia
        return ua <= 0 ? 0 : Double(ia / ua)
    }

    /// Returns a track for each input bbox, in the same order.
    func update(_ boxes: [CGRect], ts: Double) -> [TrackedFace] {
        tracks.removeAll { ts - $0.lastSeen > dropAfter }
        var result: [TrackedFace] = []
        var claimed = Set<ObjectIdentifier>()
        for b in boxes {
            var best: TrackedFace? = nil
            var bestScore = iouThreshold
            for t in tracks where !claimed.contains(ObjectIdentifier(t)) {
                let s = iou(b, t.bbox)
                if s >= bestScore { bestScore = s; best = t }
            }
            if let t = best {
                t.bbox = b; t.frames += 1; t.lastSeen = ts
                claimed.insert(ObjectIdentifier(t))
                result.append(t)
            } else {
                let t = TrackedFace(bbox: b, ts: ts)
                tracks.append(t); claimed.insert(ObjectIdentifier(t))
                result.append(t)
            }
        }
        return result
    }

    func reset() { tracks.removeAll() }
}

// MARK: - Geometry descriptor

/// Landmark regions used for the geometric descriptor, in a fixed order.
/// Order matters: the descriptor is positional, so it must never be reordered
/// without bumping the enrollment schema version.
enum GeomRegion: Int, CaseIterable {
    case faceContour, leftEye, rightEye, leftEyebrow, rightEyebrow
    case nose, noseCrest, medianLine, outerLips, innerLips
}

func regionPoints(_ lm: FaceObservation.Landmarks2D, _ r: GeomRegion) -> [NormalizedPoint] {
    switch r {
    case .faceContour:  return lm.faceContour.points
    case .leftEye:      return lm.leftEye.points
    case .rightEye:     return lm.rightEye.points
    case .leftEyebrow:  return lm.leftEyebrow.points
    case .rightEyebrow: return lm.rightEyebrow.points
    case .nose:         return lm.nose.points
    case .noseCrest:    return lm.noseCrest.points
    case .medianLine:   return lm.medianLine.points
    case .outerLips:    return lm.outerLips.points
    case .innerLips:    return lm.innerLips.points
    }
}

/// Builds a pose-normalized geometric descriptor from face landmarks.
///
/// Normalization: origin at the pupil midpoint, x-axis along the inter-pupil line
/// (removes in-plane roll), scale = inter-pupillary distance (removes scale/distance).
/// This removes roll and scale but NOT yaw/pitch — out-of-plane rotation still
/// changes the descriptor, which is why the core also weighs head pose.
///
/// Returns nil when the landmarks needed for normalization are unavailable.
func geometryDescriptor(_ obs: FaceObservation, imageSize: CGSize) -> [Double]? {
    guard let lm = obs.landmarks else { return nil }
    let box = obs.boundingBox

    func img(_ p: NormalizedPoint) -> CGPoint {
        p.toImageCoordinates(from: box, imageSize: imageSize, origin: .upperLeft)
    }
    func centroid(_ pts: [NormalizedPoint]) -> CGPoint? {
        guard !pts.isEmpty else { return nil }
        var sx = 0.0, sy = 0.0
        for p in pts { let q = img(p); sx += q.x; sy += q.y }
        return CGPoint(x: sx / Double(pts.count), y: sy / Double(pts.count))
    }

    // Prefer pupils; fall back to eye-region centroids when pupils are missing.
    let lp = centroid(lm.leftPupil.points) ?? centroid(lm.leftEye.points)
    let rp = centroid(lm.rightPupil.points) ?? centroid(lm.rightEye.points)
    guard let L = lp, let R = rp else { return nil }

    let dx = R.x - L.x, dy = R.y - L.y
    let iod = (dx * dx + dy * dy).squareRoot()
    guard iod > 1e-6 else { return nil }

    let ang = atan2(dy, dx)
    let ca = cos(-ang), sa = sin(-ang)
    let ox = (L.x + R.x) / 2, oy = (L.y + R.y) / 2

    func norm(_ p: CGPoint) -> (Double, Double) {
        let tx = (p.x - ox) / iod, ty = (p.y - oy) / iod
        return (tx * ca - ty * sa, tx * sa + ty * ca)
    }

    var desc: [Double] = []
    for r in GeomRegion.allCases {
        guard let c = centroid(regionPoints(lm, r)) else { return nil }
        let (x, y) = norm(c)
        desc.append(x); desc.append(y)
    }

    // Scalar shape ratios (all normalized by inter-pupillary distance).
    func extent(_ r: GeomRegion) -> (Double, Double)? {
        let pts = regionPoints(lm, r).map { img($0) }
        guard !pts.isEmpty else { return nil }
        var nx: [Double] = [], ny: [Double] = []
        for p in pts { let (a, b) = norm(p); nx.append(a); ny.append(b) }
        return ((nx.max()! - nx.min()!), (ny.max()! - ny.min()!))
    }
    for r in [GeomRegion.faceContour, .outerLips, .nose, .leftEye, .rightEye] {
        guard let (w, h) = extent(r) else { return nil }
        desc.append(w); desc.append(h)
    }
    return desc
}

// MARK: - Frame holder (single-slot, latest wins)

final class FrameSlot {
    private let lock = NSLock()
    private var buf: CVPixelBuffer?
    private var seq: Int = 0
    /// Told about every frame and every frame overwritten before analysis.
    /// Counts only — the buffer itself never reaches it.
    weak var monitor: PipelineMonitor?
    func put(_ b: CVPixelBuffer) {
        lock.lock()
        let replaced = buf != nil
        buf = b; seq += 1
        lock.unlock()
        monitor?.captured(replacing: replaced)
    }
    func take() -> (CVPixelBuffer, Int)? {
        lock.lock(); defer { lock.unlock() }
        guard let b = buf else { return nil }
        buf = nil
        return (b, seq)
    }
    func clear() { lock.lock(); buf = nil; lock.unlock() }
}

final class CaptureDelegate: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let slot: FrameSlot
    init(slot: FrameSlot) { self.slot = slot }
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer,
                       from connection: AVCaptureConnection) {
        guard let pb = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        slot.put(pb)
    }
}
