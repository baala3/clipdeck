#[cfg(target_os = "windows")]
mod appearance;
#[cfg(target_os = "windows")]
mod menu_actions;
#[cfg(target_os = "windows")]
mod menu_images;
#[cfg(target_os = "windows")]
mod menu_keys;
#[cfg(target_os = "windows")]
mod menu_preview;
#[cfg(target_os = "windows")]
mod menu_tips;
#[cfg(target_os = "windows")]
mod paste;
#[cfg(target_os = "windows")]
mod windows_adapter;

#[cfg(target_os = "windows")]
pub use appearance::set_menu_appearance;
#[cfg(target_os = "windows")]
pub use menu_actions::{
    clear_menu_actions, on_menu_action, set_menu_actions, MenuRowActions, PickedAction,
};
#[cfg(target_os = "windows")]
pub use menu_images::{clear_menu_images, set_menu_images, MenuImage};
#[cfg(target_os = "windows")]
pub use menu_keys::{held_modifiers, take_activation_modifiers, MenuKeyHook};
#[cfg(target_os = "windows")]
pub use menu_preview::on_image_preview;
#[cfg(target_os = "windows")]
pub use menu_tips::{clear_menu_tips, set_menu_tips, MenuSlot};
#[cfg(target_os = "windows")]
pub use paste::{focus_window_and_paste, foreground_window, restore_foreground};
#[cfg(target_os = "windows")]
pub use windows_adapter::WindowsClipboardSource;
