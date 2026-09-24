// Lantern sensing layer — body, hands, scene and light.
//
// Measurements only, like everything in this layer: joint positions with
// Vision's confidence, whole-frame classification labels with Vision's
// confidence, and the mean brightness of the frame. What any of it MEANS
// ("upper body visible", "hand near face") is decided in the core against
// configured thresholds.
//
// These run on a slower cadence than face analysis, set by the core. They are
// the most expensive requests Lantern makes, and a posture or a room does not
// change four times a second.

import Foundation
import Vision
import CoreVideo

struct JointPoint: Codable {
    /// Normalized to the frame, origin at the TOP-LEFT.
    let x: Double
    let y: Double
    let confidence: Float
}

struct BodyMeasurement: Codable {
    let confidence: Float
    /// Keyed by Vision's joint name: nose, neck, leftShoulder, ...
    let joints: [String: JointPoint]
}

struct HandMeasurement: Codable {
    /// left | right | unknown, as Vision reports it.
    let chirality: String
    let confidence: Float
    /// Extent of the joints Vision located with confidence above 0.3.
    let boundingBox: BBox?
    let wrist: JointPoint?
    let jointsLocated: Int
}

struct SceneLabel: Codable {
    let identifier: String
    let confidence: Float
}

struct AnimalMeasurement: Codable {
    let label: String
    let confidence: Float
    let boundingBox: BBox
}

struct PoseResult {
    let bodies: [BodyMeasurement]
    let hands: [HandMeasurement]
    let brightness: Double?
    let error: String?
}

struct SceneResult {
    let labels: [SceneLabel]
    let animals: [AnimalMeasurement]
    let error: String?
}

private func topLeft(_ p: NormalizedPoint, _ c: Float) -> JointPoint {
    JointPoint(x: Double(p.x), y: 1 - Double(p.y), confidence: c)
}

private let bodyJoints: [HumanBodyPoseObservation.JointName] = [
    .nose, .leftEye, .rightEye, .leftEar, .rightEar, .neck,
    .leftShoulder, .rightShoulder, .leftElbow, .rightElbow, .leftWrist, .rightWrist,
    .root, .leftHip, .rightHip,
]

func analyzePose(_ pb: CVPixelBuffer) async -> PoseResult {
    let brightness = meanBrightness(pb)
    var errors: [String] = []

    var bodies: [BodyMeasurement] = []
    do {
        let req = DetectHumanBodyPoseRequest()
        let found = try await req.perform(on: pb, orientation: .up)
        for b in found {
            let all = b.allJoints()
            var joints: [String: JointPoint] = [:]
            for name in bodyJoints {
                if let j = all[name], j.confidence > 0 {
                    joints[String(describing: name)] = topLeft(j.location, j.confidence)
                }
            }
            bodies.append(BodyMeasurement(confidence: b.confidence, joints: joints))
        }
    } catch {
        errors.append("body_pose: \(error)")
    }

    var hands: [HandMeasurement] = []
    do {
        var req = DetectHumanHandPoseRequest()
        req.maximumHandCount = 2
        let found = try await req.perform(on: pb, orientation: .up)
        for h in found {
            let located = h.allJoints().values.filter { $0.confidence > 0.3 }
            var box: BBox? = nil
            if !located.isEmpty {
                let xs = located.map { Double($0.location.x) }
                let ys = located.map { 1 - Double($0.location.y) }
                box = BBox(x: xs.min()!, y: ys.min()!, w: xs.max()! - xs.min()!, h: ys.max()! - ys.min()!)
            }
            let wrist = h.allJoints()[.wrist].flatMap { $0.confidence > 0 ? topLeft($0.location, $0.confidence) : nil }
            hands.append(HandMeasurement(
                chirality: h.chirality.map { String(describing: $0) } ?? "unknown",
                confidence: h.confidence, boundingBox: box, wrist: wrist, jointsLocated: located.count))
        }
    } catch {
        errors.append("hand_pose: \(error)")
    }

    return PoseResult(bodies: bodies, hands: hands, brightness: brightness,
                      error: errors.isEmpty ? nil : errors.joined(separator: "; "))
}

func analyzeScene(_ pb: CVPixelBuffer) async -> SceneResult {
    var errors: [String] = []
    var labels: [SceneLabel] = []
    do {
        let found = try await ClassifyImageRequest().perform(on: pb, orientation: .up)
        // The full taxonomy has over a thousand labels, almost all near zero.
        // Anything below 0.05 is not sent; the core applies its own threshold.
        labels = found.filter { $0.confidence >= 0.05 }
            .sorted { $0.confidence > $1.confidence }
            .prefix(12)
            .map { SceneLabel(identifier: $0.identifier, confidence: $0.confidence) }
    } catch {
        errors.append("classify_image: \(error)")
    }

    var animals: [AnimalMeasurement] = []
    do {
        let found = try await RecognizeAnimalsRequest().perform(on: pb, orientation: .up)
        for a in found {
            guard let top = a.labels.first else { continue }
            let r = a.boundingBox.cgRect
            animals.append(AnimalMeasurement(
                label: top.identifier, confidence: top.confidence,
                boundingBox: BBox(x: Double(r.minX), y: Double(1 - r.maxY), w: Double(r.width), h: Double(r.height))))
        }
    } catch {
        errors.append("recognize_animals: \(error)")
    }
    return SceneResult(labels: labels, animals: animals, error: errors.isEmpty ? nil : errors.joined(separator: "; "))
}

/// Mean luma of the frame, 0 (black) to 1 (white), sampled on a sparse grid.
/// Explains detection failures: Vision misses faces in a dark room.
func meanBrightness(_ pb: CVPixelBuffer) -> Double? {
    guard CVPixelBufferGetPixelFormatType(pb) == kCVPixelFormatType_32BGRA else { return nil }
    CVPixelBufferLockBaseAddress(pb, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(pb, .readOnly) }
    guard let base = CVPixelBufferGetBaseAddress(pb) else { return nil }
    let w = CVPixelBufferGetWidth(pb), h = CVPixelBufferGetHeight(pb)
    let stride = CVPixelBufferGetBytesPerRow(pb)
    let p = base.assumingMemoryBound(to: UInt8.self)
    var sum = 0.0
    var n = 0
    var y = 0
    while y < h {
        var x = 0
        while x < w {
            let o = y * stride + x * 4
            sum += 0.114 * Double(p[o]) + 0.587 * Double(p[o + 1]) + 0.299 * Double(p[o + 2])
            n += 1
            x += 12
        }
        y += 12
    }
    return n > 0 ? sum / Double(n) / 255.0 : nil
}
