# Clipdeck

A lightweight, cross-platform clipboard manager. It captures what you copy, lets you browse and re-use it later, and stays out of your way otherwise.

## Language

**Clip**:
A single entry automatically captured the moment the user copies something (text or image) to the system clipboard.
A copy that carries both text and an image is captured as its text.
A copy is not captured when it is only whitespace, when its app marked it as not for clipboard tools (as password managers do), when its app is on the Exclusion list, or while capture is paused; copies made during a pause are not picked up afterwards either.
A very large image (over 40 megapixels, or 20 MB as a PNG) is not captured either.
See ADR-0012.
_Avoid_: Entry, item, clipboard entry

**History**:
The rolling, chronological list of captured Clips, bounded by a capacity the user configures (50 by default, up to an app-enforced maximum). Oldest Clips are evicted first once capacity is reached.
Selecting a Clip, or copying content already in History, moves that Clip to the top instead of adding a duplicate.
Two images are the same content when their pixels match.
Images in History share a disk budget; once it is exceeded the oldest images are evicted even if History has room.
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
The Clipy-style menu a global hotkey opens at the mouse cursor: the main menu (History and Pinned together), or History or Pinned alone. It is a glass window of Clipdeck's own, not a native OS menu (ADR-0015). Every menu ends with a single "More" holding the app actions: Clear History, Pause capture, Settings, Quit. Pinned items are written in the Settings Window, not from the Menu (ADR-0014). An image Clip shows as a thumbnail with its size ("Image 800 × 500"). Items are shortened to one line and, unless the user turns numbering off, numbered; past the first few they're grouped into "11 - 20" style folders. A folder, More, a Clip's actions (Edit, Pin, Delete in History; Unpin, Edit, Delete in Pinned) and a Clip's full text or picture open in a side panel beside the list: a folder or More when the pointer rests on it or it's chosen, the actions on a right-click or the arrow key toward the panel (ADR-0011), the full text or picture when the pointer rests on a shortened or image Clip; a plain click still pastes. Choosing an item copies it and pastes it into the app the user was working in, which gets the keyboard back as the Menu closes; on macOS that needs the Accessibility permission (ADR-0005). On Linux a hotkey opens its menu when its keys are let go, and Clipdeck runs in X11 sessions only (ADR-0013). Holding the delete or pin modifier while choosing deletes or pins it instead. Clicking the tray icon opens the main menu; on Linux, whose tray icons report no clicks, the tray shows a native menu with the same Clips and actions instead. See ADR-0006 and ADR-0015.
_Avoid_: Popup, panel (when "window" alone is said, it means the Settings Window)

**Settings Window**:
The only persistent, full application window. Opened from the Menu's "Settings" item, or by "Edit" in a Clip's actions menu - never opened just to browse or paste Clips. Each part of it is a page of its own, picked from a sidebar. Houses all end-user-facing configuration (light or dark appearance, hotkeys, Menu layout and modifiers, History capacity, exclusion list, encryption toggle) and is where pinned items are written and edited; a pinned image can only be removed there.
_Avoid_: Preferences pane, main window

**Exclusion list**:
A user-maintained list of apps/processes whose clipboard output Clipdeck never captures, enforced in addition to any OS-level "don't persist me" signal the source app sets.
_Avoid_: Blocklist, ignore list
