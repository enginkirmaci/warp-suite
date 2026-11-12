# App repos, kit releases and the suite assembly

Status: proposal, 2026-11-12. Decisions settled (see *Decisions*). Phase 1
(kit line, `kits-release.yml`, `scripts/build/kit-build.sh`, the symlink tooling and
the local-setup guide) is implemented on the working tree and verified in a
scratch checkout. Phase 2 waits on the app repositories.

## Goal

- Every app lives in its own GitHub repository, with its own version, its own
  CI and its own release tag.
- `kits/` stays in warp-suite. The kit crates are versioned and released from
  here; app repos consume a kit release, never a path into this checkout.
- A developer machine keeps `warp-suite/apps/` as symlinks to the sibling app
  checkouts, so one suite workspace still builds every app against the local
  kits.
- A suite release is assembled in warp-suite from pinned app releases. warp-suite
  stops building app sources in CI; it downloads, verifies and combines.

## Target repositories

| Repo | Holds | Versioned by |
|---|---|---|
| `warp-suite` (private) | `kits/`, suite assembler, `suite.toml`, install/deploy scripts | `version.toml` (kit line) + `suite.toml` (suite release) |
| `warp-station` (private) | `warp-station` + `warp-station-core` crates, packaging | own `version.toml` |
| `warp-agent` (private) | one crate, packaging | own `version.toml` |
| `warp-notes` (private) | one crate, packaging | own `version.toml` |
| `warp-snapit` (private) | one crate, Windows-only, packaging | own `version.toml` |
| `warp-android-scribe` (private) | Gradle project + Rust bridge | own `version.toml` |

The old per-app repos are gone from the account (checked 2026-11-12); these are
new repositories created from this history.

## Version model

Three version lines, deliberately independent:

1. **Kit line** — `version.toml` in warp-suite, one `MAJOR.MINOR.PATCH` for all
   six `warpui-kit-*` crates (`[workspace.package].version`). Tag:
   `kits-v<version>`. The pre-commit hook keeps its current PATCH-per-commit
   behaviour; a kit tag is cut when an app should see the change.
2. **App line** — one `version.toml` per app repo with its own `version` and
   `build` counter; the same `version.py`/hook mechanics, retargeted from
   `[workspace.package]` to `[package]`. Tag: `v<version>`. App versions move
   independently of each other and of the kit line.
3. **Suite line** — `suite.toml` in warp-suite pins one app tag per app and one
   kit tag, plus a suite version. Tag: `v<suite version>`. A suite release is a
   reproducible statement of "these app releases, on this kit line".

Do not make the suite version a function of the app versions. The pin table is
the statement; the number is just the release name.

### Build numbers

Every repo keeps its own monotonic build counter in its own `version.toml`, and
every repo bumps it on its own build. There is no shared counter:

- **App repo:** its `scripts/build/build.sh` / `build.ps1` calls that repo's
  `version.py build bump` first, exactly as the suite does today, so every local
  app build consumes the next number. A release job reads the tag's
  `version.toml` and passes the number as `WARP_BUILD`, so a re-run of the same
  tag cannot mint a second number.
- **Kit repo:** a new `scripts/build/kit-build.sh` (the suite's build entry point after
  the split) bumps the kit build counter and packages the six crates;
  `kits-release.yml` reads the tag's value and records it in `kits.toml`.
- **Suite:** `suite.toml` records the suite build; the assembler writes it to
  the suite `REVISIONS.txt` next to every app's own build number.

The `WARP_SUITE_*` names stay as aliases in `warpui-kit-build`; each app passes
its own `WARP_BUILD` / `WARP_COMMIT` / `WARP_CHANNEL` / `WARP_RELEASED`. About
and `--version` then show that app's build, not the suite's.

## Kit consumption contract

App repos depend on the kits by git tag:

```toml
warpui-kit-core = { git = "https://github.com/enginkirmaci/warp-suite", tag = "kits-v0.2.0", package = "warpui-kit-core" }
```

- The URL is identical in every app and in the suite's `[patch]` table. Cargo
  keys a patch on the source URL, not on the tag.
- The app commits its `Cargo.lock`; the lock pins the kit commit, not just the
  tag.
- `warpui-kit-build` stays a build-dependency of each app's `build.rs`. It reads
  the app's own `version.toml` because `CARGO_MANIFEST_DIR` is the app's, not
  the kit checkout's.
- Probe done 2026-11-12: a scratch crate depending on `warpui-kit-build` by
  `git = "file://…"` resolved and built. Cargo resolves the kit manifests'
  `version.workspace = true` / `[workspace.dependencies]` through the git
  checkout's workspace root, so the kits do not need standalone manifests.

The suite root carries the reverse override for local development:

```toml
[patch."https://github.com/enginkirmaci/warp-suite"]
warpui-kit-core = { path = "kits/warpui-kit-core" }
# …one line per kit
```

This turns every app's kit git dependency into the local checkout when the app
is built through `warp-suite/apps/`. It must be verified for a tag-qualified
git source in Phase 1; if Cargo refuses the patch, the fallback is a local
`[patch]` written into each app checkout by the link script (and reverted by
the same script), which is worse but still workable.

Alternative not chosen: `cargo publish` the six kits to crates.io. It removes
the private-repo token and makes app CI cheaper, but adds registry versions,
publishing rights and a second release ceremony; the kit sources are already
tagged here. Keep it as a later option, not the first move.

## Local development: `apps/` stays, as symlinks

The developer keeps sibling checkouts:

```
~/Developments/warp-suite
~/Developments/warp-station
~/Developments/warp-agent
~/Developments/warp-notes
~/Developments/warp-snapit
```

`scripts/dev/link-apps.sh` creates `warp-suite/apps/<name> -> ../../<name>` (POSIX)
and `scripts/dev/link-apps.ps1` creates junctions on Windows. `apps/*` is
gitignored except a short `apps/README.md`.

The workspace model that makes this useful:

- App repositories carry **no `[workspace]` table**. A path dependency inside
  the app (station's `crates/warp-station-core`) still works; Cargo treats the
  app root as an implicit workspace and writes the app's own `Cargo.lock`.
- warp-suite's root workspace takes `apps/*` as members by glob next to
  `kits/*`. With the symlinks present, one `cargo build -p warp-notes` builds
  the app against the local kit sources, one lockfile, one target directory.
  With them absent (CI, a fresh clone), the glob simply matches nothing and
  warp-suite still builds and tests its kits.
- App repos carry no `.cargo/config.toml`, so building through the symlink
  uses warp-suite's target dir; a standalone app build uses its default
  `target/`. App `rust-toolchain.toml` is copied so a standalone build pins the
  same channel.
- App repos do not use `[workspace.dependencies]`; the gpui pins
  (`gpui-pre =0.3.11`, `gpui-pre-platform =0.3.11`, `gpui-kit 0.9`) are written
  literally in each app manifest. A suite-level check in Phase 1 compares the
  pins across app repos and fails on drift.
- The app's `Cargo.lock` and warp-suite's `Cargo.lock` differ by design: the app
  lock has git kit sources, the suite lock has path sources after `[patch]`.
  Do not copy one over the other.

## CI and release flows

### App repo

- `ci.yml` on push/PR: `cargo fmt --check`, clippy `-D warnings`, tests,
  `--locked`. The kit git fetch needs a read token for the private warp-suite:
  a fine-grained PAT in each app repo's secrets, applied with a
  `url.https://x-access-token:${TOKEN}@github.com/.insteadOf` git config in the
  job. Never echo it.
- `release.yml` on a `v*` tag: Linux and Windows jobs, then a GitHub Release
  with bundle-ready assets and a `SHA256SUMS`:
  - `<app>-<version>-linux.tar.gz` (binary + `.desktop` + hicolor icons),
  - `<app>-<version>-windows-x64.zip` (portable),
  - `<app>-<version>-setup-x64.exe` only where the app has an `.iss`,
  - `REVISIONS.txt` with the app commit, kit tag and kit commit.
- Releases are cut only from a clean tag: the release job checks
  `version.toml` against the tag, as `suite-release.yml` does today.

### Kit repo (warp-suite)

- `kits.yml` remains the gate: fmt, clippy and tests on the six kits. It no
  longer builds app sources.
- `kits-release.yml` on a `kits-v*` tag: check the tag against `version.toml`,
  run the gate (fmt, clippy, tests), build the source snapshot with
  `scripts/build/kit-build.sh --no-bump`, then publish a GitHub Release with
  `warp-suite-kits-<version>.tar.gz`, `kits.toml` (version, build, commit, tag)
  and `SHA256SUMS`. `cargo package` is not usable yet: the kit manifests inherit
  path dependencies without versions, so packaging stops on "all dependencies
  must have a version requirement". `cargo publish` stays a later opt-in job
  behind a `CARGO_REGISTRY_TOKEN` secret, and it needs those version fields
  first.
- `version.py` keeps owning `version.toml` and `Cargo.toml`; its app checks
  (`layout_errors`, workspace-member scan) move to the app repos' copy.

### Suite assembly (warp-suite)

- `suite.toml`:

```toml
[suite]
version = "0.2.0"

[kits]
tag = "kits-v0.2.0"

[apps.warp-station]
repo = "enginkirmaci/warp-station"
tag = "v0.4.0"

[apps.warp-agent]
repo = "enginkirmaci/warp-agent"
tag = "v0.3.1"
# …
```

- `suite-release.yml`, dispatched manually with a suite version (same
  tag/dispatch check as today):
  1. verify every pinned tag exists and `suite.toml` parses;
  2. download each app release's assets by tag with `gh release download`;
  3. verify `SHA256SUMS`;
  4. open the Linux tarballs, merge binaries, desktop entries and icons into
     one `warp-suite-<version>-linux/` bundle, merge the Windows assets into
     one `dist/windows/` set, write one `REVISIONS.txt` naming every app tag,
     commit and the kit tag;
  5. publish suite release `v<suite version>` with the tarball, installers and
     zips. Refuse a second release for the same tag, as today.
- `app.toml` in each app repo carries the build knobs that used to live in
  `apps.toml` (`package`, `icon_prefix`, `scalable_svg`, `linux_skip`, asset
  names). The suite reads it through the symlink; app CI releases use the same
  file, so a local build and a release cannot disagree.
- `scripts/build/build.sh` / `build.ps1` keep their one-workspace behaviour locally
  (the symlink workspace) and gain a `--pinned` mode: materialize the tags from
  `suite.toml` into `apps/` and build those, which is what the integration job
  uses.
- Every app repo's build script bumps that repo's build counter; the suite
  assembler only reads the released number and never rewrites an app's counter.
- `install.sh`, `deploy.sh`, `deploy.ps1`, `bundle-install.sh` keep installing
  from a suite release; they do not need to know app repos exist.

### Agent setup entry point

The local setup must be discoverable without asking a human. Two files, one
short and one complete:

- `AGENTS.md`: a "Local setup" section near the top with the five commands
  (clone the siblings, link, verify, install hooks, build).
- `docs/agents/local-setup.md`: the full recipe — required toolchain and Python
  versions, the sibling directory layout, what `scripts/dev/link-apps.sh` does
  (and its Windows junction twin), the private-repo token, the `[patch]`
  behaviour, how to verify `cargo metadata` sees the symlinked apps, and how to
  add or remove an app.

If the agent runtime loads repository skills, the same file is wrapped as a
`SKILL.md` under a repo-local skill directory; the markdown content stays
single-sourced so the two cannot drift.

### Integration gate

`integration.yml` in warp-suite (nightly and on `suite.toml` changes):

1. check out warp-suite and every app at the pinned tag into a temp tree,
2. symlink them into `apps/`,
3. `cargo fmt --check`, clippy and `cargo test` in the one workspace,
4. run the cross-app smoke probes (agent door handshake, snapit settings read,
   notes launch arguments) against the built binaries.

This is the only place the cross-app contracts are exercised together once the
repos split. It is the price of independent versioning.

## Migration phases

0. **Decide.** Answer the four questions at the bottom. Freeze app work on
   `main` for the duration of Phase 2–3.
1. **Kit line ready** (warp-suite):
   - `version.toml` becomes the kit line; `version.py` drops app-layout checks.
   - add `kits-release.yml`, `scripts/build/kit-build.sh`,
     `scripts/dev/link-apps.sh` / `.ps1` and the conditional app checks in
     `version.py`;
   - the `[patch]` table and the `apps/*` member glob land with Phase 3: while
     the apps still use path dependencies, a patch table only warns, and the
     glob needs the post-split app roots. Phase 1 proves both in a scratch
     checkout.
   - prove two things in a scratch checkout: a tag-qualified git dependency is
     patched by the path table, and `apps/*` glob members work through symlinks;
   - create `kits-v<current>` tag and publish a dry-run release (pending the
     first commit and push of this working tree).
2. **Create app repos from history**:
   - `git subtree split --prefix=apps/<name> -b split/<name>`, push the branch to
     the new repo's `main`, then tag `v<app version>`; the Android project uses
     `android-apps/warp-android-scribe` as its prefix;
   - keep the split branches out of warp-suite's long-term refs.
3. **Convert each app repo**:
   - drop the `[workspace]` table, make package fields literal, replace kit
     path dependencies with git-tag dependencies, copy `rust-toolchain.toml`,
     add `version.toml`, `app.toml`, `scripts/meta/version.py` (app variant),
     `.githooks/pre-commit`, `ci.yml`, `release.yml`, `README.md`/`AGENTS.md`;
   - in warp-suite, add the `[patch]` table and turn the workspace members into
     `kits/*` + `apps/*`;
   - add that repo's `scripts/build/build.sh` / `build.ps1` with the build-counter
     bump and the app-local `version.py` hook;
   - the app variant of `version.py` manages `[package] version` and the app
     lock; `warpui-kit-build` gets `WARP_BUILD_*` aliases for the
     `WARP_SUITE_*` env names, keeping the old names working.
4. **Slim warp-suite**:
   - remove `apps/` sources, add `apps/README.md`;
   - rewrite `apps.toml` into `suite.toml` (pins) + per-app `app.toml` files;
   - adjust `check-manifests.py`, `manifest.py`, build/deploy scripts and
     `version.py`;
   - update `AGENTS.md`, `README.md`, `docs/agents/versioning.md` and
     `docs/agents/testing-and-captures.md`.
   - add the short "Local setup" section to `AGENTS.md` and write
     `docs/agents/local-setup.md`; both point at `scripts/dev/link-apps.sh`.
5. **Suite assembly**:
   - implement `suite-release.yml` download/verify/merge;
   - cut a trial suite release from one app release per app;
   - verify `install.sh` and `deploy.ps1` against it.
6. **Integration gate and cleanup**:
   - add `integration.yml`;
   - delete finished plans (`plans/` entry for this one, the repo-split notes);
   - record the new model in `docs/agents/repo-model.md`.

## Costs and risks

- **Kit fetch auth.** App CI must read a private warp-suite. A PAT per app repo
  is an operational secret; rotating it breaks every app build until updated.
  If that becomes annoying, publishing the kits (public repo or crates.io) is
  the escape hatch.
- **Cross-app contracts.** The agent door, the snapit settings files, the notes
  store and `~/.warp/ui.json` used to change in one commit. Now they need
  coordinated PRs and the integration gate; a station release can ship against
  an older agent than the one it was tested with.
- **gpui cache spread.** App CI caches are per repository, so gpui-pre is
  compiled once per app repo per OS instead of once in the suite. Locally the
  symlink workspace and the shared `.build/target` keep the single compile;
  CI does not. Private-repo Actions minutes grow accordingly.
- **Pin drift.** Each app pins its own kit tag; "which kit version is app X on"
  is answered by `suite.toml` only for the suite release, not for an app's
  `main`. The integration job and a small `pins` check (compare app
  `Cargo.lock` kit commit against `suite.toml`) keep it visible.
- **Symlink workspace.** The one-workspace local build depends on the symlink
  targets existing and on Cargo following directory symlinks in member globs.
  Phase 1 proves both. A dangling symlink must fail the link script, not a cargo
  build an hour later.
- **Two lockfiles per app.** The app lock (git kits) and the suite lock (path
  kits) can disagree; `--locked` in each place catches a stale one. Never
  commit the suite lock into an app repo.
- **Private repos.** All app repos private means no public CI, no anonymous
  download of artifacts, and Actions minutes on every tag. Accept it or make
  the apps public.

## Decisions

Settled 2026-11-12:

1. **Kit consumption:** git tag dependency. Apps pin `kits-vX.Y.Z`; warp-suite
   `[patch]`es the same source to the local `kits/` while building through the
   symlink workspace.
2. **Local workspace model:** apps carry no `[workspace]`; warp-suite's
   `apps/*` member glob pulls the symlinked checkouts into one workspace with
   the local kits. One lockfile and one gpui compile locally.
3. **Android:** `warp-android-scribe` moves to its own repo too; its bridge
   takes the kits by tag and its Gradle build reads that repo's `version.toml`.
4. **History:** every app repo is created with `git subtree split`, keeping
   file history and attribution.
5. **Build counters:** every repo owns its counter; every build in that repo
   bumps it, and a release re-run of the same tag reuses the tag's number.
6. **Agent setup:** `AGENTS.md` carries the short path and
   `docs/agents/local-setup.md` the full recipe, with an optional repo-local
   `SKILL.md` wrapper over the same content.

## Acceptance criteria

- A clean `warp-suite` clone with no `apps/` still runs the kit gate green.
- `scripts/dev/link-apps.sh` + `cargo build -p <app>` builds every app against the
  local kits; `cargo test -p <app>` passes.
- An app repo checkout builds standalone with the pinned kit tag and no
  warp-suite checkout on disk.
- A `kits-v*` tag publishes a kit release; an app bump to that tag builds green
  in that app's CI.
- `suite-release.yml` produces one suite release whose `REVISIONS.txt` names
  every app tag, app commit and the kit tag, and whose Linux bundle installs
  with `bundle-install.sh`.
- `integration.yml` fails when an app tag is built against a kit tag it does
  not support, and passes for the pinned combination.
- Every app build consumes that repo's next build number; a release re-run of
  the same tag reports the same number; `REVISIONS.txt` shows each app's own
  build and the kit build.
- A fresh machine can follow `docs/agents/local-setup.md` from a clean account
  and end with `cargo build -p <app>` working through the symlinks.
