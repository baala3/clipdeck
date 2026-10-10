//! Tooltips for Clipdeck's native menus: resting on an item whose label was
//! shortened shows the whole Clip next to the menu.
//!
//! Win32 menus have no tooltips of their own, and their owner window (which
//! gets `WM_MENUSELECT`) belongs to Tauri, not to us. So while one of our menus
//! is open, a timer looks up its highlighted item and drives a tracking
//! tooltip by hand. That covers the tray menu and the hotkey popups alike,
//! and keyboard navigation as well as the mouse.

use clip_engine::menu::MenuTip;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Controls::{
    SetWindowTheme, TOOLTIPS_CLASSW, TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW, TTM_SETMARGIN,
    TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP,
    TTS_NOPREFIX, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DestroyWindow, FindWindowExW, GetMenuItemRect, GetMenuState,
    GetSubMenu, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, KillTimer, SendMessageW,
    SetTimer, SetWindowsHookExW, CW_USEDEFAULT, HMENU, MF_BYPOSITION, MF_HILITE, MSGF_MENU,
    WH_MSGFILTER, WINDOW_STYLE, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// Asks a menu window which menu it's showing.
const MN_GETHMENU: u32 = 0x01E1;
const POLL_INTERVAL_MS: u32 = 50;
/// How long the highlight must rest on an item before its tooltip shows.
const HOVER_DELAY: Duration = Duration::from_millis(400);
/// Tooltip width limit and its distance from the menu, at 96 DPI.
const MAX_WIDTH: i32 = 460;
const GAP: i32 = 4;
/// Space around the tooltip's text (horizontal, vertical), at 96 DPI.
const PADDING: (i32, i32) = (6, 4);

/// Clipdeck keeps two menus alive at once: the tray menu, and whichever popup
/// a hotkey opened last.
#[derive(Clone, Copy)]
pub enum MenuSlot {
    Tray,
    Popup,
}

struct Registered {
    hmenu: isize,
    tips: Vec<MenuTip>,
}

/// A highlighted item that has a tooltip.
#[derive(Clone, PartialEq)]
struct Hover {
    /// The menu window showing the item, and the item's place in it.
    window: isize,
    hmenu: isize,
    position: u32,
    text: String,
}

#[derive(Default)]
struct State {
    menus: [Option<Registered>; 2],
    hook_installed: bool,
    polling: bool,
    tooltip: Option<HWND>,
    /// Backs the tooltip's text pointer while it's showing.
    text: Vec<u16>,
    hover: Option<Hover>,
    hover_since: Option<Instant>,
    showing: bool,
}

// Menus are built, shown, and torn down on the main thread only.
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Says what hovering each row of `hmenu` shows, replacing whatever menu held
/// `slot` before. Call it on the thread that shows the menu.
pub fn set_menu_tips(slot: MenuSlot, hmenu: isize, tips: Vec<MenuTip>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.menus[slot as usize] = Some(Registered { hmenu, tips });
        if !state.hook_installed {
            // Lives as long as the app; it only tells us a menu is open.
            let hook = unsafe {
                SetWindowsHookExW(WH_MSGFILTER, Some(menu_filter), None, GetCurrentThreadId())
            };
            state.hook_installed = hook.is_ok();
        }
    });
}

/// Forgets the menu in `slot`, once it's gone.
pub fn clear_menu_tips(slot: MenuSlot) {
    STATE.with(|state| state.borrow_mut().menus[slot as usize] = None);
}

unsafe extern "system" fn menu_filter(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 {
        STATE.with(|state| {
            if let Ok(mut state) = state.try_borrow_mut() {
                if !state.polling {
                    state.polling = SetTimer(None, 0, POLL_INTERVAL_MS, Some(poll)) != 0;
                }
            }
        });
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn poll(_: HWND, _: u32, timer: usize, _: u32) {
    STATE.with(|state| {
        if let Ok(mut state) = state.try_borrow_mut() {
            state.poll(timer);
        }
    });
}

impl State {
    unsafe fn poll(&mut self, timer: usize) {
        let open = open_menu_windows();
        if open.is_empty() {
            let _ = KillTimer(None, timer);
            self.polling = false;
            self.hover = None;
            self.hide();
            return;
        }
        let hover = self
            .menus
            .iter()
            .flatten()
            .find_map(|menu| highlighted(menu, &open));
        if hover != self.hover {
            self.hide();
            self.hover_since = hover.is_some().then(Instant::now);
            self.hover = hover;
        } else if !self.showing && self.hover_since.is_some_and(|t| t.elapsed() >= HOVER_DELAY) {
            if let Some(hover) = self.hover.clone() {
                self.show(&hover);
            }
        }
    }

    unsafe fn show(&mut self, hover: &Hover) {
        let Some(tooltip) = self.tooltip() else {
            return;
        };
        let (mut item, mut menu) = (RECT::default(), RECT::default());
        if GetMenuItemRect(None, HMENU(hover.hmenu as _), hover.position, &mut item).is_err()
            || GetWindowRect(HWND(hover.window as _), &mut menu).is_err()
        {
            return;
        }
        let scale = |px: i32| px * GetDpiForWindow(HWND(hover.window as _)).max(96) as i32 / 96;

        // Tooltips draw a tab as a box.
        self.text = hover
            .text
            .replace('\t', "    ")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut tool = tool_info();
        tool.lpszText = PWSTR(self.text.as_mut_ptr());
        let tool = LPARAM(&tool as *const _ as isize);
        // A width limit is also what makes a tooltip honor line breaks.
        SendMessageW(
            tooltip,
            TTM_SETMAXTIPWIDTH,
            WPARAM(0),
            LPARAM(scale(MAX_WIDTH) as isize),
        );
        // Menus follow the system's app theme; a light tooltip beside a dark
        // menu would glare.
        let theme = if apps_use_dark_theme() {
            w!("DarkMode_Explorer")
        } else {
            w!("Explorer")
        };
        let _ = SetWindowTheme(tooltip, theme, None);
        let padding = RECT {
            left: scale(PADDING.0),
            top: scale(PADDING.1),
            right: scale(PADDING.0),
            bottom: scale(PADDING.1),
        };
        SendMessageW(
            tooltip,
            TTM_SETMARGIN,
            WPARAM(0),
            LPARAM(&padding as *const _ as isize),
        );
        SendMessageW(tooltip, TTM_UPDATETIPTEXTW, WPARAM(0), tool);

        // Beside the menu, level with the item. Its size is only known once
        // it's up, so it's then moved to the left if the right is off-screen,
        // and nudged up if it runs off the bottom.
        let place = |x: i32, y: i32| {
            let at = (x as u16 as isize) | ((y as u16 as isize) << 16);
            SendMessageW(tooltip, TTM_TRACKPOSITION, WPARAM(0), LPARAM(at));
        };
        let x = menu.right + scale(GAP);
        place(x, item.top);
        SendMessageW(tooltip, TTM_TRACKACTIVATE, WPARAM(1), tool);
        self.showing = true;

        let mut shown = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let corner = POINT { x, y: item.top };
        if GetWindowRect(tooltip, &mut shown).is_err()
            || !GetMonitorInfoW(
                MonitorFromPoint(corner, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool()
        {
            return;
        }
        let work = monitor.rcWork;
        let (width, height) = (shown.right - shown.left, shown.bottom - shown.top);
        let fitted_x = if x + width > work.right {
            menu.left - scale(GAP) - width
        } else {
            x
        };
        let fitted_y = item.top.min(work.bottom - height).max(work.top);
        if (fitted_x, fitted_y) != (x, item.top) {
            place(fitted_x, fitted_y);
        }
    }

    unsafe fn hide(&mut self) {
        if let (true, Some(tooltip)) = (self.showing, self.tooltip) {
            let tool = tool_info();
            let tool = LPARAM(&tool as *const _ as isize);
            SendMessageW(tooltip, TTM_TRACKACTIVATE, WPARAM(0), tool);
        }
        self.showing = false;
    }

    /// The tooltip window, created the first time one is needed.
    unsafe fn tooltip(&mut self) -> Option<HWND> {
        if self.tooltip.is_none() {
            let tooltip = CreateWindowExW(
                // Click-through, and it must never take focus from the menu.
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                TOOLTIPS_CLASSW,
                w!(""),
                WS_POPUP | WINDOW_STYLE(TTS_NOPREFIX | TTS_ALWAYSTIP),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                None,
                None,
            )
            .ok()?;
            let tool = tool_info();
            let added = SendMessageW(
                tooltip,
                TTM_ADDTOOLW,
                WPARAM(0),
                LPARAM(&tool as *const _ as isize),
            );
            if added.0 == 0 {
                let _ = DestroyWindow(tooltip);
                return None;
            }
            self.tooltip = Some(tooltip);
        }
        self.tooltip
    }
}

fn apps_use_dark_theme() -> bool {
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

/// The one "tool" our tooltip has: not tied to a window, placed by hand.
fn tool_info() -> TTTOOLINFOW {
    TTTOOLINFOW {
        // Without the trailing `lpReserved`: the size every version of the
        // common controls accepts, whether or not the exe has a v6 manifest.
        cbSize: (std::mem::size_of::<TTTOOLINFOW>() - std::mem::size_of::<usize>()) as u32,
        uFlags: TTF_TRACK | TTF_ABSOLUTE,
        ..Default::default()
    }
}

/// This thread's visible menu windows, each with the menu it shows.
unsafe fn open_menu_windows() -> Vec<(isize, isize)> {
    let thread = GetCurrentThreadId();
    let mut open = Vec::new();
    let mut window = HWND::default();
    // "#32768" is the system class of every popup menu window.
    while let Ok(next) = FindWindowExW(None, window, w!("#32768"), None) {
        if next.0.is_null() {
            break;
        }
        window = next;
        if GetWindowThreadProcessId(window, None) == thread && IsWindowVisible(window).as_bool() {
            let hmenu = SendMessageW(window, MN_GETHMENU, WPARAM(0), LPARAM(0)).0;
            open.push((window.0 as isize, hmenu));
        }
    }
    open
}

/// The highlighted item of `menu` (or of its open folder), if it has a tooltip.
unsafe fn highlighted(menu: &Registered, open: &[(isize, isize)]) -> Option<Hover> {
    let window_of = |hmenu: isize| {
        open.iter()
            .find(|(_, shown)| *shown == hmenu)
            .map(|(window, _)| *window)
    };
    let is_highlighted = |hmenu: isize, position: usize| {
        let state = GetMenuState(HMENU(hmenu as _), position as u32, MF_BYPOSITION);
        state != u32::MAX && state & MF_HILITE.0 != 0
    };
    let hover = |window, hmenu, position: usize, text: &str| Hover {
        window,
        hmenu,
        position: position as u32,
        text: text.to_string(),
    };

    let window = window_of(menu.hmenu)?;
    let position = (0..menu.tips.len()).find(|&position| is_highlighted(menu.hmenu, position))?;
    match &menu.tips[position] {
        MenuTip::None => None,
        MenuTip::Text(text) => Some(hover(window, menu.hmenu, position, text)),
        MenuTip::Folder(tips) => {
            let folder = GetSubMenu(HMENU(menu.hmenu as _), position as i32).0 as isize;
            let window = window_of(folder)?;
            let position = (0..tips.len()).find(|&position| is_highlighted(folder, position))?;
            let text = tips[position].as_deref()?;
            Some(hover(window, folder, position, text))
        }
    }
}
