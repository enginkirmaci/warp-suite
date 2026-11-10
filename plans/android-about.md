# About on the Android keyboard

Status: proposal — not started. The three GPUI apps now carry the family About
screen (`warpui_kit_core::about`, documented in
`docs/agents/design-system.md` §"The About screen"). The Android IME is
deliberately outside that shell, so it gets its own compact block rather than a
rail section or a new screen.

## What it shows

At the bottom of `SettingsScreen`, above the existing
`warp-android-scribe · <version>` footnote, in the app's own Material3 language
(`ScribeColors`, `ScribeText`, `ScribeShape` — not the GPUI tokens):

- the app mark, `warp-android-scribe`, and `versionName (versionCode)`;
- the bridge facts the screen already knows: mode (Local/Remote), server URL,
  and the two engines with their readiness — **never the token**;
- Repository / Releases / Report an issue as `Intent`s to the suite repo;
- **Copy version**, and **License** (GPL-3.0) opening the repo's LICENSE.

## Why a block and not a screen

The mock's DESIGN.md says the Android app has its own design language and is
not part of the shell; a second navigation level for four facts would be more
than the app has anywhere else. Settings is already the one place a user looks
for the version.

## Steps

1. `appVersion()` in `SettingsScreen.kt` reads `versionName`; add the
   `versionCode` beside it (`longVersionCode` on API 28+, `versionCode`
   before).
2. Bind the block to the state the screen already holds (`mode`, server URL,
   engine readiness); no new request, no new permission.
3. Links through `Intent.ACTION_VIEW` with the suite repository URLs; the
   license row opens `apps/warp-android-scribe/LICENSE` in the browser.
4. Copy version through `ClipboardManager`, one line:
   `warp-android-scribe <versionName> (<versionCode>) · GPL-3.0`.
5. Remove the old version footnote once the block carries it.

## Done when

- `./gradlew assembleDebug` succeeds (there is no CI job for the Gradle
  project).
- The block shows on a phone-sized screen without pushing Save off the
  viewport, and the token appears nowhere in the UI, the log or a screenshot.
- The version line matches the About screen's tone: name, version, channel.
