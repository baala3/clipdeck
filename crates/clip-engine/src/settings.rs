//! User settings, persisted as a human-editable TOML file.
//!
//! The Settings Window and a developer hand-editing the file are equally valid
//! ways to change settings, so writes preserve whatever comments and layout the
//! file already has, and the app re-reads the file when it changes on disk.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Opens the full menu (History and Pinned together). Missing from files
    /// written before it existed, where it loads unbound rather than clashing
    /// with the hotkeys those files already use.
    #[serde(default)]
    pub main_hotkey: String,
    pub history_hotkey: String,
    pub pinned_hotkey: String,
    /// Empty means unbound.
    pub clear_history_hotkey: String,
    pub history_capacity: usize,
    pub excluded_apps: Vec<String>,
    /// Held with 1-9 and 0 while a menu is open to pick one of the first ten items.
    pub number_shortcut_modifier: ShortcutModifier,
    /// Held while clicking a menu item to delete it instead of pasting it.
    pub delete_modifier: ShortcutModifier,
    /// Held while clicking a History item to pin it instead of pasting it.
    pub pin_modifier: ShortcutModifier,
    /// Longest menu item title, in characters.
    pub menu_title_length: usize,
    /// Items shown directly in the menu before the "11 - 20" style folders start.
    pub menu_items_inline: usize,
    pub menu_items_per_folder: usize,
}

impl Default for Settings {
    /// Mirrors Clipy's defaults, with Clipy's Command key mapped to Ctrl on
    /// Windows, except that the first ten items sit directly in the menu
    /// (Clipy puts every item in a folder) so the newest Clips are one click away.
    fn default() -> Self {
        Self {
            main_hotkey: "CommandOrControl+Shift+V".into(),
            history_hotkey: "CommandOrControl+Alt+V".into(),
            pinned_hotkey: "CommandOrControl+Shift+B".into(),
            clear_history_hotkey: String::new(),
            history_capacity: crate::DEFAULT_HISTORY_CAPACITY,
            excluded_apps: Vec::new(),
            number_shortcut_modifier: ShortcutModifier::CommandOrControl,
            delete_modifier: ShortcutModifier::Alt,
            pin_modifier: ShortcutModifier::Shift,
            menu_title_length: 20,
            menu_items_inline: 10,
            menu_items_per_folder: 10,
        }
    }
}

/// A modifier key held alongside a menu action. `None` means no modifier
/// (only meaningful for number shortcuts); `Off` disables the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShortcutModifier {
    CommandOrControl,
    Alt,
    Shift,
    None,
    Off,
}

impl ShortcutModifier {
    /// True when exactly this modifier is held (for `None`, when nothing is),
    /// so Ctrl+Shift+click doesn't count as a Ctrl+click.
    pub fn is_held(self, held: crate::menu::HeldModifiers) -> bool {
        let only = |command_or_control, alt, shift| {
            held == crate::menu::HeldModifiers { command_or_control, alt, shift }
        };
        match self {
            ShortcutModifier::CommandOrControl => only(true, false, false),
            ShortcutModifier::Alt => only(false, true, false),
            ShortcutModifier::Shift => only(false, false, true),
            ShortcutModifier::None => only(false, false, false),
            ShortcutModifier::Off => false,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ShortcutModifier::CommandOrControl => "CommandOrControl",
            ShortcutModifier::Alt => "Alt",
            ShortcutModifier::Shift => "Shift",
            ShortcutModifier::None => "None",
            ShortcutModifier::Off => "Off",
        }
    }
}

impl Settings {
    /// The subset of settings the core engine enforces. Hotkeys are the app
    /// shell's concern, not the engine's.
    pub fn engine_config(&self) -> crate::EngineConfig {
        crate::EngineConfig {
            history_capacity: self.history_capacity,
            excluded_apps: self.excluded_apps.iter().cloned().collect(),
        }
    }

    pub fn menu_layout(&self) -> crate::menu::MenuLayout {
        crate::menu::MenuLayout {
            title_length: self.menu_title_length,
            items_inline: self.menu_items_inline,
            items_per_folder: self.menu_items_per_folder,
        }
    }

    /// Each hotkey setting by its field name, bound or not.
    pub fn hotkeys(&self) -> [(&'static str, &str); 4] {
        [
            ("main_hotkey", &self.main_hotkey),
            ("history_hotkey", &self.history_hotkey),
            ("pinned_hotkey", &self.pinned_hotkey),
            ("clear_history_hotkey", &self.clear_history_hotkey),
        ]
    }

    pub fn validate(&self) -> Result<(), SettingsError> {
        check_range("history_capacity", self.history_capacity, 1, crate::MAX_HISTORY_CAPACITY)?;
        check_range("menu_title_length", self.menu_title_length, 5, 200)?;
        check_range("menu_items_inline", self.menu_items_inline, 0, crate::MAX_HISTORY_CAPACITY)?;
        check_range("menu_items_per_folder", self.menu_items_per_folder, 1, 100)?;

        let bound: Vec<_> = self.hotkeys().into_iter().filter(|(_, key)| !key.is_empty()).collect();
        for (i, (field_a, key_a)) in bound.iter().enumerate() {
            if let Some((field_b, _)) = bound[i + 1..].iter().find(|(_, key_b)| key_a.eq_ignore_ascii_case(key_b)) {
                return Err(SettingsError::Invalid(format!(
                    "{field_a} and {field_b} must be different (both are {key_a:?})"
                )));
            }
        }

        for (field, modifier) in [("delete_modifier", self.delete_modifier), ("pin_modifier", self.pin_modifier)] {
            if modifier == ShortcutModifier::None {
                return Err(SettingsError::Invalid(format!(
                    "{field} can't be None - a plain click pastes. Use Off to disable it."
                )));
            }
        }
        // On macOS a number shortcut arrives as a menu click with its modifier
        // held, so it must not also mean "delete" or "pin".
        let modifiers = [
            ("number_shortcut_modifier", self.number_shortcut_modifier),
            ("delete_modifier", self.delete_modifier),
            ("pin_modifier", self.pin_modifier),
        ];
        for (i, (field_a, a)) in modifiers.iter().enumerate() {
            if *a == ShortcutModifier::Off {
                continue;
            }
            if let Some((field_b, _)) = modifiers[i + 1..].iter().find(|(_, b)| b == a) {
                return Err(SettingsError::Invalid(format!(
                    "{field_a} and {field_b} must be different (both are {:?})",
                    a.as_str()
                )));
            }
        }
        Ok(())
    }
}

fn check_range(field: &str, value: usize, min: usize, max: usize) -> Result<(), SettingsError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(SettingsError::Invalid(format!(
            "{field} must be between {min} and {max} (got {value})"
        )))
    }
}

const NEW_FILE_HEADER: &str = "\
# Clipdeck settings.
# Edit this file directly or use the Settings window - both stay in sync.
";

#[derive(Debug)]
pub enum SettingsError {
    Io(std::io::Error),
    Invalid(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::Io(err) => write!(f, "{err}"),
            SettingsError::Invalid(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(err: std::io::Error) -> Self {
        SettingsError::Io(err)
    }
}

pub struct SettingsFile {
    path: PathBuf,
    /// The file contents as of our last read or write, so a poll can tell a
    /// hand edit apart from the app's own save.
    last_seen: Option<String>,
}

impl SettingsFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            last_seen: None,
        }
    }

    /// Reads the settings file, creating it with the defaults if it doesn't exist yet.
    pub fn load(&mut self) -> Result<Settings, SettingsError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                let parsed = parse(&text);
                self.last_seen = Some(text);
                parsed
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let settings = Settings::default();
                self.save(&settings)?;
                Ok(settings)
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Writes `settings` back to disk, keeping any comments and formatting the
    /// file already has. The write goes through a temp file and a rename so a
    /// reader never sees a half-written file.
    pub fn save(&mut self, settings: &Settings) -> Result<(), SettingsError> {
        settings.validate()?;
        let existing = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok());
        let is_new_file = existing.is_none();
        let mut doc = existing.unwrap_or_default();
        // Serializing gives the fields in struct order, so a new file reads top
        // to bottom like the Settings window; existing keys keep their comments.
        let fresh: toml_edit::DocumentMut = toml::to_string(settings)
            .expect("settings always serialize to TOML")
            .parse()
            .expect("serialized settings are valid TOML");
        for (key, item) in fresh.iter() {
            if let Some(value) = item.as_value() {
                set_value(&mut doc, key, value.clone());
            }
        }

        let tmp_path = self.path.with_extension("toml.tmp");
        let text = if is_new_file {
            format!("{NEW_FILE_HEADER}{doc}")
        } else {
            doc.to_string()
        };
        std::fs::write(&tmp_path, &text)?;
        std::fs::rename(&tmp_path, &self.path)?;
        self.last_seen = Some(text);
        Ok(())
    }

    /// Meant to be polled. Returns `None` while the file still holds what we last
    /// read or wrote, and the freshly parsed result once someone else changes it.
    /// A missing file is ignored here, since some editors briefly delete a file
    /// while saving it.
    pub fn reload_if_changed(&mut self) -> Option<Result<Settings, SettingsError>> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        if self.last_seen.as_deref() == Some(text.as_str()) {
            return None;
        }
        let parsed = parse(&text);
        self.last_seen = Some(text);
        Some(parsed)
    }
}

/// Replaces a top-level value while keeping the comments/whitespace around it.
fn set_value(doc: &mut toml_edit::DocumentMut, key: &str, mut value: toml_edit::Value) {
    match doc.get_mut(key).and_then(|item| item.as_value_mut()) {
        Some(existing) => {
            *value.decor_mut() = existing.decor().clone();
            *existing = value;
        }
        None => doc[key] = toml_edit::Item::Value(value),
    }
}

fn parse(text: &str) -> Result<Settings, SettingsError> {
    let settings: Settings =
        toml::from_str(text).map_err(|err| SettingsError::Invalid(err.to_string()))?;
    settings.validate()?;
    Ok(settings)
}
