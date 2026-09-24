// kue-auth — asks macOS to authenticate the owner, and reports what macOS said.
//
// One invocation, one answer, then exit. KUE never sees a password or a
// fingerprint: LocalAuthentication runs the prompt and returns only success or
// the reason for failure. KUE cannot bypass it, and does not try.
//
//   kue-auth probe
//   kue-auth strong   "<reason shown by macOS>"   Touch ID, or the login password
//   kue-auth physical "<reason shown by macOS>"   Touch ID only: a finger on the sensor
//
// stdout: one JSON line.

import Foundation
import LocalAuthentication

func out(_ d: [String: Any]) -> Never {
    var d = d
    d["v"] = 1
    let data = (try? JSONSerialization.data(withJSONObject: d)) ?? Data("{}".utf8)
    FileHandle.standardOutput.write(data + Data("\n".utf8))
    exit(0)
}

func biometryName(_ ctx: LAContext) -> String {
    switch ctx.biometryType {
    case .touchID: return "TOUCH_ID"
    case .faceID: return "FACE_ID"
    case .opticID: return "OPTIC_ID"
    case .none: return "NONE"
    @unknown default: return "UNKNOWN"
    }
}

func code(_ error: Error?) -> String {
    guard let e = error as? LAError else { return error == nil ? "UNKNOWN" : "FAILED" }
    switch e.code {
    case .authenticationFailed: return "AUTHENTICATION_FAILED"
    case .userCancel: return "USER_CANCELLED"
    case .userFallback: return "USER_FALLBACK"
    case .systemCancel: return "SYSTEM_CANCELLED"
    case .appCancel: return "APP_CANCELLED"
    case .passcodeNotSet: return "PASSCODE_NOT_SET"
    case .biometryNotAvailable: return "BIOMETRY_NOT_AVAILABLE"
    case .biometryNotEnrolled: return "BIOMETRY_NOT_ENROLLED"
    case .biometryLockout: return "BIOMETRY_LOCKOUT"
    case .notInteractive: return "NOT_INTERACTIVE"
    case .invalidContext: return "INVALID_CONTEXT"
    default: return "LA_ERROR_\(e.code.rawValue)"
    }
}

let args = CommandLine.arguments
guard args.count >= 2 else { out(["result": "BAD_ARGUMENTS"]) }
let ctx = LAContext()

switch args[1] {
case "probe":
    var e1: NSError?, e2: NSError?
    let bio = ctx.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &e1)
    let owner = ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &e2)
    out(["result": "PROBE", "biometry": biometryName(ctx),
         "physical_available": bio, "physical_unavailable_reason": bio ? NSNull() : code(e1),
         "strong_available": owner, "strong_unavailable_reason": owner ? NSNull() : code(e2)])
case "strong", "physical":
    guard args.count >= 3, !args[2].isEmpty else { out(["result": "BAD_ARGUMENTS"]) }
    let policy: LAPolicy = args[1] == "physical" ? .deviceOwnerAuthenticationWithBiometrics : .deviceOwnerAuthentication
    // A physical confirmation is a finger on the sensor, not a typed password.
    if args[1] == "physical" { ctx.localizedFallbackTitle = "" }
    var err: NSError?
    guard ctx.canEvaluatePolicy(policy, error: &err) else {
        out(["result": "UNAVAILABLE", "reason": code(err), "level": args[1]])
    }
    let started = Date()
    ctx.evaluatePolicy(policy, localizedReason: args[2]) { ok, error in
        out(["result": ok ? "SUCCESS" : "FAILED", "reason": ok ? NSNull() : code(error),
             "level": args[1], "biometry": biometryName(ctx),
             "seconds": Date().timeIntervalSince(started)])
    }
    RunLoop.main.run()
default:
    out(["result": "BAD_ARGUMENTS"])
}
