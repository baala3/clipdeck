# Clipdeck

A lightweight, cross-platform clipboard manager. It captures what you copy, lets you browse and re-use it later, and stays out of your way otherwise.

## Language

**Clip**:
A single entry automatically captured the moment the user copies something (text or image) to the system clipboard.
_Avoid_: Entry, item, clipboard entry

**History**:
The rolling, chronological list of captured Clips, bounded by a capacity the user configures (up to an app-enforced maximum). Oldest Clips are evicted first once capacity is reached.
Selecting a Clip, or copying content already in History, moves that Clip to the top instead of adding a duplicate.
_Avoid_: Clipboard log, cache

**Pin / Pinned list**:
A separate, user-curated, flat list of Clips the user has explicitly chosen to keep. Pinning *copies* a Clip into this list - the original stays in History and is still evicted on its normal schedule. The Pinned list itself has no capacity cap.
The user can also write text straight into the Pinned list without copying it first; such an item is a pinned Clip with no source app, and never passes through History.
The same content is never pinned twice.
_Avoid_: Favorite, bookmark

**Snippet**:
A reusable piece of text the user authors directly, not derived from a copy event, with its own features (templates, placeholders, organization).
Still out of scope as a separate concept. Plain user-written text lives in the Pinned list instead (see Pin), which covers the simple case without a second list.
_Avoid_: Template (for v1 purposes, this concept doesn't exist yet)

**Menu**:
The native, Clipy-style menu a global hotkey opens at the mouse cursor: the main menu (History and Pinned together), or History or Pinned alone. Every menu ends with the same app actions: Clear History, Edit Pinned..., Settings..., Pause capture, Quit. Wherever Pinned appears it ends with "New Item...", which opens the Settings Window ready to write a new pinned item. Items are numbered and shortened to one line; past the first few they're grouped into "11 - 20" style folders. Choosing an item copies it and, on Windows, pastes it into whatever was focused before the menu opened (ADR-0005; macOS only copies). Holding the delete or pin modifier while choosing deletes or pins it instead. The tray icon shows the same main menu. See ADR-0006.
_Avoid_: Popup, window, panel (reserve "window" for the Settings Window)

**Settings Window**:
The only persistent, full application window. Opened from the Menu's "Settings..." or "Edit Pinned..." items - never opened just to browse or paste Clips. Houses all end-user-facing configuration (hotkeys, Menu layout and modifiers, History capacity, exclusion list, encryption toggle) and is where pinned items are written and edited.
_Avoid_: Preferences pane, main window

**Exclusion list**:
A user-maintained list of apps/processes whose clipboard output Clipdeck never captures, enforced in addition to any OS-level "don't persist me" signal the source app sets.
_Avoid_: Blocklist, ignore list
