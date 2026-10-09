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

macOS auto-paste is deferred.
The equivalent there (`CGEvent` keystroke synthesis via the Accessibility API) needs the user to grant Clipdeck Accessibility permission, a materially bigger ask than anything the macOS adapter has needed so far, and it's unverifiable in this dev environment regardless (see the macOS adapter's own unverified-status note).
Until that's built, macOS stays copy-only.

**Status**: accepted
