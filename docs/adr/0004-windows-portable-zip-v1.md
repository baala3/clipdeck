# Ship Windows v1 as a portable zip, not an installer

Ticket 7's original wording called for "an installable Windows package (e.g. `.msi`/`.exe`)" built through Tauri's own bundler.
Attempting that from this Linux/WSL dev environment requires cross-compiling to the MSVC target with `cargo-xwin`, which in turn needs `clang`, `lld`, `llvm`, and NSIS installed, plus downloading Microsoft's Windows SDK/CRT headers through `xwin`.
Each missing piece surfaced one at a time as a new build failure, turning "produce an installer" into a multi-step toolchain bring-up with no functional benefit over what already worked.

We already have a proven, working path from Ticket 2 onward: cross-compiling to `x86_64-pc-windows-gnu` with the `mingw-w64` toolchain, which needed only one `apt` package and has produced a correctly running app (tray icon, hotkeys, popups) on real Windows hardware via WSL interop testing.

We decided to ship v1 as a portable zip instead: `clipdeck-app.exe` + `WebView2Loader.dll` + a `README.txt`, built with the existing GNU toolchain, zipped and attached to a GitHub Release.
No installer wizard, no registry entries, nothing to uninstall beyond deleting the folder.
This fits Clipdeck's own stated values (lightweight, developer-friendly) better than a traditional installer, and avoids taking on an entire second cross-compilation toolchain for v1.

This doesn't block revisiting a proper NSIS/MSI installer later - the toolchain pieces we discovered (`cargo-xwin`, `clang`, `lld`, `llvm`, `nsis`) are now known and documented here if that's picked back up.

**Status**: accepted
