# Host build budget

## Bounded overlap (2026-11-12)

Call the guarded make targets and browser runner directly. Do not wrap a whole
test/build/release pipeline in an outer `host_build.py -- sh -c ...` lease: its
exclusive ownership would remain held between steps and suppress parallelism.
Each normal entry point acquires and releases its own permit.

Agent count is independent of build concurrency. Local entry points now admit
one `compile` job and one `browser` suite together, in different worktrees.
Cargo build/check/clippy/test/rustc, the web build and relay build use the compile
permit. Ordinary content/content-free browser checks use the browser permit.
Relay, native, manual, and timing-sensitive checks (marked `exclusive` in the
browser manifest) stay exclusive, as do mixed selections containing one. Use
`--exclusive` on the browser runner for other performance measurements. Desktop/Android packaging,
`cargo run`, unknown commands and older guards remain exclusive. CI is unchanged.

Admission requires normal macOS memory pressure and, for the parallel kinds,
at least 25% headroom reported by `memory_pressure -Q`. This is an admission
check, not a hard memory quota or a guarantee about later compiler peaks. It
never kills existing jobs. Two compiler jobs or two browser suites still cannot
overlap. Performance measurements should use an exclusive lease to avoid
contention from the other workload.

A shared lock on the existing `heavy.lock` excludes old exclusive guards; a
separate lock per kind bounds concurrency. Waiting for the legacy/exclusive lock
uses a bounded kernel wait rather than repeatedly racing older polling guards. A canonical Git-worktree lock prevents
rebuilding files under a test, including commands launched from subdirectories
or through symlink paths. `make web` holds it while copying both the page shell
and compiled engine. Supported direct web/browser entry points normalize their
working directory to the checkout they write. Cross-kind nested calls fail with
an instruction to use an exclusive outer lease instead of waiting in a deadlock.
Same-kind nesting and children of an exclusive job reuse their parent's permit.

Compiler processes normally belong to the admitted process tree. For sccache,
whose daemon parents rustc, admission matches the exact compiler arguments to
an admitted client's request; unrelated compiler processes still defer entry.
Lock metadata is trusted only while the kernel lock is held. Resource and
checkout locks are passed to the direct child and released when holders exit.

After tests, update only the installed Cargo guard with:

```sh
python3 tools/install_host_build_budget.py --repo "$PWD" --guard-only --apply
```

This atomically replaces its runtime file without restarting the desktop watcher
or interrupting existing work. New worktrees must incorporate this change to use
the new make/browser entry points. Already-running and old-worktree jobs keep
the conservative exclusive behavior. Use `python3 tools/host_build.py -- <command>`
to request exclusive execution explicitly. Do not use `CI=1` to bypass host limits.

Verification of the bounded guard: 37 host guard tests, six browser admission
tests, workspace Clippy and PR #585 CI passed. A real Wasm check and public
browser boot suite held permits concurrently in separate worktrees, with normal
sampled memory pressure. The browser passed its loading/title assertions but
timed out in simulated content-error handling; its full suite did not pass.
The Wasm probe final exit result was not retained. This confirms concurrent
admission, not an end-to-end speedup or optimized-link peak memory budget.

## Original exclusive guard (2026-10-28)

Implementation strategy (2026-10-28): serialize new local heavy build/test commands
with one advisory host lease; cap Cargo jobs at two; wait for pre-existing compiler,
desktop-build and browser-test work to finish rather than interrupting it. Defer
new work when macOS reports warning/critical memory pressure. CI keeps its existing
parallelism. Use internal storage for small live locks/state/logs and SSD storage
for source, targets, caches and artifacts.

The opt-in host installation also guards the user's rustup Cargo entry point so
older worktrees benefit without editing another chat's checkout. Preserve rustup's
original proxy and resolve the active toolchain per invocation. Explicit toolchain
binaries and unrelated tools can bypass this cooperative limit; do not claim an
OS-wide CPU/RAM quota. Keep active builds and live services running.

Desktop automatic release attempts need a cooldown across failures and newer
commits, atomic durable state, and storage preflight before expensive work. Host
installation must wait for the old watcher to finish before moving state or
reloading its LaunchAgent. Keep publication in the existing authorized flow.

Verification: synthetic subprocess tests for serialization, nesting, cancellation,
legacy busy detection, pressure deferral and cooldown/persistence. No gameplay
changes; no local full-game rebuild required for this tooling checkpoint.

## Entry points and installation

`make check/web/relay/desktop/android/browser-*`, direct Python engine/desktop/Android
builders, and the browser runner use `tools/host_build.py`. Local Cargo receives two
jobs by default; independent commands share a single lock. The optional installer
replaces only a recognized rustup Cargo proxy with a small shim, preserving
`~/.cargo/bin/cargo.fury-original`. It resolves the active toolchain (including
`+toolchain`) at invocation time; metadata/help/version/format operations remain light.
No toolchain binary is modified. Explicit `-j`, `CARGO_BUILD_JOBS`, `CI`, direct toolchain
paths or unrelated tool entry points can override/bypass this cooperative policy.

Waiting commands report why and time out after 30 minutes (exit 75). They don't stop
existing processes. A watcher pass skips busy hosts immediately and tries again on its
normal schedule. Pressure detection is available on macOS; on other local Unix hosts
the lock and legacy-build detection still apply. Nested descendants reuse the owner's
lease after verifying ancestry; merely retaining an old environment variable is not
enough. The kernel releases the lock when holders exit. CI keeps its existing jobs.

The browser runner registers servers before readiness checks and tracks owned child
identities/descendants, including detached browser processes, for failure/timeout and
completion cleanup. It never uses broad process-name termination. A one-second sample
can miss a detached child that is born and orphaned entirely between samples; individual
test harnesses must still close their browsers in finally blocks.

The installer copies only small Python/runtime files to internal service storage.
It does not edit other task worktrees, change the default VM, stop simulators or restart
live services. The dedicated desktop VM keeps its tested sizing; exclusive admission
and memory-pressure checks reduce overlap but are not a hard memory limit on a running
linker. The active release present at installation is grandfathered and may finish
using its original resource settings.

## Reversal

After guarded work finishes, restore Cargo with
`mv ~/.cargo/bin/cargo.fury-original ~/.cargo/bin/cargo`. Preserve the installed guard
runtime until all current guard processes exit. The former watcher plist is saved as
`~/.local/share/project-service-data/fury-desktop-release/previous-launchagent.plist`;
only restore/reload it when the watcher is idle. Do not discard the migrated release
history or copy stale legacy state back over it. A rustup self-update may replace its
Cargo proxy; rerun the installer to reinstate the guard if that happens.

## Desktop throughput follow-up (2026-10-28)

Strategy: remove the unused native `atv-app` shared-library link by making its
default library type `rlib`. Web and Android explicitly request `cdylib` through
Cargo's supported `rustc --crate-type` option. Keep the release optimization level,
full LTO, game code, graphics settings and content unchanged; ThinLTO is not enabled
without a native performance comparison. This eliminates unnecessary work without
trading away runtime optimization. Capture Cargo build timings for future comparisons.

Build macOS before opening the dedicated Docker builder, then close only a VM this
invocation started. Future starts use two CPUs; retain 8 GiB because an earlier
4 GiB full-LTO link ran out of memory. Never resize/restart an already running VM.
Automatic full-platform releases should coalesce main changes for seven days and
perform at most one release per watcher pass. Explicit release tags remain an
intentional override. Apply the watcher changes through the existing idle-only handoff.

Validation: focused synthetic tests for phase ordering, VM ownership/environment
restoration, weekly cooldown and one-release dispatch; CI checks actual native linking,
WebAssembly output and Android shared-library output without game content. A live
full-release timing comparison is deferred until the current release and host pressure
clear. Do not claim a measured speedup or restart the host to obtain one.
