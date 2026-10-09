use clip_engine::{Settings, SettingsFile, ShortcutModifier};

fn settings_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("settings.toml")
}

#[test]
fn loading_a_missing_settings_file_creates_it_with_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);

    let settings = SettingsFile::new(&path).load().unwrap();

    assert_eq!(
        settings,
        Settings {
            launch_at_login: true,
            main_hotkey: "CommandOrControl+Shift+V".into(),
            history_hotkey: "CommandOrControl+Alt+V".into(),
            pinned_hotkey: "CommandOrControl+Shift+B".into(),
            clear_history_hotkey: "".into(),
            history_capacity: 200,
            excluded_apps: vec![],
            number_shortcut_modifier: ShortcutModifier::CommandOrControl,
            delete_modifier: ShortcutModifier::Alt,
            pin_modifier: ShortcutModifier::Shift,
            menu_title_length: 20,
            menu_items_inline: 10,
            menu_items_per_folder: 10,
        }
    );
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("# Clipdeck settings."), "{written}");
    assert!(written.contains("history_capacity = 200"), "{written}");
    assert!(written.contains("launch_at_login = true"), "{written}");
}

#[test]
fn saving_from_the_settings_window_keeps_comments_a_developer_wrote_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    std::fs::write(
        &path,
        "# my dotfiles copy\nhistory_capacity = 500 # plenty\nexcluded_apps = [\"KeePass.exe\"]\n",
    )
    .unwrap();
    let mut file = SettingsFile::new(&path);
    let mut settings = file.load().unwrap();
    assert_eq!(settings.history_capacity, 500);
    assert_eq!(settings.excluded_apps, vec!["KeePass.exe".to_string()]);

    settings.history_capacity = 750;
    file.save(&settings).unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("# my dotfiles copy"), "{written}");
    assert!(
        written.contains("history_capacity = 750 # plenty"),
        "{written}"
    );
    assert_eq!(SettingsFile::new(&path).load().unwrap(), settings);
}

#[test]
fn a_history_capacity_outside_1_to_2000_is_rejected_with_a_readable_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);

    for bad in ["0", "2001"] {
        std::fs::write(&path, format!("history_capacity = {bad}\n")).unwrap();
        let err = SettingsFile::new(&path).load().unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("history_capacity must be between 1 and 2000 (got {bad})")
        );
    }
}

#[test]
fn the_history_and_pinned_popups_cannot_share_a_hotkey() {
    let settings = Settings {
        history_hotkey: "Ctrl+Shift+V".into(),
        pinned_hotkey: "ctrl+shift+v".into(),
        ..Settings::default()
    };

    assert_eq!(
        settings.validate().unwrap_err().to_string(),
        "history_hotkey and pinned_hotkey must be different (both are \"Ctrl+Shift+V\")"
    );
}

#[test]
fn reload_if_changed_reports_hand_edits_but_not_the_apps_own_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    let mut file = SettingsFile::new(&path);
    let mut settings = file.load().unwrap();
    assert!(file.reload_if_changed().is_none());

    settings.history_capacity = 300;
    file.save(&settings).unwrap();
    assert!(file.reload_if_changed().is_none());

    std::fs::write(&path, "history_capacity = 42\n").unwrap();
    let reloaded = file.reload_if_changed().unwrap().unwrap();
    assert_eq!(reloaded.history_capacity, 42);
    assert!(file.reload_if_changed().is_none());

    std::fs::write(&path, "history_capacity = \"lots\"\n").unwrap();
    assert!(file.reload_if_changed().unwrap().is_err());
    assert!(
        file.reload_if_changed().is_none(),
        "a broken file is reported once, not every poll"
    );
}

#[test]
fn a_settings_file_from_before_the_main_hotkey_existed_loads_with_it_unbound() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    std::fs::write(
        &path,
        "history_hotkey = \"CommandOrControl+Shift+V\"\npinned_hotkey = \"CommandOrControl+Shift+P\"\n",
    )
    .unwrap();

    let settings = SettingsFile::new(&path).load().unwrap();

    assert_eq!(settings.main_hotkey, "");
    assert_eq!(settings.history_hotkey, "CommandOrControl+Shift+V");
}

#[test]
fn a_settings_file_from_before_launch_at_login_existed_loads_with_it_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    std::fs::write(&path, "history_capacity = 50\n").unwrap();

    let settings = SettingsFile::new(&path).load().unwrap();

    assert!(settings.launch_at_login);
    assert_eq!(settings.history_capacity, 50);
}

#[test]
fn no_two_bound_hotkeys_may_be_the_same() {
    let settings = Settings {
        clear_history_hotkey: "CommandOrControl+Shift+V".into(),
        ..Settings::default()
    };

    assert_eq!(
        settings.validate().unwrap_err().to_string(),
        "main_hotkey and clear_history_hotkey must be different (both are \"CommandOrControl+Shift+V\")"
    );
}

#[test]
fn the_delete_and_pin_click_modifiers_must_differ_unless_turned_off() {
    let clash = Settings {
        delete_modifier: ShortcutModifier::Shift,
        pin_modifier: ShortcutModifier::Shift,
        ..Settings::default()
    };
    assert_eq!(
        clash.validate().unwrap_err().to_string(),
        "delete_modifier and pin_modifier must be different (both are \"Shift\")"
    );

    let both_off = Settings {
        delete_modifier: ShortcutModifier::Off,
        pin_modifier: ShortcutModifier::Off,
        ..Settings::default()
    };
    assert!(both_off.validate().is_ok());
}

#[test]
fn modifier_settings_are_written_as_readable_names() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    std::fs::write(
        &path,
        "number_shortcut_modifier = \"Alt\"\ndelete_modifier = \"Off\"\n",
    )
    .unwrap();

    let settings = SettingsFile::new(&path).load().unwrap();

    assert_eq!(settings.number_shortcut_modifier, ShortcutModifier::Alt);
    assert_eq!(settings.delete_modifier, ShortcutModifier::Off);
    std::fs::write(&path, "number_shortcut_modifier = \"Hyper\"\n").unwrap();
    let err = SettingsFile::new(&path).load().unwrap_err().to_string();
    assert!(err.contains("Hyper"), "{err}");
}

#[test]
fn the_number_shortcut_modifier_cannot_double_as_a_click_modifier() {
    let clash = Settings {
        number_shortcut_modifier: ShortcutModifier::Alt,
        delete_modifier: ShortcutModifier::Alt,
        ..Settings::default()
    };

    assert_eq!(
        clash.validate().unwrap_err().to_string(),
        "number_shortcut_modifier and delete_modifier must be different (both are \"Alt\")"
    );
}
