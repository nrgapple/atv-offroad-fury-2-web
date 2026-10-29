# Standalone desktop releases

The shared Rust engine (`crates/atv-app`, binary `fury`) also ships as standalone desktop
apps with the game's runtime resources packed in, so a download is the whole game: no disc,
emulator, installer or separate content folder. Archives are built and uploaded from the
owner's machine, never in CI, because they contain the extracted runtime resources
(AGENTS.md: zipped desktop builds as GitHub Release assets are the one place that content may
be published outside the R2 content package). The ISO, BIOS, raw boot executable
(`SCUS_972.11`), unrelated disc content and reverse-engineering output never enter an archive.

Status: since the switch-over (2026-10-23, `SWITCHOVER.md`) the browser game and the desktop
apps run the same Rust engine, so desktop releases carry the same menus, races, HUD and
multiplayer as `play.html` (see `RUST_ENGINE.md` for what is and is not ported yet). Releases
are marked prerelease until the port is further along, and are built automatically after app
changes merge (see "Automatic releases").

## What is in an archive

| Platform | Archive | Contents |
| --- | --- | --- |
| macOS (Apple silicon + Intel) | `fury-remastered-<version>-<commit>-macos-universal.zip` | `Fury Remastered/Fury Remastered.app` (universal Mach-O via `lipo`, `Info.plist`, icon, ad hoc signature, `Contents/Resources/content.pack`) and `README.txt` |
| Linux x86_64 | `...-linux-x86_64.tar.gz` | `fury` (pack appended) and `README.txt`; built on glibc 2.31 (Debian 11) |
| Linux aarch64 | `...-linux-aarch64.tar.gz` | same, for arm64 |
| Windows x86_64 | `...-windows-x86_64.zip` | `fury.exe` (pack appended, `x86_64-pc-windows-gnu`) and `README.txt` |

Each `README.txt` says what the download is (an unofficial fan port, nothing else needed) and
how to get past the first-launch warnings. `SHA256SUMS.txt` and `release.json` (commit, sizes,
SHA-256, build seconds, smoke results) sit beside the archives in
`~/LargeFiles/fury-desktop/<short-commit>/` (SSD backed).

## How the content is packed

`tools/pack_content.py` flattens the private hosted package
(`private/reports/hosted-content`: `manifest.json` plus SHA-256 named chunks, the same files
the browser fetches from R2) into one pack file in `~/LargeFiles/fury-desktop/packs/`. Format
and reader: `crates/atv-content/src/pack.rs` (`Store::Pack`). Properties:

- Only manifest records that pass the runtime allowlist are packed: absolute `/PATH/NAME`
  files ending `.RES`, `.VAG` or `.LSI`, the USA v2.00 build hash, contiguous chunks, no
  `SCUS*`, ISO, BIOS, ELF, CUE, ZIP or `..` paths, plus each song record's `audio/mp4`
  soundtrack stream. Anything else fails the build. It also
  refuses to write the pack anywhere inside the repository that Git does not ignore.
- Chunks are deduplicated and verified against their SHA-256 while packing; `verify` re-hashes
  every blob. The reader checks the manifest digest on open, and `Content::file` re-hashes
  each chunk it reads, exactly as it does for a package directory.
- The pack is uncompressed. Measured on this package: DEFLATE saves 9% overall (`.RES` 2%,
  `.LSI` 0.1%, `.VAG` 17%), so the pack costs nothing to open and the archives are only
  slightly smaller than the pack. Not worth startup time or code.
- The 21 soundtrack `.m4a` songs the web streams are packed too (87.4 MB), because the
  desktop app plays the same remaster (AUDIO.md, "Desktop soundtrack"). `--no-streams`
  leaves them out (`fury-content-<hash>-nostreams.pack`); that app then plays the
  original VAGs. `build_desktop.py` refuses a pack that lacks any song.
- 541.9 MB, 510 blobs, 219 files plus 21 songs (454.4 MB and 489 blobs without the
  songs).

```sh
python3 tools/pack_content.py build            # -> ~/LargeFiles/fury-desktop/packs/fury-content-<hash>.pack
python3 tools/pack_content.py verify PACK
```

### Where the pack lives (and why)

`fury` looks for content in this order: `--content DIR` / `FURY_CONTENT`, `--pack FILE` /
`FURY_PACK`, a pack compiled in (cargo feature `atv-app/embedded-content`), a pack appended
to the executable, `content.pack` beside the executable or in `Contents/Resources`. With none
of them it exits with a message explaining the options. `fury --verify-content` reads and
hashes every file of whichever source it found (no window or GPU needed).

- **macOS: `Contents/Resources/content.pack`.** An appended or embedded pack would sit inside
  the Mach-O and be duplicated in both `lipo` slices (about 910 MB) and invalidate the
  signature. In Resources the bundle stays sanely signable (ad hoc today, Developer ID and
  notarization later) and one copy serves both architectures.
- **Linux and Windows: appended to the executable.** `build_desktop.py` builds the normal
  content-free binary (the very same compile CI does) and appends `pack + "FURYEXE1" + length`.
  The compiler never sees the content: the cargo `embedded-content` feature
  (`include_bytes!` of `FURY_PACK`) works and was measured, but it costs about 4.3 GB of
  compiler memory for a 454 MB pack, more than the 4 GiB Colima VM has, and it would repeat
  that per target. Appending is instant. Caveat: a later Windows Authenticode signature
  would need to be applied to a content-free exe with a sidecar `content.pack`, because
  signing moves the end of the file (the sidecar lookup already works on every OS).

Normal and CI builds never enable the feature and never see a pack. CI compiles
`atv-app` with default features, and the `Desktop compile check` workflow
(`.github/workflows/desktop-check.yml`) compiles and tests the content readers on macOS
and Windows, and builds the `embedded` feature against a 700 byte synthetic fixture pack
(`pack_content.py fixture`; the repository tracks no binaries, so the unit-test copy is the hex file `crates/atv-content/tests/fixtures/mini.pack.hex`).

## Build

```sh
make desktop                                   # or: python3 tools/build_desktop.py
python3 tools/build_desktop.py --platforms macos-universal,windows-x86_64
python3 tools/build_desktop.py --smoke         # also launch the Linux builds under Docker
python3 tools/build_desktop.py --allow-dirty   # local test build of an uncommitted tree
```

Requirements: Rust with `aarch64-apple-darwin` and `x86_64-apple-darwin` targets
(`rustup target add x86_64-apple-darwin`), Xcode command line tools (`lipo`, `codesign`,
`ditto`, `iconutil`, `sips`), and Docker (Colima is fine) for Linux and Windows. The build
requires a clean tree (the commit is the version) and uses `git archive HEAD` for Docker.

- macOS builds natively for both architectures (`MACOSX_DEPLOYMENT_TARGET=11.0`), runs
  `lipo`, assembles the bundle (icon from the authored `web/fury-app-icons.json` 512 px art via
  `iconutil`), signs ad hoc and zips with `ditto` (bundle structure and symlinks preserved).
- Linux and Windows build in a dedicated Colima VM, profile `fury-builder` (6 CPUs, 8 GiB, vz with
  Rosetta for the x86_64 smoke run), kept on the SSD under `COLIMA_HOME=~/LargeFiles/colima`.
  `build_desktop.py` starts it on demand, points `docker` at its socket (`DOCKER_HOST`) and stops
  it afterwards to give the memory back (`FURY_KEEP_BUILDER_VM=1` keeps it running;
  `--builder-vm none` uses the current docker instead). Only `~/.cargo/registry` is mounted.
  Why: the owner's default 2 CPU / 4 GiB VM runs other services, and the fat-LTO link of `fury`
  for aarch64 was OOM-killed there on bd347cc (2026-10-24). Cold builds in the new VM (fresh
  target volumes): linux-x86_64 10m45s, linux-aarch64 10m57s, windows-x86_64 7m20s.
- Windows: Rust hides a DLL's internal symbols with `-exclude-symbols` linker directives that
  Debian 11's MinGW ld (binutils 2.35) ignores, so the `atv-app` cdylib (built alongside the
  `fury` bin and unused on Windows) exported about 70 000 symbols and failed with "export ordinal
  too large". The Windows build passes `-Wl,--exclude-all-symbols`.
- Linux and Windows run in the image `fury-desktop-builder:bullseye-5` (built from
  `rust:1-bullseye` plus `gcc-x86-64-linux-gnu`, `gcc-aarch64-linux-gnu`, `gcc-mingw-w64-x86-64`,
  `libudev-dev` and `libasound2-dev` for both Linux architectures (gilrs reads controllers
  through libudev; cpal sound output links ALSA) and the three Rust targets). The VM shares
  only the cargo registry and the repo is on the SSD, so the
  source is piped into the container as a tar on stdin, each target uses its own named Docker
  volume for `target/` (`fury-desktop-target-<triple>`; the older `fury-desktop-target` volume
  from earlier experiments can be removed), `$HOME/.cargo/registry` is mounted so crates are not
  re-downloaded, and only the finished binary comes back out as a tar on stdout. The owner's
  default Colima VM and its containers are untouched.
- Linux archives are `tar.gz` (the executable bit survives extraction); macOS and Windows are
  zips. Timestamps are the commit time and owners are normalized.
- Every archive is checked after it is written (`tools/desktop_archive.py`): a strict member
  allowlist, one top-level folder, no ISO/BIOS/`SCUS`/private/report paths, correct
  executable magic, and the packed content verified end to end.

Cargo target directories: native builds use `$CARGO_TARGET_DIR` or
`~/LargeFiles/fury-desktop/target`.

### Verification (Apple M4, macOS 27, Colima 2 CPU / 4 GiB; first release commit a2547c8, 2026-10-21)

`--capture OUT.png 1280x720` renders the ride's first frame and exits; it was used on every
platform that can run here. Content check: `fury --verify-content` reads and hashes all 219
files (454 MB).

| Platform | Result |
| --- | --- |
| macOS universal | Ran from the unzipped app (arm64 slice, Metal): ride-view capture in 1.0 s warm (5.3 s on the very first launch of a fresh download while macOS assesses it); content check 2.1 s; an SX05 capture also renders the full stadium. `open -n` launches and keeps running. The x86_64 slice was built and signed but not run (no Rosetta on this Mac). `spctl` rejects the app, as expected for an ad hoc signature. |
| Linux aarch64 | Launched natively in Docker (Debian 12, Xvfb, Mesa lavapipe software Vulkan): content check 2.1 s, capture 2.9 s including software rendering; the quad, rider and scenery are drawn. |
| Linux x86_64 | Launched in Docker under qemu emulation on the arm64 VM (same image recipe, `linux/amd64`): content check 6.6 s, capture 15.4 s (qemu); same frame. |
| Windows x86_64 | Built and structurally verified only. Wine 8 (Debian 12) under qemu-user on this arm64 VM cannot start even a hello-world Rust `x86_64-pc-windows-gnu` executable ("failed to start"), so no runtime result either way. Needs a run on a real Windows machine. |

Sizes: archives are about 417-419 MB each (the 454 MB pack plus a roughly 9-12 MB executable)
(DEFLATE/gzip saves about 9%). Bundling the soundtrack songs (2026-10-22) adds about 87 MB
to each archive (DEFLATE saves 0.6% on AAC), so expect about 505 MB each. That has not
been measured on a release build yet.

## Publish

```sh
python3 tools/publish_desktop_release.py --dry-run   # all checks, prints the gh command
python3 tools/publish_desktop_release.py             # type the tag to confirm
python3 tools/publish_desktop_release.py --verification "what was launched and what was not"
```

It refuses to run when `GITHUB_ACTIONS` or `CI` is set, with a dirty tree, or when `HEAD` is
not an ancestor of `origin/main`; requires archives for all four platforms built from exactly
that commit; re-verifies checksums, the allowlist and the packed content; and refuses an
existing tag. The release is `desktop-v<version>-<short-commit>` (override with `--tag`),
marked prerelease (`--stable` changes that), with the four archives and `SHA256SUMS.txt`.
The tests (`tools/test_desktop_release.py`, part of `make check`) cover the pack builder,
allowlist, publish guards and the watcher with synthetic data only.

The watcher below publishes automatically; run these by hand to release outside its schedule.

## Automatic releases (installed on the owner's Mac)

Desktop releases are part of the delivery pipeline: GitHub Actions deploys the web game on
every merge, and a LaunchAgent on the owner's Mac turns merged app changes into desktop
releases, because the archives contain game content and must never be built in CI.

`tools/desktop_release_watcher.py --auto-main` makes one pass every 15 minutes (and at login):

- **New `desktop-v*` tags** on origin are built and published as before (push a tag to force
  a release of a specific commit).
- **origin/main** is released as `desktop-v<workspace version>-<short commit>` when it changed
  anything the apps are built from since the newest desktop release (`RELEASE_PATHS`:
  `crates/`, `third_party/` (patched winit), `Cargo.toml`/`Cargo.lock`, the desktop, Android and pack build tools, the app icon art),
  and that release is at least 12 hours old (`--min-hours`). Documentation, dev-log, web-shell
  and test-only merges never start a release. The 12 hour floor keeps a busy merge day to
  one or two roughly 2 GB releases instead of one per merge.

Each build adds a temporary worktree at the commit, links `private`, runs
`build_desktop.py --smoke --output ~/LargeFiles/fury-desktop/<short>`, refuses to publish when
a Linux launch failed, builds the Android APK with `build_android.py --output <out>/android`
(ANDROID.md; it carries no content), then runs `publish_desktop_release.py --tag <tag> --yes
--android <out>/android` (still limited to commits on `origin/main`; the APK must come from the
same clean commit, match its checksum and pass the APK member allowlist) with release notes
stating exactly what was launched. A failed Android build still publishes the desktop archives;
the state entry records `android_error` and the notification says so. Results go
to `~/.local/share/project-service-data/fury-desktop-release/watcher-state.json` and a macOS
notification; job and per-release logs live beside that state. The small installed runtime
also stays there, independent of the shared checkout's current revision. Archives, caches,
content and temporary worktrees stay on the verified external SSD.

State writes are atomic and errors fail closed. The 12-hour automatic-attempt cooldown
includes failures and newer main commits; any release failure additionally backs off the
whole watcher for one hour. New explicit tags remain pending during that backoff. A busy
host defers without recording a failed attempt. The watcher checks the SSD UUID, source
and content readability, and output writability before starting a build. It shares the
host heavy-work lease with local builds/tests and never interrupts an existing job.

The first pass adopts existing remote tags. A failed tag is not retried until its failed
entry is deliberately removed; removing it does not bypass the global cooldown. The
legacy state is migrated only after its old writer exits. Published archive retention
remains three releases; unmerged work and failed build evidence are preserved.

Bump the workspace `version` in `Cargo.toml` for a notable release; the next automatic pass
publishes it under the new number.

Install or update from a tested task worktree (see [host budget](HOST_BUILD_BUDGET.md)):

```sh
python3 tools/install_host_build_budget.py --repo "$HOME/Projects/atv-offroad-fury-2-web" --apply
```

The installer preserves the rustup Cargo proxy, installs a reversible guard for future
Cargo invocations, disables new starts of the old watcher without stopping its active
pass, and starts a one-shot handoff. That handoff waits for the active watcher and its
state lock before migrating state and reloading launchd. Inspect
`~/.local/share/project-service-data/fury-desktop-release/handoff.json` and `handoff.log`.
The installed watcher uses Background priority and nice 10. Do not bootout or kickstart
an active release to apply an update.

macOS privacy protection requires approval for the application owning the actual interpreter.
On this host, Python is embedded in **Xcode.app**. Grant Xcode access to Removable Volumes
under Files and Folders, or add `/Applications/Xcode.app` to Full Disk Access if that
narrower option is unavailable. Approving only the nested Python app was insufficient:
TCC's authorization subject was `com.apple.dt.Xcode`. The installer now pins the running
framework executable reported by `ps`; it does not use `/usr/bin/python3`, whose shared
Apple launcher identity was being attributed to `/usr/bin/git` in background checks.
No privacy database is modified by these tools. The user grants access in System Settings.

A newly launched one-shot job using the framework executable passed source/content reads
and SSD output writes after Xcode was approved. Existing releases were not restarted.
After an Xcode upgrade or interpreter relocation, rerun the installer from the intended
Python runtime. The `@PYTHON@` template placeholder is filled by the installer.

The job needs the SSD, Docker (Colima running), `gh` signed in and the private content
package (`private/reports/hosted-content` in the shared checkout) when it fires; a pass
that finds one missing records a failure and notifies. The Mac must be awake, so releases
follow merges by up to 15 minutes plus the build time (about 30 to 60 minutes for all four
platforms) whenever the machine is on.

## Signing and first-launch warnings

- **macOS:** ad hoc signed (`codesign -s -`), not notarized. Gatekeeper blocks a downloaded
  copy: Control-click, Open; or `xattr -dr com.apple.quarantine "Fury Remastered.app"`. The
  first launch of a fresh download is also slower while macOS assesses it. A Developer ID
  certificate plus `notarytool` would remove the warning; the bundle layout already supports it.
- **Windows:** unsigned, so SmartScreen shows "Windows protected your PC": More info, Run
  anyway. Unsigned executables with embedded data can also draw antivirus false positives.
  The exe is a console-subsystem program, so a console window appears (useful while the app
  prints its diagnostics); it has no embedded icon yet.
- **Linux:** needs a Vulkan driver, an X11 or Wayland session, `libudev.so.1` (controllers),
  `libasound.so.2` (sound) and glibc 2.31 or newer. The local smoke image
  `fury-desktop-smoke:bookworm-3-<arch>` includes both libraries. A successful headless
  content or capture check does not establish working sound on a desktop audio device.

## Known gaps

- Archives are about 505 MB each with the soundtrack songs (about 2 GB per release). GitHub
  allows 2 GiB per asset.
- The desktop app inherits the shared engine's current gaps; `RUST_ENGINE.md` tracks
  subsystem progress. The archive measurements above predate the in-progress audio and UI work.
- Windows has not been run at all and Linux was only run on software Vulkan under Docker;
  real GPU backends (DX12/Vulkan on PC, Metal on Intel Macs) are untested.
- No Windows icon resource, no Developer ID or Authenticode signing, no notarization.
