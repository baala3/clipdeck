//! Starting Clipdeck at login, per the `launch_at_login` setting.
//!
//! Uses `auto-launch` directly rather than Tauri's autostart plugin, so the
//! Windows `Run` entry is always per-user (the plugin writes the machine-wide
//! one when the app happens to run elevated) and the exe path is quoted (the
//! plugin's isn't, which breaks install paths containing spaces).

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

/// The `Run` value on Windows and the LaunchAgent name on macOS. The NSIS
/// uninstaller deletes the `Run` value by this name.
const LOGIN_ITEM_NAME: &str = "Clipdeck";

/// Registers or unregisters the running exe to start at login. Registering
/// again rewrites the path, so this is also how a moved app stays registered.
/// Debug builds never touch the OS, so dev runs don't start at login.
pub fn sync(enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let login_item =
        login_item().map_err(|err| format!("Couldn't set up start at login: {err}"))?;
    let result = if enabled {
        login_item.enable()
    } else {
        login_item.disable()
    };
    result.map_err(|err| {
        let action = if enabled { "turn on" } else { "turn off" };
        format!("Couldn't {action} start at login: {err}")
    })
}

fn login_item() -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(LOGIN_ITEM_NAME);
    #[cfg(windows)]
    builder
        .set_app_path(&format!("\"{}\"", exe.display()))
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    #[cfg(not(windows))]
    builder.set_app_path(&exe.display().to_string());
    builder.build().map_err(|err| err.to_string())
}
