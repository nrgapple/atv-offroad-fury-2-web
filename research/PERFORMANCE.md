# Browser rendering performance investigation

This investigation records the initial rendering and update-loop costs in the
ATV Offroad Fury 2 browser port and the status of the current optimization
candidate. The candidate adds conservative terrain and scenery frustum culling,
reuses terrain buffers while updating affected geometry, reduces HUD backing
storage when the full canvas is unnecessary, and sleeps the single-player render
loop while inactive. Host Chromium component comparisons show pixel-identical
output for the checked views and HUD fixtures. Final integrated timing and broad
regression evidence are still pending. The 390×844/DPR3 viewport is a host
simulation, not a physical phone, and establishes no battery or thermal result.

## Baseline and measurement scope

The source baseline is commit `04759d7`. The cache-version-stripped public game
script matches the source baseline used for this investigation. The current
optimization candidate is based on that source; main commit `b5fd793` contains
security changes awaiting integration and does not change the renderer. Initial
captures used host Chromium at 390×844 CSS pixels and device pixel ratio 3. This
is a viewport simulation on the host Mac, not a physical phone. Counters describe
submitted work and main-thread callback time; they do not measure GPU execution
time.

| Scene | Non-instanced draws per frame | Vegetation draws per frame | Terrain triangles | Static instances |
| --- | ---: | ---: | ---: | ---: |
| ENV01 | 588 | 10 | 205,952 | 275 |
| ENV08 | 758 | 8 | 207,872 | 314 |

At the baseline, static terrain and scenery had no CPU-side view culling. The
candidate now applies conservative frustum bounds to terrain and scenery without
instancing or reordering objects. The counts above remain an initial workload
inventory, not a target draw count or evidence that every submitted object is
visible or expensive on every GPU.

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
fixed-tile LOD redesign. Paired geometry comparisons cover 18 terrain centers,
with matching GPU-byte hashes. The paired-render timing report is a pre-final
run, so it does not establish the final callback-time or frame-time benefit.
Record final integrated measurements after the remaining comparison fixtures
pass.

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
backing-store reductions above. The terrain timing report remains pre-final;
forced-crash fixtures are still under comparison. Broad map, frontend,
minimap-instrumentation, renderer, and touch-batch component checks have passed
so far, but the full gate and public release have not been established.

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
| Current | Conservative terrain and scenery frustum culling | Component comparisons pass for 45 camera/map views across nine maps; full integration gate pending |
| Current | Reuse terrain GPU buffers and regenerate affected geometry | 18 terrain-center GPU-byte hash comparisons pass; timing report is pre-final and forced-crash fixtures remain under comparison |
| Current | Reduce HUD backing storage/redraws and sleep inactive single-player RAF | Final HUD component raster comparisons pass across 48 fixtures/four viewports; full lifecycle/integration gate pending |
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
| Culling component comparison | 45 camera/map views across all nine maps | Checked pixel outputs match; conservative frustum bounds, no reordering; integrated timing pending |
| Terrain reuse component comparison | 18 terrain centers; GPU-byte hashes | Hashes match; paired timing report is pre-final and is not the final gain claim |
| HUD component comparison | 48 fixtures across four viewports | Pixel-identical; portrait standalone-speed storage 11,849,760→2,096,640 bytes (82.3%); no-content/paused storage 4 bytes |
| Integration and release | Forced-crash fixtures and full gate | Forced-crash comparison and full gate pending; no public release claim |

Update this record and the screenshot dev log when final checked metrics and an
inspected screenshot are available. Screenshot files belong in ignored
`private/reports/devlog-images/`; never add them to GitHub source. A public
release claim requires the separately verified current-main artifact and public
deployment gate.
