# Releasing Clipdeck

Releases are built by the [Build workflow](../.github/workflows/build.yml) on GitHub's Windows and macOS runners, never on a dev machine (see [ADR-0007](adr/0007-ci-built-installers-with-auto-update.md)).

## Cutting a release

1. Bump `version` in `crates/clipdeck-app/Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and commit to `main`.
2. Tag that commit and push the tag:

   ```sh
   git tag v0.3.0
   git push origin v0.3.0
   ```

3. The workflow checks the tag matches the version, builds the Windows installer and the universal macOS `.dmg`, and attaches them plus `latest.json` to a **draft** release.
4. Download both installers from the draft, install them, and smoke-test them (capture, the menus, Settings).
5. Write the release notes into the draft and publish it.
   Installed copies pick the update up within a day; the notes are shown in the update prompt.

Publishing is what releases the update: the updater reads `releases/latest/download/latest.json`, which only ever points at the newest published, non-prerelease release.
Mark a release as a pre-release to share it without offering it as an update.

## The update signing key

Every update is signed, and installed copies refuse updates that aren't signed with the key whose public half is in `tauri.conf.json` (`plugins.updater.pubkey`).
This is separate from OS code signing, which Clipdeck doesn't have (ADR-0002).

- The private key and its password are the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.
- The maintainer keeps a backup of both in a password manager.
- **If the private key is lost, installed copies can never be updated again**: users would have to download and reinstall by hand once a new key is in place.
  Never rotate it without that plan.

Without the secrets (e.g. pull requests from forks), the workflow still lints and tests but skips bundling.
