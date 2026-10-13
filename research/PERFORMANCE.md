# Browser rendering performance investigation

This investigation records the initial rendering and update-loop costs in the
ATV Offroad Fury 2 browser port and the status of the current optimization
candidate. The candidate adds conservative terrain and scenery frustum culling,
reuses terrain buffers while updating affected geometry, reduces HUD backing
storage when the full canvas is unnecessary, and sleeps the single-player render
loop while inactive. Host Chromium component comparisons show pixel-identical
output for the checked views and HUD fixtures. A matched `gl.finish` render-wall
benchmark records lower median time in every checked map. Those dedicated
measurements do not establish a main-thread CPU, normal frame-rate, or battery
gain; see the separate live-callback comparison below. The 390×844/DPR3 viewport
is a host simulation, not a physical phone. The candidate has passed its first
PR CI run; final CI and public-release verification remain pending.

## Baseline and measurement scope

The source baseline is commit `04759d7`. The cache-version-stripped public game
script matches the source baseline used for this investigation. The optimization
candidate preserves the recovered gameplay WASM hash `5248c34f…` and
incorporates remote-main security checkpoint `b5fd793` via merge commit
`99fd9aa`. Initial captures used host Chromium at 390×844 CSS pixels and device
pixel ratio 3. This is a viewport simulation on the host Mac, not a physical
phone. Counters describe submitted work and main-thread callback time; they do
not measure GPU execution time.

| Scene | Non-instanced draws per frame | Vegetation draws per frame | Terrain triangles | Static instances |
| --- | ---: | ---: | ---: | ---: |
| ENV01 | 588 | 10 | 205,952 | 275 |
| ENV08 | 758 | 8 | 207,872 | 314 |

At the baseline, static terrain and scenery had no CPU-side view culling. The
candidate now applies shared conservative frustum bounds to terrain and scenery
without instancing or reordering objects. The same generic bounds and buffer-pool
path serves every map; it is not tuned to Caribou or one benchmark route. The
counts above remain an initial workload inventory, not a target draw count or
evidence that every submitted object is visible or expensive on every GPU.

## Initial findings

### Terrain rebuild during long rides

On Caribou Peak, a run covering 48 samples and 216 feet along an axis recorded a
terrain rebuild. The render callback took 18.3 ms on the main thread for that
sample. The rebuild deleted 64 buffers, issued 64 `bufferData` uploads, and
uploaded 3,797,304 bytes. The baseline path deleted all terrain chunks and
regenerated the complete far grid and fine grid. This is a periodic rebuild
cost, not a steady per-frame cost. The candidate reuses existing terrain buffers
and regenerates only affected coarse masks and fine vertices. It retains the
original terrain samples, topology, and seam interpolation; it is not a
fixed-tile LOD redesign. Paired geometry comparisons across 18 terrain centers
in ENV01, ENV04, and FREE05 match GPU-byte hashes. At Caribou interior centers
(700, 690) and
(760, 720), terrain upload bytes fell from 3,868,416 to 1,176,192 (69.6%),
with no buffer deletions at those two centers. A separate ENV04 geometry case
deleted six buffers, so zero deletions is not a general result for every
update. The corresponding single-sample update times are reported in the
separate timing section below.

### HUD work and inactive rendering

At the 390×844 CSS viewport with DPR 3, the baseline HUD backing canvas is
1170×2532 pixels. It was cleared even when the GPS owned the speed display and
there was no stunt content to draw. In a simulated visible-but-blurred interval
lasting two seconds, the baseline produced 25 full-scene frames on each map and
zero physics steps.

The final HUD component comparison covers 48 fixtures across four viewports and
matches the baseline raster exactly. Cropping retains the original raster origin;
a translated crop changed antialiasing and was discarded. In the 390×844/DPR3
portrait standalone-speed case, backing storage falls from 11,849,760 bytes to
2,096,640 bytes (82.3%). With no HUD content or while paused, storage falls to
four bytes. Animated stunts retain the full original canvas. Identical HUD states
skip redraws. These are backing-store and pixel-comparison results, not measured
battery or physical-device savings.

### Display cadence and physics cadence

During active play, `requestAnimationFrame` renders at the display callback
cadence while physics advances at 60 Hz. The candidate sleeps the main game RAF
when single-player is inactive; lifecycle and touch-focus events wake it. The
multiplayer simulation clocks remain live when unfocused or when the menu is
open. Any later rendering-rate or power policy must preserve 60 Hz physics and
the established input and event ordering.

The initial same-host lifecycle comparison used two-second active, paused, and
synthetic blur-visible windows. Active play remained at 120 frames and 120
physics steps in both runs (about 60 FPS). Across the active window, submitted
draw work fell from 71,760 to 14,520 and HUD clears from 120 to zero. While
paused, RAF callbacks fell from 120 to zero; during blur-visible play they also
fell from 120 to zero while physics remained stopped. The draw, clear, frame,
physics, and RAF counts are recorded for the tested states; they do not establish
battery use or physical-phone savings.

### State serialization and allocations

Driving captures serialized 5,098,129 bytes across 267 steps on ENV01 and
5,466,238 bytes across 258 steps on ENV08. The current path builds
`serde_json` trees, repeatedly flattens matrices into allocated arrays, retains
full intermediate poses, and converts through JSON in the funny-crash path.
These byte totals identify serialization volume, not an equivalent amount of
network traffic or a measured allocation count.

### Vegetation processing

Vegetation already uses a 64×64 spatial grid, a max heap, and instanced draws.
However, the current frame path rescans 3,482–3,807 plants, sorts up to 1,500
placements, and reuploads data each frame, including while the camera is frozen.
The likely next investigation is whether spatial-cell visibility and a cached
camera result can avoid repeated work while preserving the current selected
instances and ordering where visible output depends on it. This is a code-path
finding; a measured CPU cost and image comparison are still needed.

### Other code-review candidates

- Multiplayer name labels write styles and text, then read `offsetWidth` and
  `offsetHeight` for each peer. Those reads can force layout after the writes.
- Wet particles can reach a high-water mark of 10,800. Expired particles are
  hidden in the vertex shader, but if any particles remain alive the renderer
  still submits the full high-water range. The measured active-count and GPU
  impact remain unknown.

These two items are code-review observations and are lower priority until the
rendering and terrain changes have repeatable before-and-after measurements.

## Active optimization work

The candidate implements conservative CPU visibility culling, terrain
GPU-buffer reuse, HUD cleanup, and inactive single-player RAF sleep. The culling
comparison covers 45 camera/map views across all nine maps; checked pixel outputs
match, and ordering is unchanged. Terrain center GPU-byte hashes match across
the 18 checked centers. The final HUD fixtures match exactly and show the
backing-store reductions above. Forced-crash tests now wait 125 steps for the
original two-second cooldown; the earlier failure exposed a test timing
assumption, with no Rust physics or input change. Baseline and candidate pass the
real held-touch wreck route. The host regression gates also pass workspace
checks and hygiene/security/release/Cloudflare checks, all-map driving,
124-step touch, race AI, minimap, keyboard/gamepad, context restoration,
Chromium/WebKit lifecycle, reflections/water, and multiplayer join/contact/train/
reconnect/departure routes. Motorcycle controls also pass 178 paired WASM/camera
steps, portrait touch turns in both directions, grip and foot continuity, and
interpolated presentation. Final `make check` and `make verify` pass after the
last HUD visibility guard. PR #42's first CI run passed; its final CI result and
public deployment are pending.

## Benchmark method

Use the same host, Chromium build, GPU backend, viewport, DPR, course, vehicle,
camera state, browser focus, and workload for each before-and-after pair. Keep
the 390×844/DPR3 viewport results labeled as host emulation. A physical device
run, when available, is a separate result with the device, OS, browser, refresh
rate, orientation, and power/thermal conditions identified.

For each map, let the scene settle before sampling. Capture repeated fixed-length
driving windows with a reproducible route and separate stationary/frozen-camera
and blurred-visible windows. Record the run duration, sample count, and any
concurrent workload. Compare distributions and event counts rather than one
best frame. Keep steady-state frames separate from terrain rebuild events.

### Dedicated render-wall comparison (2026-10-13)

The paired render report used one host Mac, Chromium Metal, a 390×844 CSS
viewport at DPR3, matched spawn patch/state/recovered follow camera, and four
serial alternating pairs of 80 frames after 20 warm-up frames. Its timer includes
`gl.finish` and excludes physics. Median before/after render-wall time was:

| Environment | Before | Candidate | Change |
| --- | ---: | ---: | ---: |
| ENV01 | 1.771 ms | 0.877 ms | 50.4% lower |
| ENV02 | 2.101 ms | 0.868 ms | 58.7% lower |
| ENV03 | 1.859 ms | 0.824 ms | 55.7% lower |
| ENV04 | 2.285 ms | 0.571 ms | 75.0% lower |
| ENV05 | 1.908 ms | 0.584 ms | 69.4% lower |
| ENV06 | 2.266 ms | 0.647 ms | 71.4% lower |
| ENV07 | 0.932 ms | 0.814 ms | 12.7% lower |
| ENV08 | 2.259 ms | 0.838 ms | 62.9% lower |
| FREE05 | 1.231 ms | 0.729 ms | 40.8% lower |

All 45 matched map/camera pixel comparisons were exact, and all 18 terrain
center GPU-byte hashes matched. These values are dedicated render-wall
measurements with GPU completion forced by `gl.finish`; they are not main-thread
CPU time, normal gameplay frame time, or FPS gains. The host reports are
`work/verification/performance/paired-render.json`,
`work/verification/performance/paired-hud.json`, and
`work/verification/performance/paired-lifecycle.json`. The live callback
report is `work/verification/performance/paired-live-cpu.json`. The screenshot
`private/reports/devlog-images/performance-caribou-host.png` shows a real host
gameplay frame after keyboard driving, not a fixed-camera benchmark view.

### Live callback elapsed time

`work/verification/performance/paired-live-cpu.json` records the actual Caribou
active loop with no draw-call instrumentation, one browser at a time, four
alternating serial pairs, two seconds of warm-up and ten seconds of sampling per
run, on the same host Chromium Metal setup at 390×844/DPR3. The measured value is
callback elapsed wall time, including any graphics-driver wait; it does not
isolate CPU execution.

| Pair | Before mean | Candidate mean | Before p95 | Candidate p95 |
| --- | ---: | ---: | ---: | ---: |
| 1 | 1.898 ms | 1.911 ms | 2.5 ms | 2.9 ms |
| 2 | 1.632 ms | 1.359 ms | 2.4 ms | 2.8 ms |
| 3 | 1.623 ms | 1.898 ms | 2.1 ms | 2.9 ms |
| 4 | 1.655 ms | 1.909 ms | 2.6 ms | 2.9 ms |
| Aggregate | 1.702 ms / 2,401 samples | 1.769 ms / 2,401 samples | — | — |

The candidate's mean is lower in one pair and higher in three; its p95 is higher
in all four. Both versions ran at about 60 FPS. This comparison does not show a
main-thread CPU, FPS, or battery gain, and it does not establish an overall
gameplay regression. A five-second profiler sample with 100 μs sampling
recorded about 337 ms of total non-idle time before and 244 ms after; culling
accounted for about 3 ms in the candidate sample, while vegetation remained the
largest profiled workload. That single profile is diagnostic evidence, not a
separate CPU-benefit measurement.

The two Caribou interior geometry updates above took 10.4→4.7 ms and 11.2→7.6 ms
in their respective before/candidate captures. These individual update timings
are not distributions or representative whole-frame CPU results.

Repeat the live callback comparison with the authored tool and caller-provided
baseline, candidate, and Playwright paths:

```sh
PLAYWRIGHT_MODULE=/path/to/playwright ATV_PERF_BASELINE=https://baseline-host.example ATV_TEST_URL=https://candidate-host.example node tools/benchmark_live_render.mjs
```

The tool writes `work/verification/performance/paired-live-render.json` by
default. Set `ATV_PERF_OUTPUT` to choose another report directory. Replace the
example URLs with approved host-accessible baseline and candidate URLs; the
Playwright package path comes from the caller's environment.

Record at least:

- frame interval distribution, rendered frames per second, and render callback
  CPU time, including p50 and p95 when the available instrumentation supports
  them;
- non-instanced and instanced draw counts, submitted triangles and instances,
  and the number and size of terrain buffer deletions and uploads;
- terrain-rebuild count and callback duration, shown separately from
  steady-state frame timing;
- vegetation candidates scanned, placements sorted, instances uploaded, and
  selected visible placements;
- HUD backing dimensions, clears, redraws, and whether GPS owns speed while
  stunt content is absent;
- RAF callbacks, physics steps, and visibility/focus state, so render cadence
  is not confused with simulation cadence;
- JSON byte volume and, if instrumentation is added, allocation counts and time
  spent constructing, flattening, and converting state.

Use A/B runs against the same source and workload, then inspect representative
frames and driving behavior for visual or gameplay regressions. Do not infer a
causal FPS improvement from a small difference in host snapshots or report
submitted draw work as GPU time. Existing diagnostics define draw CPU time as
main-thread submission time, not GPU execution; host and headless-browser
results do not establish physical-mobile performance.

## Regression constraints

Optimization must retain the original game behavior as the baseline. In
particular:

- Preserve transparent-object ordering and reflection rendering when culling
  or reusing scene data.
- Preserve existing input sampling, event ordering, and 60 Hz physics while
  changing render cadence or visibility handling.
- Preserve single-player pause behavior and live multiplayer ride behavior.
- Preserve camera movement, map transitions, frozen-camera vegetation output,
  and remote-player labels and visibility.
- Keep WebGL context-loss restoration able to recreate released or cached GPU
  resources.
- Compare actual rendered output at matching scenes and camera positions; a
  lower draw count alone is not a correctness result.

## Follow-up backlog

| Priority | Investigation | Status |
| --- | --- | --- |
| Host verified | Conservative terrain and scenery frustum culling | 45/45 camera/map pixel comparisons exact; dedicated render-wall medians lower in all nine maps; PR #42 final CI/public verification pending |
| Host verified | Reuse terrain GPU buffers and regenerate affected geometry | 18/18 terrain-center GPU-byte hashes match; original samples/topology/seams preserved; update bytes reduced at checked Caribou centers |
| Host verified | Reduce HUD backing storage/redraws and sleep inactive single-player RAF | 48/48 HUD fixtures pixel-identical; 82.3% less portrait standalone-speed backing storage; active physics/FPS unchanged; paused/blurred RAF sleeps |
| Next | Cache vegetation visibility by spatial cell/camera state and avoid per-frame sort/upload when unchanged | Candidate from code-path review; no measured gain |
| Next | Reduce JSON tree creation, repeated matrix flattening, retained full poses, and funny-crash JSON conversion | Captured byte totals; stage-by-stage CPU/allocation profile pending |
| Later | Avoid multiplayer DOM forced-layout reads after per-peer writes | Code-review candidate; profile before changing layout behavior |
| Later | Submit only live wet-particle ranges instead of the high-water allocation | Code-review candidate; verify ordering and particle appearance |
| Later | Add an explicit optional adaptive render-rate/resolution/antialias power policy | Proposal only; preserve 60 Hz physics, input and events |

## Progress record

| Gate | Evidence | Result |
| --- | --- | --- |
| Initial workload inventory | Host Chromium, 390×844 CSS viewport, DPR3; ENV01 and ENV08 draw/triangle/instance counts | Recorded above; not a physical-device result |
| Long-ride terrain update | Caribou, 48 samples/216 feet along an axis; callback, delete/upload counts and uploaded bytes | 18.3 ms callback, 64 deletes, 64 uploads, 3,797,304 bytes |
| Culling and render-wall comparison | 45 camera/map views across all nine maps; four alternating 80-frame pairs after 20 warmups | Pixels exact; 1.771→0.877 ms ENV01 through 1.231→0.729 ms FREE05 median render-wall; `gl.finish` included, physics excluded |
| Terrain reuse component comparison | 18 terrain centers; GPU-byte hashes | All hashes match; at Caribou centers (700,690)/(760,720), bytes 3,868,416→1,176,192 (69.6%), zero deletes at those centers; separate ENV04 case deletes six buffers |
| HUD component comparison | 48 fixtures across four viewports | Pixel-identical; portrait standalone-speed storage 11,849,760→2,096,640 bytes (82.3%); no-content/paused storage 4 bytes |
| Lifecycle comparison | Three two-second actual-game windows | Active 120 frames/steps and 60 FPS both runs; draws 71,760→14,520, HUD clears 120→0; paused/blur RAF 120→0 |
| Live callback timing | Four actual-loop serial pairs, 2,401 samples each | See the scoped results above; callback wall includes driver wait, with no CPU/FPS/battery gain claim |
| Integration and release | Host regression routes and PR #42 | Host gates pass with no gameplay/FPS regression detected; first CI run passed, final CI/public release pending; no public optimization claim |

The inspected host screenshot is stored in ignored
`private/reports/devlog-images/`; screenshot files never enter GitHub source. A
public release claim requires the separately verified current-main artifact and
public deployment gate.
