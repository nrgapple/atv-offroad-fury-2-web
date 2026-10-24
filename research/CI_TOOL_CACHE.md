# Pinned Worker build tool cache

The PWA release's PR CI spent 259 seconds installing `worker-build 0.8.7`
([38438794714](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38438794714));
main spent 260 seconds on the same compilation
([38439883981](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38439883981)).
Main's implementation checks took 113 seconds, application build 82 seconds,
Cloudflare deployment 19 seconds and public CI verification 25 seconds. This
checkpoint addresses the repeated tool installation; those other gates remain.

The web job derives a key from the pinned tool version, runner OS/architecture,
the complete resolved `rustc -vV` output and Cargo version. The schema prefix
allows deliberate invalidation when this cache policy changes. Project commits
and game data are not key inputs because this executable is independent of them.

Only two files are restored/saved: the dedicated temporary tool directory's
`bin/worker-build` and `identity.json`. No Cargo home/registry, credentials,
Wrangler configuration, project target/build outputs or private game assets are
cached. The installer compiles with `--version 0.8.7 --locked --force` into a
separate temporary Cargo target. The validated executable is then installed at
the path the existing `make relay` command already uses.

`tools/worker_build_cache.py` accepts a cache only on an exact action hit. Before
executing it, it checks the compatibility record and binary checksum; it then
checks the executable's own `--version` output. Missing, partial, corrupt,
incompatible, non-executable or wrong-version file entries trigger a locked cold
installation. Symlinked cache directories fail closed; a symlinked executable is
removed without touching its target. A failed install cannot leave a valid record.
Cache service errors are optional; tool validation/installation, tests, builds,
artifact validation, deployment and public hash verification are required.

Restore and save use separate, commit-pinned `actions/cache` v6 actions. PRs only
restore; the save action runs after a successful main web build/artifact upload,
only for main `push` or `workflow_dispatch`. No broader restore keys or cross-OS
archive reuse are enabled. GitHub prevents main from restoring PR merge-ref
caches; this adds a main-only producer policy as well. A checksum record is an
integrity/compatibility check, not a cryptographic signature: the producer scope
is the trust boundary. An invalid immutable cache entry falls back to cold builds
until that entry is removed or the schema/key is changed.

Primary sources reviewed on 2026-10-13:

- [GitHub cache scopes and restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub restore/save actions and inputs](https://github.com/actions/cache)
- [Pinned worker-build source](https://github.com/cloudflare/workers-rs/blob/454fbd3c6ba3109be6158b19a04102abfcabfef1/worker-build/src/main.rs), which implements `--version` as an immediate version response

Host validation covers eight failure/reuse/policy tests, the existing 34 release
tests (one skip), syntax, setup and source hygiene. A seeded actual macOS/ARM64
0.8.7 executable passes the helper's warm validation in 0.206 seconds without
installation. This is not a GitHub cache timing measurement.

Remote validation requires a cold PR build, a successful main build that populates
the trusted cache, and a rerun of the PR that restores that main cache. The rerun
skips deployment through the existing PR condition, while retaining tests and
release packaging. Preserve exact run IDs, action hit/miss, helper source/duration,
tool identity and step wall times in the private verification receipt. Do not
claim a total-CI speedup until those runs establish it. The separate R2 package
and single-publisher current-main flow remain unchanged.

## Playwright browsers and system packages

**Evidence (2026-10-22/23).** The PR-only `Install pinned Playwright browsers` step
ran `playwright install --with-deps --only-shell chromium webkit` under an 8-minute
limit. It normally took 26-66 s, but five PR builds hit the limit and needed
`gh run rerun --failed`: #290
([39774525917](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/39774525917)),
#292 ([39784534795](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/39784534795)),
#297 ([39838414396](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/39838414396)),
#300 ([39895420286](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/39895420286))
and #304 ([39905405658](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/39905405658)),
all attempt 1. Every failed log stops inside the `--with-deps` apt download: 181 new
and 8 upgraded packages, "Need to get 119 MB", fetched from
`azure.archive.ubuntu.com` at tens of KB/s (#304 reached package 29 of 189 in eight
minutes). The npm install and `apt-get update` had finished within seconds, and no
failed run reached the browser download. On the successful rerun of #304 the same
119 MB took 17 s (6.9 MB/s), dpkg about 12 s, and both browsers 8 s from
`cdn.playwright.dev`. The hang is a slow Ubuntu mirror, not Playwright's CDN.

**Change.** `tools/playwright_ci.py` replaces the one-line install:

- Every network phase is bounded by `timeout` and retried once with a longer limit:
  npm 60/90 s, `apt-get update` 45/60 s, package download 120/180 s, browser
  download 90/150 s. Before the apt retry the runner's mirror list
  (`/etc/apt/apt-mirrors.txt`) drops `azure.archive.ubuntu.com`, so the retry uses
  `archive.ubuntu.com`; partial downloads resume. The step limit is 12 minutes.
- Packages are downloaded with `--download-only` into a dedicated archive directory,
  then installed with `--no-download`. A timeout can only interrupt a download, never
  dpkg.
- The package list comes from the pinned `playwright install-deps --dry-run chromium
  webkit` (the same dependency groups `--with-deps --only-shell` used).

**Caches.** Two exact keys, both with the `v1` schema prefix:

| Cache | Paths | Key |
|---|---|---|
| Package and browsers | `$RUNNER_TEMP/fury-playwright/package` (npm `playwright@1.58.2`), `~/.cache/ms-playwright` | `fury-playwright-v1-<RUNNER_OS>-<RUNNER_ARCH>-<ImageOS>-1.58.2-chromium-headless-shell-webkit` |
| Ubuntu packages | `$RUNNER_TEMP/fury-playwright/apt/*.deb` | `fury-playwright-apt-v1-<RUNNER_OS>-<RUNNER_ARCH>-<ImageOS>-<ImageVersion>-1.58.2-chromium-headless-shell-webkit` |

The browser builds are per distribution (`webkit-ubuntu-24.04`), so the first key
uses `ImageOS`. The packages are what a given runner image still lacks, so the
second also uses `ImageVersion`. No restore-key prefixes, no credentials, no
project files. The trust policy is the same as the Worker tool's: pinned
`actions/cache` v6, PRs restore only, and only main `push` or `workflow_dispatch`
saves. On a main push the restore steps use `lookup-only`. When both entries exist,
main does no Playwright work (it runs no smoke tests). When one is missing, main
installs once under `continue-on-error` (a failure there cannot block deployment)
and saves only what validated.

**Validation on restore; any mismatch falls back to a clean install.**

- Packages: `apt-get --print-uris` (against an empty directory) lists the archives
  this image needs, and `apt-cache show name=version` gives each file's size and
  SHA-256 from the runner's signed indexes. (`--print-uris` itself reports only MD5,
  or no hash for the security pocket.) Restored archives that do not match, are not
  listed, or are symlinks are deleted, and apt downloads the rest. After the download
  every expected file is hashed again before the offline install.
- npm package: `playwright` and `playwright-core` must report 1.58.2 and the CLI must
  exist, otherwise the directory is reinstalled.
- Browsers: `playwright install --only-shell chromium webkit` runs anyway (a no-op
  when complete, a download when anything is missing). Then both browsers must launch
  headless and render a page. If either fails, the browser directory is deleted,
  reinstalled clean and launched again.

The step summary records the source of each part (cache or cold), the counts of
cached, downloaded and discarded archives, the installed browser directories, and
each phase's attempts and seconds.

**Host validation (2026-10-24).** 7 unit tests cover keys, dry-run parsing, archive
listing with epochs and missing hashes, hash pruning (wrong hash with the right size,
unlisted file, symlink), the mirror rewrite, bounded retry commands, package version
checks and the clean browser reinstall. An end-to-end run in `node:24-bookworm`
(Debian 12 arm64, not the CI image) installed cold (288 archives, both browsers
launched). A fresh container then restored those files with one archive corrupted and
one unlisted file added: it kept 287, discarded 2, downloaded 1, and reused the
package and browsers, which launched. These are not GitHub timings.

**Remote validation.** The PR runs the cold path (no entries exist yet). The first
main build after merge populates both caches. The next PR measures the restore path.
Until those runs are recorded, no CI speedup is claimed. On a healthy mirror the
saving is bounded by about 17 s of package download and 8 s of browser download per
PR; dpkg still installs the packages (about 12 s). The main benefit is that a slow
mirror no longer reaches the timeout: a hit avoids the mirror entirely, and a miss
retries on another mirror. Actions cache storage was already 11.4 GB, mostly
1.3 GB per-commit Rust entries, so GitHub's least-recently-used eviction applies. PR
restores keep these entries recent.

## Local compiler cache shared across worktrees (sccache)

Each agent worktree builds Rust from scratch: the Makefile exports a per-worktree
`CARGO_TARGET_DIR`, which overrides the shared `target-dir` in `~/.cargo/config.toml`
because concurrent builds of different checkouts in one shared target directory
produced false compile errors. That global setting is unchanged.

**Setup on the owner's Mac (2026-10-24).** `brew install sccache` (0.19.0). Its user
config, `~/Library/Application Support/Mozilla.sccache/config`, puts the disk cache at
`/Volumes/External SSD/Developer/Caches/sccache` with a 20 GiB cap. The Makefile sets
`RUSTC_WRAPPER=tools/sccache-rustc` only when `sccache` is on `PATH`, `CI` is unset,
`FURY_SCCACHE` is not `0` and `RUSTC_WRAPPER` is not already set. CI is unaffected;
no `.cargo/config.toml` is committed, so other clones and platforms are unaffected.
Recipes and the Python build scripts they run inherit it: `make check` (test and
clippy), `make web` (wasm), `make desktop` (macOS host builds; the Docker Linux and
Windows builds do not pass it in) and `make relay`. Direct `cargo` commands need
`RUSTC_WRAPPER="$PWD/tools/sccache-rustc"`.

`tools/sccache-rustc` exists for two reasons found while measuring:

- sccache hashes every `CARGO_*` variable in the compiler's environment, so the
  exported per-worktree `CARGO_TARGET_DIR` made every crate miss in another worktree
  (24 of 221 native hits). rustc never reads it; the wrapper unsets it. Without it,
  hits rose to 178-202 of 197-221 cacheable crates.
- macOS drops `DYLD_*` variables when it starts `/bin/sh`, including the library path
  the rustup proxy sets so `rust-lld` finds `libLLVM.dylib`. Without restoring it, wasm
  linking failed with `Library not loaded: @rpath/libLLVM.dylib`. The wrapper sets
  `DYLD_FALLBACK_LIBRARY_PATH` from the toolchain path (Darwin only).

**What cannot be shared.** No profile settings were changed.

- Workspace crates in dev/test builds are incremental, and sccache does not cache
  incremental compilations (50 non-cacheable calls per native build, with build-script
  binaries and proc-macros). Making them cacheable (`CARGO_INCREMENTAL=0`) would slow
  repeated builds within a worktree. In release (wasm) builds they are cacheable but
  their working directory is the worktree, so they still miss in another worktree.
- Crates whose build script output depends on the target directory miss in every new
  target directory: proc-macro2, serde/serde_core, thiserror, zerocopy, darling, naga,
  wgpu, wgpu-core and wgpu-hal (19 native misses with the same source path and a new
  target directory). Their dependants miss with them. The wgpu/naga stack is among
  the slowest dependencies, which limits the saving.

**Measured (host only: M-series Mac, 10 cores, `origin/main` cbac0e4, 2026-10-24).**
Fresh detached worktrees, empty target directories, registry already downloaded.
Phases run in order in one worktree, as `make check` and `make web` would: `cargo build
--locked -p atv-app`, `tools/build_engine.py` (wasm release), `cargo test --workspace
--no-run`, `cargo clippy --workspace --all-targets`. Other agents were building at the
same time (load average 20-110 and higher), so only the interleaved round compares like
with like. Each phase ran without sccache in one worktree, then with a warm cache in
another fresh worktree, back to back:

| Phase | No cache (s) | Warm sccache (s) | sccache hits / misses / not cacheable |
|---|---:|---:|---|
| Native `cargo build -p atv-app` (dev) | 123 | 96 | 178 / 19 / 50 |
| Wasm engine (`build_engine.py`, release) | 162 | 113 | 80 / 22 / 37 |
| `cargo test --workspace --no-run` | 129 | 97 | 69 / 34 / 49 |
| `cargo clippy --workspace --all-targets` | 72 | 62 | 210 / 37 / 49 |
| Total | 486 | 368 | |

That is about a quarter of a fresh worktree's first build (118 s of 486 s in this round).
Populating an empty cache costs extra (a cold sccache round under heavier load took
205 s for the native build against 167 s without sccache earlier in that round). A
no-cache native build took 30 s on a lightly loaded machine, so these absolute times
mostly measure contention. The cache held 1.0 GB after these runs. Incremental
rebuilds inside one worktree are unchanged. Repeat on an idle machine before quoting
these as typical figures.
