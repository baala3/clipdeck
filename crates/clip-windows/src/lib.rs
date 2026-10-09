#[cfg(target_os = "windows")]
mod paste;
#[cfg(target_os = "windows")]
mod windows_adapter;

#[cfg(target_os = "windows")]
pub use paste::{focus_window_and_paste, foreground_window};
#[cfg(target_os = "windows")]
pub use windows_adapter::WindowsClipboardSource;
