import CoreGraphics
import Foundation

// Polls CGWindowList for a layer-0 window owned by the given pid and prints
// elapsed milliseconds until one appears (max 20s). No permissions required.
let pidArg: pid_t = CommandLine.arguments.dropFirst().first.flatMap { pid_t($0) } ?? 0
guard pidArg > 0 else { print("no-pid"); exit(2) }

let t0 = Date()
while Date().timeIntervalSince(t0) < 20 {
    if let list = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements],
        kCGNullWindowID
    ) as? [[String: Any]] {
        let found = list.contains { entry in
            (entry[kCGWindowOwnerPID as String] as? Int) == Int(pidArg)
                && (entry[kCGWindowLayer as String] as? Int) == 0
        }
        if found {
            print(Int(Date().timeIntervalSince(t0) * 1000))
            exit(0)
        }
    }
}
print("timeout")
exit(1)
