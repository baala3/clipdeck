//! Keyboard handling for Clipdeck's popup menus while they're open.
//!
//! Windows popup menus (`TrackPopupMenu`) run their own modal loop that ignores
//! accelerators, so Ctrl+1 can't pick an item the way Cmd+1 does in a macOS
//! menu. A `WH_MSGFILTER` hook sees every message in that loop on our thread,
//! which lets us catch the number shortcut, close the menu, and report which
//! digit was pressed. It also records which modifiers were held when an item
//! was activated, for Clipy-style modifier+click actions.

use clip_engine::menu::HeldModifiers;
use clip_engine::ShortcutModifier;
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EndMenu, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSG, MSGF_MENU,
    WH_MSGFILTER, WM_KEYDOWN, WM_LBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const VK_RETURN: u16 = 0x0D;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: u16 = 0x12;

const NO_DIGIT: i32 = -1;
const NOT_RECORDED: u8 = u8::MAX;

/// Only one popup menu can be open at a time (it's modal on the main thread),
/// so the hook's inputs and outputs live in statics.
static NUMBER_MODIFIER: AtomicU8 = AtomicU8::new(0);
static PICKED_DIGIT: AtomicI32 = AtomicI32::new(NO_DIGIT);
static ACTIVATION_MODIFIERS: AtomicU8 = AtomicU8::new(NOT_RECORDED);

/// The modifier keys physically held right now.
pub fn held_modifiers() -> HeldModifiers {
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    HeldModifiers {
        command_or_control: down(VK_CONTROL),
        alt: down(VK_MENU as i32),
        shift: down(VK_SHIFT),
    }
}

/// The modifiers held when the last menu item was clicked or chosen with
/// Enter, or the current ones if nothing was recorded (e.g. the tray menu,
/// which this hook doesn't watch). Reading it clears the record.
pub fn take_activation_modifiers() -> HeldModifiers {
    match ACTIVATION_MODIFIERS.swap(NOT_RECORDED, Ordering::SeqCst) {
        NOT_RECORDED => held_modifiers(),
        bits => from_bits(bits),
    }
}

/// Watches a popup menu for the lifetime of this value. Create it on the
/// thread that shows the menu, right before showing it.
pub struct MenuKeyHook {
    hook: HHOOK,
}

impl MenuKeyHook {
    pub fn install(number_modifier: ShortcutModifier) -> Option<Self> {
        NUMBER_MODIFIER.store(modifier_code(number_modifier), Ordering::SeqCst);
        PICKED_DIGIT.store(NO_DIGIT, Ordering::SeqCst);
        ACTIVATION_MODIFIERS.store(NOT_RECORDED, Ordering::SeqCst);
        let hook = unsafe {
            SetWindowsHookExW(WH_MSGFILTER, Some(menu_filter), None, GetCurrentThreadId())
        };
        hook.ok().map(|hook| Self { hook })
    }

    /// The number key (1-9, or 0 for the tenth item) that closed the menu, if any.
    pub fn picked_digit(&self) -> Option<char> {
        match PICKED_DIGIT.load(Ordering::SeqCst) {
            NO_DIGIT => None,
            digit => char::from_digit(digit as u32, 10),
        }
    }
}

impl Drop for MenuKeyHook {
    fn drop(&mut self) {
        let _ = unsafe { UnhookWindowsHookEx(self.hook) };
    }
}

unsafe extern "system" fn menu_filter(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 {
        let msg = &*(lparam.0 as *const MSG);
        let vk = msg.wParam.0 as u16;
        match msg.message {
            // Pressing Alt normally dismisses a popup menu; swallow it so
            // Alt+click and Alt+digit can work.
            WM_SYSKEYDOWN | WM_SYSKEYUP if vk == VK_MENU => return LRESULT(1),
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(digit) = digit_for_key(vk) {
                    let required = modifier_from_code(NUMBER_MODIFIER.load(Ordering::SeqCst));
                    if required.is_held(held_modifiers()) {
                        PICKED_DIGIT.store(digit, Ordering::SeqCst);
                        let _ = EndMenu();
                        return LRESULT(1);
                    }
                } else if vk == VK_RETURN {
                    record_activation_modifiers();
                }
            }
            WM_LBUTTONUP => record_activation_modifiers(),
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn record_activation_modifiers() {
    ACTIVATION_MODIFIERS.store(to_bits(held_modifiers()), Ordering::SeqCst);
}

/// Top-row and numpad digits.
fn digit_for_key(vk: u16) -> Option<i32> {
    match vk {
        0x30..=0x39 => Some((vk - 0x30) as i32),
        0x60..=0x69 => Some((vk - 0x60) as i32),
        _ => None,
    }
}

fn to_bits(held: HeldModifiers) -> u8 {
    held.command_or_control as u8 | (held.alt as u8) << 1 | (held.shift as u8) << 2
}

fn from_bits(bits: u8) -> HeldModifiers {
    HeldModifiers {
        command_or_control: bits & 1 != 0,
        alt: bits & 2 != 0,
        shift: bits & 4 != 0,
    }
}

fn modifier_code(modifier: ShortcutModifier) -> u8 {
    match modifier {
        ShortcutModifier::CommandOrControl => 0,
        ShortcutModifier::Alt => 1,
        ShortcutModifier::Shift => 2,
        ShortcutModifier::None => 3,
        ShortcutModifier::Off => 4,
    }
}

fn modifier_from_code(code: u8) -> ShortcutModifier {
    match code {
        0 => ShortcutModifier::CommandOrControl,
        1 => ShortcutModifier::Alt,
        2 => ShortcutModifier::Shift,
        3 => ShortcutModifier::None,
        _ => ShortcutModifier::Off,
    }
}
