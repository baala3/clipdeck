# A Clip's actions open on a right-click, and a plain click still pastes

Pinning a History item meant holding Shift while clicking it, and deleting one meant holding Alt (ADR-0006).
Nothing in the menu says so, and the user found it unfriendly.
They asked for the actions to appear when a Clip is hovered, in place of the tooltip.

A native menu can only do that by giving every Clip a submenu, and a row with a submenu can't be clicked to paste: the click opens the submenu.
ADR-0008 rejected a submenu per Clip for that reason, and it still holds, because pasting is what the menu is opened for many times a day.

So the actions sit one gesture away instead.
Right-clicking a Clip, or pressing the Right arrow on it, opens a small actions menu beside it:

| Clip | Actions |
| --- | --- |
| In History | Paste, Pin, Delete |
| In History, already pinned | Paste, Delete |
| Pinned text | Paste, Edit, Remove |
| Pinned image | Paste, Remove |

A plain click or Enter pastes in one step, as before, and the tooltip stays.
Dismissing the actions menu with Escape or the Left arrow goes back to the Clips, like leaving a submenu.
The modifier+click shortcuts keep working.
"Edit" opens the Settings Window with that item's editor open, since a menu can't hold a text box.
From the tray menu on Windows "Paste" reads "Copy", because choosing a Clip there only copies it (ADR-0006).

Which actions a Clip offers, and what they're called, is decided in `clip-engine` (`menu::history_clip_actions`, `menu::pinned_clip_actions`), so it is tested without a GUI.

## Windows only, for now

Win32 reports a right-click on a menu item to the menu's owner window, which belongs to Tauri (popups) or the tray-icon crate (tray menu), not to us.
So `clip-windows/src/menu_actions.rs` watches the menu's input with a message filter, as the tooltips and number shortcuts already do, finds the row under the mouse or the highlight, and opens the actions menu on top of the open one with `TrackPopupMenuEx` and `TPM_RECURSE`.
Windows only allows that when both menus belong to the same window, so the actions menu borrows the open menu's owner.

macOS has no actions menu yet.
An `NSMenu` treats a right-click on an item as a click, and Tauri doesn't expose the `NSMenu` behind a menu, so nothing in the macOS adapter can tell the two apart; a guess that can't be run here could break the plain click.
The modifier+click shortcuts are the way to pin and delete there.

Considered and rejected:

- **A "Pin a Clip" submenu under More** listing recent History.
  It works everywhere, but it puts a second copy of History two levels down.
- **A hint row** ("Shift+click to pin").
  It makes the shortcut discoverable without making it any friendlier.

**Status**: accepted
