#[cfg(target_os = "macos")]
mod macos_adapter;

#[cfg(target_os = "macos")]
pub use macos_adapter::{copy_png, MacClipboardSource};
#[cfg(target_os = "macos")]
mod modifiers;
#[cfg(target_os = "macos")]
pub use modifiers::held_modifiers;
#[cfg(target_os = "macos")]
mod paste;
#[cfg(target_os = "macos")]
pub use paste::{ask_to_allow_pasting, paste_into_frontmost_app, pasting_is_allowed};
