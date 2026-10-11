#![cfg(target_os = "windows")]
//! Opens a real popup menu and asks for a Clip's actions menu with the
//! keyboard and with a right-click. The keys are posted to this thread's own
//! queue, so nothing is typed into whatever else is running. The right-click
//! is a real one, since a posted one carries no position: the mouse cursor is
//! put over the row, which has the mouse captured, and put back afterwards.

use clip_windows::{
    clear_menu_actions, on_menu_action, set_menu_actions, MenuRowActions, MenuSlot, PickedAction,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, EndMenu,
    FindWindowExW, GetCursorPos, GetMenuItemCount, GetMenuItemRect, GetMenuStringW,
    GetWindowThreadProcessId, IsWindowVisible, KillTimer, PostMessageW, SendMessageW, SetCursorPos,
    SetTimer, TrackPopupMenu, HMENU, MF_BYPOSITION, MF_GRAYED, MF_POPUP, MF_STRING, TPM_LEFTALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, WINDOW_EX_STYLE, WM_KEYDOWN, WS_POPUP,
};

const MN_GETHMENU: u32 = 0x01E1;
const VK_RETURN: usize = 0x0D;
const VK_LEFT: usize = 0x25;
const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;
/// Long enough for a menu to open or close between steps.
const STEP_MS: u32 = 300;

/// Two menus can't be open on one desktop at once.
static ONE_MENU_AT_A_TIME: Mutex<()> = Mutex::new(());

type Step = fn(usize, HMENU) -> bool;

thread_local! {
    static OWNER: RefCell<HWND> = RefCell::new(HWND::default());
    static MENU: RefCell<HMENU> = RefCell::new(HMENU::default());
    static STEPS: RefCell<Option<Step>> = const { RefCell::new(None) };
    static STEP: RefCell<usize> = const { RefCell::new(0) };
    /// The open menus' labels, innermost last, as they were before each step.
    static SEEN: RefCell<Vec<Vec<Vec<String>>>> = const { RefCell::new(Vec::new()) };
}

unsafe fn press(key: usize) {
    let owner = OWNER.with(|owner| *owner.borrow());
    PostMessageW(owner, WM_KEYDOWN, WPARAM(key), LPARAM(0)).unwrap();
}

/// Puts the mouse on a row of `menu` and right-clicks there.
unsafe fn right_click(menu: HMENU, position: u32) {
    let mut item = RECT::default();
    GetMenuItemRect(None, menu, position, &mut item).unwrap();
    SetCursorPos((item.left + item.right) / 2, (item.top + item.bottom) / 2).unwrap();
    let button = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    SendInput(
        &[button(MOUSEEVENTF_RIGHTDOWN), button(MOUSEEVENTF_RIGHTUP)],
        std::mem::size_of::<INPUT>() as i32,
    );
}

/// The labels of every menu this thread is showing, in the order they opened.
unsafe fn open_menus() -> Vec<Vec<String>> {
    let mut menus = Vec::new();
    let mut window = HWND::default();
    while let Ok(next) = FindWindowExW(None, window, w!("#32768"), None) {
        if next.0.is_null() {
            break;
        }
        window = next;
        if GetWindowThreadProcessId(window, None) != GetCurrentThreadId()
            || !IsWindowVisible(window).as_bool()
        {
            continue;
        }
        let menu = HMENU(SendMessageW(window, MN_GETHMENU, WPARAM(0), LPARAM(0)).0 as _);
        let labels = (0..GetMenuItemCount(menu).max(0) as u32)
            .map(|position| {
                let mut label = [0u16; 64];
                let len = GetMenuStringW(menu, position, Some(&mut label), MF_BYPOSITION);
                String::from_utf16_lossy(&label[..len as usize])
            })
            .collect();
        // Windows lists the newest window first.
        menus.insert(0, labels);
    }
    menus
}

unsafe extern "system" fn walk(_: HWND, _: u32, timer: usize, _: u32) {
    let step = STEP.with(|step| step.replace_with(|step| *step + 1));
    SEEN.with(|seen| seen.borrow_mut().push(open_menus()));
    let steps = STEPS.with(|steps| steps.borrow().unwrap());
    let menu = MENU.with(|menu| *menu.borrow());
    if !steps(step, menu) {
        let _ = KillTimer(None, timer);
        let _ = EndMenu();
    }
}

struct Outcome {
    /// What the Clips' menu itself returned: 0 unless one of its rows was chosen.
    command: i32,
    picked: Vec<PickedAction>,
    seen: Vec<Vec<Vec<String>>>,
}

/// Shows a menu with a header, two Clips and a folder of two more, and runs
/// `steps` against it, one every `STEP_MS`, until it returns false.
unsafe fn run(steps: Step) -> Outcome {
    let _guard = ONE_MENU_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    AppendMenuW(folder, MF_STRING, 10, w!("1. gamma")).unwrap();
    AppendMenuW(folder, MF_STRING, 11, w!("2. delta")).unwrap();
    let menu = CreatePopupMenu().unwrap();
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 1, w!("History")).unwrap();
    AppendMenuW(menu, MF_STRING, 2, w!("1. alpha")).unwrap();
    AppendMenuW(menu, MF_STRING, 3, w!("2. beta")).unwrap();
    AppendMenuW(menu, MF_POPUP, folder.0 as usize, w!("3 - 4")).unwrap();
    MENU.with(|cell| *cell.borrow_mut() = menu);

    let actions = |row, folder_row, labels: &[&str]| MenuRowActions {
        row,
        folder_row,
        labels: labels.iter().map(|label| label.to_string()).collect(),
    };
    set_menu_actions(
        MenuSlot::Popup,
        menu.0 as isize,
        vec![
            actions(1, None, &["Edit", "Pin", "Delete"]),
            actions(2, None, &["Pin", "Delete"]),
            actions(3, Some(0), &["Edit", "Pin", "Delete"]),
            actions(3, Some(1), &["Edit", "Pin", "Delete"]),
        ],
    );
    let picked = Rc::new(RefCell::new(Vec::new()));
    let sink = picked.clone();
    on_menu_action(move |action| sink.borrow_mut().push(action));

    STEPS.with(|cell| *cell.borrow_mut() = Some(steps));
    STEP.with(|step| *step.borrow_mut() = 0);
    SEEN.with(|seen| seen.borrow_mut().clear());
    SetTimer(None, 0, STEP_MS, Some(walk));
    let command = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        60,
        60,
        0,
        owner,
        None,
    )
    .0;

    clear_menu_actions(MenuSlot::Popup);
    let _ = DestroyMenu(menu);
    let _ = DestroyWindow(owner);
    let picked = picked.borrow().clone();
    Outcome {
        command,
        picked,
        seen: SEEN.with(|seen| seen.borrow().clone()),
    }
}

fn labels(menu: &[&str]) -> Vec<String> {
    menu.iter().map(|label| label.to_string()).collect()
}

const CLIPS: [&str; 4] = ["History", "1. alpha", "2. beta", "3 - 4"];

#[test]
fn the_right_arrow_on_a_clip_opens_its_actions_and_left_goes_back() {
    let outcome = unsafe {
        run(|step, _| {
            match step {
                // Onto the disabled header (the keyboard stops there too),
                // then "1. alpha", then ask for its actions.
                0 => {
                    press(VK_DOWN);
                    press(VK_DOWN);
                    press(VK_RIGHT);
                }
                1 => press(VK_LEFT),
                // Again, and this time choose the highlighted first action's
                // neighbor, "Pin".
                2 => press(VK_RIGHT),
                3 => {
                    press(VK_DOWN);
                    press(VK_RETURN);
                }
                _ => return false,
            }
            true
        })
    };

    assert_eq!(
        outcome.seen[1],
        vec![labels(&CLIPS), labels(&["Edit", "Pin", "Delete"])],
        "the actions menu opens on top of the Clips' menu"
    );
    assert_eq!(
        outcome.seen[2],
        vec![labels(&CLIPS)],
        "Left closes the actions menu and leaves the Clips' menu open"
    );
    assert_eq!(
        outcome.picked,
        vec![PickedAction {
            slot: MenuSlot::Popup,
            row: 1,
            folder_row: None,
            action: 1,
        }]
    );
    assert_eq!(outcome.command, 0, "the Clip itself wasn't chosen");
    assert!(
        outcome.seen.len() <= 5,
        "choosing an action closes the Clips' menu too: {:?}",
        outcome.seen
    );
}

#[test]
fn the_right_arrow_opens_a_folder_first_and_then_the_actions_of_a_clip_inside_it() {
    let outcome = unsafe {
        run(|step, _| {
            match step {
                // Down to the folder, into it, and Right again on its first Clip.
                0 => {
                    for _ in 0..4 {
                        press(VK_DOWN);
                    }
                    press(VK_RIGHT);
                }
                1 => press(VK_RIGHT),
                2 => press(VK_RETURN),
                _ => return false,
            }
            true
        })
    };

    assert_eq!(
        outcome.seen[1],
        vec![labels(&CLIPS), labels(&["1. gamma", "2. delta"])],
        "Right on a folder opens the folder, as it always did"
    );
    assert_eq!(
        outcome.seen[2].last(),
        Some(&labels(&["Edit", "Pin", "Delete"]))
    );
    assert_eq!(
        outcome.picked,
        vec![PickedAction {
            slot: MenuSlot::Popup,
            row: 3,
            folder_row: Some(0),
            action: 0,
        }]
    );
}

#[test]
fn right_clicking_a_clip_opens_its_actions_and_other_rows_ignore_it() {
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor).unwrap() };
    let outcome = unsafe {
        run(|step, menu| {
            match step {
                // The header isn't a Clip.
                0 => right_click(menu, 0),
                // "2. beta" is; choose its second action, "Delete".
                1 => right_click(menu, 2),
                2 => {
                    press(VK_DOWN);
                    press(VK_DOWN);
                    press(VK_RETURN);
                }
                _ => return false,
            }
            true
        })
    };
    unsafe { SetCursorPos(cursor.x, cursor.y).unwrap() };

    assert_eq!(
        outcome.seen[1],
        vec![labels(&CLIPS)],
        "nothing opens for a row that isn't a Clip"
    );
    assert_eq!(
        outcome.seen[2],
        vec![labels(&CLIPS), labels(&["Pin", "Delete"])]
    );
    assert_eq!(
        outcome.picked,
        vec![PickedAction {
            slot: MenuSlot::Popup,
            row: 2,
            folder_row: None,
            action: 1,
        }]
    );
    assert_eq!(outcome.command, 0, "the Clip itself wasn't chosen");
}
