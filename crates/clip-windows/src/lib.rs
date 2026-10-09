#[cfg(target_os = "windows")]
mod windows_adapter;

#[cfg(target_os = "windows")]
pub use windows_adapter::WindowsClipboardSource;
