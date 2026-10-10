# Selecting a Clip pastes it directly, instead of only copying

The original interview explicitly settled on copy-only: selecting a Clip would only write it to the clipboard, and the user would paste it themselves.
Live testing on Windows surfaced the real friction in that design: the global hotkey that opens the popup steals the keystroke from whatever app the user was about to paste into, so after selecting a Clip there was no app left in focus to paste into at all.
The user asked for selection to paste directly instead.

On Windows, selecting a Clip now: writes it to the clipboard (unchanged), hides the popup, restores foreground focus to whatever window was focused right before the popup opened, and synthesizes a `Ctrl+V` keystroke into it.

This has two real costs we're accepting knowingly rather than discovering later:

- **Synthetic keystroke injection is a classic malware/keylogger heuristic.**
  Reputation systems (including the same Smart App Control / Defender stack already blocking our unsigned build per ADR-0002) tend to score "injects input into other processes" more suspiciously, not less.
- **It isn't 100% reliable by OS design.**
  Secure input fields (password boxes) and elevated/admin windows can legitimately refuse synthetic input from a non-elevated process.
  When that happens, Clipdeck silently no-ops the paste - the Clip is still on the clipboard, so a manual `Ctrl+V` still works as a fallback.

macOS auto-paste was first deferred, and macOS shipped copy-only.
Testing on a real Mac showed the same friction as on Windows: choosing a Clip looked like it did nothing, because the user still had to press `Cmd+V` themselves.
So macOS now pastes too: selecting a Clip writes it to the clipboard and synthesizes `Cmd+V` (a `CGEvent` key press) into the frontmost app.
A menu never takes focus on macOS, so there is no window to restore first, and the tray menu pastes as well as the hotkey menus.

This adds costs of its own:

- **It needs the Accessibility permission.**
  macOS only lets an app send keystrokes to other apps once the user allows it under System Settings > Privacy & Security > Accessibility.
  The first time a Clip is chosen without it, macOS shows its own prompt; until it is granted, Clipdeck stays copy-only.
- **The grant may not survive an update.**
  macOS ties the permission to the app's code signature, and our builds are only ad-hoc signed (ADR-0002), so a new version can look like a different app and need the permission switched off and on again.
- **The keystroke is the key in the V position, not the letter V.**
  On a layout that moves V (Dvorak, for example) it sends a different shortcut; the Clip is still on the clipboard.

**Status**: accepted
