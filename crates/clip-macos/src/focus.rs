// UNVERIFIED, like the rest of this crate: written from objc2-app-kit's API
// but never built or run on macOS (no Apple SDK in this dev environment).

//! Which app is in front, and putting it back there. Clipdeck's glass menu
//! is a window of its own, so opening it brings Clipdeck to the front; the
//! app the user was in has to be brought back before a Clip is pasted into it.

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use std::thread;
use std::time::Duration;

/// The process id of the app in front right now. Call it before showing the
/// menu, so the menu isn't what gets recorded.
pub fn frontmost_app() -> Option<i32> {
    unsafe {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
    }
}

/// Brings the app with process id `pid` back to the front, and waits briefly
/// until it's there, since a keystroke sent before then would land in
/// Clipdeck. False if that app has quit or didn't come forward.
pub fn activate_app(pid: i32) -> bool {
    let Some(app) = (unsafe { NSRunningApplication::runningApplicationWithProcessIdentifier(pid) })
    else {
        return false;
    };
    // Clipdeck is the active app while its menu is open, so it may hand
    // activation on without the deprecated "ignoring other apps" option.
    unsafe { app.activateWithOptions(NSApplicationActivationOptions::empty()) };
    for _ in 0..30 {
        if frontmost_app() == Some(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}
