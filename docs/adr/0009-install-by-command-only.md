# Install by one-line command only

Amends [ADR-0007](0007-ci-built-installers-with-auto-update.md), which shipped a downloadable Windows installer and a macOS `.dmg`.

Clipdeck is unsigned (ADR-0002), and both operating systems distrust an unsigned app only when a browser downloaded it.
A hand-downloaded installer gets a SmartScreen warning on Windows, and a hand-downloaded `.dmg` produces an app macOS refuses to open until the user finds "Open Anyway" in System Settings.
The one-line installers (`install.ps1`, `install.sh`) download with PowerShell and `curl`, which don't mark the file, so the same build installs and opens with no warning at all.

Offering both routes meant documenting the worse one in detail and letting new users pick it by accident.
We decided the commands are the only way to install that we document or link to: the website and `docs/INSTALL.md` no longer point at release downloads, and the release no longer builds a `.dmg`, which existed only for downloading by hand.

The release still carries the Windows installer and the macOS app archive, because the commands and the in-app updater download exactly those files.
They stay publicly reachable; they just aren't advertised.

Smart App Control on Windows blocks unsigned apps however they arrive, so this doesn't help there (ADR-0002).
If Clipdeck is ever code-signed, downloadable installers become worth offering again.

**Status**: accepted
