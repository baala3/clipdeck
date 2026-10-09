# Ship CI-built installers with in-app updates

Supersedes [ADR-0004](0004-windows-portable-zip-v1.md).

ADR-0004 shipped Windows as a portable zip because building Tauri's NSIS installer meant cross-compiling to MSVC from the Linux/WSL dev box.
That reason was about where we built, not what users should get, and it left real gaps: no macOS build at all (there is no Apple SDK on the dev box, so the macOS code had never even compiled), no start at login, no updates, and an upgrade that lost History if the new zip was unzipped somewhere else.

We decided to build releases on GitHub Actions' own Windows and macOS runners, where Tauri's bundler works natively, and to ship:

- a per-user NSIS installer (`*-setup.exe`) on Windows, needing no administrator rights;
- a universal (Apple silicon + Intel) `.dmg` on macOS, ad-hoc signed so Gatekeeper offers "Open Anyway" instead of calling the app damaged;
- in-app updates through Tauri's updater, which checks `latest.json` on the newest GitHub Release, asks before installing, and verifies each update against a public key compiled into the app.

The repository was made public for this, since release downloads (and the updater's `latest.json`) must be reachable without a GitHub login.

Start at login is a setting, on by default as is usual for a clipboard manager, applied with `auto-launch` directly instead of Tauri's autostart plugin, because the plugin writes the machine-wide `Run` key when the app runs elevated and doesn't quote the exe path.

The updater's signing key is a new long-lived secret: losing it strands every installed copy on its current version.
See [RELEASING.md](../RELEASING.md).

Builds stay unsigned per ADR-0002; this changes how they're built and delivered, not whether they're code-signed.

**Status**: accepted
