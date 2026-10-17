//! What the menus show and what choosing in them does. The hotkeys open the
//! glass menu (`popup`), and so does the tray icon on Windows and macOS. A
//! Linux tray can only show a native menu, which is built here.

use crate::popup::PreviousFocus;
use crate::{settings, AppState};
use clip_engine::menu::{ClipAction, HeldModifiers};
use clip_engine::{ClipContent, Settings};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

pub const TRAY_ID: &str = "main";

#[derive(Clone, Copy, PartialEq)]
pub enum MenuKind {
    /// History and Pinned together (Clipy's main menu); also the tray menu.
    Main,
    History,
    Pinned,
}

#[derive(Clone, Copy, PartialEq)]
pub enum List {
    History,
    Pinned,
}

/// What a menu item acts on. Items are matched back to Clips by content,
/// which is unique in both lists, so a Clip captured while the menu is open
/// can't make an item act on the wrong Clip.
#[derive(Clone)]
pub struct Target {
    pub list: List,
    pub content: ClipContent,
}

/// The app actions at the end of every menu, under "More".
pub const CLEAR_HISTORY: &str = "clear_history";
pub const SETTINGS: &str = "settings";
pub const PAUSE: &str = "pause";
pub const QUIT: &str = "quit";
/// The row a macOS menu starts with while Clipdeck may not paste.
#[cfg(target_os = "macos")]
pub const ALLOW_PASTING: &str = "allow_pasting";

/// Everything a menu shows, copied out so no lock is held while building it.
pub struct Snapshot {
    pub history: Vec<ClipContent>,
    pub pinned: Vec<ClipContent>,
    /// The thumbnail PNG of every image Clip in either list, by digest.
    pub thumbnails: HashMap<String, Vec<u8>>,
    pub settings: Settings,
    pub paused: bool,
}

pub fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let newest_first = |clips: Vec<clip_engine::Clip>| -> Vec<ClipContent> {
        clips.into_iter().rev().map(|clip| clip.content).collect()
    };
    let (history, pinned, thumbnails) = {
        let engine = state.engine.lock().unwrap();
        let history = newest_first(engine.history());
        let pinned = newest_first(engine.pinned());
        let thumbnails = history
            .iter()
            .chain(&pinned)
            .filter_map(|content| match content {
                ClipContent::Image(image) => Some(image),
                ClipContent::Text(_) => None,
            })
            .filter_map(|image| Some((image.digest.clone(), engine.image_thumbnail_png(image)?)))
            .collect();
        (history, pinned, thumbnails)
    };
    let settings = state.active_settings.lock().unwrap().clone();
    Snapshot {
        history,
        pinned,
        thumbnails,
        settings,
        paused: state.paused.load(Ordering::SeqCst),
    }
}

/// Carries out one of the app actions under "More".
pub fn run_command(app: &AppHandle, command: &str) {
    match command {
        CLEAR_HISTORY => confirm_and_clear_history(app),
        SETTINGS => settings::show_settings_window_at(app, None),
        PAUSE => {
            let state = app.state::<AppState>();
            state.paused.fetch_xor(true, Ordering::SeqCst);
            refresh_tray_menu(app);
        }
        QUIT => app.exit(0),
        #[cfg(target_os = "macos")]
        ALLOW_PASTING => clip_macos::ask_to_allow_pasting(),
        _ => {}
    }
}

/// Where a chosen Clip goes once it's on the clipboard.
#[derive(Clone, Copy)]
pub enum PasteInto {
    /// Into the app that was in front before the glass menu opened.
    Previous(PreviousFocus),
    /// Into whatever has the keyboard once a native menu has closed (the
    /// Linux tray menu, which hands the keyboard back by itself).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Focused,
    /// Nowhere: it's only copied. The Windows tray's glass menu, since the
    /// taskbar is what was in front.
    #[cfg_attr(not(windows), allow(dead_code))]
    Nowhere,
}

/// A plain click pastes; with the delete or pin modifier held (Clipy's beta
/// options), it deletes or pins instead. Returns whether it pasted.
pub fn activate(app: &AppHandle, target: &Target, held: HeldModifiers, into: PasteInto) -> bool {
    let settings = app
        .state::<AppState>()
        .active_settings
        .lock()
        .unwrap()
        .clone();
    if settings.delete_modifier.is_held(held) {
        perform(app, target, ClipAction::Delete);
        false
    } else if target.list == List::History && settings.pin_modifier.is_held(held) {
        perform(app, target, ClipAction::Pin);
        false
    } else {
        paste(app, target, into);
        true
    }
}

/// Carries out `action` on the target, whether it was asked for with a
/// modifier+click or from the Clip's actions.
pub fn perform(app: &AppHandle, target: &Target, action: ClipAction) {
    let state = app.state::<AppState>();
    let editor = {
        let mut engine = state.engine.lock().unwrap();
        let index_in = |engine: &crate::Engine, list: List| {
            let clips = match list {
                List::History => engine.history(),
                List::Pinned => engine.pinned(),
            };
            clips.iter().position(|clip| clip.content == target.content)
        };
        match action {
            ClipAction::Pin => {
                if let Some(index) = index_in(&engine, List::History) {
                    engine.pin(index);
                }
                None
            }
            ClipAction::Unpin => {
                if let Some(index) = index_in(&engine, List::Pinned) {
                    engine.unpin(index);
                }
                None
            }
            // A History Clip goes from History and stays pinned if it was; a
            // pinned item goes from both lists.
            ClipAction::Delete => {
                if target.list == List::Pinned {
                    if let Some(index) = index_in(&engine, List::Pinned) {
                        engine.unpin(index);
                    }
                }
                if let Some(index) = index_in(&engine, List::History) {
                    engine.delete(index);
                }
                None
            }
            // Editing happens in Settings, in the editor of the pinned item
            // or of the History Clip itself.
            ClipAction::Edit => match (target.list, &target.content) {
                (List::Pinned, _) => {
                    index_in(&engine, List::Pinned).map(|index| format!("pinned-edit:{index}"))
                }
                (List::History, ClipContent::Text(text)) => Some(format!("clip-edit:{text}")),
                (List::History, ClipContent::Image(_)) => None,
            },
        }
    };
    match editor {
        Some(section) => settings::show_settings_window_at(app, Some(&section)),
        None => refresh_tray_menu(app),
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
/// pastes it (ADR-0005) where `into` says.
pub fn paste(app: &AppHandle, target: &Target, into: PasteInto) {
    if let Err(err) = copy(app, &target.content) {
        eprintln!("couldn't write the clipboard: {err}");
        return;
    }
    if target.list == List::History {
        with_target_index(app, target, |engine, _, index| engine.promote(index));
    }
    refresh_tray_menu(app);

    match into {
        PasteInto::Previous(previous) => previous.paste(),
        PasteInto::Focused => {
            // A native menu holds the keyboard while it's open, and its item
            // is chosen just before it lets go, so the keys wait a moment.
            #[cfg(target_os = "linux")]
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if !clip_linux::paste_into_focused_app() {
                    eprintln!("copied only: couldn't send the paste keys");
                }
            });
        }
        PasteInto::Nowhere => {}
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
    let image = clip_engine::Rgba::from_png(png).ok_or("the image's file can't be read")?;
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

/// Keeps what shows Clips current after they change: the Linux tray's native
/// menu, and the Settings Window's pinned items. Safe to call from any
/// thread; the work happens on the event loop. The glass menu needs nothing,
/// since it's filled afresh each time it opens.
pub fn refresh_tray_menu(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        #[cfg(target_os = "linux")]
        match tray::build(&handle) {
            Ok(menu) => {
                if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                    let _ = tray.set_menu(Some(menu));
                }
            }
            Err(err) => eprintln!("couldn't build the tray menu: {err}"),
        }
        let _ = handle.emit_to(settings::SETTINGS_WINDOW, "pinned-changed", ());
    });
}

/// The Linux tray's native menu: a Linux tray icon reports no clicks, so it
/// can't open the glass menu and shows this instead (ADR-0015). It lists the
/// same Clips and app actions, numbered and folded into folders the same way.
#[cfg(target_os = "linux")]
pub mod tray {
    use super::{snapshot, List, Snapshot, Target, PAUSE};
    use crate::AppState;
    use clip_engine::menu::{layout, MenuEntry, MenuItemModel};
    use clip_engine::{ClipContent, Rgba};
    use tauri::menu::{
        CheckMenuItem, IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tauri::{AppHandle, Manager, Wry};

    pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
        let snap = snapshot(app);
        let menu = Menu::new(app)?;
        let mut targets = std::collections::HashMap::new();
        section(
            app,
            &menu,
            &snap,
            "History",
            List::History,
            &snap.history,
            &mut targets,
        )?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        section(
            app,
            &menu,
            &snap,
            "Pinned",
            List::Pinned,
            &snap.pinned,
            &mut targets,
        )?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let more = Submenu::with_id(
            app,
            "t:more",
            if snap.paused {
                "More (capture paused)"
            } else {
                "More"
            },
            true,
        )?;
        more.append(&MenuItem::with_id(
            app,
            super::CLEAR_HISTORY,
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
            super::SETTINGS,
            "Settings",
            true,
            None::<&str>,
        )?)?;
        more.append(&PredefinedMenuItem::separator(app)?)?;
        more.append(&MenuItem::with_id(
            app,
            super::QUIT,
            "Quit Clipdeck",
            true,
            None::<&str>,
        )?)?;
        menu.append(&more)?;
        *app.state::<AppState>().tray_targets.lock().unwrap() = targets;
        Ok(menu)
    }

    fn section(
        app: &AppHandle,
        menu: &Menu<Wry>,
        snap: &Snapshot,
        title: &str,
        list: List,
        clips: &[ClipContent],
        targets: &mut std::collections::HashMap<String, Target>,
    ) -> tauri::Result<()> {
        menu.append(&MenuItem::with_id(
            app,
            format!("t:label:{title}"),
            title,
            false,
            None::<&str>,
        )?)?;
        if clips.is_empty() {
            return menu.append(&MenuItem::with_id(
                app,
                format!("t:empty:{title}"),
                "(empty)",
                false,
                None::<&str>,
            )?);
        }
        let code = match list {
            List::History => "h",
            List::Pinned => "p",
        };
        let mut item = |model: &MenuItemModel| -> tauri::Result<Box<dyn IsMenuItem<Wry>>> {
            let id = format!("t:{code}:{}", model.position);
            let content = &clips[model.position];
            targets.insert(
                id.clone(),
                Target {
                    list,
                    content: content.clone(),
                },
            );
            // Menu labels treat "&" as a mnemonic marker; Clip text shows it literally.
            let label = model.label.replace('&', "&&");
            let thumbnail = match content {
                ClipContent::Image(image) => snap
                    .thumbnails
                    .get(&image.digest)
                    .and_then(|png| Rgba::from_png(png)),
                ClipContent::Text(_) => None,
            };
            Ok(match thumbnail {
                Some(thumbnail) => Box::new(IconMenuItem::with_id(
                    app,
                    id,
                    label,
                    true,
                    Some(tauri::image::Image::new_owned(
                        thumbnail.pixels,
                        thumbnail.width,
                        thumbnail.height,
                    )),
                    None::<&str>,
                )?),
                None => Box::new(MenuItem::with_id(app, id, label, true, None::<&str>)?),
            })
        };
        for entry in layout(clips, &snap.settings.menu_layout()) {
            match entry {
                MenuEntry::Item(model) => menu.append(item(&model)?.as_ref())?,
                MenuEntry::Folder { title, items } => {
                    let built: Vec<Box<dyn IsMenuItem<Wry>>> =
                        items.iter().map(&mut item).collect::<tauri::Result<_>>()?;
                    let refs: Vec<&dyn IsMenuItem<Wry>> =
                        built.iter().map(|item| item.as_ref()).collect();
                    menu.append(&Submenu::with_items(app, title, true, &refs)?)?;
                }
            }
        }
        Ok(())
    }

    /// Something chosen from the tray menu: a Clip, or one of the app actions.
    pub fn handle_menu_event(app: &AppHandle, id: &str) {
        let target = app
            .state::<AppState>()
            .tray_targets
            .lock()
            .unwrap()
            .get(id)
            .cloned();
        match target {
            Some(target) => {
                super::activate(
                    app,
                    &target,
                    clip_linux::held_modifiers(),
                    super::PasteInto::Focused,
                );
            }
            None => super::run_command(app, id),
        }
    }
}
