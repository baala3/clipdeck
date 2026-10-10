#![cfg(target_os = "windows")]
//! Opens a real popup menu, walks its highlight with the keyboard, and
//! watches for the tooltip window. The keys are posted to this thread's own
//! queue, so nothing is typed into whatever else is running.

use clip_engine::menu::MenuTip;
use clip_windows::{clear_menu_tips, set_menu_tips, MenuSlot};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, DispatchMessageW,
    EndMenu, FindWindowExW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, KillTimer, PeekMessageW, PostMessageW, SetTimer, TrackPopupMenu, HMENU,
    MF_GRAYED, MF_POPUP, MF_STRING, MSG, PM_REMOVE, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    WINDOW_EX_STYLE, WM_KEYDOWN, WS_POPUP,
};

const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;
const LONG: &str = "https://github.com/baala3/clipdeck/pull/11";
const LETTER: &str = "Dear team,\nthe release is out.\n\tThanks!";

/// What the tooltip looked like at one step of the walk.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    text: Option<String>,
    rect: RECT,
}

thread_local! {
    static OWNER: RefCell<HWND> = RefCell::new(HWND::default());
    static STEP: RefCell<usize> = const { RefCell::new(0) };
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
    static MENU_RECT: RefCell<RECT> = RefCell::new(RECT::default());
}

/// The visible tooltip on this thread, if any.
unsafe fn tooltip() -> Seen {
    let mut window = HWND::default();
    while let Ok(next) = FindWindowExW(None, window, w!("tooltips_class32"), None) {
        if next.0.is_null() {
            break;
        }
        window = next;
        if GetWindowThreadProcessId(window, None) == GetCurrentThreadId()
            && IsWindowVisible(window).as_bool()
        {
            let mut text = [0u16; 512];
            let len = GetWindowTextW(window, &mut text) as usize;
            let mut rect = RECT::default();
            GetWindowRect(window, &mut rect).unwrap();
            return Seen {
                text: Some(String::from_utf16_lossy(&text[..len])),
                rect,
            };
        }
    }
    Seen {
        text: None,
        rect: RECT::default(),
    }
}

unsafe fn press(key: usize) {
    let owner = OWNER.with(|owner| *owner.borrow());
    PostMessageW(owner, WM_KEYDOWN, WPARAM(key), LPARAM(0)).unwrap();
}

/// One step a second, well past the hover delay: record what the last step led to, then act.
unsafe extern "system" fn walk(_: HWND, _: u32, timer: usize, _: u32) {
    let step = STEP.with(|step| step.replace_with(|step| *step + 1));
    if step > 0 {
        SEEN.with(|seen| seen.borrow_mut().push(tooltip()));
    }
    match step {
        // Onto the disabled header (the keyboard stops there too), then
        // "short", whose label shows all of it.
        0 => {
            press(VK_DOWN);
            press(VK_DOWN);
        }
        // Onto the shortened URL.
        1 => {
            let mut window = HWND::default();
            while let Ok(next) = FindWindowExW(None, window, w!("#32768"), None) {
                if next.0.is_null() {
                    break;
                }
                window = next;
                if GetWindowThreadProcessId(window, None) == GetCurrentThreadId() {
                    MENU_RECT.with(|rect| GetWindowRect(window, &mut *rect.borrow_mut()).unwrap());
                }
            }
            press(VK_DOWN)
        }
        // Onto the folder, then into it: its first row is the letter.
        2 => {
            press(VK_DOWN);
            press(VK_RIGHT);
        }
        // Down to the folder's second row, which needs no tooltip.
        3 => press(VK_DOWN),
        _ => {
            let _ = KillTimer(None, timer);
            let _ = EndMenu();
        }
    }
}

#[test]
fn resting_on_a_shortened_item_shows_the_whole_clip_beside_the_menu() {
    unsafe {
        let owner = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        OWNER.with(|cell| *cell.borrow_mut() = owner);

        let folder = CreatePopupMenu().unwrap();
        AppendMenuW(folder, MF_STRING, 10, w!("1. Dear team,")).unwrap();
        AppendMenuW(folder, MF_STRING, 11, w!("2. ok")).unwrap();
        let menu = CreatePopupMenu().unwrap();
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 1, w!("History")).unwrap();
        AppendMenuW(menu, MF_STRING, 2, w!("1. short")).unwrap();
        AppendMenuW(menu, MF_STRING, 3, w!("2. https://github.co...")).unwrap();
        AppendMenuW(menu, MF_POPUP, folder.0 as usize, w!("3 - 4")).unwrap();
        set_menu_tips(
            MenuSlot::Popup,
            menu.0 as isize,
            vec![
                MenuTip::None,
                MenuTip::None,
                MenuTip::Text(LONG.into()),
                MenuTip::Folder(vec![Some(LETTER.into()), None]),
            ],
        );

        SetTimer(None, 0, 1000, Some(walk));
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
            60,
            60,
            0,
            owner,
            None,
        );

        // The menu is gone; give the tooltip's own timer a moment to notice.
        let closed = Instant::now();
        while closed.elapsed() < Duration::from_millis(300) {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let after_close = tooltip();
        clear_menu_tips(MenuSlot::Popup);
        let _ = DestroyMenu(HMENU(menu.0));
        let _ = DestroyWindow(owner);

        let seen = SEEN.with(|seen| seen.borrow().clone());
        let menu_rect = MENU_RECT.with(|rect| *rect.borrow());
        let texts: Vec<Option<&str>> = seen.iter().map(|s| s.text.as_deref()).collect();
        assert_eq!(
            texts,
            vec![
                None,
                Some(LONG),
                Some("Dear team,\nthe release is out.\n    Thanks!"),
                None,
            ],
            "tooltip after each step"
        );
        assert!(
            seen[1].rect.left >= menu_rect.right,
            "the tooltip sits to the right of the menu, not over it: {:?} vs {menu_rect:?}",
            seen[1].rect
        );
        assert!(
            (menu_rect.top..menu_rect.bottom).contains(&seen[1].rect.top),
            "level with the menu: {:?} vs {menu_rect:?}",
            seen[1].rect
        );
        assert_eq!(
            after_close.text, None,
            "the tooltip goes when the menu does"
        );
    }
}
