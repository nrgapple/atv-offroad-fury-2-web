# Standalone desktop releases

The shared Rust engine (`crates/atv-app`, binary `fury`) also ships as standalone desktop
apps with the game's runtime resources packed in, so a download is the whole game: no disc,
emulator, installer or separate content folder. Archives are built and uploaded from the
owner's machine, never in CI, because they contain the extracted runtime resources
(AGENTS.md: zipped desktop builds as GitHub Release assets are the one place that content may
be published outside the R2 content package). The ISO, BIOS, raw boot executable
(`SCUS_972.11`), unrelated disc content and reverse-engineering output never enter an archive.

Status: the desktop app lets you ride the default quad around ENV01 Freeride (see
`RUST_ENGINE.md` for what is and is not ported yet), so releases are marked prerelease until
the port is further along.

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
  `SCUS*`, ISO, BIOS, ELF, CUE, ZIP or `..` paths. Anything else fails the build. It also
  refuses to write the pack anywhere inside the repository that Git does not ignore.
- Chunks are deduplicated and verified against their SHA-256 while packing; `verify` re-hashes
  every blob. The reader checks the manifest digest on open, and `Content::file` re-hashes
  each chunk it reads, exactly as it does for a package directory.
- The pack is uncompressed. Measured on this package: DEFLATE saves 9% overall (`.RES` 2%,
  `.LSI` 0.1%, `.VAG` 17%), so the pack costs nothing to open and the archives are only
  slightly smaller than the pack. Not worth startup time or code.
- The 21 browser jukebox `.m4a` streams are not packed by default (the desktop engine has no
  audio yet; the `.VAG` originals are in the pack). `--include-streams` adds them.
- 454 MB, 489 blobs, 219 files.

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
- Linux and Windows run in the local image `fury-desktop-builder:bullseye-4` (built from
  `rust:1-bullseye` plus `gcc-x86-64-linux-gnu`, `gcc-aarch64-linux-gnu`, `gcc-mingw-w64-x86-64`,
  `libudev-dev` for both Linux architectures (gilrs reads controllers through libudev) and the
  three Rust targets). Colima only shares `$HOME` and the repo is on the SSD, so the
  source is piped into the container as a tar on stdin, each target uses its own named Docker
  volume for `target/` (`fury-desktop-target-<triple>`; the older `fury-desktop-target` volume
  from earlier experiments can be removed), `$HOME/.cargo/registry` is mounted so crates are not
  re-downloaded, and only the finished binary comes back out as a tar on stdout. Other
  containers are untouched and no Colima setting changes.
- Linux archives are `tar.gz` (the executable bit survives extraction); macOS and Windows are
  zips. Timestamps are the commit time and owners are normalized.
- Every archive is checked after it is written (`tools/desktop_archive.py`): a strict member
  allowlist, one top-level folder, no ISO/BIOS/`SCUS`/private/report paths, correct
  executable magic, and the packed content verified end to end.

Cargo target directories: native builds use `$CARGO_TARGET_DIR` or
`~/LargeFiles/fury-desktop/target`.

### Verification done on 2026-10-20 (Apple M4, macOS 27, Colima 2 CPU / 4 GiB)

`--capture OUT.png 1280x720` renders the ride's first frame and exits; it was used on every
platform that can run here. Content check: `fury --verify-content` reads and hashes all 219
files (454 MB).

| Platform | Result |
| --- | --- |
| macOS universal | Ran from the unzipped app (arm64 slice, Metal): capture in 0.65 s warm (4 s on the very first launch of a fresh download while macOS assesses it); the PNG is byte-identical to a capture from the package directory. `open -n` launches and keeps running. The x86_64 slice was built and signed but not run (no Rosetta on this Mac). `spctl` rejects the app, as expected for an ad hoc signature. |
| Linux aarch64 | Launched natively in Docker (Debian 12, Xvfb, Mesa lavapipe software Vulkan): content check 2.0 s, capture 1.1 s including software rendering; the quad and rider are drawn. |
| Linux x86_64 | Launched in Docker under qemu emulation on the arm64 VM (same image recipe, `linux/amd64`): content check 6.9 s, capture 8.3 s; same frame content. |
| Windows x86_64 | Built and structurally verified only. Wine 8 (Debian 12) under qemu-user on this arm64 VM cannot start even a hello-world Rust `x86_64-pc-windows-gnu` executable ("failed to start"), so no runtime result either way. Needs a run on a real Windows machine. |

Sizes: macOS app 8.7 MB executable plus the 454 MB pack; archives are about 415 MB each
(DEFLATE/gzip saves about 9%).

## Publish

```sh
python3 tools/publish_desktop_release.py --dry-run   # all checks, prints the gh command
python3 tools/publish_desktop_release.py             # type the tag to confirm
```

It refuses to run when `GITHUB_ACTIONS` or `CI` is set, with a dirty tree, or when `HEAD` is
not an ancestor of `origin/main`; requires archives for all four platforms built from exactly
that commit; re-verifies checksums, the allowlist and the packed content; and refuses an
existing tag. The release is `desktop-v<version>-<short-commit>` (override with `--tag`),
marked prerelease (`--stable` changes that), with the four archives and `SHA256SUMS.txt`.
The tests (`tools/test_desktop_release.py`, part of `make check`) cover the pack builder,
allowlist, publish guards and the watcher with synthetic data only.

The first publication is the owner's decision.

## Automatic builds on a `desktop-v*` tag (optional, not installed)

`tools/desktop_release_watcher.py` makes one pass: it lists `desktop-v*` tags on origin
(`git ls-remote`), and for each tag it has not handled it adds a temporary worktree at the
tagged commit, links `private`, runs `build_desktop.py`, then `publish_desktop_release.py
--tag <tag> --yes` (still limited to commits on `origin/main`) and records the result in
`~/LargeFiles/fury-desktop/watcher-state.json`; a failed tag is not retried until its entry is
removed. A macOS notification reports each outcome. To run it every 15 minutes:

```sh
sed -e "s|@REPO@|$HOME/Projects/atv-offroad-fury-2-web|g" -e "s|@HOME@|$HOME|g" \
  tools/desktop/com.furyremastered.desktop-release.plist.in \
  > ~/Library/LaunchAgents/com.furyremastered.desktop-release.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.furyremastered.desktop-release.plist
# stop: launchctl bootout gui/$(id -u)/com.furyremastered.desktop-release
```

Try `python3 tools/desktop_release_watcher.py --dry-run` first. The job needs the SSD, Docker
(Colima) and the private content package to be available when it fires.

## Signing and first-launch warnings

- **macOS:** ad hoc signed (`codesign -s -`), not notarized. Gatekeeper blocks a downloaded
  copy: Control-click, Open; or `xattr -dr com.apple.quarantine "Fury Remastered.app"`. The
  first launch of a fresh download is also slower while macOS assesses it. A Developer ID
  certificate plus `notarytool` would remove the warning; the bundle layout already supports it.
- **Windows:** unsigned, so SmartScreen shows "Windows protected your PC": More info, Run
  anyway. Unsigned executables with embedded data can also draw antivirus false positives.
  The exe is a console-subsystem program, so a console window appears (useful while the app
  prints its diagnostics); it has no embedded icon yet.
- **Linux:** needs a Vulkan driver, an X11 or Wayland session, `libudev.so.1` (present on desktop
  distributions; gilrs links it dynamically) and glibc 2.31 or newer.

## Known gaps

- Archives are about 415 MB each (about 1.7 GB per release). GitHub allows 2 GiB per asset.
- The desktop app inherits the engine's gaps: no audio, HUD, menus, sky or scenery yet.
- Windows has not been run at all and Linux was only run on software Vulkan under Docker;
  real GPU backends (DX12/Vulkan on PC, Metal on Intel Macs) are untested.
- No Windows icon resource, no Developer ID or Authenticode signing, no notarization.
