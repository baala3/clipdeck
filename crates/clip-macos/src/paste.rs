// UNVERIFIED, like the rest of this crate: it type-checks against the Apple
// target, but has never been run on macOS from this dev environment.

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::sync::atomic::{AtomicBool, Ordering};

/// The key in the V position (kVK_ANSI_V). Key codes are positions, not
/// letters, so on a layout that moves V (Dvorak, say) this is another key.
const KEY_V: CGKeyCode = 9;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
}

/// Whether macOS lets Clipdeck send keystrokes to other apps (the
/// Accessibility permission). With `prompt`, macOS also shows its own dialog
/// pointing the user at System Settings when the answer is no.
fn accessibility_granted(prompt: bool) -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = if prompt {
        CFBoolean::true_value()
    } else {
        CFBoolean::false_value()
    };
    let options = CFDictionary::from_CFType_pairs(&[(key, value)]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0 }
}

/// Synthesizes Cmd+V into the frontmost app, so selecting a Clip pastes it
/// directly rather than only copying it. A menu never takes focus on macOS,
/// so the frontmost app is still the one the user was working in.
///
/// Returns false when nothing was sent because Clipdeck lacks the
/// Accessibility permission; macOS is asked to prompt for it once per run.
/// Best-effort beyond that: secure input fields can refuse synthetic input.
/// Either way the Clip is still on the clipboard, so a manual Cmd+V works.
pub fn paste_into_frontmost_app() -> bool {
    static PROMPTED: AtomicBool = AtomicBool::new(false);
    if !accessibility_granted(false) {
        if !PROMPTED.swap(true, Ordering::SeqCst) {
            accessibility_granted(true);
        }
        return false;
    }

    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return false;
    };
    let key_events = (
        CGEvent::new_keyboard_event(source.clone(), KEY_V, true),
        CGEvent::new_keyboard_event(source, KEY_V, false),
    );
    let (Ok(key_down), Ok(key_up)) = key_events else {
        return false;
    };
    // Explicit flags, so modifiers still held from choosing the item (a
    // number shortcut's, say) don't turn this into a different shortcut.
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::AnnotatedSession);
    key_up.post(CGEventTapLocation::AnnotatedSession);
    true
}
