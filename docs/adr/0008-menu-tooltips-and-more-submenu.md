# Menus show Clips first: app actions fold into "More", and hovering shows the whole Clip

Every menu used to end with five app actions (Clear History, Edit Pinned..., Settings..., Pause capture, Quit), plus "New Item..." under Pinned.
The user found them distracting: they open a menu many times a day to paste, and almost never to reach those.
So every menu now ends with a single "More" submenu that holds all six, and the rest of the menu is Clips.
While capture is paused the row reads "More (capture paused)", since the check mark that used to show it is now one level down.

Menu items are cut to one short line, so the user also asked to see the whole Clip on hover.
ADR-0006 dropped Clipy's tooltips because Tauri's menus have none; this brings them back on Windows without leaving native menus.
Win32 menus have no tooltips of their own, and the window that receives `WM_MENUSELECT` belongs to Tauri (popups) or the tray-icon crate (tray menu), not to us.
So `clip-windows/src/menu_tips.rs` is told, row by row, what each menu's items should show, and while a menu is open a timer finds the highlighted item and drives a tracking tooltip beside it.
One mechanism covers the tray menu, the hotkey popups, folders, and keyboard navigation.
A tooltip only appears when the label hides part of the Clip, after the highlight rests for a moment, and is capped at 20 lines or 1000 characters.

Numbering ("1. ") is now optional (`menu_show_numbers`, on by default).
Number shortcuts keep working with it off, and their "Ctrl+1" hints stay; turning the shortcut modifier Off removes those too.

Considered and rejected:

- **A submenu per Clip that shows its full text.**
  Clicking an item that has a submenu opens it instead of pasting, which would break the one-click paste.
- **Going back to a webview popup** to get real hover cards.
  That reverses ADR-0006 for one feature.

macOS has no tooltips yet.
`NSMenuItem.toolTip` would do it natively, but Tauri doesn't expose the `NSMenu` behind a menu, and nothing in the macOS adapter can be run here to verify a workaround.

**Status**: accepted
