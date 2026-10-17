# The menus become a glass window of Clipdeck's own

Replaces the native menus of [ADR-0006](0006-clipy-style-native-menus.md) on every hotkey, and on the tray icon of Windows and macOS.
It keeps what those menus show and do, and moves the Windows-only extras of [ADR-0008](0008-menu-tooltips-and-more-submenu.md) and [ADR-0011](0011-a-clips-actions-menu-on-right-click.md) into the menu itself, on every platform.

After the Settings Window took the website's liquid-glass look (ADR-0014), the user wanted the menu, the part of Clipdeck they use most, to match it.
A native menu is drawn by the OS, so no styling reaches it.
The menu is now a borderless, always-on-top window (`menu`) holding a page, `dist/menu.html`, which `popup.rs` fills, places and takes choices from.

What it shows is unchanged: History and Pinned with numbers, shortened titles, "11 - 20" folders, image thumbnails, the number shortcuts, and More with Clear History, Pause capture, Settings and Quit Clipdeck.
Numbers sit in a column of their own, shortcuts are written on key chips ("Ctrl 1", "⌘1"), and a pause shows as a "Capture paused" badge on the first heading instead of renaming More.

Anything that used to cascade opens in a side panel instead: the panel widens the same glass pane, beside the list (or on its left near the right edge of the screen).
It shows:
- a folder;
- More;
- a Clip's actions, from a right-click or the arrow key toward the panel;
- a shortened Clip's full text, or an image Clip's picture, after the pointer rests on its row.

The panel stays in the same window as the list: cascading menus as separate windows were rejected below.

The glass itself comes from the OS where it can:
- Windows: Acrylic, which blurs whatever is behind the window.
- macOS: Liquid Glass on macOS 26, and the menu material before that.
- Linux: X11 has no such effect, so the page paints a tinted glass of its own over an opaque window.

The page only adds a tint on top, and follows the light or dark appearance like Settings.

The menu is a window that takes the keyboard, where a native menu left the keyboard with the app underneath.
So before it shows, Clipdeck notes who had the keyboard: the foreground window on Windows, the frontmost app on macOS, the window the window manager calls active on X11.
It gives the keyboard back when the menu closes, and pastes a chosen Clip there (ADR-0005).
A click outside the menu closes it without giving the keyboard back, since the click already put it somewhere.
On X11 a window manager may refuse the keyboard to a window opened from a hotkey, so the menu also takes it directly.

Doing it in a page also removes the Windows machinery that made native menus do more than they were built for: the keyboard hook for number shortcuts, the tooltip timer, the owner-drawn thumbnails, the actions menu on a right-click, and the menu theming.
The page reads modifier keys straight from the click or key press, and the number shortcut is checked by the same `ShortcutModifier::is_held` as before.

Linux keeps one native menu: its tray icon.
A Linux tray icon (AppIndicator) reports no clicks, only a menu to show, so the tray there shows the native menu of before, with the same Clips and More.

Considered and rejected:

- **Cascades as separate glass windows**, as the design mockup drew them.
  Each would be its own OS window to place, focus and close together, and Acrylic can't span the gap between them.
- **A full-screen see-through window** so cascades could float anywhere.
  Without a compositor (common on X11) a see-through window is black, and it would sit over every other app while open.
- **Keeping native menus and styling only what the OS allows.**
  Windows and Linux menus can't be made translucent; only macOS 26 draws its own menus in glass.

**Status**: accepted
