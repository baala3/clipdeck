# Releasing Clipdeck

## How a change reaches users

```
open a PR ──> CI lints and tests it ──> merge to main ──> run the Release workflow ──> users get it
              (a few minutes)           (nothing ships)   (builds and publishes)
```

1. **A PR runs [CI](../.github/workflows/ci.yml).**
   It lints and tests on Linux, Windows, and macOS, and builds no installers.
   PRs that only touch the website, docs, or the install scripts skip the lint and tests; only the quick `CI gate` check runs.
   `main` accepts nothing but PRs whose `CI gate` check is green (see [Rules on `main` and on release tags](#rules-on-main-and-on-release-tags)).
2. **Merging to `main` ships nothing.**
   `main` is where finished work collects until the next release.
   The website is the one exception: [Pages](../.github/workflows/pages.yml) redeploys it whenever `site/` changes.
3. **Running the [Release workflow](../.github/workflows/release.yml) ships everything on `main`.**
   This is the only way a new version goes out.

Users get Clipdeck from the newest published GitHub Release, whichever way they install:

| How they install | What it reads |
| --- | --- |
| One-line command (`install.ps1`, `install.sh`), the only install we document ([ADR-0009](adr/0009-install-by-command-only.md)) | `releases/latest` |
| Already installed (in-app updater, checks daily) | `releases/latest/download/latest.json` |

So the install commands change exactly when a release is published, and never because of a merge.

## Cutting a release

On GitHub: **Actions > Release > Run workflow**, type the new version (like `0.4.0`), and run it.
Or from a terminal:

```sh
gh workflow run release.yml -f version=0.4.0
```

That is the whole job.
The workflow then:

1. checks the version is newer than the current one, and bumps it in `crates/clipdeck-app/Cargo.toml` and `Cargo.lock`;
2. commits that to `main` as "Release 0.4.0" and tags it `v0.4.0`, pushing with the [release deploy key](#the-release-deploy-key);
3. builds the Windows installer and the universal macOS app on GitHub's own runners, never on a dev machine (see [ADR-0007](adr/0007-ci-built-installers-with-auto-update.md));
4. publishes them as the latest release, once both builds have succeeded;
5. checks that the install commands now serve the new version.

It takes roughly ten minutes.
If a build fails, nothing is published and users keep the previous version.

### Options

- **notes**: what's new, shown on the release page and in the in-app update prompt.
  Left empty, it is the list of PRs merged since the last release.
- **draft**: stops before publishing, so you can try the build first.
  Fetch its files with `gh release download v0.4.0`, which, unlike a browser download, does not trip the unsigned-app warnings.
  Publish the draft from the Releases page (or `gh release edit v0.4.0 --draft=false --latest`) when you're happy with it.
  Use this for risky changes; installed copies are only offered a release once it is published.

### If something goes wrong

- **A build failed for a reason unrelated to the code** (a runner hiccup): open the run and choose "Re-run failed jobs".
- **A build failed because of the code**: fix it on `main`, delete the draft release and its tag (`gh release delete v0.4.0 --cleanup-tag`), and release the next patch version.
- **A published release is bad**: mark it as a pre-release on the Releases page.
  The install commands and the updater go back to the previous release straight away, since `releases/latest` skips pre-releases.
  Then fix it and release a new version; installed copies never downgrade on their own.

## Rules on `main` and on release tags

Three repository rulesets (**Settings > Rules > Rulesets**) guard what ships:

| Ruleset | Applies to | What it enforces | Who can bypass |
| --- | --- | --- | --- |
| Protect main | `main` | no force pushes, no deletion | nobody |
| Main needs a PR and green CI | `main` | changes arrive by PR, the `CI gate` check has passed, review threads are resolved | the release deploy key |
| Protect release tags | `v*` tags | tags can't be moved or deleted | the repository admin, for the cleanup in [If something goes wrong](#if-something-goes-wrong) |

`CI gate` is the last job in the CI workflow, and the only required check.
It fails if any lint or test job failed, and passes when they were skipped because the PR touches no code.
Require that one check, never the individual jobs: a skipped job never reports, and a PR waiting on it could not merge.

### The release deploy key

The Release workflow commits the version bump straight to `main`, which the rules above would refuse.
So it pushes with a deploy key that is on the bypass list of the "Main needs a PR and green CI" ruleset.

- The public half is the repository deploy key named "Release workflow" (**Settings > Deploy keys**), with write access.
- The private half is the `RELEASE_DEPLOY_KEY` repository secret, and exists nowhere else.
- Only the Release workflow uses it, and it still can't force-push or delete `main`.
- To rotate it, or if it is lost: generate a new pair (`ssh-keygen -t ed25519 -N "" -f key`), replace the deploy key with `key.pub` (write access on), set the secret from `key`, and delete both files.
  Nothing else depends on it.

## The update signing key

Every update is signed, and installed copies refuse updates that aren't signed with the key whose public half is in `tauri.conf.json` (`plugins.updater.pubkey`).
This is separate from OS code signing, which Clipdeck doesn't have (ADR-0002).

- The private key and its password are the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets.
- The maintainer keeps a backup of both in a password manager.
- **If the private key is lost, installed copies can never be updated again**: users would have to download and reinstall by hand once a new key is in place.
  Never rotate it without that plan.
