//! Light or dark native menus, whatever the system is set to.
//!
//! Tauri can theme windows and menu bars, but a popup menu takes its colors
//! from a per-process "preferred app mode" that only uxtheme.dll's ordinal
//! exports can change. They're undocumented but stable since Windows 10 1903,
//! and Tauri's own window layer already calls one of them at startup to opt
//! in to dark menus. On anything older these calls do nothing useful, and
//! menus stay light.

use clip_engine::Appearance;
use std::sync::atomic::{AtomicU8, Ordering};
use windows::core::{w, PCSTR};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

const SET_PREFERRED_APP_MODE: usize = 135;
const FLUSH_MENU_THEMES: usize = 136;

static CHOSEN: AtomicU8 = AtomicU8::new(Appearance::System as u8);

/// Makes every native menu from here on light, dark, or whatever the system
/// uses.
pub fn set_menu_appearance(appearance: Appearance) {
    CHOSEN.store(appearance as u8, Ordering::SeqCst);
    // uxtheme's PreferredAppMode: 1 follows the system, 2 and 3 force a side.
    let mode: i32 = match appearance {
        Appearance::System => 1,
        Appearance::Dark => 2,
        Appearance::Light => 3,
    };
    unsafe {
        let Ok(uxtheme) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        let ordinal = |ordinal: usize| GetProcAddress(uxtheme, PCSTR(ordinal as *const u8));
        if let Some(set_mode) = ordinal(SET_PREFERRED_APP_MODE) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            set_mode(mode);
        }
        // Menus cache their theme; without this the change waits for a restart.
        if let Some(flush) = ordinal(FLUSH_MENU_THEMES) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

/// Whether menus are dark right now: the user's choice, or the system's app
/// theme when they left it to the system.
pub(crate) fn menus_are_dark() -> bool {
    match CHOSEN.load(Ordering::SeqCst) {
        chosen if chosen == Appearance::Dark as u8 => true,
        chosen if chosen == Appearance::Light as u8 => false,
        _ => system_apps_are_dark(),
    }
}

fn system_apps_are_dark() -> bool {
    let mut light: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut light as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    read.is_ok() && light == 0
}
