# Linux support is X11 only, shipped as an AppImage

Clipdeck shipped for Windows and macOS only (ADR-0001), and Linux users asked for it.
The research in `docs/research/linux-support.md` showed that "Linux" is two platforms.
On X11 everything Clipdeck does has a standard route: XFIXES reports clipboard changes, any client may grab a global hotkey, and XTEST sends the paste keystroke.
On Wayland each of those is a separate protocol or portal that differs per compositor, and GNOME, the most common desktop, has declined to let an app watch the clipboard at all.

So Linux support means X11 sessions.
`crates/clip-linux` is the adapter, written against the X protocol directly with `x11rb`, with no system libraries of its own.
On a Wayland session Clipdeck says it doesn't work there yet and quits, and the install command refuses to run.
Running half-working through XWayland was rejected: it would see only the copies made in the few apps still on X11, which looks like a broken clipboard manager rather than an unsupported one.

Two things work differently from the other platforms, both found by running the app:

- **A hotkey opens its menu when its keys are let go.**
  A pressed hotkey holds the X keyboard until its key goes up.
  A GTK menu that can't take the keyboard never appears, and the call that shows it never returns, which left Clipdeck deaf to every later hotkey.
- **Number shortcuts are caught by a key handler on the open menu.**
  A GTK popup menu neither shows accelerators nor acts on them, so "Ctrl+1" does not appear beside the items, though it works.

Other consequences:

- **The source app is the process that owns the clipboard**, asked of the X server, so the Exclusion list takes program names such as `keepassxc`.
- **A copy marked `x-kde-passwordManagerHint: secret` is not captured.**
  That is KDE's convention, which password managers follow on every desktop.
- **Pasting is Ctrl+V.**
  A terminal wants Ctrl+Shift+V, so there the Clip is only copied.
- **Only the clipboard is captured**, not the primary selection that holds whatever text is highlighted.
- **The tray icon needs a tray host.**
  GNOME has none without an extension; the hotkeys work regardless.

It ships as an AppImage, for 64-bit Intel and AMD, installed by the same one-line command as macOS (ADR-0009).
An AppImage needs no administrator rights and can replace itself, so the in-app updater works as it does elsewhere.
A `.deb` was considered and left out: it would be a second way to install, and one whose updates need a root prompt.
A copy that is not an AppImage (built from source, say) does not offer to update itself.

Nothing here runs on a developer's desk, so CI stands in for one: `.github/scripts/linux-e2e.sh` starts the built app in a headless X11 session and uses it, from copying to pasting.
It has not been tried on a real desktop.

**Status**: accepted
