//! The glass menu (ADR-0015): Clipdeck's own window, opened at the mouse
//! cursor by the hotkeys, and by the tray icon on Windows and macOS. The
//! page (`dist/menu.html`) draws it and handles the mouse and keyboard; this
//! module fills it, places it on screen, gives the keyboard back to the app
//! the user was in, and carries out what was chosen.
//!
//! A folder, More, a Clip's actions and its preview open beside their row in
//! a second window, the cascade (`dist/cascade.html`), like a native submenu.
//! It never takes the keyboard: the menu's page decides what it shows and
//! highlights, and the cascade only draws that and reports the mouse.

use crate::clip_menu::{self, List, MenuKind, PasteInto, Target};
use crate::AppState;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use clip_engine::menu::{
    history_clip_actions, image_title, layout, pinned_clip_actions, place_cascade, place_popup,
    ClipAction, HeldModifiers, MenuEntry, MenuItemModel, ScreenRect,
};
use clip_engine::{ClipContent, Rgba, ShortcutModifier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

pub const MENU_WINDOW: &str = "menu";
const CASCADE_WINDOW: &str = "cascade";
/// The widest a cascade gets in CSS pixels, as `cascade.html` draws it.
const CASCADE_WIDTH: f64 = 320.0;
/// How large an image Clip is shown in a cascade, in pixels: twice its size
/// there, for sharp pictures on high-DPI screens.
const PREVIEW_MAX: (u32, u32) = (600, 480);

/// The app that had the keyboard before the glass menu took it, so it can be
/// given back and a chosen Clip pasted into it.
#[derive(Clone, Copy, Default)]
pub struct PreviousFocus {
    /// The foreground window (HWND as a raw value; 0 for none).
    #[cfg(windows)]
    window: isize,
    /// The frontmost app's process id.
    #[cfg(target_os = "macos")]
    app: Option<i32>,
    /// The window the window manager named active.
    #[cfg(target_os = "linux")]
    window: Option<u32>,
}

impl PreviousFocus {
    /// Who has the keyboard now. Call it before the menu shows.
    fn capture() -> Self {
        Self {
            #[cfg(windows)]
            window: clip_windows::foreground_window(),
            #[cfg(target_os = "macos")]
            app: clip_macos::frontmost_app(),
            #[cfg(target_os = "linux")]
            window: clip_linux::active_window(),
        }
    }

    /// Gives the keyboard back, once the menu has been hidden.
    fn restore(self) {
        #[cfg(windows)]
        clip_windows::restore_foreground(self.window);
        #[cfg(target_os = "macos")]
        if let Some(app) = self.app {
            clip_macos::activate_app(app);
        }
        #[cfg(target_os = "linux")]
        if let Some(window) = self.window {
            // Give the window manager a moment to finish hiding the menu,
            // or it may pick a window of its own to focus afterwards.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                clip_linux::activate_window(window);
            });
        }
    }

    /// Gives the keyboard back and pastes the clipboard there.
    pub fn paste(self) {
        #[cfg(windows)]
        clip_windows::focus_window_and_paste(self.window);
        #[cfg(target_os = "macos")]
        {
            if let Some(app) = self.app {
                clip_macos::activate_app(app);
            }
            if !clip_macos::paste_into_frontmost_app() {
                eprintln!("copied only: Clipdeck needs the Accessibility permission to paste");
            }
        }
        #[cfg(target_os = "linux")]
        {
            let window = self.window;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Some(window) = window {
                    clip_linux::activate_window(window);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                if !clip_linux::paste_into_focused_app() {
                    eprintln!("copied only: couldn't send the paste keys");
                }
            });
        }
    }
}

/// The glass menu between opening and closing.
#[derive(Default)]
pub struct PopupState {
    open: bool,
    from_tray: bool,
    previous: PreviousFocus,
    /// Where it was opened and the work area of that screen, in physical
    /// pixels, and that screen's scale factor.
    cursor: (f64, f64),
    area: Option<ScreenRect>,
    scale: f64,
    /// Where it is on screen now, in physical pixels.
    window: Option<ScreenRect>,
    panel_on_left: bool,
    /// The top of the row the open cascade belongs to, in CSS pixels from
    /// the top of the menu.
    cascade_row: f64,
    targets: HashMap<String, Target>,
    /// Number shortcut digit -> id of the item it picks.
    digits: HashMap<char, String>,
}

/// Makes the glass menu's windows, hidden until a menu opens. Call it once,
/// at startup.
pub fn create_windows(app: &tauri::App) -> tauri::Result<()> {
    glass_window(app, MENU_WINDOW, "menu.html", true)?;
    glass_window(app, CASCADE_WINDOW, "cascade.html", false)
}

/// A borderless, always-on-top window of glass. One that isn't `focusable`
/// takes clicks without taking the keyboard, like a native submenu.
fn glass_window(app: &tauri::App, label: &str, page: &str, focusable: bool) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title("Clipdeck")
        .inner_size(300.0, 200.0)
        .visible(false)
        // Hidden, so this takes nothing yet; but a window made unfocused
        // can't take the keyboard on Linux until it has been drawn once.
        .focused(focusable)
        .focusable(focusable)
        .accept_first_mouse(true)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        // Windows draws a shadowed borderless window with a 1px frame line;
        // its corners are rounded through DWM instead (`round_corners`).
        .shadow(cfg!(not(windows)));
    // Real glass is the OS's to draw: Acrylic blurs whatever is behind the
    // window on Windows, Liquid Glass does on macOS 26 (the menu material on
    // older macOS). X11 has no such effect, so on Linux the page paints a
    // tinted pane of its own over an opaque window.
    #[cfg(windows)]
    let builder = builder.transparent(true).effects(
        tauri::window::EffectsBuilder::new()
            .effect(tauri::window::Effect::Acrylic)
            .build(),
    );
    #[cfg(target_os = "macos")]
    let builder = builder
        .transparent(true)
        .visible_on_all_workspaces(true)
        .effects(
            tauri::window::EffectsBuilder::new()
                .effects([
                    tauri::window::Effect::LiquidGlassRegular,
                    tauri::window::Effect::Menu,
                ])
                .state(tauri::window::EffectState::Active)
                .radius(14.0)
                .build(),
        );
    round_corners(&builder.build()?);
    Ok(())
}

/// Windows 11's rounded corners, with no border line. Asked again each time
/// a window shows, in case showing it reset them.
fn round_corners(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        clip_windows::round_without_border(hwnd.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// Opens a menu at the mouse cursor. Called from the global-shortcut and
/// tray handlers, which hold internal locks; showing a window from there
/// could deadlock, so the work is queued onto the event loop. A hotkey
/// pressed while the menu is open closes it.
pub fn request(app: &AppHandle, kind: MenuKind, from_tray: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || open(&handle, kind, from_tray));
    });
}

fn open(app: &AppHandle, kind: MenuKind, from_tray: bool) {
    if app.state::<AppState>().popup.lock().unwrap().open {
        close(app, true);
        return;
    }
    let previous = PreviousFocus::capture();
    let Ok(cursor) = app.cursor_position() else {
        return;
    };
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work = monitor.work_area();
    let area = ScreenRect {
        x: work.position.x.into(),
        y: work.position.y.into(),
        width: work.size.width.into(),
        height: work.size.height.into(),
    };

    let snap = clip_menu::snapshot(app);
    let built = build_model(kind, &snap, area.height / monitor.scale_factor());
    {
        let state = app.state::<AppState>();
        let mut popup = state.popup.lock().unwrap();
        *popup = PopupState {
            open: true,
            from_tray,
            previous,
            cursor: (cursor.x, cursor.y),
            area: Some(area),
            scale: monitor.scale_factor(),
            window: None,
            panel_on_left: false,
            cascade_row: 0.0,
            targets: built.targets,
            digits: built.digits,
        };
    }
    // The page lays it out and calls `menu_shown` with its size.
    let _ = app.emit_to(MENU_WINDOW, "menu-open", built.model);
}

/// Hides the menu. With `restore`, the keyboard goes back to the app the user
/// was in; without, it stays wherever it went (a click outside the menu).
fn close(app: &AppHandle, restore: bool) -> Option<PopupState> {
    let popup = {
        let state = app.state::<AppState>();
        let mut popup = state.popup.lock().unwrap();
        if !popup.open {
            return None;
        }
        std::mem::take(&mut *popup)
    };
    for label in [CASCADE_WINDOW, MENU_WINDOW] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
        }
    }
    if restore {
        popup.previous.restore();
    }
    Some(popup)
}

/// The menu window lost the keyboard: the user clicked somewhere else.
/// Windows also says this when the keyboard only moves into the page inside
/// the menu (a window of its own there), so the menu closes only once it's
/// clear the window itself is no longer the active one.
pub fn on_blur(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            // A click in a cascade can make it the active window; it's
            // still the menu.
            let still_active = [MENU_WINDOW, CASCADE_WINDOW].into_iter().any(|label| {
                handle
                    .get_webview_window(label)
                    .is_some_and(|window| is_active(&window))
            });
            if !still_active {
                close(&handle, false);
            }
        });
    });
}

/// Whether `window` is still the one in front. On Windows that's asked of
/// Windows itself: the window's own focus flag is off whenever the page
/// inside it has the keyboard.
fn is_active(window: &tauri::WebviewWindow) -> bool {
    #[cfg(windows)]
    return window
        .hwnd()
        .is_ok_and(|hwnd| hwnd.0 as isize == clip_windows::foreground_window());
    #[cfg(not(windows))]
    return window.is_focused().unwrap_or(false);
}

/// Called by the page once it has laid out a menu `width` x `height` CSS
/// pixels big. Puts it at the cursor and shows it; returns whether its
/// cascades open on its left.
#[tauri::command]
pub fn menu_shown(app: AppHandle, width: f64, height: f64) -> bool {
    let (rect, panel_on_left) = {
        let state = app.state::<AppState>();
        let mut popup = state.popup.lock().unwrap();
        let Some(area) = popup.area.filter(|_| popup.open) else {
            return false;
        };
        let size = (width * popup.scale, height * popup.scale);
        let placed = place_popup(popup.cursor, size, CASCADE_WIDTH * popup.scale, area);
        let rect = ScreenRect {
            x: placed.x,
            y: placed.y,
            width: size.0,
            height: size.1,
        };
        popup.window = Some(rect);
        popup.panel_on_left = placed.panel_on_left;
        (rect, placed.panel_on_left)
    };
    let Some(window) = app.get_webview_window(MENU_WINDOW) else {
        return false;
    };
    let _ = window.set_position(PhysicalPosition::new(rect.x.round(), rect.y.round()));
    let _ = window.set_size(PhysicalSize::new(rect.width.round(), rect.height.round()));
    let _ = window.show();
    round_corners(&window);
    take_keyboard(&app, &window);
    panel_on_left
}

/// Gives the menu the keyboard, and the page in it, so its keys work.
fn take_keyboard(app: &AppHandle, window: &tauri::WebviewWindow) {
    let focus = |window: &tauri::WebviewWindow| {
        let _ = window.set_focus();
        let _ = AsRef::<tauri::Webview>::as_ref(window).set_focus();
    };
    focus(window);
    // GTK shows a window on its next turn of the event loop, and a focus
    // request for a window not yet shown is dropped; so ask again once it's
    // up. A window manager may also refuse the keyboard to a window opened
    // from a hotkey (focus-stealing prevention), so it's taken through X11
    // as well.
    #[cfg(target_os = "linux")]
    {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(60));
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(window) = handle.get_webview_window(MENU_WINDOW) {
                    focus(&window);
                    take_x11_keyboard(&window);
                }
            });
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// The menu's page opens a cascade beside the row `row_top` CSS pixels from
/// its top, showing `content` (what `cascade.html` takes).
#[tauri::command]
pub fn cascade_open(app: AppHandle, content: serde_json::Value, row_top: f64) {
    app.state::<AppState>().popup.lock().unwrap().cascade_row = row_top;
    let _ = app.emit_to(CASCADE_WINDOW, "cascade-open", content);
}

/// The cascade's page has laid out what it shows, `width` x `height` CSS
/// pixels; puts it beside its row and shows it, leaving the keyboard with the
/// menu.
#[tauri::command]
pub fn cascade_shown(app: AppHandle, width: f64, height: f64) {
    let rect = {
        let state = app.state::<AppState>();
        let popup = state.popup.lock().unwrap();
        let (Some(menu), Some(area), true) = (popup.window, popup.area, popup.open) else {
            return;
        };
        let size = (width * popup.scale, height * popup.scale);
        let row_y = menu.y + popup.cascade_row * popup.scale;
        let (x, y) = place_cascade(menu, row_y, size, popup.panel_on_left, area);
        ScreenRect {
            x,
            y,
            width: size.0,
            height: size.1,
        }
    };
    if let Some(window) = app.get_webview_window(CASCADE_WINDOW) {
        let _ = window.set_position(PhysicalPosition::new(rect.x.round(), rect.y.round()));
        let _ = window.set_size(PhysicalSize::new(rect.width.round(), rect.height.round()));
        let _ = window.show();
        round_corners(&window);
    }
}

#[tauri::command]
pub fn cascade_close(app: AppHandle) {
    if let Some(window) = app.get_webview_window(CASCADE_WINDOW) {
        let _ = window.hide();
    }
}

/// The menu's page moves the cascade's highlight to row `index` (-1: none).
#[tauri::command]
pub fn cascade_highlight(app: AppHandle, index: i64) {
    let _ = app.emit_to(CASCADE_WINDOW, "cascade-highlight", index);
}

/// The cascade's page reports the mouse (a row entered, clicked or
/// right-clicked) to the menu's page, which decides what it means.
#[tauri::command]
pub fn cascade_input(app: AppHandle, input: serde_json::Value) {
    let _ = app.emit_to(MENU_WINDOW, "cascade-input", input);
    // The cascade isn't meant to take the keyboard, but a click in it can
    // make it the active window on Windows; the keys belong to the menu.
    let cascade_took_it = app
        .get_webview_window(CASCADE_WINDOW)
        .is_some_and(|window| is_active(&window));
    if cascade_took_it {
        if let Some(menu) = app.get_webview_window(MENU_WINDOW) {
            take_keyboard(&app, &menu);
        }
    }
}

#[cfg(target_os = "linux")]
fn take_x11_keyboard(window: &tauri::WebviewWindow) {
    use gtk::prelude::*;
    let xid = window
        .gtk_window()
        .ok()
        .and_then(|gtk_window| gtk_window.window())
        .and_then(|gdk_window| gdk_window.downcast::<gdkx11::X11Window>().ok())
        .map(|x11_window| x11_window.xid());
    if let Some(xid) = xid {
        // The window is mapped by GTK's own connection; wait until the X
        // server has seen that before asking for the keyboard.
        std::thread::spawn(move || {
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(20));
                if clip_linux::activate_window(xid as u32) {
                    break;
                }
            }
        });
    }
}

/// The modifier keys held while choosing, as the page saw them.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Held {
    command_or_control: bool,
    alt: bool,
    shift: bool,
}

impl From<Held> for HeldModifiers {
    fn from(held: Held) -> Self {
        HeldModifiers {
            command_or_control: held.command_or_control,
            alt: held.alt,
            shift: held.shift,
        }
    }
}

/// Closes the menu and says where its chosen Clip `id` would be pasted.
fn close_for(app: &AppHandle, id: &str) -> Option<(Target, PasteInto)> {
    let popup = close(app, false)?;
    let target = popup.targets.get(id)?.clone();
    let into = if popup.from_tray && cfg!(windows) {
        PasteInto::Nowhere
    } else {
        PasteInto::Previous(popup.previous)
    };
    Some((target, into))
}

/// A Clip was clicked, or chosen with Enter.
#[tauri::command]
pub fn menu_choose(app: AppHandle, id: String, held: Held) {
    if let Some((target, into)) = close_for(&app, &id) {
        let pasted = clip_menu::activate(&app, &target, held.into(), into);
        // Deleting or pinning instead leaves nothing to paste, but the
        // keyboard still goes back to where it was.
        if let (false, PasteInto::Previous(previous)) = (pasted, into) {
            previous.restore();
        }
    }
}

/// A digit key was pressed with `held`. Picks and pastes the Clip it's the
/// number shortcut of, if it is one; returns whether it was.
#[tauri::command]
pub fn menu_digit(app: AppHandle, digit: char, held: Held) -> bool {
    let id = {
        let state = app.state::<AppState>();
        let modifier = state
            .active_settings
            .lock()
            .unwrap()
            .number_shortcut_modifier;
        if !modifier.is_held(held.into()) {
            return false;
        }
        let popup = state.popup.lock().unwrap();
        match popup.digits.get(&digit) {
            Some(id) => id.clone(),
            None => return false,
        }
    };
    if let Some((target, into)) = close_for(&app, &id) {
        clip_menu::paste(&app, &target, into);
    }
    true
}

/// Something was chosen from a Clip's actions (ADR-0011).
#[tauri::command]
pub fn menu_action(app: AppHandle, id: String, action: String) {
    let action = [
        ClipAction::Edit,
        ClipAction::Pin,
        ClipAction::Unpin,
        ClipAction::Delete,
    ]
    .into_iter()
    .find(|candidate| candidate.label() == action);
    let Some(action) = action else {
        return;
    };
    if let Some(popup) = close(&app, true) {
        if let Some(target) = popup.targets.get(&id) {
            clip_menu::perform(&app, target, action);
        }
    }
}

/// One of the app actions under "More" was chosen.
#[tauri::command]
pub fn menu_command(app: AppHandle, command: String) {
    if close(&app, true).is_some() {
        clip_menu::run_command(&app, &command);
    }
}

/// Esc, or the hotkey again: closed without choosing anything.
#[tauri::command]
pub fn menu_dismiss(app: AppHandle) {
    close(&app, true);
}

/// The image Clip `id` as a `data:` URL, sized for its cascade.
#[tauri::command]
pub async fn menu_preview(app: AppHandle, id: String) -> Option<String> {
    let state = app.state::<AppState>();
    let image = {
        let popup = state.popup.lock().unwrap();
        match &popup.targets.get(&id)?.content {
            ClipContent::Image(image) => image.clone(),
            ClipContent::Text(_) => return None,
        }
    };
    let png = state.engine.lock().unwrap().image_png(&image)?;
    let picture = Rgba::from_png(&png)?.fit_within(PREVIEW_MAX.0, PREVIEW_MAX.1);
    Some(png_url(&picture.to_png()?))
}

fn png_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png))
}

/// What the page draws, built fresh each time a menu opens.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MenuModel {
    /// Shown on the first heading while copies aren't being captured.
    paused: bool,
    /// macOS without the Accessibility permission: a chosen Clip is only
    /// copied, so the menu starts with a row that says so.
    pasting_off: bool,
    /// "windows", "macos" or "linux": each draws the glass its own way, and
    /// only macOS has Cmd.
    platform: &'static str,
    /// The tallest the menu may be on this screen, in CSS pixels; a longer
    /// list scrolls.
    max_height: f64,
    sections: Vec<Section>,
    /// The app actions under "More"; `None` is a separator.
    more: Vec<Option<Command>>,
}

#[derive(Clone, Serialize)]
struct Section {
    title: &'static str,
    entries: Vec<Entry>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Entry {
    Clip(Row),
    Folder { title: String, rows: Vec<Row> },
}

#[derive(Clone, Serialize)]
struct Row {
    id: String,
    number: Option<usize>,
    title: String,
    /// The number shortcut, as it's written on the row ("Ctrl 1", "⌘1").
    shortcut: Option<String>,
    /// An image Clip's thumbnail, as a `data:` URL.
    thumbnail: Option<String>,
    /// The whole text, when the title shows only part of it.
    tooltip: Option<String>,
    image: bool,
    /// What its actions list offers, by label.
    actions: Vec<&'static str>,
}

#[derive(Clone, Serialize)]
struct Command {
    id: &'static str,
    label: &'static str,
    enabled: bool,
    checked: Option<bool>,
}

struct Built {
    model: MenuModel,
    targets: HashMap<String, Target>,
    digits: HashMap<char, String>,
}

fn build_model(kind: MenuKind, snap: &clip_menu::Snapshot, max_height: f64) -> Built {
    let mac = cfg!(target_os = "macos");
    let mut targets = HashMap::new();
    let mut digits = HashMap::new();
    let mut sections = Vec::new();
    let mut section = |title, list, clips: &[ClipContent], with_digits: bool| {
        let modifier = if with_digits {
            snap.settings.number_shortcut_modifier
        } else {
            ShortcutModifier::Off
        };
        let mut row = |model: &MenuItemModel| {
            let content = &clips[model.position];
            let id = format!(
                "{}:{}",
                match list {
                    List::History => "h",
                    List::Pinned => "p",
                },
                model.position
            );
            let shortcut = model
                .shortcut_digit
                .and_then(|digit| shortcut_text(modifier, digit, mac));
            if shortcut.is_some() {
                digits.insert(model.shortcut_digit.unwrap(), id.clone());
            }
            targets.insert(
                id.clone(),
                Target {
                    list,
                    content: content.clone(),
                },
            );
            let actions = match list {
                List::History => history_clip_actions(content, snap.pinned.contains(content)),
                List::Pinned => pinned_clip_actions(content),
            };
            let (title, thumbnail) = match content {
                ClipContent::Image(image) => (
                    image_title(image),
                    snap.thumbnails.get(&image.digest).map(|png| png_url(png)),
                ),
                ClipContent::Text(_) => (model.title.clone(), None),
            };
            Row {
                id,
                number: model.number,
                title,
                shortcut,
                thumbnail,
                tooltip: model.tooltip.clone(),
                image: matches!(content, ClipContent::Image(_)),
                actions: actions.into_iter().map(ClipAction::label).collect(),
            }
        };
        let entries = layout(clips, &snap.settings.menu_layout())
            .into_iter()
            .map(|entry| match entry {
                MenuEntry::Item(model) => Entry::Clip(row(&model)),
                MenuEntry::Folder { title, items } => Entry::Folder {
                    title,
                    rows: items.iter().map(&mut row).collect(),
                },
            })
            .collect();
        sections.push(Section { title, entries });
    };
    if kind != MenuKind::Pinned {
        section("History", List::History, &snap.history, true);
    }
    if kind != MenuKind::History {
        // Like Clipy, number shortcuts belong to History unless Pinned is shown alone.
        section(
            "Pinned",
            List::Pinned,
            &snap.pinned,
            kind == MenuKind::Pinned,
        );
    }
    let command = |id, label, enabled| {
        Some(Command {
            id,
            label,
            enabled,
            checked: None,
        })
    };
    let more = vec![
        command(
            clip_menu::CLEAR_HISTORY,
            "Clear History",
            !snap.history.is_empty(),
        ),
        None,
        Some(Command {
            id: clip_menu::PAUSE,
            label: "Pause capture",
            enabled: true,
            checked: Some(snap.paused),
        }),
        command(clip_menu::SETTINGS, "Settings", true),
        None,
        command(clip_menu::QUIT, "Quit Clipdeck", true),
    ];
    #[cfg(target_os = "macos")]
    let pasting_off = !clip_macos::pasting_is_allowed();
    #[cfg(not(target_os = "macos"))]
    let pasting_off = false;
    Built {
        model: MenuModel {
            paused: snap.paused,
            pasting_off,
            platform: std::env::consts::OS,
            max_height,
            sections,
            more,
        },
        targets,
        digits,
    }
}

/// How a number shortcut is written on its row: "Ctrl 1" on Windows and
/// Linux, "⌘1" on macOS.
fn shortcut_text(modifier: ShortcutModifier, digit: char, mac: bool) -> Option<String> {
    let key = match (modifier, mac) {
        (ShortcutModifier::Off, _) => return None,
        (ShortcutModifier::None, _) => return Some(digit.to_string()),
        (ShortcutModifier::CommandOrControl, true) => "⌘",
        (ShortcutModifier::CommandOrControl, false) => "Ctrl ",
        (ShortcutModifier::Alt, true) => "⌥",
        (ShortcutModifier::Alt, false) => "Alt ",
        (ShortcutModifier::Shift, true) => "⇧",
        (ShortcutModifier::Shift, false) => "Shift ",
    };
    Some(format!("{key}{digit}"))
}
