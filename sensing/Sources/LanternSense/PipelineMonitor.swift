// The perception pipeline's account of itself.
//
// WHY THIS EXISTS: a pipeline that is alive but three seconds late looks exactly
// like a pipeline that has died, from the outside. KUE has been treating both as
// "the reading is stale", and the owner's authorization has been falling
// hundreds of times an hour as a result. This file is the sensing layer's half
// of telling them apart: it says "still here, still working, here is how long
// the last analysis took", on a fixed cadence, whether or not a frame came out.
//
// WHAT IT MAY HOLD: times, counts and durations. Nothing else. No pixel buffer,
// no crop, no landmark, no descriptor, no identity — none of that is passed in,
// so none of it can leak out.

import Foundation

/// Counts and timings for the capture → analyse loop. Safe to call from the
/// capture queue, the analysis task and the heartbeat timer.
final class PipelineMonitor {
    /// A frame is "recent enough to prove the loop is running" for this long.
    /// Four frames a second means a loop that has not iterated in a second is
    /// not merely slow.
    private let loopAliveWithin: TimeInterval = 1.0

    private let lock = NSLock()

    private var framesCaptured: UInt64 = 0
    private var framesAnalyzed: UInt64 = 0
    private var framesDropped: UInt64 = 0
    private var lastCaptureAt: Date?
    private var lastAnalyzedAt: Date?
    private var lastLoopAt: Date?
    private var analyzing = false
    private var lastAnalyzeMs: Double = 0
    private var recentAnalyzeMs: [Double] = []
    private var maxCaptureGapMs: Double = 0
    private var windowStart = Date()

    /// A frame arrived from the camera. `replacing` is true when it overwrote
    /// one that was never analysed — the signal that analysis is behind.
    func captured(replacing: Bool) {
        lock.lock(); defer { lock.unlock() }
        let now = Date()
        if let last = lastCaptureAt {
            maxCaptureGapMs = max(maxCaptureGapMs, now.timeIntervalSince(last) * 1000)
        }
        lastCaptureAt = now
        framesCaptured &+= 1
        if replacing { framesDropped &+= 1 }
    }

    /// The analysis loop iterated. Called every pass, including passes with no
    /// frame to analyse: that is what proves the loop itself is alive.
    func loopTicked() {
        lock.lock(); lastLoopAt = Date(); lock.unlock()
    }

    func analysisBegan() {
        lock.lock(); analyzing = true; lock.unlock()
    }

    func analysisEnded(milliseconds: Double) {
        lock.lock(); defer { lock.unlock() }
        analyzing = false
        lastAnalyzedAt = Date()
        framesAnalyzed &+= 1
        lastAnalyzeMs = milliseconds
        recentAnalyzeMs.append(milliseconds)
        if recentAnalyzeMs.count > 64 { recentAnalyzeMs.removeFirst() }
    }

    /// The camera stopped: counters describe a session, so they start again.
    func sessionEnded() {
        lock.lock(); defer { lock.unlock() }
        framesCaptured = 0; framesAnalyzed = 0; framesDropped = 0
        lastCaptureAt = nil; lastAnalyzedAt = nil; lastLoopAt = nil
        analyzing = false; lastAnalyzeMs = 0; recentAnalyzeMs.removeAll()
        maxCaptureGapMs = 0; windowStart = Date()
    }

    struct Snapshot {
        var visionBusy: Bool
        var loopAlive: Bool
        var lastCaptureAt: Double?
        var lastAnalyzedAt: Double?
        var analyzeMsLast: Double
        var analyzeMsP50: Double
        var analyzeMsMax: Double
        var captureGapMsMax: Double
        var framesCaptured: UInt64
        var framesAnalyzed: UInt64
        var framesDropped: UInt64
    }

    /// Takes the picture and resets the per-window extremes, so
    /// `captureGapMsMax` always describes the window just ended rather than the
    /// worst moment since launch.
    func snapshot() -> Snapshot {
        lock.lock(); defer { lock.unlock() }
        let sorted = recentAnalyzeMs.sorted()
        let p50 = sorted.isEmpty ? 0 : sorted[sorted.count / 2]
        let s = Snapshot(
            visionBusy: analyzing,
            loopAlive: lastLoopAt.map { Date().timeIntervalSince($0) <= loopAliveWithin } ?? false,
            lastCaptureAt: lastCaptureAt.map { $0.timeIntervalSince1970 },
            lastAnalyzedAt: lastAnalyzedAt.map { $0.timeIntervalSince1970 },
            analyzeMsLast: lastAnalyzeMs,
            analyzeMsP50: p50,
            analyzeMsMax: sorted.last ?? 0,
            captureGapMsMax: maxCaptureGapMs,
            framesCaptured: framesCaptured,
            framesAnalyzed: framesAnalyzed,
            framesDropped: framesDropped)
        maxCaptureGapMs = 0
        windowStart = Date()
        return s
    }
}
