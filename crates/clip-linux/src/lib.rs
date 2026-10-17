//! The Linux adapter. It speaks X11 only: see docs/research/linux-support.md
//! for what Wayland would take.

#[cfg(target_os = "linux")]
mod x11_keys;
#[cfg(target_os = "linux")]
mod x11_source;

#[cfg(target_os = "linux")]
pub use x11_keys::{
    activate_window, active_window, held_modifiers, paste_into_focused_app, X11Keys,
};
#[cfg(target_os = "linux")]
pub use x11_source::X11ClipboardSource;
