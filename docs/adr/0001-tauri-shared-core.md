# Use Tauri with a shared Rust core, built cross-platform from day one

Clipdeck targets Windows and macOS with one maintainable codebase, a small footprint, and no subscription/monetization model to fund per-OS native teams. Electron was rejected for its much larger bundled footprint; two fully-native codebases (Swift + C#/WPF) were rejected as the most work for a solo/small open-source project to maintain long-term, despite best-in-class performance.

We decided on Tauri: a Rust core with OS-specific clipboard-monitoring and tray/hotkey code behind one thin interface, with both OS builds kept compiling from the first commit rather than shipping Windows-only and porting later. Research (`docs/research/tech-stack.md`) confirmed Tauri has first-party tray and global-shortcut support, but no framework - Tauri included - provides clipboard *history* out of the box: Windows exposes change events (`AddClipboardFormatListener`), macOS only exposes polling (`NSPasteboard.changeCount`). That asymmetry has to be designed for explicitly inside the shared-core abstraction, not discovered later.

**Status**: accepted
