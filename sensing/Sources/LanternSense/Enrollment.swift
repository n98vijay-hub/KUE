// Lantern sensing layer — face enrollment store and descriptor distances.
//
// PRIVACY: this file stores DERIVED DESCRIPTORS ONLY. No image, crop, or pixel
// buffer is ever written to disk. A geometry descriptor is a vector of landmark
// ratios; a FeaturePrint is an opaque Vision embedding. Neither can be rendered
// back into a photograph.
//
// HONESTY: neither descriptor is a production face-recognition embedding.
//  - Geometry  : landmark ratios. Pose-sensitive, low discriminative power.
//  - FeaturePrint: Apple's GENERAL IMAGE similarity embedding, not a face-identity
//    model. On a face crop it is materially influenced by lighting, background,
//    glasses and hair. Treat as weak evidence; the core defaults to UNCERTAIN.

import Foundation
import Vision

struct EnrollSample: Codable {
    let ts: Double
    let geometry: [Double]
    let featurePrint: Data?
    let featurePrintRevision: Int?
    let captureQuality: Float?
    let yawDeg: Double
    let pitchDeg: Double
    let rollDeg: Double
}

/// Version of the DESCRIPTOR EXTRACTION pipeline, not of the file format.
///
/// Bump this whenever the geometry descriptor or the FeaturePrint crop changes:
/// samples captured by a different extractor are not comparable, and silently
/// measuring against them would produce confident nonsense.
///   1 — initial: face crop padded by 0.25
///   2 — crop tightened to 0.08 after the padded crop was measured drifting
///       4.5x on the same face across a lighting change
let kDescriptorVersion = 2

struct EnrollFile: Codable {
    var schemaVersion: Int = 1
    var descriptorVersion: Int = kDescriptorVersion
    var createdAt: Double
    var samples: [EnrollSample]
}

/// Distance between two geometry descriptors: mean absolute difference per element.
/// Returns nil if the vectors are not comparable.
func geometryDistance(_ a: [Double], _ b: [Double]) -> Double? {
    guard a.count == b.count, !a.isEmpty else { return nil }
    var acc = 0.0
    for i in 0..<a.count { acc += abs(a[i] - b[i]) }
    return acc / Double(a.count)
}

final class EnrollmentStore {
    private(set) var file: EnrollFile
    private let url: URL
    private let q = DispatchQueue(label: "dev.lantern.sense.enroll")

    /// Cached decoded FeaturePrint observations, rebuilt on load/mutate.
    private var prints: [FeaturePrintObservation] = []

    init(directory: URL) {
        self.url = directory.appendingPathComponent("enrollment.json")
        if let d = try? Data(contentsOf: url), let f = try? JSONDecoder().decode(EnrollFile.self, from: d) {
            if f.descriptorVersion == kDescriptorVersion {
                self.file = f
            } else {
                // Descriptors from an older extractor cannot be compared against
                // current ones. Discard rather than mislead; the UI reports zero
                // samples and asks for re-enrollment.
                logErr("enrollment discarded: built by descriptor v\(f.descriptorVersion), this build is v\(kDescriptorVersion)")
                self.file = EnrollFile(createdAt: nowTs(), samples: [])
            }
        } else {
            self.file = EnrollFile(createdAt: nowTs(), samples: [])
        }
        rebuildPrints()
    }

    private func rebuildPrints() {
        prints = file.samples.compactMap { s in
            guard let d = s.featurePrint else { return nil }
            return try? JSONDecoder().decode(FeaturePrintObservation.self, from: d)
        }
    }

    var sampleCount: Int { file.samples.count }

    /// A consistent copy of the samples, safe to read from another queue.
    func samplesSnapshot() -> [EnrollSample] { q.sync { file.samples } }
    var hasEnrollment: Bool { !file.samples.isEmpty }

    func add(_ s: EnrollSample) {
        q.sync {
            file.samples.append(s)
            persist()
            rebuildPrints()
        }
    }

    /// Removes the most recent `n` samples. Needed because a mis-capture — a
    /// different person's face, a bad frame — otherwise silently becomes part of
    /// the owner's identity with no way back short of starting over.
    @discardableResult
    func removeLast(_ n: Int) -> Int {
        q.sync {
            let k = min(n, file.samples.count)
            file.samples.removeLast(k)
            persist()
            rebuildPrints()
            return k
        }
    }

    func reset() {
        q.sync {
            file = EnrollFile(createdAt: nowTs(), samples: [])
            persist()
            rebuildPrints()
        }
    }

    private func persist() {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                    withIntermediateDirectories: true)
            let d = try JSONEncoder().encode(file)
            try d.write(to: url, options: .atomic)
        } catch {
            logErr("failed to persist enrollment: \(error)")
        }
    }

    /// Minimum geometry distance from `g` to any enrolled sample.
    func minGeometryDistance(_ g: [Double]) -> Double? {
        var best: Double? = nil
        for s in file.samples {
            if let d = geometryDistance(g, s.geometry) {
                if best == nil || d < best! { best = d }
            }
        }
        return best
    }

    /// Minimum Vision FeaturePrint distance from `p` to any enrolled print.
    /// Returns (distance, status).
    func minFeaturePrintDistance(_ p: FeaturePrintObservation) -> (Double?, String) {
        if prints.isEmpty { return (nil, file.samples.isEmpty ? "NO_ENROLLMENT" : "FEATUREPRINT_FAILED") }
        var best: Double? = nil
        var sawIncompatible = false
        for ep in prints {
            do {
                let d = try ep.distance(to: p)
                if best == nil || d < best! { best = d }
            } catch {
                sawIncompatible = true
            }
        }
        if best == nil { return (nil, sawIncompatible ? "INCOMPATIBLE_REVISION" : "FEATUREPRINT_FAILED") }
        return (best, "OK")
    }

    /// Leave-one-out self-distance statistics. This is the honest measurement of how
    /// tightly the user's own enrolled samples cluster — the core uses it to calibrate.
    func stats() -> EnrollmentStats {
        let n = file.samples.count
        guard n >= 2 else {
            return EnrollmentStats(sampleCount: n, createdAt: file.createdAt,
                                   featurePrintRevision: file.samples.first?.featurePrintRevision,
                                   geometrySelfMean: nil, geometrySelfMax: nil, geometrySelfP95: nil,
                                   featurePrintSelfMean: nil, featurePrintSelfMax: nil, featurePrintSelfP95: nil,
                                   yawSpreadDeg: nil, pitchSpreadDeg: nil)
        }
        var geo: [Double] = [], fp: [Double] = []
        for i in 0..<n {
            for j in (i + 1)..<n {
                if let d = geometryDistance(file.samples[i].geometry, file.samples[j].geometry) { geo.append(d) }
            }
        }
        for i in 0..<prints.count {
            for j in (i + 1)..<prints.count {
                if let d = try? prints[i].distance(to: prints[j]) { fp.append(d) }
            }
        }
        func mean(_ a: [Double]) -> Double? { a.isEmpty ? nil : a.reduce(0, +) / Double(a.count) }
        func maxv(_ a: [Double]) -> Double? { a.max() }
        func p95(_ a: [Double]) -> Double? {
            guard !a.isEmpty else { return nil }
            let s = a.sorted()
            let idx = min(s.count - 1, Int((Double(s.count) * 0.95).rounded(.down)))
            return s[idx]
        }
        let yaws = file.samples.map { $0.yawDeg }
        let pitches = file.samples.map { $0.pitchDeg }
        return EnrollmentStats(
            sampleCount: n, createdAt: file.createdAt,
            featurePrintRevision: file.samples.first?.featurePrintRevision,
            geometrySelfMean: mean(geo), geometrySelfMax: maxv(geo), geometrySelfP95: p95(geo),
            featurePrintSelfMean: mean(fp), featurePrintSelfMax: maxv(fp), featurePrintSelfP95: p95(fp),
            yawSpreadDeg: (yaws.max() ?? 0) - (yaws.min() ?? 0),
            pitchSpreadDeg: (pitches.max() ?? 0) - (pitches.min() ?? 0))
    }
}
