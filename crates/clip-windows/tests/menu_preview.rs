#![cfg(target_os = "windows")]
//! Opens a real popup menu with an image Clip in it, walks the highlight onto
//! and off the image with the keyboard, and watches for the preview window.
//! The keys are posted to this thread's own queue, so nothing is typed into
//! whatever else is running.

use clip_engine::menu::MenuTip;
use clip_engine::Rgba;
use clip_windows::{
    clear_menu_images, clear_menu_tips, on_image_preview, set_menu_images, set_menu_tips,
    MenuImage, MenuSlot,
};
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, DispatchMessageW,
    EndMenu, FindWindowExW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, KillTimer,
    PeekMessageW, PostMessageW, SetTimer, TrackPopupMenu, HMENU, MF_GRAYED, MF_STRING, MSG,
    PM_REMOVE, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, WINDOW_EX_STYLE, WM_KEYDOWN, WS_POPUP,
};

const VK_DOWN: usize = 0x28;
const DIGEST: &str = "a-screenshot";
/// A wide screenshot, far bigger than a preview gets.
const WIDTH: u32 = 1600;
const HEIGHT: u32 = 800;

thread_local! {
    static OWNER: RefCell<HWND> = RefCell::new(HWND::default());
    static STEP: RefCell<usize> = const { RefCell::new(0) };
    /// The preview's place on screen after each step, if it was showing.
    static SEEN: RefCell<Vec<Option<RECT>>> = const { RefCell::new(Vec::new()) };
    static MENU_RECT: RefCell<RECT> = RefCell::new(RECT::default());
    static DPI: RefCell<u32> = const { RefCell::new(96) };
}

/// Blocks of color, so a look at the screen shows whether it came out right.
fn screenshot() -> Rgba {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let block = (x / 200 + y / 200) % 2 == 0;
            let red = (x * 255 / WIDTH) as u8;
            let blue = (y * 255 / HEIGHT) as u8;
            pixels.extend_from_slice(&[red, if block { 200 } else { 90 }, blue, 255]);
        }
    }
    Rgba::new(WIDTH, HEIGHT, pixels).unwrap()
}

/// This thread's windows of `class` that are showing.
unsafe fn visible_windows(class: PCWSTR, title: PCWSTR) -> Vec<HWND> {
    let mut found = Vec::new();
    let mut window = HWND::default();
    while let Ok(next) = FindWindowExW(None, window, class, title) {
        if next.0.is_null() {
            break;
        }
        window = next;
        if GetWindowThreadProcessId(window, None) == GetCurrentThreadId()
            && IsWindowVisible(window).as_bool()
        {
            found.push(window);
        }
    }
    found
}

unsafe fn preview() -> Option<RECT> {
    let window = *visible_windows(w!("STATIC"), w!("Clipdeck image preview")).first()?;
    let mut rect = RECT::default();
    GetWindowRect(window, &mut rect).unwrap();
    Some(rect)
}

unsafe fn press(key: usize) {
    let owner = OWNER.with(|owner| *owner.borrow());
    PostMessageW(owner, WM_KEYDOWN, WPARAM(key), LPARAM(0)).unwrap();
}

/// One step a second, well past the hover delay: record what the last step led to, then act.
unsafe extern "system" fn walk(_: HWND, _: u32, timer: usize, _: u32) {
    let step = STEP.with(|step| step.replace_with(|step| *step + 1));
    if step > 0 {
        SEEN.with(|seen| seen.borrow_mut().push(preview()));
    }
    match step {
        // Onto the disabled header (the keyboard stops there too), then the image.
        0 => {
            press(VK_DOWN);
            press(VK_DOWN);
        }
        1 => {
            if let Some(&menu) = visible_windows(w!("#32768"), PCWSTR::null()).first() {
                MENU_RECT.with(|rect| GetWindowRect(menu, &mut *rect.borrow_mut()).unwrap());
                DPI.with(|dpi| *dpi.borrow_mut() = GetDpiForWindow(menu).max(96));
            }
            // On to the text Clip below it.
            press(VK_DOWN);
        }
        _ => {
            let _ = KillTimer(None, timer);
            let _ = EndMenu();
        }
    }
}

#[test]
fn resting_on_an_image_shows_it_larger_beside_the_menu() {
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

        let menu = CreatePopupMenu().unwrap();
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 1, w!("History")).unwrap();
        AppendMenuW(menu, MF_STRING, 2, w!("Image 1600 \u{d7} 800")).unwrap();
        AppendMenuW(menu, MF_STRING, 3, w!("cargo test --workspace")).unwrap();
        let full = screenshot();
        set_menu_images(
            MenuSlot::Popup,
            menu.0 as isize,
            vec![MenuImage {
                row: 1,
                folder_row: None,
                image: full.fit_within(192, 64),
            }],
        );
        set_menu_tips(
            MenuSlot::Popup,
            menu.0 as isize,
            vec![MenuTip::None, MenuTip::Image(DIGEST.into()), MenuTip::None],
        );
        on_image_preview(move |digest| (digest == DIGEST).then(|| full.clone()));

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

        // The menu is gone; give the previews' own timer a moment to notice.
        let closed = Instant::now();
        while closed.elapsed() < Duration::from_millis(300) {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let after_close = preview();
        clear_menu_tips(MenuSlot::Popup);
        clear_menu_images(MenuSlot::Popup);
        let _ = DestroyMenu(HMENU(menu.0));
        let _ = DestroyWindow(owner);

        let seen = SEEN.with(|seen| seen.borrow().clone());
        let menu_rect = MENU_RECT.with(|rect| *rect.borrow());
        let dpi = DPI.with(|dpi| *dpi.borrow()) as i32;
        let shown = seen[0].expect("resting on the image shows its preview");
        let (width, height) = (shown.right - shown.left, shown.bottom - shown.top);
        // 480 x 360 at 96 DPI holds a 2:1 picture 480 wide, plus a 1px line around it.
        assert_eq!(
            (width, height),
            (480 * dpi / 96 + 2, 240 * dpi / 96 + 2),
            "scaled down to fit, keeping its proportions"
        );
        assert!(
            shown.left >= menu_rect.right,
            "the preview sits to the right of the menu, not over it: {shown:?} vs {menu_rect:?}"
        );
        assert!(
            (menu_rect.top..menu_rect.bottom).contains(&shown.top),
            "level with the menu: {shown:?} vs {menu_rect:?}"
        );
        assert_eq!(seen[1], None, "moving on to a text Clip takes it away");
        assert_eq!(after_close, None, "the preview goes when the menu does");
    }
}
