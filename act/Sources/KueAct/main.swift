// kue-act — the Action Broker's hands for applications, links and notifications.
//
// One invocation performs ONE already-authorized, allowlisted action and then
// checks that it happened. It prints a single JSON line:
//   {"result": SUCCEEDED | FAILED | UNKNOWN_RESULT, "reason": ..., "verification": ...}
// SUCCEEDED is printed only with verification evidence.
//
// There is no generic command here: no shell, no AppleScript, no arbitrary
// arguments. It never force-quits.
//
//   kue-act open-app | focus-app | close-app | open-url | notify | open-file | open-folder
//           | trash | untrash
//
// The verb is the only argument. The target arrives as ONE JSON line on
// standard input — {"v":1,"verb":...,"target":{"name"|"url"|"path"|"title","body"}}
// — so no app name, link, path or notification text is visible in the process
// table. Nothing is written to disk or logged.

import AppKit
import Foundation
import UserNotifications

func out(_ result: String, _ reason: String? = nil, _ verification: String? = nil, landed: String? = nil) -> Never {
    var d: [String: Any] = ["v": 1, "result": result]
    if let r = reason { d["reason"] = r }
    if let v = verification { d["verification"] = v }
    // Where a file ended up, for putting it back exactly. macOS renames on a
    // name collision in the Trash, so the name alone would not find it again.
    if let l = landed { d["landed"] = l }
    let data = (try? JSONSerialization.data(withJSONObject: d)) ?? Data("{}".utf8)
    FileHandle.standardOutput.write(data + Data("\n".utf8))
    exit(0)
}

/// Apps KUE will not quit: itself (use the kill switch) and the system shell.
let protectedBundles: Set<String> = ["dev.lantern.desktop", "dev.lantern.sense", "dev.lantern.act", "com.apple.finder",
    "com.apple.loginwindow", "com.apple.dock", "com.apple.systemuiserver", "com.apple.WindowManager"]

func findApp(_ name: String) -> URL? {
    let fm = FileManager.default
    let target = name.lowercased().replacingOccurrences(of: ".app", with: "")
    let dirs = ["/Applications", "/System/Applications", "/System/Applications/Utilities", "/Applications/Utilities",
                NSHomeDirectory() + "/Applications"]
    for d in dirs {
        guard let items = try? fm.contentsOfDirectory(atPath: d) else { continue }
        if let hit = items.first(where: { $0.lowercased() == target + ".app" }) { return URL(fileURLWithPath: d).appendingPathComponent(hit) }
    }
    // Running apps by their displayed name (e.g. an app installed elsewhere).
    return NSWorkspace.shared.runningApplications.first { ($0.localizedName ?? "").lowercased() == target }?.bundleURL
}

func running(_ name: String) -> NSRunningApplication? {
    let target = name.lowercased().replacingOccurrences(of: ".app", with: "")
    return NSWorkspace.shared.runningApplications.first {
        ($0.localizedName ?? "").lowercased() == target
            || ($0.bundleURL?.deletingPathExtension().lastPathComponent.lowercased() == target)
    }
}

func waitUntil(_ seconds: Double, _ cond: () -> Bool) -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while Date() < end {
        if cond() { return true }
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    }
    return cond()
}

let argv = CommandLine.arguments
guard argv.count == 2 else { out("FAILED", "BAD_ARGUMENTS") }
let verb = argv[1]
let input = FileHandle.standardInput.readDataToEndOfFile()
guard let request = (try? JSONSerialization.jsonObject(with: input)) as? [String: Any],
      request["verb"] as? String == verb,
      let target = request["target"] as? [String: String] else { out("FAILED", "BAD_REQUEST") }
func field(_ key: String) -> String {
    guard let v = target[key], !v.isEmpty else { out("FAILED", "BAD_REQUEST") }
    return v
}

switch verb {
case "open-app", "focus-app":
    let name = field("name")
    if verb == "focus-app" && running(name) == nil { out("FAILED", "\(name) is not running.") }
    guard let url = findApp(name) ?? running(name)?.bundleURL else { out("FAILED", "No application named \(name) was found.") }
    guard let bundleId = Bundle(url: url)?.bundleIdentifier else { out("FAILED", "\(url.lastPathComponent) has no bundle identifier.") }
    let cfg = NSWorkspace.OpenConfiguration()
    cfg.activates = true
    var launchError: Error?
    var done = false
    NSWorkspace.shared.openApplication(at: url, configuration: cfg) { _, err in launchError = err; done = true }
    _ = waitUntil(15) { done }
    if let e = launchError { out("FAILED", "macOS could not open \(name): \(e.localizedDescription)") }
    let launched = waitUntil(15) {
        NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).contains { $0.isFinishedLaunching }
    }
    guard launched, let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).first else {
        out("UNKNOWN_RESULT", "\(name) did not report that it finished launching.")
    }
    let front = waitUntil(3) { NSWorkspace.shared.frontmostApplication?.bundleIdentifier == bundleId }
    if verb == "focus-app" && !front {
        out("UNKNOWN_RESULT", "\(name) is running but macOS did not bring it to the front.", "pid \(app.processIdentifier) running")
    }
    out("SUCCEEDED", nil, "\(bundleId) running as pid \(app.processIdentifier), finished launching\(front ? ", frontmost" : "")")

case "close-app":
    let name = field("name")
    guard let app = running(name) else { out("FAILED", "\(name) is not running.") }
    let bid = app.bundleIdentifier ?? ""
    if protectedBundles.contains(bid) { out("FAILED", "KUE will not quit \(name). To stop KUE, use the kill switch.") }
    let pid = app.processIdentifier
    guard app.terminate() else { out("FAILED", "macOS refused to ask \(name) to quit.") }
    if waitUntil(10, { app.isTerminated }) {
        out("SUCCEEDED", nil, "pid \(pid) (\(bid)) has exited")
    }
    out("UNKNOWN_RESULT", "\(name) is still running — it may be asking you to save. KUE never force-quits.")

case "open-url":
    guard let url = URL(string: field("url")), let scheme = url.scheme?.lowercased(), scheme == "https" || scheme == "http",
          url.host != nil else { out("FAILED", "Only http and https links may be opened.") }
    var handler: NSRunningApplication?
    var openError: Error?
    var done = false
    NSWorkspace.shared.open(url, configuration: NSWorkspace.OpenConfiguration()) { app, err in handler = app; openError = err; done = true }
    _ = waitUntil(15) { done }
    if let e = openError { out("FAILED", "macOS could not open the link: \(e.localizedDescription)") }
    guard let app = handler, !app.isTerminated else { out("UNKNOWN_RESULT", "macOS did not report which app received the link.") }
    out("SUCCEEDED", nil, "handed to \(app.localizedName ?? app.bundleIdentifier ?? "the browser") (pid \(app.processIdentifier)); whether the page loaded is not verified")

case "open-file":
    // The broker has already checked the path against the document folders.
    // Checked again here: a document type, a regular file, not a link.
    let allowed: Set<String> = ["pdf", "doc", "docx", "pages", "rtf", "txt", "md", "odt", "key", "ppt", "pptx",
                                "numbers", "xls", "xlsx", "csv", "png", "jpg", "jpeg", "heic", "epub"]
    let path = field("path")
    let url = URL(fileURLWithPath: path)
    guard path.hasPrefix("/"), allowed.contains(url.pathExtension.lowercased()) else {
        out("FAILED", "Only documents may be opened this way.")
    }
    guard let attrs = try? FileManager.default.attributesOfItem(atPath: path),
          (attrs[.type] as? FileAttributeType) == .typeRegular else {
        out("FAILED", "That file no longer exists or is not a regular file.")
    }
    guard let appURL = NSWorkspace.shared.urlForApplication(toOpen: url) else {
        out("FAILED", "No application on this Mac opens .\(url.pathExtension) files.")
    }
    var handler: NSRunningApplication?
    var openError: Error?
    var done = false
    let cfg = NSWorkspace.OpenConfiguration()
    cfg.activates = true
    NSWorkspace.shared.open([url], withApplicationAt: appURL, configuration: cfg) { app, err in handler = app; openError = err; done = true }
    _ = waitUntil(20) { done }
    if let e = openError { out("FAILED", "macOS could not open the document: \(e.localizedDescription)") }
    guard let app = handler, !app.isTerminated else { out("UNKNOWN_RESULT", "macOS did not report which app opened the document.") }
    let front = waitUntil(3) { NSWorkspace.shared.frontmostApplication?.processIdentifier == app.processIdentifier }
    out("SUCCEEDED", nil, "handed to \(app.localizedName ?? appURL.lastPathComponent) (pid \(app.processIdentifier))\(front ? ", frontmost" : ""); whether its window shows the file is not verified")

case "open-folder":
    // The broker has already found this folder in Desktop, Documents, Downloads or ~/KUE
    // and checked it. Checked again here: a real folder under your home, not a link or a package.
    let path = field("path")
    let url = URL(fileURLWithPath: path)
    let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
    guard path.hasPrefix("/"), url.standardizedFileURL.path.hasPrefix(home + "/") else {
        out("FAILED", "Only folders in your home folder may be opened this way.")
    }
    guard let attrs = try? FileManager.default.attributesOfItem(atPath: path),
          (attrs[.type] as? FileAttributeType) == .typeDirectory else {
        out("FAILED", "That folder no longer exists or is not a folder.")
    }
    guard !NSWorkspace.shared.isFilePackage(atPath: path) else { out("FAILED", "Apps and packages are not opened as folders.") }
    guard let finder = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.finder") else {
        out("FAILED", "Finder could not be found.")
    }
    var handler: NSRunningApplication?
    var openError: Error?
    var done = false
    let cfg = NSWorkspace.OpenConfiguration()
    cfg.activates = true
    NSWorkspace.shared.open([url], withApplicationAt: finder, configuration: cfg) { app, err in handler = app; openError = err; done = true }
    _ = waitUntil(15) { done }
    if let e = openError { out("FAILED", "macOS could not open the folder: \(e.localizedDescription)") }
    guard let app = handler, !app.isTerminated else { out("UNKNOWN_RESULT", "macOS did not report that Finder received the folder.") }
    let front = waitUntil(3) { NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "com.apple.finder" }
    out("SUCCEEDED", nil, "handed to Finder (pid \(app.processIdentifier))\(front ? ", frontmost" : ""); whether its window shows the folder is not verified")

case "trash":
    // ONE file to the Trash, reversibly: `trashItem` is macOS's own move, so the
    // Finder's Put Back works afterwards. Never `removeItem` — nothing here
    // deletes. The broker has already checked this path against the folders the
    // owner allowed and against what KUE itself found; checked again here.
    let path = field("path")
    let url = URL(fileURLWithPath: path).standardizedFileURL
    let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
    guard path.hasPrefix("/"), url.path.hasPrefix(home + "/") else {
        out("FAILED", "Only files in your home folder may be moved to the Trash.")
    }
    // Your own Library is where applications keep what they need to run, KUE
    // included. Nothing there is ever a cleanup candidate.
    guard !url.path.hasPrefix(home + "/Library/") else { out("FAILED", "Files in your Library folder are left alone.") }
    var isDir: ObjCBool = false
    guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else {
        out("FAILED", "That file is no longer there.")
    }
    guard !isDir.boolValue else { out("FAILED", "Only files are moved this way, not folders.") }
    guard (try? FileManager.default.attributesOfItem(atPath: url.path))?[.type] as? FileAttributeType != .typeSymbolicLink else {
        out("FAILED", "Links are left alone.")
    }
    var trashed: NSURL?
    do { try FileManager.default.trashItem(at: url, resultingItemURL: &trashed) }
    catch { out("FAILED", "macOS did not move it: \(error.localizedDescription)") }
    guard let landed = trashed as URL?, FileManager.default.fileExists(atPath: landed.path) else {
        out("UNKNOWN_RESULT", "macOS did not say where the file went.")
    }
    guard !FileManager.default.fileExists(atPath: url.path) else {
        out("UNKNOWN_RESULT", "A file is still at the original location.")
    }
    // The Trash is on the same volume, so no space has come back yet. That is
    // said here rather than left for something further up to assume.
    out("SUCCEEDED", nil, "moved to \(landed.path); nothing remains at the original location; the space returns when the Trash is emptied",
        landed: landed.path)

case "untrash":
    // Back where it came from, and only if nothing has taken its place.
    let from = URL(fileURLWithPath: field("from")).standardizedFileURL
    let to = URL(fileURLWithPath: field("to")).standardizedFileURL
    let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
    guard from.path.hasPrefix(home + "/.Trash/") else { out("FAILED", "Only KUE's own move can be undone.") }
    guard to.path.hasPrefix(home + "/"), !to.path.hasPrefix(home + "/Library/") else {
        out("FAILED", "That is not a place KUE puts files back.")
    }
    guard FileManager.default.fileExists(atPath: from.path) else { out("FAILED", "It is no longer in the Trash.") }
    guard !FileManager.default.fileExists(atPath: to.path) else {
        out("FAILED", "Something is already where it came from, so KUE has left both alone.")
    }
    do { try FileManager.default.moveItem(at: from, to: to) }
    catch { out("FAILED", "macOS did not move it back: \(error.localizedDescription)") }
    guard FileManager.default.fileExists(atPath: to.path), !FileManager.default.fileExists(atPath: from.path) else {
        out("UNKNOWN_RESULT", "macOS did not report the move back.")
    }
    out("SUCCEEDED", nil, "back at \(to.path); nothing remains in the Trash for it")

case "notify":
    let (title, body) = (target["title"] ?? "KUE", field("body"))
    let center = UNUserNotificationCenter.current()
    var granted = false, asked = false
    center.requestAuthorization(options: [.alert, .sound]) { ok, _ in granted = ok; asked = true }
    _ = waitUntil(60) { asked }
    guard granted else { out("FAILED", "PERMISSION_REQUIRED: Notifications from KUE are not allowed in System Settings.") }
    let content = UNMutableNotificationContent()
    content.title = String(title.prefix(80))
    content.body = String(body.prefix(400))
    let id = "kue-\(UUID().uuidString)"
    var addError: Error?, added = false
    center.add(UNNotificationRequest(identifier: id, content: content, trigger: nil)) { err in addError = err; added = true }
    _ = waitUntil(10) { added }
    if let e = addError { out("FAILED", "macOS rejected the notification: \(e.localizedDescription)") }
    var delivered = false
    _ = waitUntil(5) {
        let sem = DispatchSemaphore(value: 0)
        center.getDeliveredNotifications { list in delivered = list.contains { $0.request.identifier == id }; sem.signal() }
        _ = sem.wait(timeout: .now() + 1)
        return delivered
    }
    if delivered { out("SUCCEEDED", nil, "Notification Center lists it as delivered") }
    out("UNKNOWN_RESULT", "macOS accepted the notification but did not list it as delivered (Focus may be on).")

default:
    out("FAILED", "UNKNOWN_ACTION")
}
