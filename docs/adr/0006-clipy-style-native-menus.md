# Clipy-style native menus replace the list popups

Clipdeck's History and Pinned popups were webview windows: a scrollable list with a type-to-filter search box (Ticket 4), pin icons, and a box for writing pinned text.
The user, a long-time Clipy user, asked to copy Clipy's UI and behavior instead.
We read Clipy's own source (`MenuManager.swift`, `HotKeyService.swift`, `CPYUtilities.swift`) and matched it: global hotkeys open a native OS menu at the mouse cursor, items are numbered and cut to their first line (20 characters by default), they're grouped into "1 - 10", "11 - 20" folders, and Cmd/Ctrl+1...0 pick the first ten.
The defaults follow Clipy's, with Command mapped to Ctrl on Windows: main menu Ctrl/Cmd+Shift+V, History Ctrl/Cmd+Alt+V, Pinned (Clipy's Snippets) Ctrl/Cmd+Shift+B, Clear History unbound.

This deliberately reverses earlier decisions:

- **Type-to-filter search is gone.**
  A native menu has no text field; Clipy has no search either.
- **The tray icon shows the main menu**, as Clipy's status item does, instead of opening the Settings Window (the v1 spec's user story 22).
  Settings opens from the menu's "Settings..." item.
- **Pinning and writing pinned text moved.**
  A modifier+click on a History item pins it, and pinned items are written, edited, and removed in a Pinned section of the Settings Window, the counterpart of Clipy's "Edit Snippets" window.
  Deleting with a modifier+click mirrors one of Clipy's beta options.

Two platform gaps shaped the implementation:

- **Windows popup menus ignore Ctrl+digit.**
  `TrackPopupMenu` runs a modal loop without accelerator translation, so a `WH_MSGFILTER` hook on the menu's thread catches the number shortcut, closes the menu, and reports the digit (`clip-windows/src/menu_keys.rs`).
  The same hook records which modifiers were held when an item was clicked, and swallows Alt, which would otherwise dismiss the menu.
  On macOS, NSMenu key equivalents do this natively, so a number shortcut arrives as a click with its modifier held; that's why the number, delete, and pin modifiers must all differ.
- **The menu needs a window to attach to**, so an invisible 1x1 "menu-host" window owns it.
  On Windows the menu takes focus from the app the user was in; focus is handed back when it closes, and a chosen Clip is pasted there (ADR-0005).
  From the tray there's no such app to return to (the taskbar has focus), so choosing a Clip there only copies it.

Not carried over: Clipy's tooltips and image thumbnails on menu items (Tauri's menus support neither), and its Snippet folders (Clipdeck's Pinned list stays flat).
macOS behavior is written but unverified, like the rest of the macOS adapter.

**Status**: accepted
