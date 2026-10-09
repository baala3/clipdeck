# clipdeck - Tech Stack Research (Primary Sources)

Research date: 2026-10-09.
Scope: Tauri v2 plugin support, Windows/macOS clipboard change detection APIs, the nspasteboard.org "do not persist" convention, and feature/license facts for five existing clipboard manager projects.
All claims below are sourced from official documentation, official plugin repos, official project README/docs, or official vendor sites.
Where a primary source could not be located, this is stated explicitly rather than substituting a secondary source.

## 1. Tauri v2: tray, global shortcuts, clipboard, and size claims

### System tray

- System tray is a core Tauri feature (not a separate plugin) in Tauri v2.
  It requires enabling the `tray-icon` Cargo feature flag: `tauri = { version = "2.0.0", features = [ "tray-icon" ] }`.
  Source: [System Tray - Tauri v2 docs](https://v2.tauri.app/learn/system-tray/)
- The tray API is exposed in both JavaScript and Rust, with `TrayIcon.new` on the JS side and `TrayIconBuilder` in Rust.
  Source: [System Tray - Tauri v2 docs](https://v2.tauri.app/learn/system-tray/)
- On Linux, tray click events are explicitly documented as unsupported.
  The docs state the click event "is not emitted even though the icon is shown and will still show a context menu on right click."
  Source: [System Tray - Tauri v2 docs](https://v2.tauri.app/learn/system-tray/)

### Global shortcuts

- Global shortcuts are provided by the official `tauri-plugin-global-shortcut` plugin, maintained in the `tauri-apps/plugins-workspace` monorepo (the `tauri-apps/tauri-plugin-global-shortcut` repo is a read-only mirror pointing to that monorepo for development and issues).
  Source: [tauri-apps/tauri-plugin-global-shortcut (GitHub)](https://github.com/tauri-apps/tauri-plugin-global-shortcut)
- Supported platforms are Windows, Linux, and macOS only.
  Mobile (Android, iOS) is not supported.
  Source: [Global Shortcut - Tauri v2 docs](https://v2.tauri.app/plugin/global-shortcut/)
- No shortcut permissions are enabled by default.
  The docs state: "No features are enabled by default, as we believe the shortcuts can be inherently dangerous."
  Developers must explicitly allow `allow-register`, `allow-unregister`, and `allow-is-registered` in the app's capabilities configuration.
  Source: [Global Shortcut - Tauri v2 docs](https://v2.tauri.app/plugin/global-shortcut/)

### Clipboard read/write

- The official `tauri-plugin-clipboard-manager` plugin reads and writes the system clipboard, supporting Linux, Windows, macOS, Android, and iOS.
  Install via `tauri-plugin-clipboard-manager = "2.0.0"` (Cargo) and `@tauri-apps/plugin-clipboard-manager` (npm).
  Source: [tauri-apps/tauri-plugin-clipboard-manager (GitHub)](https://github.com/tauri-apps/tauri-plugin-clipboard-manager), [Clipboard - Tauri v2 docs](https://v2.tauri.app/plugin/clipboard/)
- It supports text, image, and HTML content, with dedicated permissions `clipboard-manager:allow-read-image` / `allow-write-image` and `clipboard-manager:allow-write-html`.
  On Android and iOS, the docs note "Only plain-text content support" - i.e. image/HTML support is desktop-only.
  Source: [Clipboard - Tauri v2 docs](https://v2.tauri.app/plugin/clipboard/)
- The official clipboard-manager docs make no mention of RTF or file-list (file path) clipboard formats.

### Clipboard history

- There is no official Tauri plugin for clipboard history.
  The official `tauri-plugin-clipboard-manager` plugin and its docs only cover single-value read/write (current clipboard contents), with no history/listening API described.
  Source: [Clipboard - Tauri v2 docs](https://v2.tauri.app/plugin/clipboard/)
- A well-maintained community (non-tauri-apps) plugin, `tauri-plugin-clipboard` by CrossCopy (MIT licensed), adds clipboard change monitoring/listening (`onTextUpdate()`, `onImageUpdate()`, `onFilesUpdate()`) on top of text, HTML, RTF, file, and image read/write.
  It does not itself provide a stored "history" data structure either - it only emits events when the clipboard changes, leaving history storage to the app.
  Source: [CrossCopy/tauri-plugin-clipboard (GitHub README)](https://github.com/CrossCopy/tauri-plugin-clipboard)
- Conclusion for clipdeck planning: clipboard history tracking is not covered by any official tauri-apps plugin and would have to be hand-rolled on top of either the official clipboard-manager plugin (text/image/HTML read-write only, no change events) or the community CrossCopy plugin (adds change-listening events across more formats, still no history storage).

### Bundle size / resource footprint claims

- The official Tauri homepage states: "By using the OS's native web renderer, the size of a Tauri app can be little as 600KB."
  No explicit memory-footprint numbers or direct Electron comparison figures appear on this page.
  Source: [tauri.app homepage](https://tauri.app/)
- The official Tauri Philosophy doc does not give binary-size or resource-footprint numbers.
  Its one explicit Electron-related statement is about packaging format, not size: "And if you are coming from the Electron ecosystem - rest assured - by default Tauri only ships binaries, not ASAR files."
  Source: [Tauri Philosophy - Tauri v2 docs](https://v2.tauri.app/about/philosophy/)
- No official tauri.app/v2.tauri.app page with a direct, numeric Tauri-vs-Electron size/RAM comparison table was found.
  Numeric comparisons (e.g. "3-10 MB vs 120-200 MB", "42 MB vs 168 MB RAM", "380ms vs 1420ms startup") that circulate widely come from third-party blog posts (e.g. rustify.rs, tech-insider.org, buildmvpfast.com), not from tauri.app itself, and are therefore excluded from this primary-sourced writeup aside from flagging that they exist as secondary claims.

## 2. Windows clipboard change detection (Microsoft Learn)

- `AddClipboardFormatListener` places a window in the system-maintained clipboard format listener list; once registered, the window is posted a `WM_CLIPBOARDUPDATE` message whenever clipboard contents change.
  Minimum supported client: Windows Vista (desktop apps only).
  Source: [AddClipboardFormatListener function - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener)
- `WM_CLIPBOARDUPDATE` (`0x031D`) is "Sent when the contents of the clipboard have changed."
  Both `wParam` and `lParam` are unused and must be zero.
  Source: [WM_CLIPBOARDUPDATE message - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/dataxchg/wm-clipboardupdate)
- Microsoft's "Using the Clipboard" guide explicitly ranks the three monitoring mechanisms and recommends the modern ones: "There are three ways of monitoring changes to the clipboard. The oldest method is to create a clipboard viewer window. Windows 2000 added the ability to query the clipboard sequence number, and Windows Vista added support for clipboard format listeners. Clipboard viewer windows are supported for backward compatibility with earlier versions of Windows. New programs should use clipboard format listeners or the clipboard sequence number."
  Source: [Using the Clipboard - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard)
- The same guide states the format-listener approach is preferred specifically over the older viewer-chain approach: "This method is recommended over creating a clipboard viewer window because it is simpler to implement and avoids problems if programs fail to maintain the clipboard viewer chain properly or if a window in the clipboard viewer chain stops responding to messages."
  Source: [Using the Clipboard - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard)
- `SetClipboardViewer`'s own reference page confirms it is legacy: "The SetClipboardViewer function exists to provide backward compatibility with earlier versions of Windows. The clipboard viewer chain can be broken by an application that fails to handle the clipboard chain messages properly. New applications should use more robust techniques such as the clipboard sequence number or the registration of a clipboard format listener."
  Source: [SetClipboardViewer function - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboardviewer)
- Microsoft's docs do not use the literal word "obsolete" for `SetClipboardViewer`; they describe it as existing only "to provide backward compatibility" and steer new development away from it toward format listeners or the sequence number, as quoted above.
- Caveat on the clipboard sequence number (the polling alternative, via `GetClipboardSequenceNumber`): "Note that this is a not a notification method and should not be used in a polling loop. To be notified when clipboard contents change, use a clipboard format listener or a clipboard viewer."
  Source: [Using the Clipboard - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard)
- Delay-rendered format caveat: if a window places `NULL` in `SetClipboardData` (delayed rendering) and another application later requests that format, the window receives `WM_RENDERFORMAT`; if the window is about to be destroyed while formats remain unrendered, it receives `WM_RENDERALLFORMATS` first, and in that case it "must open the clipboard and check that the window still owns the clipboard before calling SetClipboardData, and it must close the clipboard before returning."
  Source: [Using the Clipboard - Win32 apps, Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard)
- No separate multi-window/multi-thread caveat specific to `AddClipboardFormatListener` itself is documented beyond the general viewer-chain fragility problem that format listeners were designed to avoid (quoted above); the format-listener and `WM_CLIPBOARDUPDATE` reference pages do not list additional threading restrictions.

## 3. macOS clipboard change detection (NSPasteboard)

- `NSPasteboard.changeCount` is documented as: "The change count is a computer-wide variable that increments every time the contents of the pasteboard changes (a new owner is declared). An independent change count is maintained for each named pasteboard. By examining the change count, an application can determine whether the current data in the pasteboard is the same as the data it last received."
  Source: [Pasteboard Concepts - Apple Developer Documentation Archive](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbConcepts.html)
  Note: Apple's current `developer.apple.com/documentation/appkit/nspasteboard/changecount` reference page is a JavaScript-rendered single page that could not be retrieved as static text by the tools used in this research; the quoted text above comes from Apple's older, still Apple-hosted "Pasteboard Concepts" guide (listed elsewhere by Apple as a "Retired Document" but not removed from developer.apple.com), which is the closest full-text primary source found for this property's semantics.
- That same Apple document describes only a polling model for change detection: an app must read `changeCount`, store it, and later compare a new read against the stored value to tell whether the pasteboard changed.
  It does not document any notification, delegate callback, or other push-based mechanism for `NSPasteboard` change detection.
  Source: [Pasteboard Concepts - Apple Developer Documentation Archive](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbConcepts.html)
- This research did not find an Apple-documented `NSPasteboard`-level change notification (the iOS/iPadOS-only `UIPasteboard` has its own `changedNotification`, which is a UIKit API for a different class on a different platform and is not evidence of a macOS/AppKit `NSPasteboard` push mechanism).
  The specific current Apple reference page for `changeCount` (`developer.apple.com/documentation/appkit/nspasteboard/changecount`) could not be fetched as readable text in this research; its content is treated as consistent with the archived Pasteboard Concepts guide but was not directly confirmed here, and that limitation is flagged explicitly rather than papered over.
- Taken together, the available primary Apple documentation supports the premise that `NSPasteboard` change detection on macOS is polling-based via `changeCount`, with no push/event API offered by `NSPasteboard` itself.

## 4. The nspasteboard.org "do not persist" convention

Primary source: [nspasteboard.org](https://nspasteboard.org/)

- `org.nspasteboard.ConcealedType`: "This marker's presence indicates content that should be treated as confidential. If displayed on screen it should be visually obfuscated."
  Source: [nspasteboard.org](https://nspasteboard.org/)
- `org.nspasteboard.TransientType`: "This marker's presence indicates content will be on the pasteboard only momentarily. The pasteboard will either be restored to previous content, or the current content will be replaced within seconds."
  Source: [nspasteboard.org](https://nspasteboard.org/)
- `org.nspasteboard.AutoGeneratedType`: "This marker's presence indicates content was generated by an application; the user had no intention to Copy this content and may not expect to find it on the pasteboard."
  Source: [nspasteboard.org](https://nspasteboard.org/)
- A fourth, related marker is also defined on the same page: `org.nspasteboard.source`, described as: "This marker's presence indicates that the source of the content is the application with the bundle identifier matching its UTF-8 string content."
  Source: [nspasteboard.org](https://nspasteboard.org/)
- The page frames this as a voluntary convention aimed at two groups of developers: apps that put sensitive or momentary content on the pasteboard (e.g. password managers, text-expansion tools, automation utilities) should set these markers, and clipboard-history/clipboard-manager apps should check for these markers and skip recording matching items.
  Source: [nspasteboard.org](https://nspasteboard.org/)
- For clipdeck's macOS support, this means: to respect password-manager convention, clipdeck's macOS pasteboard-read code should check incoming pasteboard items for the presence of `org.nspasteboard.ConcealedType` and `org.nspasteboard.TransientType` (and optionally `org.nspasteboard.AutoGeneratedType`) and skip persisting those items to history.

## 5. Existing clipboard manager projects (own README/docs/source)

### Clipy (macOS)

- License: MIT.
  The README states: "Clipy is available under the MIT license. See the LICENSE file for more info."
  Source: [Clipy/Clipy README (GitHub)](https://github.com/Clipy/Clipy)
- Images/files in history: the official Clipy site states it is "a clipboard extension app supporting multiple formats such as plain text and images" (「プレーンテキストや画像といった複数の形式に対応したクリップボード拡張アプリです。」), confirming image support in addition to text.
  Source: [clipy-app.com](https://clipy-app.com)
  The GitHub README itself does not restate this feature list; it focuses on build/install/licensing/localization information.
  Source: [Clipy/Clipy README (GitHub)](https://github.com/Clipy/Clipy)
- Default history retention cap: not stated in either the README or the official site content reviewed.
  This is flagged explicitly as not found in a primary source, rather than assumed.
- Plugin/scripting/extension API: none found.
  Clipy's official site documents a "Snippets" feature (user-defined boilerplate text snippets), which is a built-in app feature, not a third-party plugin/scripting/extension API.
  Source: [clipy-app.com](https://clipy-app.com); [Clipy/Clipy README (GitHub)](https://github.com/Clipy/Clipy)

### Maccy (macOS)

- License: MIT.
  Source: [p0deje/Maccy README (GitHub)](https://github.com/p0deje/Maccy)
- Images/files in history: the official README and official site (maccy.app) do not explicitly document image or file support in their feature text.
  However, Maccy's own GitHub issue tracker shows an "Images" checkbox in the app's Storage preferences and ongoing work on image handling (e.g. dragging images out of history), indicating image history support exists in the app even though it is not called out in the README's feature list.
  Source: [p0deje/Maccy README (GitHub)](https://github.com/p0deje/Maccy); [p0deje/Maccy Issue #44 "Support images" (GitHub)](https://github.com/p0deje/Maccy/issues/44); [p0deje/Maccy Issue #1259 (GitHub)](https://github.com/p0deje/Maccy/issues/1259)
- Default history retention cap: Maccy's own issue tracker documents a hardcoded maximum of 999 history items, attributed by the maintainer to memory-management constraints ("Because Maccy is not working its best in regards to memory management, it will be solved eventually in #310").
  A later 2.0 beta discussion in the same repo describes raising this maximum to 9999.
  These figures come from the project's own GitHub issue/discussion threads rather than the README itself, which states no numeric cap.
  Source: [p0deje/Maccy Issue #456 (GitHub)](https://github.com/p0deje/Maccy/issues/456); [p0deje/Maccy Discussion #818 "Maccy 2.0.0 beta" (GitHub)](https://github.com/p0deje/Maccy/discussions/818)
- Plugin/scripting/extension API: none found.
  The README documents only user-facing keyboard shortcuts and preferences, with no mention of a plugin, scripting, or extension interface.
  Source: [p0deje/Maccy README (GitHub)](https://github.com/p0deje/Maccy)

### Ditto (Windows)

- License: GPL-3.0, as shown on the repository itself.
  Source: [sabrogden/Ditto (GitHub)](https://github.com/sabrogden/Ditto)
- Images/files in history: yes.
  The README states Ditto "allows you to save any type of information that can be put on the clipboard, text, images, html, custom formats."
  Source: [sabrogden/Ditto README (GitHub)](https://github.com/sabrogden/Ditto)
- Default history retention cap: Ditto's own wiki confirms a configurable cap exists - "Two ways to auto delete clips" via Options - General: "Maximum number of saved copies" and "Paste entries expire after X days" - but the wiki page itself does not state what the default numeric value is.
  A specific default of 500 appears only in the project's SourceForge user discussion forum, which is community/user-generated content rather than official README/wiki documentation, so that specific number is flagged here as not confirmed by a primary source.
  Source: [sabrogden/Ditto Wiki - Auto Delete (GitHub)](https://github.com/sabrogden/Ditto/wiki/Auto-Delete)
- Plugin/scripting/extension API: yes.
  Ditto's own wiki documents an "On Copy Scripts" / "On Paste Scripts" feature (Options - Advanced) using ChaiScript, exposing functions such as `GetAsciiString()`, `SetAsciiString()`, `GetClipMD5()`, `GetClipSize()`, `GetActiveApp()`, `GetActiveAppTitle()`, `AsciiTextMatchesRegex()`, `AsciiTextReplaceRegex()`, `FormatExists()`, `RemoveFormat()`, and `SetParentId()`, with the caveat "Only ascii text is supported, limitation of chaiscript."
  Source: [sabrogden/Ditto Wiki - Scripting (GitHub)](https://github.com/sabrogden/Ditto/wiki/Scripting)

### ClipAngel (Windows)

- License: GPL-3.0, as shown on the repository itself.
  Source: [tormozit/ClipAngel (GitHub)](https://github.com/tormozit/ClipAngel)
- Images/files in history: yes.
  The project's own wiki (hosted on SourceForge, the project's official distribution site) states ClipAngel "can log clipboard history and record data in several formats: plain text, RTF (Rich Text Format) and images (BMP), html, files and so on."
  Source: [ClipAngel Wiki - Description (SourceForge)](https://sourceforge.net/p/clip-angel/wiki/Description/)
- Default history retention cap: not stated.
  Neither the GitHub repository content nor the official SourceForge wiki description page reviewed states a default maximum number of retained items; this is flagged as not found in a primary source.
- Plugin/scripting/extension API: not found.
  No plugin, scripting, or extension API is documented in the GitHub repository or the official SourceForge wiki description page reviewed.

### ClipboardFusion (Windows) - closed-source, Binary Fortress Software

- License: proprietary/commercial, not open source.
  The official EULA states the free tier is personal-use only: "The free version of ClipboardFusion is ONLY valid for PERSONAL USE," and that paid tiers (Pro Standard, Pro Personal, Pro Site, Pro Enterprise) are required for company/organizational use.
  The EULA frames the software as licensed rather than sold ("The software is licensed, not sold"), consistent with closed-source commercial freeware-plus-paid-upgrade distribution; the terms "open source," "GPL," or "MIT" do not appear.
  Source: [ClipboardFusion License (EULA) - Binary Fortress Software](https://www.clipboardfusion.com/License/)
- No public source-code repository was found for ClipboardFusion; this is consistent with the task's premise that it is closed-source freeware/commercial software, and no GitHub/GitLab repo claim is made here.
- Images/files in history: the official Free-vs-Pro comparison page lists "image" storage ("storing images locally") as available in both the Free tier and all Pro tiers.
  Source: [Free vs Pro - ClipboardFusion by Binary Fortress Software](https://www.clipboardfusion.com/Compare/)
- Default history retention cap: not stated.
  Neither the homepage nor the Free-vs-Pro comparison page reviewed states a numeric cap on clipboard history size or item count; this is flagged as not found in a primary source.
- Plugin/scripting/extension API: yes, in the form of user-authored macros.
  The official homepage states: "Create your own macros using C# in the integrated editor to perform completely customized transformations on your text."
  The Free-vs-Pro comparison page lists "Powerful macros (custom code)" as available in both Free and all Pro tiers, without detailing any functional differences between tiers.
  Source: [ClipboardFusion homepage - Binary Fortress Software](https://www.clipboardfusion.com/); [Free vs Pro - ClipboardFusion by Binary Fortress Software](https://www.clipboardfusion.com/Compare/)

## Summary of gaps where a primary source was not found

- Tauri: no official tauri.app page gives a direct numeric Tauri-vs-Electron size/RAM/startup comparison table; only the generic "little as 600KB" claim is confirmed on tauri.app itself.
- macOS: the current live Apple reference page for `NSPasteboard.changeCount` (`developer.apple.com/documentation/appkit/nspasteboard/changecount`) could not be rendered as text by the fetch tooling used; the polling-only behavior was instead confirmed from Apple's older (but still Apple-hosted) "Pasteboard Concepts" guide.
- Clipy: no stated default history retention cap found in the README or official site.
- Maccy: image/file support is not stated in the README/site text itself, only inferable from the project's own GitHub issues; the 999/9999 history cap figures come from GitHub issues/discussions rather than the README.
- Ditto: a retention-cap setting is confirmed to exist in the official wiki, but its default numeric value (500) is only documented in a community discussion forum, not in the official README/wiki.
- ClipAngel: no stated default history retention cap and no plugin/scripting API found in the GitHub repo or official SourceForge wiki description page.
- ClipboardFusion: no stated default history retention cap found on the official site pages reviewed.
