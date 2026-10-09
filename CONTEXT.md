# Clipdeck

A lightweight, cross-platform clipboard manager. It captures what you copy, lets you browse and re-use it later, and stays out of your way otherwise.

## Language

**Clip**:
A single entry automatically captured the moment the user copies something (text or image) to the system clipboard.
_Avoid_: Entry, item, clipboard entry

**History**:
The rolling, chronological list of captured Clips, bounded by a capacity the user configures (up to an app-enforced maximum). Oldest Clips are evicted first once capacity is reached.
_Avoid_: Clipboard log, cache

**Pin / Pinned list**:
A separate, user-curated, flat list of Clips the user has explicitly chosen to keep. Pinning *copies* a Clip into this list - the original stays in History and is still evicted on its normal schedule. The Pinned list itself has no capacity cap.
_Avoid_: Favorite, bookmark

**Snippet**:
A reusable piece of text the user authors directly, not derived from a copy event. Explicitly out of scope for v1 - deferred as a separate future feature so it isn't conflated with Clips.
_Avoid_: Template (for v1 purposes, this concept doesn't exist yet)

**Popup**:
The ephemeral, hotkey-triggered overlay used to browse and select from History or from the Pinned list. Closes immediately after a selection or on dismiss. Not the main app window.
Selecting a Clip copies it to the clipboard and (on Windows; see ADR-0005) pastes it directly into whatever was focused before the popup opened.
_Avoid_: Window, panel (reserve "window" for the Settings Window)

**Settings Window**:
The only persistent, full application window. Opened via the tray/menu-bar icon (or a dedicated command) - never opened just to browse or paste Clips. Houses all end-user-facing configuration (hotkeys, History capacity, exclusion list, encryption toggle, theme).
_Avoid_: Preferences pane, main window

**Exclusion list**:
A user-maintained list of apps/processes whose clipboard output Clipdeck never captures, enforced in addition to any OS-level "don't persist me" signal the source app sets.
_Avoid_: Blocklist, ignore list
