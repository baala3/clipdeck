# Settings gets a sidebar of pages, and the More submenu stops opening it at Pinned

Amends [ADR-0008](0008-menu-tooltips-and-more-submenu.md), which put "New Pinned Item" and "Edit Pinned" in every menu's "More" submenu.

Both items only opened the Settings Window, one of them with an empty pinned-item editor ready.
The user found them redundant next to "Settings", which opens the same window, so More now holds Clear History, Pause capture, Settings and Quit Clipdeck.
A pinned item is still edited from its own actions menu (ADR-0011), which opens its editor in Settings, and new pinned items are written with Settings' "New" button.

The Settings Window was one long scroll, and at its 560 x 640 size only General and Hotkeys showed without scrolling.
It is now laid out like macOS System Settings: a sidebar picks one page at a time (General, Hotkeys, Menu, Pinned, History, Excluded apps, About), so every setting is one click away and nothing sits below the fold.
It shares the website's liquid-glass look: translucent panes over soft color orbs, Funnel Display for headings and JetBrains Mono for keys and Clip text, with both fonts shipped inside the app so it never loads anything from the network.
Appearance became a three-way segmented control, and the number settings became steppers that show their range.

The default History capacity also dropped from 200 to 50 Clips, so a fresh install's History menu stays short.
Like the hotkey defaults in ADR-0010, this only reaches new installs: the settings file records the capacity the first time Clipdeck runs.

Considered and rejected:

- **Real window transparency** (Mica on Windows, vibrancy on macOS) behind the panes.
  It needs a different native API per platform and has none on X11, so the glass is drawn by the page itself and looks the same everywhere.

**Status**: accepted
