// Identity check — measuring the matcher against a face that is NOT the owner.
//
// WHY THIS EXISTS: until this was built, the only face action in Lantern was
// "capture enrollment sample", which always appends to the owner's profile.
// There was no way to test whether the matcher can separate two people without
// corrupting the very profile you were testing — and it happened in practice.
//
// Probes live in their own file and are NEVER used for matching. They exist
// only to answer one question: do the owner's samples and a different person's
// samples actually occupy different regions of descriptor space?
//
// PRIVACY: as with enrollment, descriptors only. No image is ever stored.

import Foundation
import Vision

struct ProbeFile: Codable {
    var schemaVersion: Int = 1
    var descriptorVersion: Int = kDescriptorVersion
    var label: String
    var samples: [EnrollSample]
}

/// Separation report for one descriptor.
struct DescriptorSeparation: Codable {
    let name: String
    let withinOwnerMin: Double?
    let withinOwnerMedian: Double?
    let withinOwnerMax: Double?
    let ownerVsProbeMin: Double?
    let ownerVsProbeMedian: Double?
    let ownerVsProbeMax: Double?
    /// closest between-person distance ÷ widest within-owner distance.
    /// Above 1.0 the two sets are cleanly separated; at or below 1.0 they overlap.
    let separationRatio: Double?
    /// SEPARATED | OVERLAPPING | INSUFFICIENT_DATA
    let verdict: String
}

struct SeparationReport: Codable {
    let ownerSamples: Int
    let probeSamples: Int
    let probeLabel: String
    let geometry: DescriptorSeparation
    let featurePrint: DescriptorSeparation
    /// The honest headline: both descriptors must separate for the reject side
    /// to mean anything, because a claim requires both to agree.
    let rejectSideValidated: Bool
    let note: String
}

private func median(_ a: [Double]) -> Double? {
    guard !a.isEmpty else { return nil }
    let s = a.sorted()
    return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
}

private func separation(_ name: String, within: [Double], between: [Double]) -> DescriptorSeparation {
    guard let wMax = within.max(), let bMin = between.min(), !within.isEmpty, !between.isEmpty else {
        return DescriptorSeparation(name: name,
            withinOwnerMin: within.min(), withinOwnerMedian: median(within), withinOwnerMax: within.max(),
            ownerVsProbeMin: between.min(), ownerVsProbeMedian: median(between), ownerVsProbeMax: between.max(),
            separationRatio: nil, verdict: "INSUFFICIENT_DATA")
    }
    let ratio = wMax > 0 ? bMin / wMax : nil
    return DescriptorSeparation(name: name,
        withinOwnerMin: within.min(), withinOwnerMedian: median(within), withinOwnerMax: wMax,
        ownerVsProbeMin: bMin, ownerVsProbeMedian: median(between), ownerVsProbeMax: between.max(),
        separationRatio: ratio,
        verdict: (ratio ?? 0) > 1.0 ? "SEPARATED" : "OVERLAPPING")
}

final class ProbeStore {
    private(set) var file: ProbeFile
    private let url: URL
    private var prints: [FeaturePrintObservation] = []
    /// Serialises every read and write. Captures arrive from the analysis task
    /// while reports are computed on the command queue.
    private let q = DispatchQueue(label: "dev.lantern.sense.probes")

    init(directory: URL) {
        self.url = directory.appendingPathComponent("identity_probes.json")
        if let d = try? Data(contentsOf: url),
           let f = try? JSONDecoder().decode(ProbeFile.self, from: d),
           f.descriptorVersion == kDescriptorVersion {
            self.file = f
        } else {
            self.file = ProbeFile(label: "another person", samples: [])
        }
        rebuild()
    }

    private func rebuild() {
        prints = file.samples.compactMap { s in
            guard let d = s.featurePrint else { return nil }
            return try? JSONDecoder().decode(FeaturePrintObservation.self, from: d)
        }
    }

    var sampleCount: Int { q.sync { file.samples.count } }

    func add(_ s: EnrollSample) {
        q.sync {
            file.samples.append(s)
            persist()
            rebuild()
        }
    }

    func reset() {
        q.sync {
            file = ProbeFile(label: file.label, samples: [])
            persist()
            rebuild()
        }
    }

    private func persist() {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                    withIntermediateDirectories: true)
            try JSONEncoder().encode(file).write(to: url, options: .atomic)
        } catch {
            logErr("failed to persist probes: \(error)")
        }
    }

    /// Compares the owner's enrolled samples against the probe samples.
    func report(against enrollment: EnrollmentStore) -> SeparationReport {
        let owner = enrollment.samplesSnapshot()
        let (probe, prints) = q.sync { (file.samples, self.prints) }

        var geoWithin: [Double] = [], geoBetween: [Double] = []
        for i in 0..<owner.count {
            for j in (i + 1)..<owner.count {
                if let d = geometryDistance(owner[i].geometry, owner[j].geometry) { geoWithin.append(d) }
            }
        }
        for o in owner {
            for p in probe {
                if let d = geometryDistance(o.geometry, p.geometry) { geoBetween.append(d) }
            }
        }

        let ownerPrints: [FeaturePrintObservation] = owner.compactMap { s in
            guard let d = s.featurePrint else { return nil }
            return try? JSONDecoder().decode(FeaturePrintObservation.self, from: d)
        }
        var fpWithin: [Double] = [], fpBetween: [Double] = []
        for i in 0..<ownerPrints.count {
            for j in (i + 1)..<ownerPrints.count {
                if let d = try? ownerPrints[i].distance(to: ownerPrints[j]) { fpWithin.append(d) }
            }
        }
        for o in ownerPrints {
            for p in prints {
                if let d = try? o.distance(to: p) { fpBetween.append(d) }
            }
        }

        let g = separation("Landmark geometry", within: geoWithin, between: geoBetween)
        let f = separation("Image feature print", within: fpWithin, between: fpBetween)
        let validated = g.verdict == "SEPARATED" && f.verdict == "SEPARATED"
            && owner.count >= 3 && probe.count >= 3

        var note: String
        if probe.count == 0 {
            note = "No probe samples yet. Capture a few of someone who is not you to find out whether this matcher can tell you apart."
        } else if probe.count < 3 || owner.count < 3 {
            note = "Too few samples to conclude anything. Capture at least 3 of each."
        } else if validated {
            note = "Both descriptors separate the two people on this sample. This is a small test, not a guarantee."
        } else {
            let failing = [g, f].filter { $0.verdict != "SEPARATED" }.map { $0.name }.joined(separator: " and ")
            note = "\(failing) did NOT separate the two people: a different person landed closer to your profile than your own samples land to each other. The reject side cannot be trusted."
        }

        return SeparationReport(
            ownerSamples: owner.count, probeSamples: probe.count, probeLabel: q.sync { file.label },
            geometry: g, featurePrint: f, rejectSideValidated: validated, note: note)
    }
}
