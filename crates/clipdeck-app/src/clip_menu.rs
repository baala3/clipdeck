//! Clipy-style native menus: the popup menus opened by the global hotkeys
//! (at the mouse cursor) and the tray icon's menu, plus what happens when one
//! of their items is chosen.

use crate::{settings, AppState};
#[cfg(windows)]
use clip_engine::menu::{history_clip_actions, pinned_clip_actions};
use clip_engine::menu::{layout, ClipAction, HeldModifiers, MenuEntry, MenuItemModel, MenuTip};
use clip_engine::{ClipContent, Rgba, Settings, ShortcutModifier};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
#[cfg(not(windows))]
use tauri::menu::IconMenuItem;
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
    content: ClipContent,
}

/// Where a Clip's row sits in a built menu: its position, and for a row in a
/// folder, its position inside the folder at that position.
#[cfg(windows)]
type Row = (usize, Option<usize>);

/// What each Clip's row acts on, and the actions its actions menu lists.
#[cfg(windows)]
type ClipRows = HashMap<Row, (Target, Vec<ClipAction>)>;

/// Item ids of the menus that currently exist, mapped to their targets.
#[derive(Default)]
pub struct MenuTargets {
    tray: HashMap<String, Target>,
    popup: HashMap<String, Target>,
    /// The same menus' Clips by row, which is how Windows reports an action
    /// chosen from a Clip's actions menu (ADR-0011).
    #[cfg(windows)]
    tray_rows: ClipRows,
    #[cfg(windows)]
    popup_rows: ClipRows,
    /// The action chosen from the open popup's actions menu, carried out
    /// once the popup has closed.
    #[cfg(windows)]
    popup_action: Option<(Target, ClipAction)>,
}

const CLEAR_HISTORY: &str = "clear_history";
const EDIT_PINNED: &str = "edit_pinned";
const NEW_PINNED: &str = "new_pinned";
const SETTINGS: &str = "settings";
const PAUSE: &str = "pause";
const QUIT: &str = "quit";

/// Everything a menu shows, copied out so no lock is held while building it.
struct Snapshot {
    history: Vec<ClipContent>,
    pinned: Vec<ClipContent>,
    /// The thumbnail of every image Clip in either list, by digest.
    thumbnails: HashMap<String, Rgba>,
    settings: Settings,
    paused: bool,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let newest_first = |clips: Vec<clip_engine::Clip>| -> Vec<ClipContent> {
        clips.into_iter().rev().map(|clip| clip.content).collect()
    };
    let (history, pinned, thumbnail_pngs) = {
        let engine = state.engine.lock().unwrap();
        let history = newest_first(engine.history());
        let pinned = newest_first(engine.pinned());
        let thumbnail_pngs: HashMap<String, Vec<u8>> = history
            .iter()
            .chain(&pinned)
            .filter_map(|content| match content {
                ClipContent::Image(image) => Some(image),
                ClipContent::Text(_) => None,
            })
            .filter_map(|image| Some((image.digest.clone(), engine.image_thumbnail_png(image)?)))
            .collect();
        (history, pinned, thumbnail_pngs)
    };
    let thumbnails = thumbnail_pngs
        .into_iter()
        .filter_map(|(digest, png)| Some((digest, Rgba::from_png(&png)?)))
        .collect();
    let settings = state.active_settings.lock().unwrap().clone();
    Snapshot {
        history,
        pinned,
        thumbnails,
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
    /// What hovering each row shows. Only Windows can show it (see ADR-0008).
    #[cfg_attr(not(windows), allow(dead_code))]
    tips: Vec<MenuTip>,
    /// The thumbnails of the image Clips' rows. Windows sets them on the
    /// native menu; elsewhere each one is its item's icon already.
    #[cfg(windows)]
    images: Vec<clip_windows::MenuImage>,
    #[cfg(windows)]
    clip_rows: ClipRows,
}

/// Appends rows to a menu while keeping each row's tooltip at its position.
struct Rows<'a> {
    menu: &'a Menu<Wry>,
    tips: Vec<MenuTip>,
}

impl Rows<'_> {
    fn push(&mut self, item: &dyn IsMenuItem<Wry>, tip: MenuTip) -> tauri::Result<()> {
        self.tips.push(tip);
        self.menu.append(item)
    }

    fn plain(&mut self, item: &dyn IsMenuItem<Wry>) -> tauri::Result<()> {
        self.push(item, MenuTip::None)
    }
}

fn build(
    app: &AppHandle,
    kind: MenuKind,
    snap: &Snapshot,
    id_prefix: &str,
) -> tauri::Result<BuiltMenu> {
    let menu = Menu::new(app)?;
    let mut rows = Rows {
        menu: &menu,
        tips: Vec::new(),
    };
    let mut built_targets = HashMap::new();
    #[cfg(windows)]
    let mut digits = HashMap::new();
    #[cfg(windows)]
    let mut images = Vec::new();
    #[cfg(windows)]
    let mut clip_rows = ClipRows::new();
    let mut section = |rows: &mut Rows,
                       title: &str,
                       list: List,
                       clips: &[ClipContent],
                       with_digits: bool|
     -> tauri::Result<()> {
        rows.plain(&MenuItem::with_id(
            app,
            format!("{id_prefix}label:{title}"),
            title,
            false,
            None::<&str>,
        )?)?;
        if clips.is_empty() {
            return rows.plain(&MenuItem::with_id(
                app,
                format!("{id_prefix}empty:{title}"),
                "(empty)",
                false,
                None::<&str>,
            )?);
        }
        let number_modifier = if with_digits {
            snap.settings.number_shortcut_modifier
        } else {
            ShortcutModifier::Off
        };
        let mut item = |model: &MenuItemModel| -> tauri::Result<Box<dyn IsMenuItem<Wry>>> {
            let id = format!("{id_prefix}{}:{}", list_code(list), model.position);
            let target = Target {
                list,
                content: clips[model.position].clone(),
            };
            let accelerator = model
                .shortcut_digit
                .and_then(|digit| accelerator(number_modifier, digit));
            #[cfg(windows)]
            if accelerator.is_some() {
                digits.insert(model.shortcut_digit.unwrap(), target.clone());
            }
            built_targets.insert(id.clone(), target);
            let label = escape_mnemonics(&model.label);
            #[cfg(not(windows))]
            if let Some(thumbnail) = thumbnail(snap, &clips[model.position]) {
                let icon =
                    tauri::image::Image::new(&thumbnail.pixels, thumbnail.width, thumbnail.height);
                return Ok(Box::new(IconMenuItem::with_id(
                    app,
                    id,
                    label,
                    true,
                    Some(icon),
                    accelerator.as_deref(),
                )?));
            }
            Ok(Box::new(MenuItem::with_id(
                app,
                id,
                label,
                true,
                accelerator.as_deref(),
            )?))
        };
        for entry in layout(clips, &snap.settings.menu_layout()) {
            // Rows are counted by their tooltips, one per row.
            #[cfg(windows)]
            let row = rows.tips.len();
            // What Windows adds to a Clip's row: a thumbnail, and its actions.
            #[cfg(windows)]
            let mut clip_row = |model: &MenuItemModel, folder_row: Option<usize>| {
                let content = &clips[model.position];
                if let Some(thumbnail) = thumbnail(snap, content) {
                    images.push(clip_windows::MenuImage {
                        row,
                        folder_row,
                        image: thumbnail.clone(),
                    });
                }
                let actions = match list {
                    List::History => history_clip_actions(snap.pinned.contains(content)),
                    List::Pinned => pinned_clip_actions(content),
                };
                let target = Target {
                    list,
                    content: content.clone(),
                };
                clip_rows.insert((row, folder_row), (target, actions));
            };
            match entry {
                MenuEntry::Item(model) => {
                    #[cfg(windows)]
                    clip_row(&model, None);
                    let tip = tip(&model, &clips[model.position]);
                    rows.push(item(&model)?.as_ref(), tip)?;
                }
                MenuEntry::Folder { title, items } => {
                    #[cfg(windows)]
                    for (folder_row, model) in items.iter().enumerate() {
                        clip_row(model, Some(folder_row));
                    }
                    let built: Vec<Box<dyn IsMenuItem<Wry>>> =
                        items.iter().map(&mut item).collect::<tauri::Result<_>>()?;
                    let refs: Vec<&dyn IsMenuItem<Wry>> =
                        built.iter().map(|item| item.as_ref()).collect();
                    rows.push(
                        &Submenu::with_items(app, title, true, &refs)?,
                        MenuTip::Folder(
                            items
                                .iter()
                                .map(|model| tip(model, &clips[model.position]))
                                .collect(),
                        ),
                    )?;
                }
            }
        }
        Ok(())
    };

    if kind != MenuKind::Pinned {
        section(&mut rows, "History", List::History, &snap.history, true)?;
    }
    if kind != MenuKind::History {
        if kind != MenuKind::Pinned {
            rows.plain(&PredefinedMenuItem::separator(app)?)?;
        }
        // Like Clipy, number shortcuts belong to History unless Pinned is shown alone.
        section(
            &mut rows,
            "Pinned",
            List::Pinned,
            &snap.pinned,
            kind == MenuKind::Pinned,
        )?;
    }
    // Every menu ends with the app actions, so they're reachable from any
    // hotkey, not just the tray icon. They sit in one submenu so that opening
    // a menu shows Clips and little else.
    rows.plain(&PredefinedMenuItem::separator(app)?)?;
    let more = Submenu::with_id(
        app,
        format!("{id_prefix}more"),
        if snap.paused {
            "More (capture paused)"
        } else {
            "More"
        },
        true,
    )?;
    if kind != MenuKind::History {
        more.append(&MenuItem::with_id(
            app,
            NEW_PINNED,
            "New Pinned Item",
            true,
            None::<&str>,
        )?)?;
    }
    more.append(&MenuItem::with_id(
        app,
        EDIT_PINNED,
        "Edit Pinned",
        true,
        None::<&str>,
    )?)?;
    more.append(&MenuItem::with_id(
        app,
        CLEAR_HISTORY,
        "Clear History",
        !snap.history.is_empty(),
        None::<&str>,
    )?)?;
    more.append(&PredefinedMenuItem::separator(app)?)?;
    more.append(&CheckMenuItem::with_id(
        app,
        PAUSE,
        "Pause capture",
        true,
        snap.paused,
        None::<&str>,
    )?)?;
    more.append(&MenuItem::with_id(
        app,
        SETTINGS,
        "Settings",
        true,
        None::<&str>,
    )?)?;
    more.append(&PredefinedMenuItem::separator(app)?)?;
    more.append(&MenuItem::with_id(
        app,
        QUIT,
        "Quit Clipdeck",
        true,
        None::<&str>,
    )?)?;
    rows.plain(&more)?;
    let tips = rows.tips;
    Ok(BuiltMenu {
        menu,
        targets: built_targets,
        #[cfg(windows)]
        digits,
        tips,
        #[cfg(windows)]
        images,
        #[cfg(windows)]
        clip_rows,
    })
}

/// What resting on a Clip's row shows: an image in full, or all of a text
/// whose label was shortened.
fn tip(model: &MenuItemModel, content: &ClipContent) -> MenuTip {
    match content {
        ClipContent::Image(image) => MenuTip::Image(image.digest.clone()),
        ClipContent::Text(_) => model.tooltip.clone().map_or(MenuTip::None, MenuTip::Text),
    }
}

/// The thumbnail to show beside `content` in a menu, if it's an image.
fn thumbnail<'a>(snap: &'a Snapshot, content: &ClipContent) -> Option<&'a Rgba> {
    match content {
        ClipContent::Image(image) => snap.thumbnails.get(&image.digest),
        ClipContent::Text(_) => None,
    }
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
            decorate(
                app,
                clip_windows::MenuSlot::Popup,
                &built.menu,
                built.tips,
                built.images,
                built.clip_rows,
            );
            let hook = clip_windows::MenuKeyHook::install(snap.settings.number_shortcut_modifier);
            if let Err(err) = host.popup_menu(&built.menu) {
                eprintln!("couldn't show the menu: {err}");
            }
            let picked = hook.as_ref().and_then(|hook| hook.picked_digit());
            drop(hook);
            clip_windows::clear_menu_tips(clip_windows::MenuSlot::Popup);
            clip_windows::clear_menu_images(clip_windows::MenuSlot::Popup);
            clip_windows::clear_menu_actions(clip_windows::MenuSlot::Popup);
            // Give focus back right away; a chosen item pastes into it afterwards.
            clip_windows::restore_foreground(previous);
            let action = state.menu_targets.lock().unwrap().popup_action.take();
            if let Some((target, action)) = action {
                // The Enter or click that chose it was in the actions menu.
                clip_windows::take_activation_modifiers();
                perform(app, &target, action, true);
            } else if let Some(target) = picked.and_then(|digit| built.digits.get(&digit)) {
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
                #[cfg(windows)]
                decorate(
                    &handle,
                    clip_windows::MenuSlot::Tray,
                    &built.menu,
                    built.tips,
                    built.images,
                    built.clip_rows,
                );
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
    #[cfg(windows)]
    decorate(
        app,
        clip_windows::MenuSlot::Tray,
        &built.menu,
        built.tips,
        built.images,
        built.clip_rows,
    );
    Ok(built.menu)
}

/// Hands a menu's tooltips, thumbnails, and Clips' actions to the Windows
/// adapter, which shows the tooltips on hover, draws the thumbnails in their
/// rows, and opens a Clip's actions menu on a right-click or the Right arrow.
/// Must run on the main thread, where menus are shown.
#[cfg(windows)]
fn decorate(
    app: &AppHandle,
    slot: clip_windows::MenuSlot,
    menu: &Menu<Wry>,
    tips: Vec<MenuTip>,
    images: Vec<clip_windows::MenuImage>,
    clip_rows: ClipRows,
) {
    use tauri::menu::ContextMenu;
    if let Ok(hmenu) = menu.hpopupmenu() {
        clip_windows::set_menu_tips(slot, hmenu, tips);
        clip_windows::set_menu_images(slot, hmenu, images);
        // From the tray there's no app to paste into, so choosing only copies.
        let pastes = slot == clip_windows::MenuSlot::Popup;
        let actions = clip_rows
            .iter()
            .map(
                |(&(row, folder_row), (target, actions))| clip_windows::MenuRowActions {
                    row,
                    folder_row,
                    labels: actions
                        .iter()
                        .map(|action| action.label(target.list == List::Pinned, pastes).into())
                        .collect(),
                },
            )
            .collect();
        clip_windows::set_menu_actions(slot, hmenu, actions);
    }
    let state = app.state::<AppState>();
    let mut targets = state.menu_targets.lock().unwrap();
    match slot {
        clip_windows::MenuSlot::Tray => targets.tray_rows = clip_rows,
        clip_windows::MenuSlot::Popup => {
            targets.popup_rows = clip_rows;
            targets.popup_action = None;
        }
    }
}

/// Lets the menus show an image Clip in full when its row is rested on. Call
/// it once, on the main thread.
#[cfg(windows)]
pub fn provide_image_previews(app: &AppHandle) {
    let app = app.clone();
    clip_windows::on_image_preview(move |digest| {
        let png = {
            let state = app.state::<AppState>();
            let engine = state.engine.lock().unwrap();
            let image = engine
                .history()
                .into_iter()
                .chain(engine.pinned())
                .find_map(|clip| match clip.content {
                    ClipContent::Image(image) if image.digest == digest => Some(image),
                    _ => None,
                })?;
            engine.image_png(&image)?
        };
        Rgba::from_png(&png)
    });
}

/// Starts carrying out what's chosen from a Clip's actions menu. Call it
/// once, on the main thread.
#[cfg(windows)]
pub fn watch_menu_actions(app: &AppHandle) {
    let app = app.clone();
    clip_windows::on_menu_action(move |picked| {
        let state = app.state::<AppState>();
        let mut targets = state.menu_targets.lock().unwrap();
        let rows = match picked.slot {
            clip_windows::MenuSlot::Tray => &targets.tray_rows,
            clip_windows::MenuSlot::Popup => &targets.popup_rows,
        };
        let Some((target, action)) = rows
            .get(&(picked.row, picked.folder_row))
            .and_then(|(target, actions)| Some((target.clone(), *actions.get(picked.action)?)))
        else {
            return;
        };
        match picked.slot {
            // `show_popup` is still waiting for the menu to close, and gives
            // focus back before anything is pasted.
            clip_windows::MenuSlot::Popup => targets.popup_action = Some((target, action)),
            // Nothing waits on the tray menu, so the work is queued behind it.
            clip_windows::MenuSlot::Tray => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let handle = app.clone();
                    let _ =
                        app.run_on_main_thread(move || perform(&handle, &target, action, false));
                });
            }
        }
    });
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
    let action = if settings.delete_modifier.is_held(held) {
        ClipAction::Delete
    } else if target.list == List::History && settings.pin_modifier.is_held(held) {
        ClipAction::Pin
    } else {
        ClipAction::Paste
    };
    perform(app, target, action, paste_into_previous);
}

/// Carries out `action` on the target, however it was asked for: a click, a
/// modifier+click, or the Clip's actions menu.
fn perform(app: &AppHandle, target: &Target, action: ClipAction, paste_into_previous: bool) {
    match action {
        ClipAction::Paste => paste(app, target, paste_into_previous),
        ClipAction::Pin => {
            with_target_index(app, target, |engine, _, index| engine.pin(index));
            refresh_tray_menu(app);
        }
        ClipAction::Delete => {
            with_target_index(app, target, |engine, list, index| match list {
                List::History => engine.delete(index),
                List::Pinned => engine.unpin(index),
            });
            refresh_tray_menu(app);
        }
        // A menu can't hold a text box, so editing happens in Settings.
        ClipAction::Edit => {
            let mut pinned_index = None;
            with_target_index(app, target, |_, _, index| {
                pinned_index = Some(index);
                true
            });
            if let Some(index) = pinned_index {
                settings::show_settings_window_at(app, Some(&format!("pinned-edit:{index}")));
            }
        }
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
    if let Some(index) = clips.iter().position(|clip| clip.content == target.content) {
        f(&mut engine, target.list, index);
    }
}

/// Copies the target to the clipboard, moves a History Clip to the top, and
/// pastes it (ADR-0005). Windows pastes into the app that was focused before
/// the popup; from the tray there's no such app (the taskbar has focus), so it
/// only copies. On macOS a menu never takes focus, so either menu pastes into
/// the frontmost app.
fn paste(app: &AppHandle, target: &Target, paste_into_previous: bool) {
    if let Err(err) = copy(app, &target.content) {
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
    #[cfg(target_os = "macos")]
    if !clip_macos::paste_into_frontmost_app() {
        eprintln!("copied only: Clipdeck needs the Accessibility permission to paste");
    }
}

fn copy(app: &AppHandle, content: &ClipContent) -> Result<(), String> {
    match content {
        ClipContent::Text(text) => app
            .clipboard()
            .write_text(text.clone())
            .map_err(|err| err.to_string()),
        ClipContent::Image(image) => {
            let png = app
                .state::<AppState>()
                .engine
                .lock()
                .unwrap()
                .image_png(image)
                .ok_or("the image's file is missing")?;
            copy_png(app, &png)
        }
    }
}

/// macOS takes the PNG as it is (see `clip_macos::copy_png`).
#[cfg(target_os = "macos")]
fn copy_png(_app: &AppHandle, png: &[u8]) -> Result<(), String> {
    clip_macos::copy_png(png)
        .then_some(())
        .ok_or_else(|| "the pasteboard refused the image".into())
}

/// Elsewhere the clipboard plugin takes pixels. On Windows it writes them as
/// both a bitmap and a PNG, so apps without PNG support can paste them too.
#[cfg(not(target_os = "macos"))]
fn copy_png(app: &AppHandle, png: &[u8]) -> Result<(), String> {
    let image = Rgba::from_png(png).ok_or("the image's file can't be read")?;
    app.clipboard()
        .write_image(&tauri::image::Image::new(
            &image.pixels,
            image.width,
            image.height,
        ))
        .map_err(|err| err.to_string())
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
