# Performance and gameplay diagnostics

F3 cycles **Off → Performance → Debug → Off**. Pause includes the same Stats
control for touch and controller users. The selected mode persists on the device;
unavailable browser storage leaves the current session control usable. The overlay
does not consume pointer input and is hidden while pause-menu choices are open.

Performance shows rendered FPS, mean and 95th-percentile frame intervals over the
last 120 rendered samples, measured main-thread draw submission time, Rust/WASM
batch time, worker round-trip latency, submitted draw/triangle counts, drawing
resolution, and allocated WASM memory. Draw CPU time does not measure GPU execution.
Worker round-trip includes message scheduling and serialization; it is distinct
from the worker's measured WASM call. WASM allocation is not total browser memory.
Pause/unfocus intervals do not enter frame timing. Browser/headless measurements
are not a claim about physical phone performance.

Debug additionally shows the same simulation snapshot exposed to browser tests:
position and velocity, four wheel contacts and suspension compression, suspension
stop/body/prop contact counts, wreck/recovery state, resolved controls, rider mode,
vegetation counts, terrain updates and WebGL errors. These are diagnostic values,
not controls that change physics. No raw game files are exposed by this overlay.

`tools/test_diagnostics.mjs` checks hotkey repeat handling, the pause control,
persistence, actual WASM/render telemetry, desktop/portrait bounds and pointer
transparency. `ATV_DIAGNOSTICS=debug` enables it during the opt-in clean landing
test; `ATV_LANDING_SCREENSHOT` stores an inspected checkpoint in ignored storage.

The authored `terrain_clearance` tool checks the rendered tire geometry from that
trace against the original terrain. It needs local, private inputs:

```sh
cargo run -p atv-tools --bin terrain_clearance -- \
  <private-quad.psg> <private-terrain.tr2> \
  work/verification/clean-landing-trace.json <maximum-overlap-feet>
```

It reports every wheel's minimum clearance and fails if the requested overlap
limit is exceeded. The current 0.75-foot regression gate rejects the recorded
1.94-foot burial while permitting the measured 0.48-foot residual. This is a
regression threshold, **not** original-game parity or acceptable final clearance.
Only recorded riding frames are measured; inter-frame extrema and wreck bodies
require their own checks. Private assets, traces and screenshots remain outside
Git and CI.


Public release `0f074b4558f864ea4e4a8103b017b60e102c6690` (CI `38202065109`) passed the diagnostics suite for F3 repeat handling, Pause selection, persistence, actual telemetry, desktop/portrait bounds and pointer transparency. The public clean preload landing check also passed with Debug enabled. The inspected public portrait screenshot remains in ignored storage; its headless FPS is not a physical-device result. HTTPS verification matched 26 authored files and 308 served runtime/gallery objects before the new verification entry.
