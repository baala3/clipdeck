#[cfg(target_os = "macos")]
mod macos_adapter;

#[cfg(target_os = "macos")]
pub use macos_adapter::MacClipboardSource;
#[cfg(target_os = "macos")]
mod modifiers;
#[cfg(target_os = "macos")]
pub use modifiers::held_modifiers;
