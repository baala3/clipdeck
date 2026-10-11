//! How a list of Clips is laid out as a Clipy-style menu: numbered, shortened
//! titles, a few items inline, the rest grouped into "11 - 20" style folders.
//! Pure presentation logic, kept here so it can be tested without a GUI.

use crate::{ClipContent, ImageInfo};

/// Only the first 10 items get a number shortcut: 1-9, then 0 for the tenth.
const SHORTCUT_DIGITS: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
const ELLIPSIS: &str = "...";
/// A tooltip is a glance at a Clip, not a viewer; past this it's cut short.
const TOOLTIP_MAX_LINES: usize = 20;
const TOOLTIP_MAX_CHARS: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct MenuLayout {
    /// Longest title shown, in characters, including the ellipsis.
    pub title_length: usize,
    /// How many items appear directly in the menu before folders start.
    pub items_inline: usize,
    pub items_per_folder: usize,
    /// Prefix each title with its number ("1. ").
    pub show_numbers: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItemModel {
    /// Position in the list that was laid out (0 = first, i.e. newest).
    pub position: usize,
    pub label: String,
    pub shortcut_digit: Option<char>,
    /// The Clip's text, shown on hover when the label doesn't show all of it.
    /// An image Clip has none; its thumbnail sits in the menu instead.
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    Item(MenuItemModel),
    Folder {
        title: String,
        items: Vec<MenuItemModel>,
    },
}

/// What hovering a row of a built menu shows, listed by row position so a
/// platform adapter can match rows to the native menu's items.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuTip {
    None,
    Text(String),
    /// A submenu, with a tooltip (or none) for each of its rows.
    Folder(Vec<Option<String>>),
}

/// The first non-blank line of `text`, cut to `max_length` characters with a
/// trailing "..." if it was longer.
pub fn menu_title(text: &str, max_length: usize) -> String {
    let first_line = text.trim().lines().next().unwrap_or("").trim_end();
    let max_length = max_length.max(ELLIPSIS.len());
    if first_line.chars().count() <= max_length {
        return first_line.to_string();
    }
    let kept: String = first_line
        .chars()
        .take(max_length - ELLIPSIS.len())
        .collect();
    format!("{kept}{ELLIPSIS}")
}

/// What hovering a menu item shows: the whole Clip, or `None` when `title`
/// (from [`menu_title`]) already shows all of it. Very long Clips are cut to a
/// screenful.
pub fn menu_tooltip(text: &str, title: &str) -> Option<String> {
    let text = text.trim();
    if text == title {
        return None;
    }
    let mut lines = text.lines();
    let mut tooltip = lines
        .by_ref()
        .take(TOOLTIP_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut cut = lines.next().is_some();
    if let Some((end, _)) = tooltip.char_indices().nth(TOOLTIP_MAX_CHARS) {
        tooltip.truncate(end);
        cut = true;
    }
    if cut {
        tooltip.truncate(tooltip.trim_end().len());
        tooltip.push_str(ELLIPSIS);
    }
    Some(tooltip)
}

/// What an image Clip is called in a menu, where its thumbnail goes with it.
pub fn image_title(image: &ImageInfo) -> String {
    format!("Image {} \u{d7} {}", image.width, image.height)
}

/// Lays out `clips` (already in display order) the way Clipy does: numbering
/// restarts at 1 inside each folder, while number shortcuts always belong to
/// the first ten items overall.
pub fn layout(clips: &[ClipContent], options: &MenuLayout) -> Vec<MenuEntry> {
    let per_folder = options.items_per_folder.max(1);
    let item = |position: usize, number: usize| {
        let (title, tooltip) = match &clips[position] {
            ClipContent::Text(text) => {
                let title = menu_title(text, options.title_length);
                let tooltip = menu_tooltip(text, &title);
                (title, tooltip)
            }
            ClipContent::Image(image) => (image_title(image), None),
        };
        MenuItemModel {
            position,
            tooltip,
            label: if options.show_numbers {
                format!("{number}. {title}")
            } else {
                title
            },
            shortcut_digit: SHORTCUT_DIGITS.get(position).copied(),
        }
    };

    let inline = options.items_inline.min(clips.len());
    let mut entries: Vec<MenuEntry> = (0..inline)
        .map(|p| MenuEntry::Item(item(p, p + 1)))
        .collect();

    let mut start = inline;
    while start < clips.len() {
        let end = (start + per_folder).min(clips.len());
        entries.push(MenuEntry::Folder {
            title: format!("{} - {}", start + 1, end),
            items: (start..end).map(|p| item(p, p - start + 1)).collect(),
        });
        start = end;
    }
    entries
}

/// Something a Clip's actions menu offers: the menu that opens beside a Clip
/// on a right-click or the Right arrow (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipAction {
    Paste,
    Pin,
    Edit,
    Delete,
}

impl ClipAction {
    /// What the actions menu calls it. `pinned` says which list the Clip is
    /// in; `pastes` is false where choosing a Clip only copies it (the tray
    /// menu on Windows).
    pub fn label(self, pinned: bool, pastes: bool) -> &'static str {
        match self {
            ClipAction::Paste if pastes => "Paste",
            ClipAction::Paste => "Copy",
            ClipAction::Pin => "Pin",
            ClipAction::Edit => "Edit",
            ClipAction::Delete if pinned => "Remove",
            ClipAction::Delete => "Delete",
        }
    }
}

/// The actions a History Clip offers, in menu order. One that's pinned
/// already has no Pin, since pinning it again would do nothing.
pub fn history_clip_actions(already_pinned: bool) -> Vec<ClipAction> {
    if already_pinned {
        vec![ClipAction::Paste, ClipAction::Delete]
    } else {
        vec![ClipAction::Paste, ClipAction::Pin, ClipAction::Delete]
    }
}

/// The actions a pinned item offers, in menu order. Only text can be edited.
pub fn pinned_clip_actions(content: &ClipContent) -> Vec<ClipAction> {
    match content {
        ClipContent::Text(_) => vec![ClipAction::Paste, ClipAction::Edit, ClipAction::Delete],
        ClipContent::Image(_) => vec![ClipAction::Paste, ClipAction::Delete],
    }
}

/// Which modifier keys were down during a menu action. `command_or_control`
/// is Ctrl on Windows and Command on macOS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldModifiers {
    pub command_or_control: bool,
    pub alt: bool,
    pub shift: bool,
}
