use clip_engine::menu::{layout, menu_title, MenuEntry, MenuItemModel, MenuLayout};

fn texts(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("clip {i}")).collect()
}

fn item(position: usize, label: &str, digit: Option<char>) -> MenuItemModel {
    MenuItemModel {
        position,
        label: label.into(),
        shortcut_digit: digit,
    }
}

#[test]
fn a_menu_title_is_the_first_line_trimmed_and_shortened_with_an_ellipsis() {
    assert_eq!(menu_title("  hello  ", 20), "hello");
    assert_eq!(
        menu_title("\n  Dear team,\nthe release is out", 20),
        "Dear team,"
    );
    assert_eq!(
        menu_title("https://github.com/baala3/clipdeck", 20),
        "https://github.co..."
    );
    assert_eq!(
        menu_title("exactly twenty chars", 20),
        "exactly twenty chars"
    );
}

#[test]
fn with_no_inline_items_everything_goes_into_numbered_folders_of_ten() {
    let entries = layout(
        &texts(13),
        &MenuLayout {
            title_length: 20,
            items_inline: 0,
            items_per_folder: 10,
        },
    );

    assert_eq!(entries.len(), 2);
    let MenuEntry::Folder { title, items } = &entries[0] else {
        panic!("expected a folder")
    };
    assert_eq!(title, "1 - 10");
    assert_eq!(items.len(), 10);
    assert_eq!(items[0], item(0, "1. clip 1", Some('1')));
    assert_eq!(items[8], item(8, "9. clip 9", Some('9')));
    assert_eq!(items[9], item(9, "10. clip 10", Some('0')));

    let MenuEntry::Folder { title, items } = &entries[1] else {
        panic!("expected a folder")
    };
    assert_eq!(title, "11 - 13");
    assert_eq!(
        items,
        &vec![
            item(10, "1. clip 11", None),
            item(11, "2. clip 12", None),
            item(12, "3. clip 13", None),
        ]
    );
}

#[test]
fn inline_items_come_first_and_folder_numbering_restarts_after_them() {
    let entries = layout(
        &texts(5),
        &MenuLayout {
            title_length: 20,
            items_inline: 3,
            items_per_folder: 10,
        },
    );

    assert_eq!(
        entries,
        vec![
            MenuEntry::Item(item(0, "1. clip 1", Some('1'))),
            MenuEntry::Item(item(1, "2. clip 2", Some('2'))),
            MenuEntry::Item(item(2, "3. clip 3", Some('3'))),
            MenuEntry::Folder {
                title: "4 - 5".into(),
                items: vec![
                    item(3, "1. clip 4", Some('4')),
                    item(4, "2. clip 5", Some('5'))
                ],
            },
        ]
    );
}

#[test]
fn a_shortcut_modifier_matches_only_when_exactly_that_kind_of_key_is_held() {
    use clip_engine::menu::HeldModifiers;
    use clip_engine::ShortcutModifier::*;
    let none = HeldModifiers::default();
    let ctrl = HeldModifiers {
        command_or_control: true,
        ..none
    };
    let ctrl_shift = HeldModifiers {
        shift: true,
        ..ctrl
    };
    let alt = HeldModifiers { alt: true, ..none };

    assert!(CommandOrControl.is_held(ctrl));
    assert!(
        !CommandOrControl.is_held(ctrl_shift),
        "extra modifiers don't count"
    );
    assert!(Alt.is_held(alt));
    assert!(!Alt.is_held(ctrl));
    assert!(None.is_held(none), "None means a bare key");
    assert!(!None.is_held(ctrl));
    assert!(!Off.is_held(none));
    assert!(!Off.is_held(ctrl));
}
