// Lantern sensing layer — self-measurement.
//
// What this process costs to run, and the thermal and power state of the Mac it
// runs on. None of it is an observation of the person, so it continues while
// paused: pause must be verifiably cheap, and that needs measuring.
//
// Everything here is read from public APIs. What cannot be read that way — GPU
// and Neural Engine use, energy impact — is not estimated; the core names it as
// not measured.

import Foundation
import IOKit.ps

enum Health {
    /// User + system CPU time consumed by this process, in seconds.
    static func cpuSeconds() -> Double {
        var ru = rusage()
        guard getrusage(RUSAGE_SELF, &ru) == 0 else { return 0 }
        func s(_ t: timeval) -> Double { Double(t.tv_sec) + Double(t.tv_usec) / 1_000_000 }
        return s(ru.ru_utime) + s(ru.ru_stime)
    }

    /// Physical memory footprint — the figure Activity Monitor shows as "Memory".
    static func footprintBytes() -> UInt64? {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? info.phys_footprint : nil
    }

    static func thermalState() -> String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: return "NOMINAL"
        case .fair: return "FAIR"
        case .serious: return "SERIOUS"
        case .critical: return "CRITICAL"
        @unknown default: return "UNKNOWN"
        }
    }

    static func lowPowerMode() -> Bool { ProcessInfo.processInfo.isLowPowerModeEnabled }

    /// Battery charge and the source currently powering the Mac. Either is nil
    /// when macOS does not report it (a desktop Mac has no battery).
    static func power() -> (percent: Double?, source: String?) {
        guard let blob = IOPSCopyPowerSourcesInfo()?.takeRetainedValue() else { return (nil, nil) }
        let source = IOPSGetProvidingPowerSourceType(blob)?.takeUnretainedValue() as String?
        guard let list = IOPSCopyPowerSourcesList(blob)?.takeRetainedValue() as? [CFTypeRef] else {
            return (nil, source)
        }
        for ps in list {
            guard let d = IOPSGetPowerSourceDescription(blob, ps)?.takeUnretainedValue() as? [String: Any],
                  let current = d[kIOPSCurrentCapacityKey] as? Int,
                  let max = d[kIOPSMaxCapacityKey] as? Int, max > 0 else { continue }
            return (Double(current) / Double(max) * 100, source)
        }
        return (nil, source)
    }
}
