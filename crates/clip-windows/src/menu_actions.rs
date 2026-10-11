//! A Clip's actions menu: right-clicking a Clip in one of Clipdeck's native
//! menus, or pressing the Right arrow on it, opens a small menu beside it
//! (Edit, Pin, Delete...), while a plain click still pastes in one step.
//!
//! Win32 tells a menu's owner window about a right-click with
//! `WM_MENURBUTTONUP`, but that window belongs to Tauri (popups) or to the
//! tray-icon crate (tray menu), not to us. So a message filter watches the
//! menu's own input instead, finds the row it's about, and opens the actions
//! menu on top of the open one, which Windows allows with `TPM_RECURSE`.
//! Dismissing the actions menu leaves the Clips' menu open; choosing an
//! action closes both and reports it.

use crate::menu_tips::open_menu_windows;
use crate::MenuSlot;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, DestroyMenu, EndMenu, GetGUIThreadInfo,
    GetMenuItemRect, GetMenuState, GetSubMenu, MenuItemFromPoint, PostMessageW, SetWindowsHookExW,
    TrackPopupMenuEx, GUITHREADINFO, HMENU, MF_BYPOSITION, MF_HILITE, MF_STRING, MSG, MSGF_MENU,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RECURSE, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN,
    WH_MSGFILTER, WM_KEYDOWN, WM_RBUTTONUP,
};

const VK_ESCAPE: usize = 0x1B;
const VK_LEFT: usize = 0x25;
const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;

/// The actions one Clip's row of a menu offers.
pub struct MenuRowActions {
    /// The row's position in the menu.
    pub row: usize,
    /// For a row inside a folder: its position in the submenu at `row`.
    pub folder_row: Option<usize>,
    /// What the actions menu lists, top to bottom.
    pub labels: Vec<String>,
}

/// An action the user chose from a Clip's actions menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickedAction {
    pub slot: MenuSlot,
    pub row: usize,
    pub folder_row: Option<usize>,
    /// The action's position in the row's `labels`.
    pub action: usize,
}

struct Registered {
    hmenu: isize,
    rows: Vec<MenuRowActions>,
}

#[derive(Default)]
struct State {
    menus: [Option<Registered>; 2],
    hook_installed: bool,
    handler: Option<Rc<dyn Fn(PickedAction)>>,
}

/// A Clip's row, found under the mouse or the highlight.
struct Found {
    slot: MenuSlot,
    row: usize,
    folder_row: Option<usize>,
    labels: Vec<String>,
    /// The menu showing the row, and the row's place in it.
    hmenu: isize,
    position: u32,
}

// Menus are built, shown, and torn down on the main thread only.
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static OPEN: Cell<bool> = const { Cell::new(false) };
}

/// Says which rows of `hmenu` are Clips and what each one's actions menu
/// lists, replacing whatever menu held `slot` before. Call it on the thread
/// that shows the menu.
pub fn set_menu_actions(slot: MenuSlot, hmenu: isize, rows: Vec<MenuRowActions>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.menus[slot as usize] = Some(Registered { hmenu, rows });
        if !state.hook_installed {
            // Lives as long as the app, like the tooltips' hook.
            let hook = unsafe {
                SetWindowsHookExW(WH_MSGFILTER, Some(menu_filter), None, GetCurrentThreadId())
            };
            state.hook_installed = hook.is_ok();
        }
    });
}

/// Forgets the menu in `slot`, once it's gone.
pub fn clear_menu_actions(slot: MenuSlot) {
    STATE.with(|state| state.borrow_mut().menus[slot as usize] = None);
}

/// Sets what happens when an action is chosen. `handler` runs on the thread
/// that shows the menus, while the Clips' menu is still closing, so it should
/// only note the choice or queue the work.
pub fn on_menu_action(handler: impl Fn(PickedAction) + 'static) {
    STATE.with(|state| state.borrow_mut().handler = Some(Rc::new(handler)));
}

/// Whether a Clip's actions menu is open right now, on top of the Clips' menu.
pub(crate) fn actions_menu_is_open() -> bool {
    OPEN.with(Cell::get)
}

unsafe extern "system" fn menu_filter(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 {
        let msg = &*(lparam.0 as *const MSG);
        let key = (msg.message == WM_KEYDOWN).then_some(msg.wParam.0);
        if actions_menu_is_open() {
            // Left goes back to the Clips' menu, as it does from a submenu.
            if key == Some(VK_LEFT) {
                let _ = PostMessageW(msg.hwnd, WM_KEYDOWN, WPARAM(VK_ESCAPE), LPARAM(0));
                return LRESULT(1);
            }
        } else if msg.message == WM_RBUTTONUP {
            if let Some(found) = find(|menu, open| row_at(menu, open, msg.pt)) {
                show(found, msg.pt, false);
                return LRESULT(1);
            }
        } else if key == Some(VK_RIGHT) {
            if let Some(found) = find(|menu, open| highlighted_row(menu, open)) {
                let mut item = RECT::default();
                let hmenu = HMENU(found.hmenu as _);
                if GetMenuItemRect(None, hmenu, found.position, &mut item).is_ok() {
                    // Where a submenu of the row would open.
                    let beside = POINT {
                        x: item.right,
                        y: item.top,
                    };
                    show(found, beside, true);
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Looks for a Clip's row in the menus that are open, using `locate` to pick
/// the row of one menu: its `(row, folder_row)` and the native menu and
/// position showing it.
unsafe fn find(
    locate: impl Fn(&Registered, &[(isize, isize)]) -> Option<(usize, Option<usize>, isize, u32)>,
) -> Option<Found> {
    let open = open_menu_windows();
    STATE.with(|state| {
        let state = state.try_borrow().ok()?;
        [MenuSlot::Tray, MenuSlot::Popup]
            .into_iter()
            .find_map(|slot| {
                let menu = state.menus[slot as usize].as_ref()?;
                let (row, folder_row, hmenu, position) = locate(menu, &open)?;
                let actions = menu
                    .rows
                    .iter()
                    .find(|actions| actions.row == row && actions.folder_row == folder_row)?;
                Some(Found {
                    slot,
                    row,
                    folder_row,
                    labels: actions.labels.clone(),
                    hmenu,
                    position,
                })
            })
            .filter(|found| !found.labels.is_empty())
    })
}

fn is_open(open: &[(isize, isize)], hmenu: isize) -> bool {
    open.iter().any(|(_, shown)| *shown == hmenu)
}

/// The folders of `menu` that are open, as `(row, submenu)`.
unsafe fn open_folders(menu: &Registered, open: &[(isize, isize)]) -> Vec<(usize, isize)> {
    let mut folders: Vec<usize> = menu
        .rows
        .iter()
        .filter(|actions| actions.folder_row.is_some())
        .map(|actions| actions.row)
        .collect();
    folders.dedup();
    folders
        .into_iter()
        .map(|row| {
            let folder = GetSubMenu(HMENU(menu.hmenu as _), row as i32);
            (row, folder.0 as isize)
        })
        .filter(|(_, folder)| is_open(open, *folder))
        .collect()
}

/// The row of `menu`, or of one of its open folders, at a point on screen.
unsafe fn row_at(
    menu: &Registered,
    open: &[(isize, isize)],
    point: POINT,
) -> Option<(usize, Option<usize>, isize, u32)> {
    if !is_open(open, menu.hmenu) {
        return None;
    }
    let position_in = |hmenu: isize| {
        let position = MenuItemFromPoint(None, HMENU(hmenu as _), point);
        (position >= 0).then_some(position as u32)
    };
    for (row, folder) in open_folders(menu, open) {
        if let Some(position) = position_in(folder) {
            return Some((row, Some(position as usize), folder, position));
        }
    }
    let position = position_in(menu.hmenu)?;
    Some((position as usize, None, menu.hmenu, position))
}

/// The highlighted row of `menu`, or of its open folder if the highlight is
/// in there.
unsafe fn highlighted_row(
    menu: &Registered,
    open: &[(isize, isize)],
) -> Option<(usize, Option<usize>, isize, u32)> {
    if !is_open(open, menu.hmenu) {
        return None;
    }
    let highlight_in = |hmenu: isize, rows: usize| {
        (0..rows as u32).find(|&position| {
            let state = GetMenuState(HMENU(hmenu as _), position, MF_BYPOSITION);
            state != u32::MAX && state & MF_HILITE.0 != 0
        })
    };
    // Only one folder is open at a time.
    if let Some((row, folder)) = open_folders(menu, open).into_iter().next() {
        let rows = menu
            .rows
            .iter()
            .filter(|actions| actions.row == row)
            .filter_map(|actions| actions.folder_row)
            .max()
            .map_or(0, |last| last + 1);
        // An open folder with nothing highlighted yet: Right belongs to it.
        let position = highlight_in(folder, rows)?;
        return Some((row, Some(position as usize), folder, position));
    }
    let rows = menu.rows.iter().map(|actions| actions.row + 1).max()?;
    let position = highlight_in(menu.hmenu, rows)?;
    Some((position as usize, None, menu.hmenu, position))
}

/// Shows `found`'s actions at `at` and waits for a choice. `by_keyboard`
/// highlights the first action, so the arrow keys and Enter carry on from it.
unsafe fn show(found: Found, at: POINT, by_keyboard: bool) {
    let Some(owner) = menu_owner() else {
        return;
    };
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    for (index, label) in found.labels.iter().enumerate() {
        let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        // Ids start at 1: 0 is what a dismissed menu returns.
        let _ = AppendMenuW(menu, MF_STRING, index + 1, PCWSTR(label.as_ptr()));
    }
    if by_keyboard {
        let _ = PostMessageW(owner, WM_KEYDOWN, WPARAM(VK_DOWN), LPARAM(0));
    }
    OPEN.with(|open| open.set(true));
    let flags =
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_NONOTIFY | TPM_RECURSE | TPM_RIGHTBUTTON;
    let chosen = TrackPopupMenuEx(menu, flags.0, at.x, at.y, owner, None).0;
    OPEN.with(|open| open.set(false));
    let _ = DestroyMenu(menu);

    if chosen > 0 {
        let _ = EndMenu();
        let handler = STATE.with(|state| state.borrow().handler.clone());
        if let Some(handler) = handler {
            handler(PickedAction {
                slot: found.slot,
                row: found.row,
                folder_row: found.folder_row,
                action: chosen as usize - 1,
            });
        }
    }
}

/// The window the open menu belongs to. Windows only lets a menu open on top
/// of another when both belong to the same window.
unsafe fn menu_owner() -> Option<HWND> {
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    GetGUIThreadInfo(GetCurrentThreadId(), &mut info).ok()?;
    (!info.hwndMenuOwner.0.is_null()).then_some(info.hwndMenuOwner)
}
