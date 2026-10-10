# Installing Clipdeck

Clipdeck is not distributed through any app store.
Download it from the [GitHub Releases page](https://github.com/baala3/clipdeck/releases/latest).
Once installed, Clipdeck keeps itself up to date: it checks for a new version shortly after it starts and once a day, and asks before installing one.
You can also check from Settings > About.

Clipdeck starts at login by default.
Turn that off in Settings > General.

Clipdeck is unsigned (see [ADR-0002](adr/0002-ship-unsigned.md)), so both Windows and macOS warn you the first time you open a copy downloaded with a browser.
The one-line installs below avoid the warning, because the operating systems only check files that a browser downloaded.
If you download by hand instead, the steps below get past the warning once; updates don't ask again.

## Windows

Open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/baala3/clipdeck/main/install.ps1 | iex
```

This downloads the latest installer, runs it, and starts Clipdeck.

To install by hand instead, download `Clipdeck_<version>_x64-setup.exe` and run it.
Either way it installs for your user only, so it doesn't need administrator rights.

- **SmartScreen** ("Windows protected your PC"), for an installer downloaded by hand: click "More info", then "Run anyway".
- **Smart App Control**: on some Windows 11 machines this blocks unsigned apps outright, with no per-file "run anyway" option.
  There is currently no workaround besides real code signing, or turning Smart App Control off entirely - which Microsoft says requires reinstalling Windows to undo, so we don't recommend it.
  See [ADR-0002](adr/0002-ship-unsigned.md) for details.

Clipdeck lives in the notification area (system tray).
Settings are kept in `%APPDATA%\dev.clipdeck.app\settings.toml`, which you can also edit by hand.
History and Pinned items are kept in `%LOCALAPPDATA%\dev.clipdeck.app`.

To uninstall, use "Add or remove programs".
Tick "Delete the application data" in the uninstaller to also remove your settings, History, and Pinned items.

### Moving from the 0.1.0 portable zip

The 0.1.0 zip kept History and Pinned items next to `clipdeck-app.exe`.
To keep them, quit the old Clipdeck from its tray icon, then move `clipdeck-history.sqlite3` and `clipdeck-pinned.sqlite3` from that folder into `%LOCALAPPDATA%\dev.clipdeck.app` before starting the installed version.
Your settings carry over on their own.
The old folder can then be deleted.

## macOS

Clipdeck runs on macOS 11 or later, on both Apple silicon and Intel Macs.
Open Terminal and run:

```sh
curl -fsSL https://raw.githubusercontent.com/baala3/clipdeck/main/install.sh | sh
```

This downloads the latest version into Applications and starts it.

To install by hand instead, download `Clipdeck_<version>_universal.dmg`, open it, and drag Clipdeck into Applications.
The first time you open a copy installed that way, macOS says it can't verify the developer.

- **Already blocked**: run `xattr -dr com.apple.quarantine /Applications/Clipdeck.app` in Terminal, then open Clipdeck again.
- **macOS 15 (Sequoia) and later**: click "Done", then open System Settings > Privacy & Security, scroll down to the message about Clipdeck, and click "Open Anyway".
- **macOS 14 and earlier**: Control-click Clipdeck in Applications, choose "Open", then "Open" again.

Clipdeck lives in the menu bar and has no Dock icon.

Choosing a Clip pastes it into the app you're working in, which macOS only allows once you give Clipdeck the Accessibility permission.
The first time you choose a Clip, macOS asks for it: open System Settings > Privacy & Security > Accessibility and switch Clipdeck on.
Until then, choosing a Clip only copies it, and you paste with Cmd+V yourself.
If pasting stops working after an update, switch Clipdeck off and on again in that list (or remove it with the "-" button and add it back).
Settings, History, and Pinned items are kept in `~/Library/Application Support/dev.clipdeck.app`.

To uninstall, quit Clipdeck from its menu bar icon, drag it from Applications to the Trash, and delete `~/Library/Application Support/dev.clipdeck.app` and `~/Library/LaunchAgents/Clipdeck.plist`.
