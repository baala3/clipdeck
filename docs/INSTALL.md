# Installing Clipdeck

Clipdeck is installed with one command, on both Windows and macOS.
There is no installer to download and no app store.

Once installed, Clipdeck keeps itself up to date: it checks for a new version shortly after it starts and once a day, and asks before installing one.
You can also check from Settings > About.

Clipdeck starts at login by default.
Turn that off in Settings > General.

## Windows

Clipdeck runs on 64-bit Windows 10 and 11.
Open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/baala3/clipdeck/main/install.ps1 | iex
```

This downloads the latest version, installs it, and starts Clipdeck.
It installs for your user only, so it doesn't need administrator rights.
Run the same command again any time to reinstall or to jump to the latest version.

- **Smart App Control**: on some Windows 11 machines this blocks unsigned apps outright, with no "run anyway" option, and the command can't get around it.
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
Run the same command again any time to reinstall or to jump to the latest version.

Clipdeck lives in the menu bar and has no Dock icon.

Choosing a Clip pastes it into the app you're working in, which macOS only allows once you give Clipdeck the Accessibility permission.
The first time you choose a Clip, macOS asks for it: open System Settings > Privacy & Security > Accessibility and switch Clipdeck on.
Until then, choosing a Clip only copies it, and you paste with Cmd+V yourself.
If pasting stops working after an update, switch Clipdeck off and on again in that list (or remove it with the "-" button and add it back).
Settings, History, and Pinned items are kept in `~/Library/Application Support/dev.clipdeck.app`.

To uninstall, quit Clipdeck from its menu bar icon, drag it from Applications to the Trash, and delete `~/Library/Application Support/dev.clipdeck.app` and `~/Library/LaunchAgents/Clipdeck.plist`.

## Why a command

Clipdeck is unsigned ([ADR-0002](adr/0002-ship-unsigned.md)).
Windows and macOS only distrust an unsigned app when a browser downloaded it: Windows shows a SmartScreen warning, and macOS refuses to open it.
A download made by PowerShell or `curl` isn't marked that way, so the same app installs and opens normally.
That makes the commands the one route that works for everyone, and the only one we offer ([ADR-0009](adr/0009-install-by-command-only.md)).

The scripts are short: read [install.ps1](../install.ps1) and [install.sh](../install.sh) before running them if you like.
