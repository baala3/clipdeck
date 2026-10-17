use clip_engine::menu::{
    history_clip_actions, layout, menu_title, menu_tooltip, pinned_clip_actions, ClipAction,
    MenuEntry, MenuItemModel, MenuLayout,
};
use clip_engine::{ClipContent, ImageInfo};

fn texts(n: usize) -> Vec<ClipContent> {
    (1..=n)
        .map(|i| ClipContent::Text(format!("clip {i}")))
        .collect()
}

/// An item as laid out; a label like "3. clip 3" is numbered 3 and titled "clip 3".
fn item(position: usize, label: &str, digit: Option<char>) -> MenuItemModel {
    let numbered = label
        .split_once(". ")
        .and_then(|(number, title)| Some((number.parse().ok()?, title)));
    MenuItemModel {
        position,
        label: label.into(),
        title: numbered.map_or(label, |(_, title)| title).into(),
        number: numbered.map(|(number, _)| number),
        shortcut_digit: digit,
        tooltip: None,
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
            show_numbers: true,
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
            show_numbers: true,
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
fn numbering_can_be_turned_off_without_losing_number_shortcuts() {
    let entries = layout(
        &texts(2),
        &MenuLayout {
            title_length: 20,
            items_inline: 1,
            items_per_folder: 10,
            show_numbers: false,
        },
    );

    assert_eq!(
        entries,
        vec![
            MenuEntry::Item(item(0, "clip 1", Some('1'))),
            MenuEntry::Folder {
                title: "2 - 2".into(),
                items: vec![item(1, "clip 2", Some('2'))],
            },
        ]
    );
}

#[test]
fn an_item_gets_a_tooltip_only_when_its_title_hides_part_of_the_clip() {
    assert_eq!(menu_tooltip("  hello  ", "hello"), None);
    assert_eq!(
        menu_tooltip("https://github.com/baala3/clipdeck", "https://github.co..."),
        Some("https://github.com/baala3/clipdeck".into())
    );
    assert_eq!(
        menu_tooltip("\n  Dear team,\nthe release is out\n", "Dear team,"),
        Some("Dear team,\nthe release is out".into())
    );

    let entries = layout(
        &[
            ClipContent::Text("short".into()),
            ClipContent::Text("a title that is too long to fit".into()),
        ],
        &MenuLayout {
            title_length: 20,
            items_inline: 2,
            items_per_folder: 10,
            show_numbers: true,
        },
    );
    let tooltips: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item(item) => item.tooltip.clone(),
            MenuEntry::Folder { .. } => panic!("expected an item"),
        })
        .collect();
    assert_eq!(
        tooltips,
        vec![None, Some("a title that is too long to fit".into())]
    );
}

#[test]
fn a_tooltip_for_a_huge_clip_is_cut_to_a_screenful() {
    let many_lines = (1..=50)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tooltip = menu_tooltip(&many_lines, "line 1").unwrap();
    assert_eq!(tooltip.lines().count(), 20);
    assert!(tooltip.ends_with("line 20..."), "{tooltip}");

    let one_long_line = "é".repeat(5000);
    let tooltip = menu_tooltip(&one_long_line, "ééé...").unwrap();
    assert_eq!(tooltip.chars().count(), 1003);
    assert!(tooltip.ends_with("é..."));
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

#[test]
fn an_image_is_titled_by_its_size_and_has_no_tooltip() {
    let entries = layout(
        &[
            ClipContent::Text("text".into()),
            ClipContent::Image(ImageInfo {
                digest: "abc".into(),
                width: 1920,
                height: 1080,
                byte_len: 0,
            }),
        ],
        &MenuLayout {
            title_length: 5,
            items_inline: 2,
            items_per_folder: 10,
            show_numbers: true,
        },
    );

    assert_eq!(
        entries[1],
        MenuEntry::Item(item(1, "2. Image 1920 \u{d7} 1080", Some('2')))
    );
}

fn image() -> ClipContent {
    ClipContent::Image(ImageInfo {
        digest: "abc".into(),
        width: 10,
        height: 10,
        byte_len: 0,
    })
}

#[test]
fn a_history_clip_can_be_edited_pinned_or_deleted_from_its_actions_menu() {
    use ClipAction::*;
    let text = ClipContent::Text("hi".into());

    assert_eq!(history_clip_actions(&text, false), vec![Edit, Pin, Delete]);
    assert_eq!(
        history_clip_actions(&text, true),
        vec![Edit, Unpin, Delete],
        "one that's pinned already can be unpinned from History too"
    );
    assert_eq!(
        history_clip_actions(&image(), false),
        vec![Pin, Delete],
        "an image has nothing to edit"
    );
    assert_eq!(history_clip_actions(&image(), true), vec![Unpin, Delete]);
}

#[test]
fn a_pinned_item_can_be_unpinned_edited_or_deleted_from_its_actions_menu() {
    use ClipAction::*;

    assert_eq!(
        pinned_clip_actions(&ClipContent::Text("hi".into())),
        vec![Unpin, Edit, Delete]
    );
    assert_eq!(
        pinned_clip_actions(&image()),
        vec![Unpin, Delete],
        "an image has nothing to edit"
    );
}

#[test]
fn pasting_is_not_an_action_since_choosing_the_clip_does_that() {
    let offered: Vec<&str> = history_clip_actions(&ClipContent::Text("hi".into()), false)
        .into_iter()
        .chain(pinned_clip_actions(&ClipContent::Text("hi".into())))
        .map(ClipAction::label)
        .collect();

    assert_eq!(
        offered,
        vec!["Edit", "Pin", "Delete", "Unpin", "Edit", "Delete"]
    );
}

#[test]
fn the_number_is_also_kept_apart_from_the_title_for_a_numbers_column() {
    let options = MenuLayout {
        title_length: 20,
        items_inline: 1,
        items_per_folder: 10,
        show_numbers: true,
    };

    let entries = layout(&texts(3), &options);

    let MenuEntry::Folder { items, .. } = &entries[1] else {
        panic!("expected a folder after the first item");
    };
    assert_eq!(
        (
            items[1].label.as_str(),
            items[1].title.as_str(),
            items[1].number
        ),
        ("2. clip 3", "clip 3", Some(2)),
        "numbering restarts in each folder"
    );
}
