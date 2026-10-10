//! Tauri side of Settings: applying a `Settings` value to the running app
//! (engine config + global hotkeys), the Settings Window's commands, and the
//! poller that picks up hand edits to the settings file.

use crate::AppState;
use clip_engine::{Settings, SettingsFile, MAX_HISTORY_CAPACITY};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut};

pub const SETTINGS_WINDOW: &str = "settings";
const FILE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// What a global hotkey does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HotkeyAction {
    MainMenu,
    HistoryMenu,
    PinnedMenu,
    ClearHistory,
}

impl HotkeyAction {
    fn from_field(field: &str) -> Self {
        match field {
            "main_hotkey" => HotkeyAction::MainMenu,
            "history_hotkey" => HotkeyAction::HistoryMenu,
            "pinned_hotkey" => HotkeyAction::PinnedMenu,
            _ => HotkeyAction::ClearHistory,
        }
    }

    fn label(self) -> &'static str {
        match self {
            HotkeyAction::MainMenu => "Main menu",
            HotkeyAction::HistoryMenu => "History menu",
            HotkeyAction::PinnedMenu => "Pinned menu",
            HotkeyAction::ClearHistory => "Clear History",
        }
    }
}

/// The bound hotkeys; unbound (empty) settings simply aren't in here.
#[derive(Clone, PartialEq, Default)]
pub struct Hotkeys {
    bindings: Vec<(HotkeyAction, Shortcut)>,
}

impl Hotkeys {
    pub fn parse(settings: &Settings) -> Result<Self, String> {
        let mut bindings: Vec<(HotkeyAction, Shortcut)> = Vec::new();
        for (field, value) in settings.hotkeys() {
            if value.is_empty() {
                continue;
            }
            let shortcut = parse_hotkey(field, value)?;
            // Catches spellings that differ but mean the same keys, e.g. "Ctrl" vs "Control".
            if let Some((other, _)) = bindings.iter().find(|(_, s)| s.id() == shortcut.id()) {
                return Err(format!(
                    "{field} {value:?} is the same shortcut as the {} hotkey",
                    other.label()
                ));
            }
            bindings.push((HotkeyAction::from_field(field), shortcut));
        }
        Ok(Self { bindings })
    }

    pub fn action_for(&self, shortcut: &Shortcut) -> Option<HotkeyAction> {
        self.bindings
            .iter()
            .find(|(_, bound)| bound == shortcut)
            .map(|(action, _)| *action)
    }

    fn register(&self, app: &AppHandle) -> Result<(), String> {
        let shortcuts = app.global_shortcut();
        for (action, shortcut) in &self.bindings {
            if let Err(err) = shortcuts.register(*shortcut) {
                self.unregister(app);
                return Err(format!(
                    "Couldn't register the {} hotkey {}: {err}. Another app may already be using it.",
                    action.label(),
                    shortcut.into_string()
                ));
            }
        }
        Ok(())
    }

    fn unregister(&self, app: &AppHandle) {
        // Unregistering a shortcut that isn't registered errors; that's fine here.
        for (_, shortcut) in &self.bindings {
            let _ = app.global_shortcut().unregister(*shortcut);
        }
    }
}

fn parse_hotkey(field: &str, value: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = value
        .parse()
        .map_err(|err| format!("{field} {value:?} is not a valid shortcut: {err}"))?;
    // A bare letter or digit as a global hotkey would swallow that key everywhere.
    if shortcut.mods.is_empty() && !is_function_key(shortcut.key) {
        return Err(format!(
            "{field} {value:?} needs at least one modifier (Ctrl, Alt, Shift, or Cmd)"
        ));
    }
    Ok(shortcut)
}

fn is_function_key(code: Code) -> bool {
    use Code::*;
    matches!(
        code,
        F1 | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | F13
            | F14
            | F15
            | F16
            | F17
            | F18
            | F19
            | F20
            | F21
            | F22
            | F23
            | F24
    )
}

/// Owned by `AppState`. One lock serializes every settings change, whether it
/// comes from the Settings Window or from the file on disk.
pub struct SettingsState {
    file: SettingsFile,
    path: PathBuf,
    current: Settings,
    /// Why the file on disk isn't what's in effect, if it isn't.
    file_error: Option<String>,
    /// Global hotkeys are released while the Settings Window records a new
    /// one, so pressing the current combo reaches the recorder instead of
    /// toggling a popup.
    hotkeys_suspended: bool,
}

impl SettingsState {
    /// Loads the settings file, falling back to defaults (without overwriting
    /// the file) if it is unreadable, so a typo can't stop the app starting.
    pub fn load(path: PathBuf) -> Self {
        let mut file = SettingsFile::new(&path);
        let (current, file_error) = match file.load() {
            Ok(settings) => match Hotkeys::parse(&settings) {
                Ok(_) => (settings, None),
                Err(err) => (Settings::default(), Some(err)),
            },
            Err(err) => (Settings::default(), Some(err.to_string())),
        };
        Self {
            file,
            path,
            current,
            file_error,
            hotkeys_suspended: false,
        }
    }

    pub fn current(&self) -> &Settings {
        &self.current
    }

    fn view(&self) -> SettingsView {
        SettingsView {
            settings: self.current.clone(),
            file_path: self.path.display().to_string(),
            file_error: self.file_error.clone(),
            max_history_capacity: MAX_HISTORY_CAPACITY,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct SettingsView {
    settings: Settings,
    file_path: String,
    file_error: Option<String>,
    max_history_capacity: usize,
}

/// Puts `new` into effect: hotkeys and start at login first, since those are
/// the steps that can fail for reasons outside our control, then the engine.
/// Nothing changes if it returns an error.
fn apply(app: &AppHandle, state: &mut SettingsState, new: Settings) -> Result<(), String> {
    new.validate().map_err(|err| err.to_string())?;
    let new_hotkeys = Hotkeys::parse(&new)?;
    let app_state = app.state::<AppState>();
    let old_hotkeys = app_state.hotkeys.lock().unwrap().clone();

    let swap_hotkeys = new_hotkeys != old_hotkeys && !state.hotkeys_suspended;
    if swap_hotkeys {
        old_hotkeys.unregister(app);
        if let Err(err) = new_hotkeys.register(app) {
            let _ = old_hotkeys.register(app);
            return Err(err);
        }
    }
    if new.launch_at_login != state.current.launch_at_login {
        if let Err(err) = crate::autostart::sync(new.launch_at_login) {
            if swap_hotkeys {
                new_hotkeys.unregister(app);
                let _ = old_hotkeys.register(app);
            }
            return Err(err);
        }
    }
    *app_state.hotkeys.lock().unwrap() = new_hotkeys;
    app_state
        .engine
        .lock()
        .unwrap()
        .reconfigure(new.engine_config());
    *app_state.active_settings.lock().unwrap() = new.clone();
    state.current = new;
    // Menu layout and number shortcuts show up in the tray menu.
    crate::clip_menu::refresh_tray_menu(app);
    Ok(())
}

pub fn register_initial_hotkeys(app: &AppHandle) -> Result<(), String> {
    app.state::<AppState>()
        .hotkeys
        .lock()
        .unwrap()
        .register(app)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsView, String> {
    Ok(state.settings.lock().unwrap().view())
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<SettingsView, String> {
    let mut settings_state = state.settings.lock().unwrap();
    apply(&app, &mut settings_state, settings)?;
    let current = settings_state.current.clone();
    settings_state
        .file
        .save(&current)
        .map_err(|err| format!("Applied, but couldn't write the settings file: {err}"))?;
    settings_state.file_error = None;
    Ok(settings_state.view())
}

#[tauri::command]
pub async fn set_hotkeys_suspended(
    app: AppHandle,
    state: State<'_, AppState>,
    suspended: bool,
) -> Result<(), String> {
    let mut settings_state = state.settings.lock().unwrap();
    if settings_state.hotkeys_suspended == suspended {
        return Ok(());
    }
    settings_state.hotkeys_suspended = suspended;
    let hotkeys = state.hotkeys.lock().unwrap().clone();
    if suspended {
        hotkeys.unregister(&app);
        Ok(())
    } else {
        hotkeys.register(&app)
    }
}

/// Opens the Settings Window, optionally at a section: "pinned" for the menu's
/// "Edit Pinned..." item, "pinned-new" for "New Pinned Item..." (opens an empty
/// pinned-item editor there).
pub fn show_settings_window_at(app: &AppHandle, section: Option<&str>) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(section) = section {
            let _ = app.emit_to(SETTINGS_WINDOW, "show-section", section);
        }
    }
}

/// Polls the settings file so hand edits take effect without a restart, and
/// pushes the result to the Settings Window if it's open.
pub fn spawn_file_watcher(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(FILE_POLL_INTERVAL);
        let state = app.state::<AppState>();
        let mut settings_state = state.settings.lock().unwrap();
        let Some(reloaded) = settings_state.file.reload_if_changed() else {
            continue;
        };
        settings_state.file_error = match reloaded {
            Ok(settings) => apply(&app, &mut settings_state, settings).err(),
            Err(err) => Some(err.to_string()),
        };
        if let Some(err) = &settings_state.file_error {
            eprintln!("settings file not applied: {err}");
        }
        let _ = app.emit_to(SETTINGS_WINDOW, "settings-changed", settings_state.view());
    });
}

pub fn settings_file_path(app: &tauri::App) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .expect("no app config directory on this OS");
    std::fs::create_dir_all(&dir).expect("failed to create the app config directory");
    dir.join("settings.toml")
}
