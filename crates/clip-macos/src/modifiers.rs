// UNVERIFIED, like the rest of this crate: written from objc2-app-kit's API
// but never built or run on macOS (no Apple SDK in this dev environment).

use clip_engine::menu::HeldModifiers;
use objc2_app_kit::{NSEvent, NSEventModifierFlags};

/// The modifier keys held right now. NSMenu delivers an item's action while
/// the click's modifiers are still current, so this is also what was held
/// when a menu item was chosen.
pub fn held_modifiers() -> HeldModifiers {
    let flags = unsafe { NSEvent::modifierFlags_class() };
    HeldModifiers {
        command_or_control: flags.contains(NSEventModifierFlags::NSEventModifierFlagCommand),
        alt: flags.contains(NSEventModifierFlags::NSEventModifierFlagOption),
        shift: flags.contains(NSEventModifierFlags::NSEventModifierFlagShift),
    }
}
