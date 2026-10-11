# clipdeck - Linux Support Research (Primary Sources)

Research date: 2026-10-11.
Scope: verification of the platform claims in issue #21 ("Linux support") for X11 and for Wayland under GNOME (Mutter), KDE (KWin), and wlroots-based compositors (Sway, Hyprland).
All claims below are sourced from protocol specifications, official documentation, or the source code and READMEs of the projects that own the behavior.
Where a primary source could not be located, this is stated explicitly rather than substituting a secondary source.
Nothing in this document was tested on a running Linux desktop session; every statement is a reading of a spec, a doc page, or source code.
Source code was read at the branch or tag named in each link on the research date, and compositor behavior can differ in the versions distributions ship.
Versions clipdeck currently locks (from `Cargo.lock`): tauri 2.12.2, muda 0.20.0, tray-icon 0.25.1, global-hotkey 0.8.0, tauri-plugin-updater 2.13.2, auto-launch 0.6.0, arboard 3.6.1.

## 1. Clipboard change detection

### X11: XFixes

- The XFixes specification states the problem and the fix: "Applications wishing to monitor the contents of current selections must poll for selection changes. XFIXES improves this by providing an event delivered whenever the selection ownership changes."
  Source: [XFIXES protocol specification, section 6 "Selection Tracking" (xorgproto)](https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/raw/master/fixesproto.txt)
- The request is `SelectSelectionInput` with `window`, `selection` (an atom) and `event-mask`, and it "Selects for events to be delivered to window when various causes of ownership of selection occur."
  Source: [XFIXES protocol specification (xorgproto)](https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/raw/master/fixesproto.txt)
- The resulting `SelectionNotify` event carries `subtype`, `window`, `owner` (a window), `selection`, `timestamp` and `selection-timestamp`, and the subtypes are `SetSelectionOwner`, `SelectionWindowDestroy` and `SelectionClientClose`.
  The event reports an ownership change and the new owner window only; it does not carry the data or the list of offered formats.
  Source: [XFIXES protocol specification (xorgproto)](https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/raw/master/fixesproto.txt)

### Wayland: plain `wl_data_device` (no extension)

- The core protocol sends the clipboard only to the focused client: "The selection event is sent to a client immediately before receiving keyboard focus and when a new selection is set while the client has keyboard focus."
  The offer is also only valid while focused: "The data_offer is valid until a new data_offer or NULL is received or until the client loses keyboard focus."
  Source: [wayland.xml, `wl_data_device.selection` (wayland core protocol)](https://gitlab.freedesktop.org/wayland/wayland/-/raw/main/protocol/wayland.xml)
- A background app with no focused surface therefore receives no selection events and holds no valid offer under the core protocol alone.
  This is a direct reading of the two sentences quoted above, not a separately documented statement.

### Wayland: `ext-data-control-v1` and `wlr-data-control-unstable-v1`

- `ext-data-control-v1` describes itself as: "This protocol allows a privileged client to control data devices. In particular, the client will be able to manage the current selection and take the role of a clipboard manager."
  Source: [ext-data-control-v1.xml (wayland-protocols, staging)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/staging/ext-data-control/ext-data-control-v1.xml)
- Its selection event is not tied to focus: "The selection event is sent to a client when a new selection is set," and "The first selection event is sent upon binding the ext_data_control_device object."
  Before each selection event the compositor introduces an offer object that "will send out ext_data_control_offer.offer events to describe the MIME types it offers."
  Source: [ext-data-control-v1.xml (wayland-protocols, staging)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/staging/ext-data-control/ext-data-control-v1.xml)
- `wlr-data-control-unstable-v1` is the older protocol with the same shape (manager version 2, with primary selection added in version 2).
  Its own file now says: "Note! This protocol is deprecated and not intended for production use. For clipboard management, use the ext-data-control-v1 protocol."
  Source: [wlr-data-control-unstable-v1.xml (wlr-protocols)](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/raw/master/unstable/wlr-data-control-unstable-v1.xml)
- Neither protocol defines any event or field that identifies the application that set the selection; the offer interface only lists MIME types and transfers data.
  Source: [ext-data-control-v1.xml (wayland-protocols, staging)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/staging/ext-data-control/ext-data-control-v1.xml)

### Which compositors implement data-control

- wlroots: `types/wlr_ext_data_control_v1.c` exists at tag 0.19.0 and does not exist at tag 0.18.2, and `types/wlr_data_control_v1.c` (the wlr protocol) is present on master.
  Source: [wlroots 0.19.0 `types/wlr_ext_data_control_v1.c`](https://gitlab.freedesktop.org/wlroots/wlroots/-/raw/0.19.0/types/wlr_ext_data_control_v1.c), [wlroots 0.18.2 path (returns 404)](https://gitlab.freedesktop.org/wlroots/wlroots/-/raw/0.18.2/types/wlr_ext_data_control_v1.c), [wlroots `types/` tree listing (GitLab API)](https://gitlab.freedesktop.org/api/v4/projects/wlroots%2Fwlroots/repository/tree?path=types&per_page=100&page=1)
- Sway: master creates both globals (`wlr_data_control_manager_v1_create` and `wlr_ext_data_control_manager_v1_create`).
  At tag 1.11 both calls are present; at tags 1.10 and 1.10.1 only the wlr one is.
  Source: [sway master `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/master/sway/server.c), [sway 1.11 `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/1.11/sway/server.c), [sway 1.10 `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/1.10/sway/server.c)
- Sway treats both data-control globals as privileged, and its global filter hides privileged globals only from clients that connected through a security context (`return security_context == NULL;`).
  Source: [sway master `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/master/sway/server.c)
- Hyprland: the main branch registers both `zwlr_data_control_manager_v1` (version 2) and `ext_data_control_manager_v1` (version 1).
  Which Hyprland release first shipped the ext protocol was not checked.
  Source: [Hyprland main `src/managers/ProtocolManager.cpp`](https://raw.githubusercontent.com/hyprwm/Hyprland/main/src/managers/ProtocolManager.cpp)
- KWin: the protocol list in `src/wayland/CMakeLists.txt` contains only `wlr-data-control-unstable-v1.xml` at tags v5.27.11, v6.0.0 and v6.3.0, both `wlr-data-control-unstable-v1.xml` and `ext-data-control-v1.xml` at v6.4.0, and only `ext-data-control-v1.xml` at v6.5.0 and on master.
  Read literally, KWin 6.5 and later no longer build the wlr protocol, so a client that speaks only `wlr-data-control` would not find it there.
  Source: [KWin v6.3.0 CMakeLists](https://invent.kde.org/plasma/kwin/-/raw/v6.3.0/src/wayland/CMakeLists.txt), [KWin v6.4.0 CMakeLists](https://invent.kde.org/plasma/kwin/-/raw/v6.4.0/src/wayland/CMakeLists.txt), [KWin v6.5.0 CMakeLists](https://invent.kde.org/plasma/kwin/-/raw/v6.5.0/src/wayland/CMakeLists.txt), [KWin master CMakeLists](https://invent.kde.org/plasma/kwin/-/raw/master/src/wayland/CMakeLists.txt)
- KWin lists `ext_data_control_manager_v1` among its `restrictedInterfaces`, and its filter denies restricted interfaces only when `client->isSandboxed()` is true; otherwise it returns `true`.
  Source: [KWin master `src/wayland_server.cpp`](https://invent.kde.org/plasma/kwin/-/raw/master/src/wayland_server.cpp)
- Mutter (GNOME): the main branch has no data-control implementation.
  `src/wayland/` contains no data-control source file, `src/wayland/protocol/` holds only `gtk-shell.xml`, `mutter-x11-interop.xml` and `xdg-session-management-v1.xml`, and `src/meson.build` has no match for "data-control" or "data_control".
  Source: [Mutter main `src/wayland` tree](https://gitlab.gnome.org/GNOME/mutter/-/tree/main/src/wayland), [Mutter main `src/meson.build`](https://gitlab.gnome.org/GNOME/mutter/-/raw/main/src/meson.build)
- Mutter's maintainers have declined both protocols.
  On the ext-data-control request (issue 3941, closed 2025-02-28), maintainer `jadahl` wrote: "There is no plan to support unmanaged clipboard access via Wayland, since it more or less provides a sandbox hole, that'd allow applications to scrape the clipboard for sensitive data. In general, for privacy sensitive data control, portals have been the path to get clipboard integration, as has been done for remote desktop like use cases, so far."
  Maintainer `swick` closed it with: "I'm going to close this because there is consensus among mutter developers that we do not want to add those kinds of protocols, for reasons mentioned above."
  On the earlier wlr-data-control request (issue 524, closed 2019-03-22), maintainer `jadahl` wrote: "There is no plan to support external clipboard managers."
  A third request (issue 4333) was closed on 2025-09-20 as a duplicate of 3941.
  The issue pages were fetched, and the comment text was read through GitLab's public GraphQL API because the HTML pages do not include comments without JavaScript.
  Source: [Mutter issue 3941 "Support ext-data-control (wayland protocol)"](https://gitlab.gnome.org/GNOME/mutter/-/issues/3941), [Mutter issue 524 "Support wlr-data-control (wayland protocol)"](https://gitlab.gnome.org/GNOME/mutter/-/issues/524), [Mutter issue 4333](https://gitlab.gnome.org/GNOME/mutter/-/issues/4333)

### GNOME alternative found: the Clipboard portal

- XDG Desktop Portal defines `org.freedesktop.portal.Clipboard` (version 1), which "does not create its own sessions. Instead, it extends sessions created by other portals with clipboard access. Currently, the Remote Desktop and Input Capture portals support clipboard integration."
  Source: [Clipboard portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Clipboard.html)
- It has a change signal: `SelectionOwnerChanged` "Notifies the session that the clipboard selection has changed," with a `mime_types` list and a `session_is_owner` flag, and "Caller will only be notified if clipboard access was given after starting the session."
  Reading uses `SelectionRead()` with a MIME type, which returns a file descriptor.
  Source: [Clipboard portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Clipboard.html)
- Backends that declare `org.freedesktop.impl.portal.Clipboard`: xdg-desktop-portal-gnome (its NEWS lists "Implement the Clipboard portal" under 45.beta) and xdg-desktop-portal-kde (present in `kde.portal` at v6.4.0 and master, absent at v6.3.0).
  xdg-desktop-portal-wlr and xdg-desktop-portal-hyprland do not declare it.
  Source: [xdg-desktop-portal-gnome `data/meson.build`](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/data/meson.build), [xdg-desktop-portal-gnome NEWS](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/NEWS), [xdg-desktop-portal-kde v6.4.0 `kde.portal`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/raw/v6.4.0/data/kde.portal), [xdg-desktop-portal-kde v6.3.0 `kde.portal`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/raw/v6.3.0/data/kde.portal), [xdg-desktop-portal-wlr `wlr.portal`](https://raw.githubusercontent.com/emersion/xdg-desktop-portal-wlr/master/wlr.portal), [xdg-desktop-portal-hyprland `hyprland.portal`](https://raw.githubusercontent.com/hyprwm/xdg-desktop-portal-hyprland/master/hyprland.portal)
- Bitwarden's desktop client uses exactly this route on GNOME.
  Its source comment reads: "On GNOME/Wayland the direct `arboard` does not work because GNOME does not implement `zwlr_data_control_manager_v1`. This implementation works via the RemoteDesktop portal instead. Essentially, Bitwarden starts a RemoteDesktop session and gets persistent rights to restart it, resulting in a single permission prompt."
  Bitwarden uses it for one-shot read and write, not for continuous monitoring.
  Source: [bitwarden/clients `desktop_native/core/src/clipboard/portal_backend.rs`](https://raw.githubusercontent.com/bitwarden/clients/main/apps/desktop/desktop_native/core/src/clipboard/portal_backend.rs)
- Not confirmed: whether a long-lived RemoteDesktop session held open by a clipboard manager on GNOME delivers `SelectionOwnerChanged` for every copy, what the user sees while such a session is open, and whether GNOME's session-restore token avoids the consent dialog on each login.
  No primary source describing this use was found.
- Not confirmed: whether an X11 client running under XWayland on GNOME receives XFixes selection events for copies made in native Wayland apps while it is unfocused.
  No Mutter documentation on this was found, and the Mutter XWayland selection code was not read.

### Rust crates

- `x11rb` (0.14.0 on docs.rs) exposes the XFixes extension, including `select_selection_input` and `SelectionNotifyEvent`, and the XTest extension, including `fake_input`.
  These are raw protocol bindings; there is no clipboard-level API.
  Source: [x11rb `protocol::xfixes` (docs.rs)](https://docs.rs/x11rb/latest/x11rb/protocol/xfixes/index.html), [x11rb `protocol::xtest` (docs.rs)](https://docs.rs/x11rb/latest/x11rb/protocol/xtest/index.html)
- `wl-clipboard-rs` states: "This crate is intended to be used by terminal applications, clipboard managers and other utilities which don't spawn Wayland surfaces (windows)," and "The protocol used for clipboard interaction is `ext-data-control` or `wlr-data-control`."
  Source: [wl-clipboard-rs README](https://raw.githubusercontent.com/YaLTeR/wl-clipboard-rs/master/README.md)
- `wl-clipboard-rs` gained change notification in v0.9.4 (released 2026-09-28): "Added a `watch` module that provides a blocking API for monitoring clipboard changes. Create a `Watcher`, then loop on `while let Some(event) = watcher.next_event()? {}`. The event contains the new MIME types and lets you receive the clipboard contents if you wish."
  ext-data-control support arrived in v0.9.2: "Added support for the `ext-data-control` protocol. It will be used instead of `wlr-data-control` when available."
  Source: [wl-clipboard-rs CHANGELOG](https://raw.githubusercontent.com/YaLTeR/wl-clipboard-rs/master/CHANGELOG.md), [wl-clipboard-rs `src/watch.rs`](https://raw.githubusercontent.com/YaLTeR/wl-clipboard-rs/master/src/watch.rs)
- `arboard` offers read and write only (`get_text`, `set_text`, `get`, `set`, `clear` and image, HTML and file-list variants); no watch or change-notification API appears in its public functions.
  Source: [arboard `src/lib.rs`](https://raw.githubusercontent.com/1Password/arboard/master/src/lib.rs)
- `arboard`'s README: "By default, `arboard`'s backend on Linux supports X11 (or XWayland implementations)," and to support Wayland "users should enable the `wayland-data-control` feature. If enabled, it will be prioritized over the X clipboard."
  It adds that the feature "relies on the data-control protocol extension(s), which _are not_ supported by all Wayland compositors."
  That feature is implemented on top of `wl-clipboard-rs`.
  Source: [arboard README](https://raw.githubusercontent.com/1Password/arboard/master/README.md), [arboard Cargo.toml](https://raw.githubusercontent.com/1Password/arboard/master/Cargo.toml)
- `tauri-plugin-clipboard-manager` (already a clipdeck dependency) depends on `arboard = { version = "3", features = ["wayland-data-control"] }`.
  Source: [plugins-workspace `plugins/clipboard-manager/Cargo.toml`](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/clipboard-manager/Cargo.toml)
- Raw protocol bindings: `wayland-protocols-wlr` (0.3.12) has `data_control`, `foreign_toplevel` and `virtual_pointer` modules; `wayland-protocols` (0.32.13) has `ext::data_control` and `ext::foreign_toplevel_list` behind its staging feature; `wayland-protocols-misc` (0.3.12) has `zwp_virtual_keyboard_v1`.
  Source: [wayland-protocols-wlr (docs.rs)](https://docs.rs/wayland-protocols-wlr/latest/wayland_protocols_wlr/), [wayland-protocols `ext` (docs.rs)](https://docs.rs/wayland-protocols/latest/wayland_protocols/ext/index.html), [wayland-protocols-misc (docs.rs)](https://docs.rs/wayland-protocols-misc/latest/wayland_protocols_misc/)

## 2. The `x-kde-passwordManagerHint` clipboard MIME type

- No specification document defining this type was found.
  The closest thing to a definition is the consumer: KDE's Klipper skips an entry when `data->data(QStringLiteral("x-kde-passwordManagerHint")) == QByteArrayView("secret")`.
  The MIME type is `x-kde-passwordManagerHint` and the value is the ASCII bytes `secret`.
  Source: [plasma-workspace `klipper/historymodel.cpp`](https://invent.kde.org/plasma/plasma-workspace/-/raw/master/klipper/historymodel.cpp)
- KeePassXC sets it on every non-macOS Unix build, not only under KDE: under `#elif defined(Q_OS_UNIX)` it calls `mime->setData("x-kde-passwordManagerHint", QByteArrayLiteral("secret"));`.
  Source: [KeePassXC `src/gui/Clipboard.cpp`](https://raw.githubusercontent.com/keepassxreboot/keepassxc/develop/src/gui/Clipboard.cpp)
- `arboard` (maintained under the 1Password GitHub organization) defines `KDE_EXCLUSION_MIME = "x-kde-passwordManagerHint"` and `KDE_EXCLUSION_HINT = b"secret"` and exposes them as `SetExtLinux::exclude_from_history()`, documented as: "This is the most widely adopted convention on Linux."
  Source: [arboard `src/platform/linux/mod.rs`](https://raw.githubusercontent.com/1Password/arboard/master/src/platform/linux/mod.rs)
- Bitwarden's desktop client calls `set.exclude_from_history().wait()` on Linux when `hide_from_history` is set, so it sets the hint through `arboard`.
  On its GNOME portal path it does not: "The portal does not support setting the password flag / removing the clipboard item from history. This means that the clipboard item will remain in history for this backend."
  Source: [bitwarden/clients `clipboard/arboard_backend.rs`](https://raw.githubusercontent.com/bitwarden/clients/main/apps/desktop/desktop_native/core/src/clipboard/arboard_backend.rs), [bitwarden/clients `clipboard/portal_backend.rs`](https://raw.githubusercontent.com/bitwarden/clients/main/apps/desktop/desktop_native/core/src/clipboard/portal_backend.rs)
- `wl-clipboard-rs` v0.9.4 added `wl-copy --sensitive` and `copy::Options::sensitive()` "to hint to clipboard managers that copied data is sensitive," and its changelog names `x-kde-passwordManagerHint` as the type involved.
  Source: [wl-clipboard-rs CHANGELOG](https://raw.githubusercontent.com/YaLTeR/wl-clipboard-rs/master/CHANGELOG.md)
- 1Password: that the `arboard` crate offers the hint is confirmed above, but whether the 1Password desktop app (closed source) actually sets it was not confirmed; no 1Password documentation stating so was found.
- Not confirmed: how the hint appears to an X11 reader.
  It is expected to be one of the selection's targets, but no X11-side source was read to confirm the exact target name Qt advertises.

## 3. Finding the source app of a copy

### X11

- The XFixes `SelectionNotify` event includes `owner`, the window that now owns the selection.
  Source: [XFIXES protocol specification (xorgproto)](https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/raw/master/fixesproto.txt)
- EWMH defines `_NET_ACTIVE_WINDOW` on the root window as "The window ID of the currently active window or None if no window has the focus. This is a read-only property set by the Window Manager."
  Source: [EWMH, Root Window Properties](https://specifications.freedesktop.org/wm-spec/latest/ar01s03.html)
- EWMH defines `_NET_WM_PID`: "If set, this property MUST contain the process ID of the client owning this window."
  The "If set" wording means a window is not required to have it.
  Source: [EWMH, Application Window Properties](https://specifications.freedesktop.org/wm-spec/latest/ar01s05.html)
- ICCCM defines `WM_CLASS` as "two consecutive null-terminated strings. These specify the Instance and Class names to be used by both the client and the window manager for looking up resources for the application or as identifying information."
  Source: [ICCCM, WM_CLASS Property](https://www.x.org/releases/X11R7.7/doc/xorg-docs/icccm/icccm.html)
- Not confirmed: whether the selection owner window is reliably a window that carries `WM_CLASS` or `_NET_WM_PID`.
  Toolkits may own selections with a separate helper window; no primary source on this was read, so "owner window to app name" versus "active window at copy time" remains an implementation question.

### Wayland

- Core `wl_data_device` and both data-control protocols expose no information about which client set the selection (see section 1).
  Source: [ext-data-control-v1.xml (wayland-protocols, staging)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/staging/ext-data-control/ext-data-control-v1.xml)
- `wlr-foreign-toplevel-management-unstable-v1` exists "to enable the creation of taskbars and docks by providing them with a list of opened applications," and per toplevel it sends `title`, `app_id` and `state` events, where the state enum includes `activated` ("the toplevel is active").
  This can answer "which app is focused now", which is a proxy for the source app, not the clipboard owner.
  Source: [wlr-foreign-toplevel-management-unstable-v1.xml (wlr-protocols)](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/raw/master/unstable/wlr-foreign-toplevel-management-unstable-v1.xml)
- `ext-foreign-toplevel-list-v1` sends `title`, `app_id` and `identifier` only, and says it "is intentionally minimalistic and expects additional functionality (e.g. creating a screencopy source from a toplevel handle, getting information about the state of the toplevel) to be implemented in extension protocols."
  It has no focus or activated state, so on its own it cannot tell which app is active.
  It also notes: "The compositor may choose to restrict this protocol to a special client launched by the compositor itself or expose it to all clients, this is compositor policy."
  Source: [ext-foreign-toplevel-list-v1.xml (wayland-protocols, staging)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml)
- Sway creates both foreign-toplevel globals and lists both as privileged (hidden only from security-context clients).
  Hyprland's main branch registers `ext_foreign_toplevel_list_v1` (version 1) and `zwlr_foreign_toplevel_manager_v1` (version 3).
  Source: [sway master `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/master/sway/server.c), [Hyprland main `src/managers/ProtocolManager.cpp`](https://raw.githubusercontent.com/hyprwm/Hyprland/main/src/managers/ProtocolManager.cpp)
- KWin: no foreign-toplevel protocol appears in its protocol list.
  It builds KDE's own `plasma-window-management.xml`, which has `app_id_changed` events and an `active` state, but that protocol says "Only one client can bind this interface at a time" and "Warning! The protocol described in this file is a desktop environment implementation detail. Regular clients must not use this protocol."
  Source: [KWin master `src/wayland/CMakeLists.txt`](https://invent.kde.org/plasma/kwin/-/raw/master/src/wayland/CMakeLists.txt), [plasma-wayland-protocols `plasma-window-management.xml`](https://invent.kde.org/libraries/plasma-wayland-protocols/-/raw/master/src/protocols/plasma-window-management.xml)
- GNOME: Mutter implements neither foreign-toplevel protocol (see the tree listing in section 1).
  GNOME Shell's `org.gnome.Shell.Introspect` D-Bus interface has `GetWindows` and `GetRunningApplications`, but it checks the caller against an allowlist containing only `org.freedesktop.impl.portal.desktop.gtk` and `org.freedesktop.impl.portal.desktop.gnome`.
  Source: [gnome-shell main `js/misc/introspect.js`](https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/js/misc/introspect.js), [Mutter main `src/wayland` tree](https://gitlab.gnome.org/GNOME/mutter/-/tree/main/src/wayland)
- Not confirmed: any other supported way for an ordinary app to learn the focused app on KDE Wayland (for example a KWin script or D-Bus call) or on GNOME Wayland (for example a Shell extension).
  These were not researched.

## 4. Global hotkeys

### The crate clipdeck uses today

- The `global-hotkey` README lists its platforms as "Windows", "macOS" and "Linux (X11 Only)".
  Source: [tauri-apps/global-hotkey README](https://raw.githubusercontent.com/tauri-apps/global-hotkey/dev/README.md)
- On Linux and the BSDs the crate compiles a single backend, `x11/mod.rs`, which opens an X connection with `RustConnection::connect(None)` and registers keys with the core `grab_key` request on the root window.
  There is no Wayland or portal backend in the source tree.
  Source: [global-hotkey `src/platform_impl/mod.rs`](https://raw.githubusercontent.com/tauri-apps/global-hotkey/dev/src/platform_impl/mod.rs), [global-hotkey `src/platform_impl/x11/mod.rs`](https://raw.githubusercontent.com/tauri-apps/global-hotkey/dev/src/platform_impl/x11/mod.rs)
- Two pull requests adding Wayland support are open and unmerged: "Wayland support" (#162) and "Unified Wayland support via XDG GlobalShortcuts portal" (#172).
  Source: [global-hotkey PR 162](https://github.com/tauri-apps/global-hotkey/pull/162), [global-hotkey PR 172](https://github.com/tauri-apps/global-hotkey/pull/172)
- The `tauri-plugin-global-shortcut` README marks Linux as supported with no X11 or Wayland qualification; the X11-only limit is stated only by the underlying crate.
  Source: [plugins-workspace `plugins/global-shortcut/README.md`](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/global-shortcut/README.md)
- Not confirmed: what happens at runtime under a Wayland session.
  Since the backend connects to X11, it would talk to XWayland when that is running, and whether key grabs made there fire while a native Wayland window has focus is not stated in any primary source read here.

### The GlobalShortcuts portal

- `org.freedesktop.portal.GlobalShortcuts` (documentation describes version 2) "lets applications create global shortcuts sessions, and register shortcuts to them. These shortcuts are activated regardless of the focused state of the application window."
  Source: [GlobalShortcuts portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- The app proposes, the user decides: `BindShortcuts()` "will typically result in the portal presenting a dialog showing the shortcuts and allowing users to configure the shortcuts," each shortcut carries a `description` and an optional `preferred_trigger`, and the response returns the subset actually bound.
  "An application can only attempt to bind shortcuts of a session once."
  Source: [GlobalShortcuts portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- The `Activated` signal carries `shortcut_id`, a `timestamp` and an optional `activation_token` "that can be used to activate a window in response to the shortcut getting activated."
  Source: [GlobalShortcuts portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- Backend support, read from each backend's interface list:
  xdg-desktop-portal-gnome declares `GlobalShortcuts`, and its NEWS lists "Add global shortcuts portal backend" under 48.rc.
  xdg-desktop-portal-kde declares it at v5.27.0 and later, and not at v5.26.5.
  xdg-desktop-portal-hyprland declares it.
  xdg-desktop-portal-wlr declares only `Screenshot` and `ScreenCast`, so it does not provide it.
  Source: [xdg-desktop-portal-gnome `data/meson.build`](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/data/meson.build), [xdg-desktop-portal-gnome NEWS](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/NEWS), [xdg-desktop-portal-kde v5.27.0 `kde.portal`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/raw/v5.27.0/data/kde.portal), [xdg-desktop-portal-kde v5.26.5 `kde.portal`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/raw/v5.26.5/data/kde.portal), [xdg-desktop-portal-hyprland `hyprland.portal`](https://raw.githubusercontent.com/hyprwm/xdg-desktop-portal-hyprland/master/hyprland.portal), [xdg-desktop-portal-wlr `wlr.portal`](https://raw.githubusercontent.com/emersion/xdg-desktop-portal-wlr/master/wlr.portal)
- Not confirmed: whether the portal works for an app that is not sandboxed and has no visible window (how the portal identifies the app, and what `parent_window` should be for `BindShortcuts`).
  The portal page read here does not address it.
- Not researched: compositor-side keybindings that run a command (Sway `bindsym ... exec`, Hyprland `bind`), which the issue lists as the fallback.

## 5. Showing a menu at the mouse cursor with no focused or visible window

### What Tauri and muda do on Linux

- Tauri depends on muda with the `gtk3` feature and on `gtk` 0.18 with feature `v3_24`, so menus are GTK 3 `GtkMenu` widgets.
  Source: [tauri `crates/tauri/Cargo.toml` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/Cargo.toml)
- Tauri's `popup_menu` path on Linux calls `window.gtk_window()` and then muda's `show_context_menu_for_gtk_window(w.as_ref(), position)`.
  Source: [tauri `crates/tauri/src/menu/menu.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/src/menu/menu.rs)
- clipdeck calls `host.popup_menu(&built.menu)` with no position.
  In muda 0.20.0, when `position` is `None`, `show_context_menu` takes the screen's root window, reads the default seat's pointer position, builds a synthetic `gdk::Event` of type `ButtonPress` with a timestamp from `glib::monotonic_time()`, and calls `gtk_menu.popup_at_rect(&window, &gdk::Rectangle::new(pos.0, pos.1, 0, 0), NorthWest, NorthWest, Some(&event))`.
  The function is `gtk_menu_popup_at_rect`, not `gtk_menu_popup_at_pointer`, and the anchor window is the root window, not the app's 1x1 host window.
  Source: [muda 0.20.0 `src/platform_impl/gtk/mod.rs`](https://raw.githubusercontent.com/tauri-apps/muda/muda-v0.20.0/src/platform_impl/gtk/mod.rs)
- Tauri's windowing layer (tao) returns a fixed `(0, 0)` for the cursor position on Wayland: `if is_wayland { Ok((0, 0).into()) }`.
  This is a separate code path from muda's pointer query above, and it shows that tao itself does not obtain a global pointer position on Wayland.
  Source: [tao `src/platform_impl/linux/util.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tao/dev/src/platform_impl/linux/util.rs)

### What Wayland requires

- A popup needs a parent: `xdg_surface.get_popup` takes a `parent` and "If null is passed as a parent, a parent surface must be specified using some other protocol, before committing the initial state," and "The parent of an xdg_popup must be mapped (see the xdg_surface description) before the xdg_popup itself."
  Source: [xdg-shell.xml (wayland-protocols, stable)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/stable/xdg-shell/xdg-shell.xml)
- A menu-style popup that takes input needs a grab tied to real input: `xdg_popup.grab` "must be used in response to some sort of user action like a button press, key press, or touch down event. The serial number of the event should be passed as 'serial'," and "If the compositor denies the grab, the popup will be immediately dismissed."
  "The parent of a grabbing popup must either be an xdg_toplevel surface or another xdg_popup with an explicit grab."
  Source: [xdg-shell.xml (wayland-protocols, stable)](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/main/stable/xdg-shell/xdg-shell.xml)
- GTK 3's Wayland backend follows those rules.
  Its source comment says: "Popup menus can appear without a transient parent, which means they cannot be positioned properly on Wayland. This attempts to guess the surface they should be positioned with by finding the surface beneath the device that created the grab for the popup window."
  If no parent is found it logs "Couldn't map as window %p as popup because it doesn't have a parent" and falls back to creating an xdg_toplevel instead of a popup.
  When it does create the popup it calls `xdg_popup_grab` with the serial from `_gdk_wayland_seat_get_last_implicit_grab_serial`, that is, the serial of the last real input event the client received.
  Source: [GTK gtk-3-24 `gdk/wayland/gdkwindow-wayland.c`](https://gitlab.gnome.org/GNOME/gtk/-/raw/gtk-3-24/gdk/wayland/gdkwindow-wayland.c)
- A global hotkey delivered through X11 key grabs or through the portal's D-Bus `Activated` signal does not give the GTK Wayland client a `wl_keyboard` or `wl_pointer` event, so the app has no fresh input serial of its own at that moment.
  This follows from the sources above but is an inference; no source states the outcome for this exact flow.
- Not confirmed: what actually happens when clipdeck's flow (hotkey, then `popup_at_rect` on the root window with a synthetic event) runs on X11 or on any Wayland compositor.
  On X11 the code path uses a real root window and real pointer coordinates, which is consistent with it working, but this was not tested.
  On Wayland, whether the menu appears at all, appears as a detached toplevel, or is dismissed immediately was not determined from any primary source.

## 6. Synthesizing a paste keystroke into another app

### X11: XTest

- The XTest extension's `XTestFakeInput` request "simulates the limited set of core protocol events within the set FAKE_EVENT_TYPE," and the event type must be one of `KeyPress`, `KeyRelease`, `MotionNotify`, `ButtonPress` or `ButtonRelease`.
  Source: [XTEST Extension Protocol (X.Org X11R7.7)](https://www.x.org/releases/X11R7.7/doc/xextproto/xtest.html)
- `x11rb` exposes it as `protocol::xtest::fake_input`.
  Source: [x11rb `protocol::xtest` (docs.rs)](https://docs.rs/x11rb/latest/x11rb/protocol/xtest/index.html)
- Not confirmed: whether XTest input sent to XWayland reaches native Wayland apps on any compositor.
  This was not researched.

### Wayland: RemoteDesktop portal and libei

- `org.freedesktop.portal.RemoteDesktop` (documentation describes version 2): the app calls `CreateSession()`, then `SelectDevices()` to choose device types (keyboard is bit 1), and "Calling Start() presents a dialog to the user."
  Source: [RemoteDesktop portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html)
- Two ways to send input: "EIS (recommended): Call ConnectToEIS() after starting the session to obtain a file descriptor for a libei sender context," or the D-Bus `Notify*` methods such as `NotifyKeyboardKeycode()`.
  "Once an EIS connection is established, the Notify* D-Bus methods must not be used."
  `ConnectToEIS` "was added in version 2 of this interface."
  Source: [RemoteDesktop portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html)
- The consent can persist: `SelectDevices()` accepts `persist_mode` (0 do not persist, 1 while the application is running, 2 "until explicitly revoked") and a `restore_token`, both added in version 2.
  "The restore token is invalidated after using it once. To restore the same session again, use the new restore token sent in response to starting this session."
  Source: [RemoteDesktop portal documentation (XDG Desktop Portal)](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html)
- libei's documentation describes the model: "the EI client is roughly equivalent to a physical input device coming from the kernel and its events feed into the normal input stack," and "To Wayland clients, they are indistinguishable from real devices."
  It names `liboeffis` as "an helper library for DBus communication with the XDG RemoteDesktop portal."
  Source: [libei documentation](https://libinput.pages.freedesktop.org/libei/)
- Backend support, read from each backend's interface list:
  xdg-desktop-portal-gnome declares `RemoteDesktop`, and its NEWS lists "Remote desktop: add the ability to communicate via an EIS socket" and "Implement restoration of remote desktop sessions" under 45.beta.
  xdg-desktop-portal-kde declares `RemoteDesktop` (present at v5.26.5 and later).
  xdg-desktop-portal-wlr and xdg-desktop-portal-hyprland do not declare `RemoteDesktop`.
  Source: [xdg-desktop-portal-gnome NEWS](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/NEWS), [xdg-desktop-portal-gnome `data/meson.build`](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/data/meson.build), [xdg-desktop-portal-kde master `kde.portal`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/raw/master/data/kde.portal), [xdg-desktop-portal-wlr `wlr.portal`](https://raw.githubusercontent.com/emersion/xdg-desktop-portal-wlr/master/wlr.portal), [xdg-desktop-portal-hyprland `hyprland.portal`](https://raw.githubusercontent.com/hyprwm/xdg-desktop-portal-hyprland/master/hyprland.portal)
- Compositor-side EIS: KWin has an `eis` plugin at tag v6.1.0 and on master, and not at v6.0.0.
  Mutter's main branch has `meta-eis.c` and `meta-eis-client.c` under `src/backends`.
  Source: [KWin v6.1.0 `src/plugins/eis/CMakeLists.txt`](https://invent.kde.org/plasma/kwin/-/raw/v6.1.0/src/plugins/eis/CMakeLists.txt), [KWin v6.0.0 path (returns 404)](https://invent.kde.org/plasma/kwin/-/raw/v6.0.0/src/plugins/eis/CMakeLists.txt), [Mutter `src/backends` tree listing (GitLab API)](https://gitlab.gnome.org/api/v4/projects/GNOME%2Fmutter/repository/tree?path=src/backends&per_page=100&page=1)
- Not confirmed: whether xdg-desktop-portal-kde implements `ConnectToEIS` (its source was not read; only the interface list was), and what the user-visible indicator and dialog look like on GNOME or KDE while a keyboard-only RemoteDesktop session is held open.

### Wayland: `virtual-keyboard-unstable-v1`

- The protocol: "The virtual keyboard provides an application with requests which emulate the behaviour of a physical keyboard," and the manager "allows an application to provide keyboard input events as if they came from a physical keyboard."
  A keymap must be set before key events are sent.
  It also says: "If the compositor enables a keyboard to perform arbitrary actions, it should present an error when an untrusted client requests a new keyboard."
  Source: [virtual-keyboard-unstable-v1.xml (wlroots)](https://gitlab.freedesktop.org/wlroots/wlroots/-/raw/master/protocol/virtual-keyboard-unstable-v1.xml)
- wlroots implements it (`types/wlr_virtual_keyboard_v1.c`), Sway exposes it as a privileged global (hidden only from security-context clients), and Hyprland's main branch registers `zwp_virtual_keyboard_manager_v1` (version 1).
  Source: [wlroots `types/` tree listing (GitLab API)](https://gitlab.freedesktop.org/api/v4/projects/wlroots%2Fwlroots/repository/tree?path=types&per_page=100&page=1), [sway master `sway/server.c`](https://raw.githubusercontent.com/swaywm/sway/master/sway/server.c), [Hyprland main `src/managers/ProtocolManager.cpp`](https://raw.githubusercontent.com/hyprwm/Hyprland/main/src/managers/ProtocolManager.cpp)
- KWin's protocol list has no virtual-keyboard entry (it has KDE's own `fake-input.xml`, which was not researched further).
  Mutter's `src/meson.build` has no match for "virtual-keyboard".
  Source: [KWin master `src/wayland/CMakeLists.txt`](https://invent.kde.org/plasma/kwin/-/raw/master/src/wayland/CMakeLists.txt), [Mutter main `src/meson.build`](https://gitlab.gnome.org/GNOME/mutter/-/raw/main/src/meson.build)

### `/dev/uinput`

- The kernel documentation: "uinput is a kernel module that makes it possible to emulate input devices from userspace. By writing to /dev/uinput (or /dev/input/uinput) device, a process can create a virtual input device with specific capabilities."
  It recommends libevdev over direct access for new software.
  Source: [uinput module (Linux kernel documentation)](https://docs.kernel.org/input/uinput.html)
- Not confirmed: the permissions needed.
  The kernel page does not state the default ownership or mode of `/dev/uinput`, and the systemd udev rule files checked (`50-udev-default.rules.in`, `70-uaccess.rules.in`, `99-systemd.rules.in`) contain no uinput rule.
  That an ordinary user cannot open it by default, and needs a udev rule or group membership, is widely repeated but was not confirmed from a primary source here.

### Ctrl+V versus Ctrl+Shift+V in terminals

- Not researched against a primary source, per the task brief; no claim is made here.

## 7. Tray icon

- Tauri depends on `tray-icon` 0.25 with the `libappindicator` feature.
  Source: [tauri `crates/tauri/Cargo.toml` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/Cargo.toml)
- tray-icon 0.25.1 lists Linux support as "Linux (AppIndicator or KSNI)".
  "The default Linux backend uses GTK 3 and `libappindicator` or `libayatana-appindicator`. The `ksni` backend does not require these system libraries unless a muda GTK backend is also enabled."
  The `ksni` feature "Uses the StatusNotifierItem D-Bus backend on Linux and BSD."
  Source: [tray-icon 0.25.1 README](https://raw.githubusercontent.com/tauri-apps/tray-icon/tray-icon-v0.25.1/README.md)
- The library is loaded at runtime, not linked: `libappindicator-sys` tries `libayatana-appindicator3.so.1`, then `libappindicator3.so.1`, then the unversioned `.so` names, and otherwise panics with "Failed to load ayatana-appindicator3 or appindicator3 dynamic library".
  A missing library is therefore a runtime panic when the tray is created, not a build or install error.
  Source: [libappindicator-rs `sys/src/lib.rs`](https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/main/sys/src/lib.rs)
- Packaging consequences: when the `tray-icon` feature is on, the Tauri CLI adds `libayatana-appindicator3-1` or `libappindicator3-1` to the `.deb` dependencies (whichever pkg-config finds on the build machine, Ayatana first, overridable with `TAURI_LINUX_AYATANA_APPINDICATOR`) and copies the matching `.so.1` into the AppImage.
  Source: [tauri-cli `src/interface/rust.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/src/interface/rust.rs)
- Tauri's Debian page says the package specifies "the dependencies `libwebkit2gtk-4.1-0` and `libgtk-3-0`, along with `libappindicator3-1` if your app uses the system tray."
  Source: [Debian - Tauri v2 docs](https://v2.tauri.app/distribute/debian/)
- Click events: Tauri's docs say "Linux: Unsupported. The event is not emitted even though the icon is shown and will still show a context menu on right click."
  tray-icon 0.25.1 scopes this to the AppIndicator backend and says the KSNI backend "Emits left- and middle-click activation events. The StatusNotifier host handles right clicks itself and does not expose them to the application."
  Source: [System Tray - Tauri v2 docs](https://v2.tauri.app/learn/system-tray/), [tray-icon 0.25.1 `src/lib.rs`](https://raw.githubusercontent.com/tauri-apps/tray-icon/tray-icon-v0.25.1/src/lib.rs)
- Other AppIndicator-backend limits in tray-icon's docs: "Once a menu is set, it cannot be removed," and "Sometimes the icon won't be visible unless a menu is set."
  Source: [tray-icon `src/lib.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tray-icon/dev/src/lib.rs)
- GNOME Shell does not show tray icons itself.
  The GNOME 3.26 release notes: "GNOME 3.26 no longer shows status icons in the bottom-left of the screen," adding that "they can be restored using the TopIcons extension."
  The gnome-shell commit that removed the legacy tray says users who still want status icons "can install one of various extensions that are available, either based on the XEmbed support that is still kept around or implementing the DBus-based StatusNotifier spec."
  Source: [GNOME 3.26 release notes](https://release.gnome.org/3-26/), [gnome-shell commit f1ee6c69 "legacyTray: Time to say goodbye ..."](https://gitlab.gnome.org/GNOME/gnome-shell/-/commit/f1ee6c69d74884e294dd5872c73691d5fd2ba09a)
- The extension that restores them describes itself as: "This extension integrates Ubuntu AppIndicators and KStatusNotifierItems (KDE's blessed successor of the systray) into GNOME Shell."
  Its `metadata.json` lists GNOME Shell versions 45 through 51.
  Its README also notes that apps load libappindicator dynamically, "so without having this library installed in your system no icon will be shown."
  Source: [gnome-shell-extension-appindicator README](https://raw.githubusercontent.com/ubuntu/gnome-shell-extension-appindicator/master/README.md), [gnome-shell-extension-appindicator `metadata.json`](https://raw.githubusercontent.com/ubuntu/gnome-shell-extension-appindicator/master/metadata.json)
- Not confirmed: which distributions preinstall that extension, and whether KDE Plasma, Sway bars and Hyprland bars each provide a StatusNotifier host by default.
  These were not researched.

## 8. Bundle formats, updater, and build requirements

### Bundle formats

- Tauri's distribution page: "For Linux you can distribute your app using the Debian package, Snap, AppImage, Flatpak, RPM or Arch User Repository (AUR) formats."
  Source: [Distribute - Tauri v2 docs](https://v2.tauri.app/distribute/)
- The bundler itself has Linux modules only for `appimage`, `debian.rs` and `rpm.rs` (plus shared `freedesktop` code).
  Snap, Flatpak and AUR are documented as separate packaging workflows, not bundler targets.
  Source: [tauri-bundler `src/bundle/linux` (GitHub API listing)](https://api.github.com/repos/tauri-apps/tauri/contents/crates/tauri-bundler/src/bundle/linux), [Distribute - Tauri v2 docs](https://v2.tauri.app/distribute/)

### What the updater can update

- The updater documentation describes only AppImage on Linux: "On Linux, Tauri will create the normal AppImage inside the `target/release/bundle/appimage/` folder," with `myapp.AppImage` ("The standard app bundle. It will be re-used by the updater.") and `myapp.AppImage.sig`.
  The page does not mention `.deb` or `.rpm`.
  Source: [Updater - Tauri v2 docs](https://v2.tauri.app/plugin/updater/)
- The plugin source does more than the docs say.
  Its Linux implementation is commented "Linux (AppImage, Deb, RPM)" and dispatches on the running app's bundle type: `Some(Installer::Deb) => self.install_deb(bytes)`, `Some(Installer::Rpm) => self.install_rpm(bytes)`, and otherwise `self.install_appimage(bytes)`.
  `install_deb`, `install_rpm` and `install_appimage` are all present at tag `updater-v2.13.2`, the version clipdeck locks.
  Source: [plugins-workspace `plugins/updater/src/updater.rs` (v2)](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/updater/src/updater.rs), [plugins-workspace `updater.rs` at updater-v2.13.2](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/updater-v2.13.2/plugins/updater/src/updater.rs)
- The `.deb` and `.rpm` paths need root and get it interactively: the package is written to a temp location and installed with `dpkg -i` or `rpm -U`, trying `pkexec` first ("graphical sudo prompt"), then a password collected through `zenity` or `kdialog` and piped to `sudo`, then plain terminal `sudo`.
  Source: [plugins-workspace `plugins/updater/src/updater.rs` (v2)](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/updater/src/updater.rs)
- The update manifest can be keyed by bundle type: the source documents a `{os}-{arch}-{bundle_type}` target lookup and a `{{bundle_type}}` endpoint variable.
  Source: [plugins-workspace `updater.rs` at updater-v2.13.2](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/updater-v2.13.2/plugins/updater/src/updater.rs)
- Not confirmed: how well the `.deb`/`.rpm` update path works in practice, since it is undocumented on the official updater page.

### Build dependencies and distro baseline

- Debian/Ubuntu build packages: `libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`.
  Fedora: `webkit2gtk4.1-devel`, `openssl-devel`, `curl`, `wget`, `file`, `libappindicator-gtk3-devel`, `librsvg2-devel`, `libxdo-devel`, plus the `c-development` group.
  Arch: `webkit2gtk-4.1`, `base-devel`, `curl`, `wget`, `file`, `openssl`, `appmenu-gtk-module`, `libappindicator-gtk3`, `librsvg`, `xdotool`.
  Source: [Prerequisites - Tauri v2 docs](https://v2.tauri.app/start/prerequisites/)
- Tauri's Cargo manifest (dev branch) requires `gtk` 0.18 with feature `v3_24` and `webkit2gtk` 2 with feature `v2_40`.
  Source: [tauri `crates/tauri/Cargo.toml` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/Cargo.toml)
- Baseline guidance from Tauri: "you must build your Tauri application using the oldest base system you intend to support that also provides Tauri v2's required WebKitGTK 4.1 packages. Ubuntu 22.04 and Debian 12 are suitable baseline examples because they provide `libwebkit2gtk-4.1-dev` from their standard package repositories."
  The reason given is glibc: "Building on a newer base system can raise the minimum glibc version required by your app."
  The same paragraph appears on both the AppImage and Debian pages.
  Source: [AppImage - Tauri v2 docs](https://v2.tauri.app/distribute/appimage/), [Debian - Tauri v2 docs](https://v2.tauri.app/distribute/debian/)
- For aarch64, the AppImage page notes GitHub's public `ubuntu-22.04-arm` and `ubuntu-24.04-arm` runners can build "with no changes."
  Source: [AppImage - Tauri v2 docs](https://v2.tauri.app/distribute/appimage/)
- Not confirmed: the oldest Fedora, openSUSE or Arch snapshot that satisfies these requirements; Tauri's docs name only Ubuntu 22.04 and Debian 12 as examples.

## 9. The `auto-launch` crate on Linux

- The README: "Linux supports two ways to achieve auto launch: XDG Autostart: Uses `.desktop` files in `~/.config/autostart/` (default)" and "systemd: Uses systemd user services in `~/.config/systemd/user/`."
  Source: [auto-launch README](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/README.md)
- The path is built as `home_dir.join(".config").join("autostart")` and the file name is `{app_name}.desktop`.
  It is hardcoded under `$HOME/.config` and does not consult `$XDG_CONFIG_HOME`.
  `is_enabled` in XDG mode is simply whether that file exists.
  Source: [auto-launch `src/linux.rs`](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/src/linux.rs)
- The generated entry contains `Type=Application`, `Version=1.0`, `Name=`, `Comment=`, `Exec=<quoted app_path> <quoted args>`, `StartupNotify=false` and `Terminal=false`.
  Source: [auto-launch `src/linux.rs`](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/src/linux.rs)
- AppImage caveat: the crate's README and Linux source contain no mention of AppImage; it writes whatever `app_path` it is given into `Exec=`.
  clipdeck passes `std::env::current_exe()` (`crates/clipdeck-app/src/autostart.rs`).
  Tauri's own autostart plugin, which wraps the same crate, special-cases this: on Linux it uses the AppImage path from the environment when present (`builder.set_app_path(&appimage)`) and only otherwise the current executable.
  Source: [auto-launch README](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/README.md), [plugins-workspace `plugins/autostart/src/lib.rs`](https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/autostart/src/lib.rs)
- Not confirmed: what `current_exe()` returns inside a running AppImage.
  The Tauri plugin's special case implies it is not the `.AppImage` file itself, but no primary source stating the actual value was read.

## 10. Where the app directories resolve for `dev.clipdeck.app`

- `app_config_dir` "Resolves to `config_dir`/`${bundle_identifier}`" and is implemented as `dirs::config_dir()` joined with the identifier.
  `app_data_dir` is `dirs::data_dir()` joined with the identifier.
  `app_local_data_dir` is `dirs::data_local_dir()` joined with the identifier, and "On Windows and Linux this is also the default data directory of the webviews."
  All of them can be overridden by the `app > appDirectoriesOverride` config.
  Source: [tauri `crates/tauri/src/path/desktop.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/src/path/desktop.rs)
- On Linux the `dirs` crate returns `$XDG_CONFIG_HOME` or `$HOME/.config` for `config_dir`, and `$XDG_DATA_HOME` or `$HOME/.local/share` for both `data_dir` and `data_local_dir`.
  Source: [dirs-rs README](https://raw.githubusercontent.com/dirs-dev/dirs-rs/main/README.md)
- With clipdeck's identifier (`"identifier": "dev.clipdeck.app"` in `crates/clipdeck-app/tauri.conf.json`) and no XDG overrides set, that gives:
  `app_config_dir` = `~/.config/dev.clipdeck.app`.
  `app_data_dir` = `~/.local/share/dev.clipdeck.app`.
  `app_local_data_dir` = `~/.local/share/dev.clipdeck.app` (the same directory as `app_data_dir` on Linux).
  `app_log_dir` = `~/.local/share/dev.clipdeck.app/logs`.
  Source: [tauri `crates/tauri/src/path/desktop.rs` (dev)](https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/src/path/desktop.rs), [dirs-rs README](https://raw.githubusercontent.com/dirs-dev/dirs-rs/main/README.md)

## Support matrix

Each cell summarizes the sourced findings above and nothing else.
"unconfirmed" means no primary source settled it; it does not mean "probably works".
Rows 8 to 10 do not depend on the session type.

| Capability | X11 | GNOME Wayland | KDE Wayland | wlroots Wayland |
| --- | --- | --- | --- | --- |
| 1. Clipboard change events | yes - XFixes `SelectionNotify` | no data-control (declined by Mutter); partial - Clipboard portal signal exists only inside a RemoteDesktop/InputCapture session, use as a monitor unconfirmed | yes - ext-data-control from KWin 6.4; wlr-data-control only up to 6.4; denied to sandboxed clients | yes - Sway 1.11+ has both protocols, Sway 1.10 wlr only; Hyprland main has both |
| 2. `x-kde-passwordManagerHint` visible | unconfirmed - apps set it, X11 read path not verified | unconfirmed - portal reports MIME types, not verified for this type | yes - offers list MIME types | yes - offers list MIME types |
| 3. Source app of a copy | partial - owner window in the event, plus `_NET_ACTIVE_WINDOW`, `WM_CLASS`, optional `_NET_WM_PID`; owner-to-app mapping unconfirmed | no - no protocol; Shell Introspect is allowlisted to portals | unconfirmed - no foreign-toplevel protocol; KDE's own protocol is single-client and marked internal | partial - focused app's `app_id` via wlr-foreign-toplevel; no clipboard-owner identity |
| 4. Global hotkeys | yes - global-hotkey crate (X11 key grabs) | crate: no Wayland backend; portal: yes from xdg-desktop-portal-gnome 48, user confirms bindings | crate: no Wayland backend; portal: yes from xdg-desktop-portal-kde 5.27 | crate: no Wayland backend; portal: Hyprland backend yes, wlr backend no |
| 5. Menu at cursor, no focused window | unconfirmed - code path uses root window and pointer position, not tested | unconfirmed - xdg_popup needs a mapped parent and an input serial | unconfirmed - same | unconfirmed - same |
| 6. Synthesized paste keystroke | yes - XTest `XTestFakeInput` | partial - RemoteDesktop portal with libei (backend 45+), behind a consent dialog; no virtual-keyboard protocol | partial - RemoteDesktop portal, KWin EIS from 6.1, portal `ConnectToEIS` unconfirmed; no virtual-keyboard protocol | yes - virtual-keyboard-unstable-v1 in Sway and Hyprland; no RemoteDesktop portal backend |
| 7. Tray icon with menu | unconfirmed - needs appindicator library and a tray host, hosts not researched | partial - needs the AppIndicator extension; stock Shell shows none | unconfirmed - host not researched | unconfirmed - host not researched |
| 8. Bundles and in-app update | deb, rpm, AppImage bundled; updater documents AppImage, source also handles deb and rpm with a root prompt | same | same | same |
| 9. Start at login | yes - `~/.config/autostart/<name>.desktop`; AppImage path not handled by the crate | same | same | same |
| 10. Config and data dirs | yes - `~/.config/dev.clipdeck.app`, `~/.local/share/dev.clipdeck.app` | same | same | same |

## Open questions

- Whether clipdeck's menu flow (hotkey, then `popup_at_rect` on the root window with a synthetic event and no focused window) shows a usable menu at the pointer on X11, and what it does on each Wayland compositor.
  This needs a test on real sessions; the sources only show that Wayland requires a mapped parent and a real input serial.
- Whether the Clipboard portal can serve as a continuous clipboard monitor on GNOME (session lifetime, user-visible indicator, consent on each login, whether every copy raises `SelectionOwnerChanged`).
- Whether an XWayland client on GNOME, KDE or wlroots receives XFixes selection events for copies made in native Wayland apps while unfocused.
- Whether X11 key grabs made through XWayland by the `global-hotkey` crate fire when a native Wayland window has focus, on any compositor.
- Whether XTest input sent through XWayland reaches native Wayland apps, on any compositor.
- Whether the GlobalShortcuts portal accepts an unsandboxed app with no visible window, and how it identifies the app.
- Whether xdg-desktop-portal-kde implements `ConnectToEIS`, and what the consent and indicator experience is for a keyboard-only RemoteDesktop session on GNOME and KDE.
- Default permissions of `/dev/uinput` and what a user must do to get access.
- How `x-kde-passwordManagerHint` is exposed to an X11 reader, and whether the Clipboard portal's `mime_types` list includes it on GNOME.
- Whether the 1Password desktop app sets the hint (the `arboard` crate it maintains can).
- Whether the X11 selection owner window can be mapped reliably to an application name, or whether the active window at copy time has to be used instead.
- Any supported way for an ordinary app to learn the focused app on KDE Wayland or GNOME Wayland.
- Which Hyprland release first shipped `ext-data-control-v1`.
- Which desktops and bars provide a StatusNotifier or AppIndicator host by default, and which distributions preinstall the GNOME AppIndicator extension.
- What `std::env::current_exe()` returns inside a running AppImage, which decides whether the `auto-launch` entry survives a restart.
- How reliable the updater's undocumented `.deb` and `.rpm` paths are, and the oldest non-Debian distributions that meet Tauri's WebKitGTK 4.1 requirement.
- Terminal paste shortcuts (Ctrl+Shift+V) and compositor-side keybindings that run a command were not researched.
