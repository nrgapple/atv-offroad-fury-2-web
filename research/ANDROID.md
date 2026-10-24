# Android app

The Rust engine (`crates/atv-app`) also builds as an Android app: one arm64 APK whose only
code is the engine library `libatv_app.so`, run by the platform's `NativeActivity` through
winit's `android-native-activity` backend (android-activity). There is no Java, Kotlin or
Gradle. The same `App` (frontend, rides, touch controls, audio, multiplayer client) runs as
on desktop and the web.

Status (2026-10-24): verified on the arm64 Android 15 emulator only (details below). Not
yet run on a physical phone; emulator results say nothing about phone frame rate, thermals or
GPU driver behavior.

## What is in the APK, and what is not

The APK carries **no game content**. On first launch the app downloads the same hosted
content package the browser autoloads (`https://furyremastered.com/content/`:
`manifest.json` plus SHA-256 named `.bin` chunks and the 21 `.m4a` soundtrack songs, about
542 MB) into its private internal storage and then reads it with `Store::Directory`, exactly
as `fury --content DIR` does on desktop. So the published APK is authored code only (18 MB),
like the web app; content stays in R2 under the existing publication rules.

APK members are checked against a strict allowlist after every build
(`build_android.check_members`): `AndroidManifest.xml`, `resources.arsc`, the launcher icons
and `lib/arm64-v8a/libatv_app.so`, plus signature files. Any `.pack`, `.RES`, `.VAG`, `.LSI`,
`.bin`, `.m4a`, ISO/ELF/`SCUS` or `manifest.json` member fails the build, as does a library
over 120 MB.

## Content download (`crates/atv-app/src/content_sync.rs`)

- The hosted `manifest.json` is fetched (ureq, rustls with webpki roots) and validated with
  the game's own `atv_content::Manifest::parse` (build hash, paths, chunk extents, streams).
- Every referenced blob not yet present at its size downloads with four parallel workers to
  `<name>.part`, is checked against its size and SHA-256 while streaming, and only then
  renamed into place. A present blob of the right size is therefore a verified one, and an
  interrupted download (app killed, network lost) resumes where it stopped.
- `manifest.json` is written last, so the previous package stays complete and readable until
  a new one is entirely present; blobs the new manifest no longer references are removed
  afterwards. A changed hosted package (new content hash) is picked up on the next launch.
- Offline launches use the last complete local package. Without one, the screen shows the
  error and a tap retries.
- `Content` still re-hashes every chunk it reads, as on every other platform.
- Unit tests (`cargo test -p atv-app content_sync`, synthetic package in memory): full sync,
  manifest-only second run, single missing blob refetched, package replacement removing stale
  blobs, a corrupt download rejected with the old package kept, offline fallback.

The download screen (`android.rs` `SyncScreen`) draws with the engine's own UI renderer and
bundled Fira Sans Condensed: title, percentage, a progress bar, MB done of total and the
measured speed, or the error and "Tap to try again". The download runs on its own thread;
the event loop stays responsive and the screen redraws at most every 100 ms.

Developer shortcut: content pushed to the app's external files directory takes precedence,
either a pack or a package directory:

```sh
adb push content.pack /sdcard/Android/data/com.furyremastered.android/files/content.pack
adb push hosted-content /sdcard/Android/data/com.furyremastered.android/files/content
```

## Engine changes for Android

- `android.rs`: `android_main` (stderr and stdout routed to logcat under the tag `fury`,
  settings file `files/settings.json` through `FURY_SETTINGS`, content store choice, the
  download screen) and the lifecycle hooks below. `lib.rs` builds the event loop with
  `EventLoopBuilderExtAndroid::with_android_app`.
- **Surface loss.** Android destroys the window's surface whenever the app leaves the screen.
  `Suspended` marks every surface stale (`atv_render::window_surface_lost`) and releases the
  app's GPU surfaces; `Resumed` makes new ones on the new native window
  (`Gpu::recreate_surface`). The device and every GPU resource (venue, ride, UI atlases)
  stay, so a ride continues where it was. A GPU that arrives from a load while the app is in
  the background gets its surface on the next resume (`fit_surface`). Desktop and web never
  mark a surface stale, so their behavior is unchanged.
- **Audio.** cpal uses AAudio. When the app loses focus the output stream is paused outright
  (the mixer's silence alone would keep the stream running in the background) and restarted
  on return.
- **Touch first.** The viewport reports `(hover: none) and (pointer: coarse)` as a phone
  browser does, so the touch riding controls, touch HUD and the coarse speed-rush taps apply.
  The Back key acts as Escape (pause menu, menu back).
- **Backends.** Vulkan where available, else OpenGL ES 3 (`Backends::VULKAN | GL`).
- **Render resolution.** On Vulkan the swap chain follows the web's render ratio (at most 1.5
  backing pixels per CSS pixel, stepped down by the adaptive `RenderScale` when frames are
  slow) and the compositor scales it to the screen, as Metal/DX12 desktops do. OpenGL ES
  keeps the native size (wgpu's EGL path pins the buffers to the window).
- **Screen on.** `FLAG_KEEP_SCREEN_ON` is set once before the event loop, so the screen stays
  on whenever the game is in front (menus included; the system ignores it in the
  background). The desktop policy (only while riding) cannot be reproduced yet:
  android-activity 0.6 takes a write lock to change window flags that its event dispatch
  already holds, so changing them from inside the app deadlocks (observed on the emulator,
  both directly and from a helper thread).
- **Not on Android:** the clipboard (arboard has no Android backend: Copy ride link reports
  failure, paste finds nothing), the `keepawake` inhibitor, and game controllers (gilrs
  compiles but has no Android backend; winit delivers pad buttons as unidentified key
  codes, which are not mapped yet).

## Toolchain (owner's Mac)

All on the SSD: SDK `~/LargeFiles/android-sdk` (command-line tools, `platforms;android-35`,
`build-tools;35.0.1`, `platform-tools`, `ndk;28.2.13676358`, `emulator`,
`system-images;android-35;google_apis;arm64-v8a`), AVDs in `~/LargeFiles/android-avd`
(`fury35`, Pixel 7 profile), Cargo output in `~/LargeFiles/fury-android/target`. Rust target
`aarch64-linux-android`. Java: the Homebrew OpenJDK 26 runs sdkmanager, keytool and
apksigner. `~/LargeFiles/fury-android/env.sh` exports the variables used below.

## Build and sign (`tools/build_android.py`)

```sh
python3 tools/build_android.py                    # clean tree required
python3 tools/build_android.py --allow-dirty      # local test build (version marked -dirty)
python3 tools/build_android.py --install --launch # adb install -r, then start it
make android ANDROID_ARGS='--install'
```

It refuses to run in CI (`GITHUB_ACTIONS`/`CI`), cross-compiles `atv-app --lib` (release,
`--locked`) with the NDK clang linker (API 26), strips the library, writes the manifest
(NativeActivity, `android.app.lib_name=atv_app`, `sensorLandscape`, fullscreen theme with
cutout use, INTERNET, `hasCode=false`, `extractNativeLibs=false`, `allowBackup=false`, minSdk
26, targetSdk 35), rasterizes the authored 192 px icon from `web/fury-app-icons.json` to the
five launcher densities (`sips`), links with `aapt2`, stores the library uncompressed and
16 KB page aligned (`zipalign -P 16`), signs with `apksigner` (v2/v3) and verifies. Output in
`~/LargeFiles/fury-android/<short-commit>/`:

- `fury-remastered-<version>-<short>-android-arm64.apk` (package `com.furyremastered.android`;
  `versionName` is the workspace version, `versionCode` the commit count
  `git rev-list --count HEAD`, which only grows along main)
- `SHA256SUMS.txt`
- `release.json` (commit, version, versionCode, SDK levels, size, SHA-256, members, seconds)

**Release key.** Made once by the script at `~/LargeFiles/fury-android/keys/fury-release.jks`
(PKCS12, RSA 4096, alias `fury`, 100 years) with its password in `fury-release.pass` next to
it; both files are 0600 in a 0700 folder and never in the repository. Every update must be
signed with this key to install over an earlier build, so **back up both files**; losing them
means users must uninstall to update.

CI (`.github/workflows/desktop-check.yml`, job `android`) runs clippy for
`atv-app --lib --target aarch64-linux-android` with the runner's preinstalled NDK; it never
builds or signs an APK. `tools/test_android_build.py` (in `make check`) tests the manifest,
member allowlist, CI refusal, version parsing, naming and key creation with no SDK.

## Verification (emulator, 2026-10-24)

Android 15 (API 35) arm64 Google APIs image, Pixel 7 profile (2400x1080), headless emulator on
an Apple M4 host, SwiftShader (software Vulkan) unless noted. Screenshots (ignored, shared
checkout): `private/reports/devlog-images/android-emulator-*.png`.

- Install and launch: the download screen appeared and the content synced from R2:
  541.7 MB at about 0.9-1.4 MB/s through the emulator's network (about 8 minutes).
  `android-emulator-content-download.png`.
- Backgrounding (Home) during the download and returning: same process, surface recreated,
  download continued. A force-stop at 32% and relaunch resumed at 37% (already present blobs
  were skipped).
- After the sync the title screen rendered, then Main Menu (touch: Press Start, Single
  Player, Freestyle, Caribou Peak, Ride), including the 3D quad preview and the live Free Ride
  rider count from the relay. `android-emulator-title.png`, `android-emulator-main-menu.png`.
- Freeride on Caribou Peak loaded and rendered with the touch HUD; holding a touch drove the
  quad (6, 11, 15 MPH in the status lines); the remaster soundtrack played.
  `android-emulator-freeride-caribou.png`.
- Home during the ride and back: same process, the ride resumed drawing; the AAudio stream
  went `started` -> `paused` in the background -> `started` (dumpsys audio).
  `android-emulator-after-background-resume.png`. Back opened the pause menu
  (`android-emulator-back-pause.png`).
- A relaunch with the content present skipped the download (manifest check only) and reached
  the title in seconds.
- Frame time under SwiftShader: about 600-1600 ms per frame (CPU rasterization; meaningless
  for phones).
- Host GPU emulator (`-gpu host`, gfxstream Vulkan forwarded to the M4): Caribou Peak
  Freeride at 27-35 fps, 29-37 ms per frame, 22-29 ms CPU per frame (the emulator's command
  forwarding is part of that), 110-116k triangles, render ratio scaled swap chain. The touch
  steering wheel, GPS map and speed display drew and responded.
  `android-emulator-hostgpu-touch-ride.png`. Still not a phone measurement.
- A tap sent while the first GPU device was still opening (several seconds on the emulator)
  produced an "isn't responding" dialog once; after startup input was immediate. Phone
  startup time is unmeasured.

## Known gaps

- No physical phone run yet: frame time, thermals, Vulkan driver quirks, GLES fallback and
  16 KB page devices are unverified.
- Safe-area insets are zero (winit does not report Android insets); the fullscreen theme
  hides the status bar, and the gesture bar overlays the bottom edge.
- No immersive mode (needs Java-side calls on the UI thread).
- Clipboard, controllers and the riding-only screen-wake policy as listed above.
- Download corruption found later at read time (a blob damaged on disk after it was
  verified) fails the load with the integrity error; a reinstall or clearing app data fixes
  it. Automatic repair is not implemented.
- Not on Google Play; distribution is the signed APK attached to each automatic GitHub
  Release beside the desktop archives (DESKTOP_RELEASES.md, "Automatic releases").
