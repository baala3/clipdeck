#[cfg(target_os = "windows")]
mod menu_keys;
#[cfg(target_os = "windows")]
mod paste;
#[cfg(target_os = "windows")]
mod windows_adapter;

#[cfg(target_os = "windows")]
pub use menu_keys::{held_modifiers, take_activation_modifiers, MenuKeyHook};
#[cfg(target_os = "windows")]
pub use paste::{focus_window_and_paste, foreground_window, restore_foreground};
#[cfg(target_os = "windows")]
pub use windows_adapter::WindowsClipboardSource;
