//! How a list of Clips is laid out as a Clipy-style menu: numbered, shortened
//! titles, a few items inline, the rest grouped into "11 - 20" style folders.
//! Pure presentation logic, kept here so it can be tested without a GUI.

/// Only the first 10 items get a number shortcut: 1-9, then 0 for the tenth.
const SHORTCUT_DIGITS: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, PartialEq)]
pub struct MenuLayout {
    /// Longest title shown, in characters, including the ellipsis.
    pub title_length: usize,
    /// How many items appear directly in the menu before folders start.
    pub items_inline: usize,
    pub items_per_folder: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItemModel {
    /// Position in the list that was laid out (0 = first, i.e. newest).
    pub position: usize,
    pub label: String,
    pub shortcut_digit: Option<char>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    Item(MenuItemModel),
    Folder {
        title: String,
        items: Vec<MenuItemModel>,
    },
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

/// Lays out `texts` (already in display order) the way Clipy does: numbering
/// restarts at 1 inside each folder, while number shortcuts always belong to
/// the first ten items overall.
pub fn layout(texts: &[String], options: &MenuLayout) -> Vec<MenuEntry> {
    let per_folder = options.items_per_folder.max(1);
    let item = |position: usize, number: usize| MenuItemModel {
        position,
        label: format!(
            "{number}. {}",
            menu_title(&texts[position], options.title_length)
        ),
        shortcut_digit: SHORTCUT_DIGITS.get(position).copied(),
    };

    let inline = options.items_inline.min(texts.len());
    let mut entries: Vec<MenuEntry> = (0..inline)
        .map(|p| MenuEntry::Item(item(p, p + 1)))
        .collect();

    let mut start = inline;
    while start < texts.len() {
        let end = (start + per_folder).min(texts.len());
        entries.push(MenuEntry::Folder {
            title: format!("{} - {}", start + 1, end),
            items: (start..end).map(|p| item(p, p - start + 1)).collect(),
        });
        start = end;
    }
    entries
}

/// Which modifier keys were down during a menu action. `command_or_control`
/// is Ctrl on Windows and Command on macOS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldModifiers {
    pub command_or_control: bool,
    pub alt: bool,
    pub shift: bool,
}
