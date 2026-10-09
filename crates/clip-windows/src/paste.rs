use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

const VK_CONTROL: VIRTUAL_KEY = VIRTUAL_KEY(0x11);
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

/// The window in the foreground right now, as a raw handle. Call this right
/// before showing a popup, so the popup itself isn't what gets captured.
pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

fn key_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { Default::default() },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Restores foreground focus to `handle` and synthesizes Ctrl+V into it, so
/// selecting a Clip pastes it directly rather than only copying it.
///
/// Best-effort: secure input fields (password boxes) and elevated windows can
/// legitimately refuse synthetic input per Windows' own security model. When
/// that happens this silently no-ops - the Clip is still on the clipboard, so
/// a manual Ctrl+V still works.
pub fn focus_window_and_paste(handle: isize) {
    if handle == 0 {
        return;
    }
    let target = HWND(handle as *mut core::ffi::c_void);
    unsafe {
        let _ = SetForegroundWindow(target);
        // SetForegroundWindow can take a moment to actually hand over focus;
        // sending the keystroke before it lands would paste into whatever
        // still had focus (often our own, now-hidden popup).
        for _ in 0..20 {
            if GetForegroundWindow() == target {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let inputs = [
            key_input(VK_CONTROL, false),
            key_input(VK_V, false),
            key_input(VK_V, true),
            key_input(VK_CONTROL, true),
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
