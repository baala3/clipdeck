//! Clipy-style native menus: the popup menus opened by the global hotkeys
//! (at the mouse cursor) and the tray icon's menu, plus what happens when one
//! of their items is chosen.

use crate::{settings, AppState};
use clip_engine::menu::{layout, HeldModifiers, MenuEntry, MenuItemModel};
use clip_engine::{ClipContent, Settings, ShortcutModifier};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// An invisible window that owns the popup menus; a menu needs a window to
/// attach to, and the Settings Window may be open on another screen.
pub const MENU_HOST_WINDOW: &str = "menu-host";
pub const TRAY_ID: &str = "main";

#[derive(Clone, Copy, PartialEq)]
pub enum MenuKind {
    /// History and Pinned together (Clipy's main menu); also the tray menu.
    Main,
    History,
    Pinned,
}

#[derive(Clone, Copy, PartialEq)]
enum List {
    History,
    Pinned,
}

/// What a menu item acts on. Items are matched back to Clips by content,
/// which is unique in both lists, so a Clip captured while the menu is open
/// can't make an item act on the wrong Clip.
#[derive(Clone)]
struct Target {
    list: List,
    text: String,
}

/// Item ids of the menus that currently exist, mapped to their targets.
#[derive(Default)]
pub struct MenuTargets {
    tray: HashMap<String, Target>,
    popup: HashMap<String, Target>,
}

const CLEAR_HISTORY: &str = "clear_history";
const EDIT_PINNED: &str = "edit_pinned";
const NEW_PINNED: &str = "new_pinned";
const SETTINGS: &str = "settings";
const PAUSE: &str = "pause";
const QUIT: &str = "quit";

/// Everything a menu shows, copied out so no lock is held while building it.
struct Snapshot {
    history: Vec<String>,
    pinned: Vec<String>,
    settings: Settings,
    paused: bool,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let texts = |clips: Vec<clip_engine::Clip>| -> Vec<String> {
        clips
            .into_iter()
            .rev()
            .filter_map(|clip| match clip.content {
                ClipContent::Text(text) => Some(text),
                ClipContent::Image(_) => None,
            })
            .collect()
    };
    let (history, pinned) = {
        let engine = state.engine.lock().unwrap();
        (texts(engine.history()), texts(engine.pinned()))
    };
    let settings = state.active_settings.lock().unwrap().clone();
    Snapshot {
        history,
        pinned,
        settings,
        paused: state.paused.load(Ordering::SeqCst),
    }
}

struct BuiltMenu {
    menu: Menu<Wry>,
    targets: HashMap<String, Target>,
    /// Number shortcut digit -> target, for the first ten items. Only Windows
    /// needs it: a digit pressed in an open menu there is caught by a keyboard
    /// hook, while macOS delivers it as a click on the item's accelerator.
    #[cfg(windows)]
    digits: HashMap<char, Target>,
}

fn build(
    app: &AppHandle,
    kind: MenuKind,
    snap: &Snapshot,
    id_prefix: &str,
) -> tauri::Result<BuiltMenu> {
    let menu = Menu::new(app)?;
    let mut built_targets = HashMap::new();
    #[cfg(windows)]
    let mut digits = HashMap::new();
    let mut section =
        |title: &str, list: List, texts: &[String], with_digits: bool| -> tauri::Result<()> {
            menu.append(&MenuItem::with_id(
                app,
                format!("{id_prefix}label:{title}"),
                title,
                false,
                None::<&str>,
            )?)?;
            if texts.is_empty() {
                menu.append(&MenuItem::with_id(
                    app,
                    format!("{id_prefix}empty:{title}"),
                    "(empty)",
                    false,
                    None::<&str>,
                )?)?;
                return Ok(());
            }
            let number_modifier = if with_digits {
                snap.settings.number_shortcut_modifier
            } else {
                ShortcutModifier::Off
            };
            let mut item = |model: &MenuItemModel| -> tauri::Result<MenuItem<Wry>> {
                let id = format!("{id_prefix}{}:{}", list_code(list), model.position);
                let target = Target {
                    list,
                    text: texts[model.position].clone(),
                };
                let accelerator = model
                    .shortcut_digit
                    .and_then(|digit| accelerator(number_modifier, digit));
                #[cfg(windows)]
                if accelerator.is_some() {
                    digits.insert(model.shortcut_digit.unwrap(), target.clone());
                }
                built_targets.insert(id.clone(), target);
                MenuItem::with_id(
                    app,
                    id,
                    escape_mnemonics(&model.label),
                    true,
                    accelerator.as_deref(),
                )
            };
            for entry in layout(texts, &snap.settings.menu_layout()) {
                match entry {
                    MenuEntry::Item(model) => menu.append(&item(&model)?)?,
                    MenuEntry::Folder { title, items } => {
                        let built: Vec<MenuItem<Wry>> =
                            items.iter().map(&mut item).collect::<tauri::Result<_>>()?;
                        let refs: Vec<&dyn IsMenuItem<Wry>> =
                            built.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
                        menu.append(&Submenu::with_items(app, title, true, &refs)?)?;
                    }
                }
            }
            Ok(())
        };

    if kind != MenuKind::Pinned {
        section("History", List::History, &snap.history, true)?;
    }
    if kind != MenuKind::History {
        if kind != MenuKind::Pinned {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        // Like Clipy, number shortcuts belong to History unless Pinned is shown alone.
        section(
            "Pinned",
            List::Pinned,
            &snap.pinned,
            kind == MenuKind::Pinned,
        )?;
        menu.append(&MenuItem::with_id(
            app,
            NEW_PINNED,
            "New Item...",
            true,
            None::<&str>,
        )?)?;
    }
    // Every menu ends with the app actions, so they're reachable from any
    // hotkey, not just the tray icon.
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        CLEAR_HISTORY,
        "Clear History",
        !snap.history.is_empty(),
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        EDIT_PINNED,
        "Edit Pinned...",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        SETTINGS,
        "Settings...",
        true,
        None::<&str>,
    )?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        PAUSE,
        "Pause capture",
        true,
        snap.paused,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        QUIT,
        "Quit Clipdeck",
        true,
        None::<&str>,
    )?)?;
    Ok(BuiltMenu {
        menu,
        targets: built_targets,
        #[cfg(windows)]
        digits,
    })
}

fn list_code(list: List) -> &'static str {
    match list {
        List::History => "h",
        List::Pinned => "p",
    }
}

/// Shown next to the item (e.g. "Ctrl+1"); on macOS it's also what makes the
/// key work, while on Windows `MenuKeyHook` handles the key itself.
fn accelerator(modifier: ShortcutModifier, digit: char) -> Option<String> {
    match modifier {
        ShortcutModifier::CommandOrControl => Some(format!("CmdOrCtrl+{digit}")),
        ShortcutModifier::Alt => Some(format!("Alt+{digit}")),
        ShortcutModifier::Shift => Some(format!("Shift+{digit}")),
        ShortcutModifier::None => Some(digit.to_string()),
        ShortcutModifier::Off => None,
    }
}

/// Menu labels treat "&" as a mnemonic marker; Clip text should show it literally.
fn escape_mnemonics(label: &str) -> String {
    label.replace('&', "&&")
}

/// Opens a menu at the mouse cursor. Called from the global-shortcut handler,
/// which runs while the shortcut plugin holds an internal lock: a modal menu
/// opened right there would deadlock as soon as any hotkey fired while it was
/// open. So the menu is queued onto the event loop instead.
pub fn request_popup(app: &AppHandle, kind: MenuKind) {
    let app = app.clone();
    std::thread::spawn(move || {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || show_popup(&handle, kind));
    });
}

fn show_popup(app: &AppHandle, kind: MenuKind) {
    let state = app.state::<AppState>();
    if state.menu_open.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(windows)]
    let previous = {
        let previous = clip_windows::foreground_window();
        *state.previous_foreground.lock().unwrap() = previous;
        previous
    };

    let snap = snapshot(app);
    let built = build(app, kind, &snap, "m:");
    let host = app.get_webview_window(MENU_HOST_WINDOW);
    if let (Ok(built), Some(host)) = (built, host) {
        state.menu_targets.lock().unwrap().popup = built.targets;

        #[cfg(windows)]
        {
            let hook = clip_windows::MenuKeyHook::install(snap.settings.number_shortcut_modifier);
            if let Err(err) = host.popup_menu(&built.menu) {
                eprintln!("couldn't show the menu: {err}");
            }
            let picked = hook.as_ref().and_then(|hook| hook.picked_digit());
            drop(hook);
            // Give focus back right away; a chosen item pastes into it afterwards.
            clip_windows::restore_foreground(previous);
            if let Some(target) = picked.and_then(|digit| built.digits.get(&digit)) {
                paste(app, target, true);
            }
        }
        #[cfg(not(windows))]
        if let Err(err) = host.popup_menu(&built.menu) {
            eprintln!("couldn't show the menu: {err}");
        }
    }
    state.menu_open.store(false, Ordering::SeqCst);
}

/// Rebuilds the tray menu so it shows the current Clips and settings. Safe to
/// call from any thread; the work happens on the event loop.
pub fn refresh_tray_menu(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let snap = snapshot(&handle);
        match build(&handle, MenuKind::Main, &snap, "t:") {
            Ok(built) => {
                handle.state::<AppState>().menu_targets.lock().unwrap().tray = built.targets;
                if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                    let _ = tray.set_menu(Some(built.menu));
                }
            }
            Err(err) => eprintln!("couldn't build the tray menu: {err}"),
        }
        // The Settings Window lists pinned items; keep it current too.
        let _ = handle.emit_to(settings::SETTINGS_WINDOW, "pinned-changed", ());
    });
}

pub fn build_initial_tray_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let built = build(app, MenuKind::Main, &snapshot(app), "t:")?;
    app.state::<AppState>().menu_targets.lock().unwrap().tray = built.targets;
    Ok(built.menu)
}

pub fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        CLEAR_HISTORY => confirm_and_clear_history(app),
        EDIT_PINNED => settings::show_settings_window_at(app, Some("pinned")),
        // A menu can't hold a text box, so writing happens in Settings.
        NEW_PINNED => settings::show_settings_window_at(app, Some("pinned-new")),
        SETTINGS => settings::show_settings_window_at(app, None),
        PAUSE => {
            let state = app.state::<AppState>();
            state.paused.fetch_xor(true, Ordering::SeqCst);
            refresh_tray_menu(app);
        }
        QUIT => app.exit(0),
        _ => {
            let (from_tray, target) = {
                let state = app.state::<AppState>();
                let targets = state.menu_targets.lock().unwrap();
                match id.split_once(':') {
                    Some(("t", _)) => (true, targets.tray.get(id).cloned()),
                    _ => (false, targets.popup.get(id).cloned()),
                }
            };
            if let Some(target) = target {
                activate(app, &target, activation_modifiers(), !from_tray);
            }
        }
    }
}

fn activation_modifiers() -> HeldModifiers {
    #[cfg(windows)]
    return clip_windows::take_activation_modifiers();
    #[cfg(target_os = "macos")]
    return clip_macos::held_modifiers();
    #[cfg(not(any(windows, target_os = "macos")))]
    HeldModifiers::default()
}

/// A plain click pastes; with the delete or pin modifier held (Clipy's beta
/// options), it deletes or pins instead.
fn activate(app: &AppHandle, target: &Target, held: HeldModifiers, paste_into_previous: bool) {
    let settings = app
        .state::<AppState>()
        .active_settings
        .lock()
        .unwrap()
        .clone();
    if settings.delete_modifier.is_held(held) {
        with_target_index(app, target, |engine, list, index| match list {
            List::History => engine.delete(index),
            List::Pinned => engine.unpin(index),
        });
        refresh_tray_menu(app);
    } else if target.list == List::History && settings.pin_modifier.is_held(held) {
        with_target_index(app, target, |engine, _, index| engine.pin(index));
        refresh_tray_menu(app);
    } else {
        paste(app, target, paste_into_previous);
    }
}

/// Finds the target's current chronological index in its list and runs `f` on it.
fn with_target_index(
    app: &AppHandle,
    target: &Target,
    f: impl FnOnce(&mut crate::Engine, List, usize) -> bool,
) {
    let state = app.state::<AppState>();
    let mut engine = state.engine.lock().unwrap();
    let clips = match target.list {
        List::History => engine.history(),
        List::Pinned => engine.pinned(),
    };
    let wanted = ClipContent::Text(target.text.clone());
    if let Some(index) = clips.iter().position(|clip| clip.content == wanted) {
        f(&mut engine, target.list, index);
    }
}

/// Copies the target to the clipboard, moves a History Clip to the top, and on
/// Windows pastes into the app that was focused before the popup (ADR-0005).
/// From the tray there's no such app (the taskbar has focus), so it only copies.
fn paste(app: &AppHandle, target: &Target, paste_into_previous: bool) {
    if let Err(err) = app.clipboard().write_text(target.text.clone()) {
        eprintln!("couldn't write the clipboard: {err}");
        return;
    }
    if target.list == List::History {
        with_target_index(app, target, |engine, _, index| engine.promote(index));
    }
    refresh_tray_menu(app);

    #[cfg(windows)]
    if paste_into_previous {
        let previous = *app.state::<AppState>().previous_foreground.lock().unwrap();
        clip_windows::focus_window_and_paste(previous);
    }
    #[cfg(not(windows))]
    let _ = paste_into_previous;
}

/// Asks before clearing, like Clipy. The dialog blocks, so it runs off the event loop.
pub fn confirm_and_clear_history(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let confirmed = app
            .dialog()
            .message("Clear all History? Pinned items are kept.")
            .title("Clear History")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Clear History".into(),
                "Cancel".into(),
            ))
            .blocking_show();
        if confirmed {
            app.state::<AppState>()
                .engine
                .lock()
                .unwrap()
                .clear_history();
            refresh_tray_menu(&app);
        }
    });
}
