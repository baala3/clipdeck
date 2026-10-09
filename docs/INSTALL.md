# Installing Clipdeck

Clipdeck is not distributed through any app store.
Download it directly from the [GitHub Releases page](https://github.com/baala3/clipdeck/releases).

## Windows

Download the `clipdeck-<version>-windows-x64-portable.zip` asset and unzip it anywhere.
There is no installer: double-click `clipdeck-app.exe` inside the unzipped folder to run it.
Keep `clipdeck-app.exe` and `WebView2Loader.dll` in the same folder, or it will not start.
Settings are kept in `%APPDATA%\dev.clipdeck.app\settings.toml`, which you can also edit by hand.
History and Pinned items are kept in `%LOCALAPPDATA%\dev.clipdeck.app`.
Earlier builds kept them next to `clipdeck-app.exe`; the first launch of a newer build moves them there automatically.
To uninstall, quit Clipdeck from its tray icon, then delete the folder, `%APPDATA%\dev.clipdeck.app`, and `%LOCALAPPDATA%\dev.clipdeck.app`.

Clipdeck is unsigned (see [ADR-0002](adr/0002-ship-unsigned.md)), so Windows may warn you on first run.

- **SmartScreen** ("Windows protected your PC"): click "More info", then "Run anyway".
- **Smart App Control**: on some Windows 11 machines this can block the app outright, with no per-file "run anyway" option.
  There is currently no workaround for this besides real code signing, or turning Smart App Control off entirely - which Microsoft says requires reinstalling Windows to undo, so we don't recommend it.
  See [ADR-0002](adr/0002-ship-unsigned.md) for details.

## macOS

Not yet packaged - see [ADR-0001](adr/0001-tauri-shared-core.md) and the open distribution ticket.
Once a `.dmg` is published, Gatekeeper will block first launch until you right-click the app and choose "Open" (or run the one-time terminal bypass Apple documents for unsigned apps).
