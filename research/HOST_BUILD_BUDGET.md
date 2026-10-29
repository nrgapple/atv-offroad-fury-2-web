# Host build budget

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
