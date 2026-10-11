# Default hotkeys named after their menus

Amends [ADR-0006](0006-clipy-style-native-menus.md), which took the default hotkeys from Clipy: main menu Ctrl/Cmd+Shift+V, History Ctrl/Cmd+Alt+V, Pinned Ctrl/Cmd+Shift+B.

Those three shared no pattern: two modifier combinations, and letters (V, V, B) that say nothing about which menu opens.
The defaults are now one modifier pair and one letter per menu:

| Menu | Default hotkey |
| --- | --- |
| Main menu (History and Pinned) | Ctrl/Cmd+Shift+D |
| History | Ctrl/Cmd+Shift+H |
| Pinned | Ctrl/Cmd+Shift+P |

Clear History stays unbound.

## Consequences

- Only new installs get these.
  The settings file records every hotkey the first time Clipdeck runs, so an existing install keeps Ctrl/Cmd+Shift+V, Ctrl/Cmd+Alt+V and Ctrl/Cmd+Shift+B until the user changes them in Settings.
  We chose not to rewrite a hotkey someone is already using.
- A global hotkey wins over the focused app, and these letters are popular: Ctrl+Shift+P is the command palette in VS Code and opens a private window in Firefox, and Ctrl+Shift+H and Ctrl+Shift+D are used by browsers and editors too.
  A user who needs one of those rebinds or unbinds the Clipdeck hotkey in Settings.
