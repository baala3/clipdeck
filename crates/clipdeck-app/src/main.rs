#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod clip_menu;
mod pinned;
mod popup;
mod settings;
mod updates;

use clip_engine::{
    adopt_legacy_store, ClipEngine, ClipboardSource, DirImageStore, Settings, SqliteClipStore,
    SqlitePinnedStore,
};
use clip_menu::MenuKind;
use settings::{HotkeyAction, Hotkeys, SettingsState, SETTINGS_WINDOW};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

type Engine = ClipEngine<SqliteClipStore, SqlitePinnedStore, DirImageStore>;

struct AppState {
    engine: Mutex<Engine>,
    paused: AtomicBool,
    settings: Mutex<SettingsState>,
    /// A copy of the settings in effect, for code that must never wait on the
    /// `settings` lock (menus are built on the event loop while a settings
    /// change may be holding that lock and waiting on the event loop itself).
    active_settings: Mutex<Settings>,
    /// The hotkeys currently in effect, read by the global shortcut handler.
    hotkeys: Mutex<Hotkeys>,
    /// The glass menu while it's open.
    popup: Mutex<popup::PopupState>,
    /// What each Clip in the Linux tray's native menu acts on, by item id.
    #[cfg(target_os = "linux")]
    tray_targets: Mutex<std::collections::HashMap<String, clip_menu::Target>>,
}

fn spawn_capture_thread(app: tauri::AppHandle, mut source: impl ClipboardSource + Send + 'static) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let state = app.state::<AppState>();
        if state.paused.load(Ordering::SeqCst) {
            state.engine.lock().unwrap().discard(&mut source);
            continue;
        }
        let mut captured = false;
        while let Some(incoming) = source.next_event() {
            state.engine.lock().unwrap().capture(incoming);
            captured = true;
        }
        if captured {
            clip_menu::refresh_tray_menu(&app);
        }
    });
}

fn on_hotkey(app: &tauri::AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::MainMenu => popup::request(app, MenuKind::Main, false),
        HotkeyAction::HistoryMenu => popup::request(app, MenuKind::History, false),
        HotkeyAction::PinnedMenu => popup::request(app, MenuKind::Pinned, false),
        HotkeyAction::ClearHistory => clip_menu::confirm_and_clear_history(app),
    }
}

/// Where the store `file_name` lives: the per-user local data dir, which is
/// machine-local (unlike the roaming settings file) since History can hold
/// large images. Older versions kept stores in the working directory, which
/// for a double-clicked exe is its own folder; those are moved over once.
fn store_path(app: &tauri::App, file_name: &str) -> PathBuf {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .expect("no app data directory on this OS");
    let legacy_dirs: Vec<PathBuf> = [
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from)),
        std::env::current_dir().ok(),
    ]
    .into_iter()
    .flatten()
    .collect();
    adopt_legacy_store(&data_dir, file_name, &legacy_dirs).unwrap_or_else(|err| {
        eprintln!("{err}");
        err.legacy
    })
}

/// Whether this login session is a Wayland one, where Clipdeck can't do its
/// job yet: it reaches the clipboard, the hotkeys, and the keyboard through
/// the X server, and under Wayland that only covers the few apps still
/// running on X11 (see docs/research/linux-support.md).
#[cfg(target_os = "linux")]
fn on_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session == "wayland")
}

/// Says why Clipdeck can't run here, then quits.
#[cfg(target_os = "linux")]
fn refuse_wayland(app: &tauri::AppHandle) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    let handle = app.clone();
    app.dialog()
        .message(
            "Clipdeck doesn't work on Wayland yet.\n\n\
             To use it, log out and pick an X11 session (often called \"Xorg\") on the login screen.",
        )
        .title("Clipdeck")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

fn main() {
    // Clipdeck talks to the X server itself, so its windows and menus must
    // be on that same server rather than on a Wayland compositor.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    tauri::Builder::default()
        // Must be the first plugin. A second Clipdeck would capture every copy
        // twice and fight this one for the hotkeys, so it hands over and exits;
        // opening Settings shows the user that Clipdeck is already running.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            settings::show_settings_window_at(app, None);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            settings::set_hotkeys_suspended,
            pinned::list_pinned,
            pinned::add_pinned,
            pinned::edit_pinned,
            pinned::remove_pinned,
            pinned::edit_history_clip,
            updates::check_for_updates,
            popup::menu_shown,
            popup::cascade_open,
            popup::cascade_shown,
            popup::cascade_close,
            popup::cascade_highlight,
            popup::cascade_input,
            popup::menu_choose,
            popup::menu_digit,
            popup::menu_action,
            popup::menu_command,
            popup::menu_dismiss,
            popup::menu_preview
        ])
        .setup(|app| {
            // A menu bar app: no Dock icon or app menu, just the tray icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "linux")]
            if on_wayland() {
                refuse_wayland(app.handle());
                return Ok(());
            }

            let settings_state = SettingsState::load(settings::settings_file_path(app));
            let store = SqliteClipStore::open(store_path(app, "clipdeck-history.sqlite3"));
            let pinned_store = SqlitePinnedStore::open(store_path(app, "clipdeck-pinned.sqlite3"));
            let current = settings_state.current().clone();
            let current_launch_at_login = current.launch_at_login;
            let current_appearance = current.appearance;
            // The PNGs behind image Clips, next to the stores that list them.
            let image_store = DirImageStore::open(
                app.path()
                    .app_local_data_dir()
                    .expect("no app data directory on this OS")
                    .join("images"),
            );
            let engine = ClipEngine::new(store, pinned_store, image_store, current.engine_config());
            let hotkeys = Hotkeys::parse(&current)
                .expect("SettingsState::load only keeps settings with valid hotkeys");
            app.manage(AppState {
                engine: Mutex::new(engine),
                paused: AtomicBool::new(false),
                settings: Mutex::new(settings_state),
                active_settings: Mutex::new(current),
                hotkeys: Mutex::new(hotkeys),
                popup: Mutex::default(),
                #[cfg(target_os = "linux")]
                tray_targets: Mutex::default(),
            });

            settings::apply_appearance(app.handle(), current_appearance);
            popup::create_windows(app)?;

            #[cfg(windows)]
            spawn_capture_thread(
                app.handle().clone(),
                clip_windows::WindowsClipboardSource::new(),
            );
            #[cfg(target_os = "macos")]
            spawn_capture_thread(app.handle().clone(), clip_macos::MacClipboardSource::new());
            #[cfg(target_os = "linux")]
            match clip_linux::X11ClipboardSource::connect(None) {
                Ok(source) => spawn_capture_thread(app.handle().clone(), source),
                Err(err) => eprintln!("can't watch the clipboard: {err}"),
            }

            // Like Clipy, clicking the tray icon shows the full menu: the glass
            // menu, or on Linux, where a tray icon reports no clicks, a native one.
            // The tray gets its own simplified icon, drawn to stay legible at 16px.
            let tray = TrayIconBuilder::with_id(clip_menu::TRAY_ID)
                .icon(tauri::include_image!("icons/tray.png"))
                .tooltip("Clipdeck");
            #[cfg(target_os = "linux")]
            let tray = tray.menu(&clip_menu::tray::build(app.handle())?);
            #[cfg(not(target_os = "linux"))]
            let tray = tray.on_tray_icon_event(|tray, event| {
                use tauri::tray::{MouseButtonState, TrayIconEvent};
                if let TrayIconEvent::Click {
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    popup::request(tray.app_handle(), MenuKind::Main, true);
                }
            });
            tray.build(app)?;

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, shortcut, event| {
                        // On X11 a pressed hotkey holds the keyboard until its
                        // key is let go, and a menu can't open without the
                        // keyboard, so there the hotkey acts on release.
                        let trigger = if cfg!(target_os = "linux") {
                            ShortcutState::Released
                        } else {
                            ShortcutState::Pressed
                        };
                        if event.state() != trigger {
                            return;
                        }
                        let action = app
                            .state::<AppState>()
                            .hotkeys
                            .lock()
                            .unwrap()
                            .action_for(shortcut);
                        if let Some(action) = action {
                            on_hotkey(app, action);
                        }
                    })
                    .build(),
            )?;
            // A hotkey another app already owns shouldn't stop Clipdeck starting;
            // the user can pick a different one in Settings.
            if let Err(err) = settings::register_initial_hotkeys(app.handle()) {
                eprintln!("{err}");
            }
            settings::spawn_file_watcher(app.handle().clone());
            // Re-registering on every launch keeps the login item pointing at
            // this exe even if the app was moved or reinstalled elsewhere.
            if let Err(err) = autostart::sync(current_launch_at_login) {
                eprintln!("{err}");
            }
            updates::spawn_background_checks(app.handle().clone());

            Ok(())
        })
        .on_menu_event(|_app, _event| {
            #[cfg(target_os = "linux")]
            clip_menu::tray::handle_menu_event(_app, _event.id.as_ref());
        })
        .on_window_event(|window, event| match event {
            // Closing Settings just hides it, so the app keeps running in the tray.
            WindowEvent::CloseRequested { api, .. } if window.label() == SETTINGS_WINDOW => {
                api.prevent_close();
                let _ = window.hide();
            }
            // A click outside the glass menu closes it, like any menu.
            WindowEvent::Focused(false) if window.label() == popup::MENU_WINDOW => {
                popup::on_blur(window.app_handle());
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
