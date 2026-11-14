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
is a host simulation, not a physical phone. The optimization is now publicly
deployed and verified on both production origins at main commit
`124b67bdac878dd1e1a481703c29fe2a5ecf00f3`; public behavior checks are listed
below, including all documented multiplayer routes.

## Baseline and measurement scope

The source baseline is commit `04759d7`. The cache-version-stripped public game
script matches the source baseline used for this investigation. The recorded
baseline gameplay WASM hash is
`5248c34fe1727149720c83a64a0536184fed6fae92dbb2892bfe3f15ad304116`. A fresh
isolated host build reproduces the frozen host baseline byte-for-byte. This
confirms the paired host game used the unchanged authored WASM; the public CI
artifact is verified against its own checked artifact and is not claimed to have
the same compiler-produced binary hash. The optimization candidate incorporates
remote-main security checkpoint `b5fd793` via merge commit `99fd9aa`. Initial
captures used host Chromium at 390×844 CSS pixels and device
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
Chromium/WebKit lifecycle, reflections/water, and host multiplayer
join/contact/train/reconnect/departure routes. Motorcycle controls also pass 178 paired WASM/camera
steps, portrait touch turns in both directions, grip and foot continuity, and
interpolated presentation. Final `make check` and `make verify` pass after the
last HUD visibility guard. PR #42 CI run `38475141913` passed; merge commit
`124b67bdac878dd1e1a481703c29fe2a5ecf00f3` passed main CI and automatic
deployment in run `38475701679`. Public verification is recorded below and in
`research/DEPLOYMENT.md`. Public multiplayer feature, transport-v3, four-browser
and abrupt-owner-close grace checks have passed; multiplayer transport figures
are not rendering-performance gains.

### Isolated build verification

The shared native Cargo target produced an unexpected `CopyRideLink` bounds-test
failure and a 357-versus-356 core-test count mismatch while another worktree was
building. Rust source was unchanged. Re-running with the dedicated ignored SSD
target `CARGO_TARGET_DIR=work/verification/native-target` passed `make check`
(356 core tests plus the remaining Rust, 98 JavaScript, Clippy and Python checks)
and `make verify`. A fresh isolated WASM build completed in 11.52 seconds and
matched the frozen host baseline exactly: SHA-256
`5248c34fe1727149720c83a64a0536184fed6fae92dbb2892bfe3f15ad304116`, with a
byte-for-byte comparison passing. This host build comparison does not claim the
public CI artifact has the same compiler-produced binary hash.

### Source pointers for follow-up profiling

The following are investigation targets, not established optimization wins.
Line numbers can shift; use the named symbols when locating them again.

- `crates/atv-core/src/vegetation_render.rs` (`Vegetation::select`: vegetation
  scan, selection/sort and instance writes, run in the main-thread WASM
  instance) and `web/game.js` `drawEcosystem` (instance uploads and draws);
  `command`, `pumpSimulation` and `acceptFrame` (browser/WASM request,
  fixed-rate simulation batching and frame/matrix conversion).
- `web/worker.js`: Worker request handling, WASM `run`, output copying and
  transferable response.
- `crates/atv-web/src/lib.rs`: simulation frame construction and JSON
  serialization, including HUD/particle fields and funny-crash JSON conversion;
  `serialized_operation` and `SimulationBatchFrame` are related batching paths.
- `crates/atv-core/src/hud.rs` and `web/hud.js` (`HeadUpDisplay`): Rust HUD frame
  construction and browser canvas allocation, bounds, redraw and release.
- `web/multiplayer.js` (`PlayerIndicators.draw`): peer-label DOM writes followed
  by `offsetWidth`/`offsetHeight` reads, a possible forced-layout source.
- `web/wheel-effects.js` (`WheelEffectsRenderer`) and
  `crates/atv-core/src/wheel_effects.rs`: wet-particle lifetime filtering,
  historical high-water draw range and frame production.

Vegetation caching, reducing state serialization/matrix allocations, batching
multiplayer label layout, and drawing only live wet particles remain backlog
hypotheses. Any changes need paired workloads and output checks while preserving
transparent ordering, reflections, input/event ordering, pause, multiplayer
clock behavior, and context restoration.

## Public release verification (2026-10-13)

Main commit `124b67bdac878dd1e1a481703c29fe2a5ecf00f3` passed CI and automatic
production deployment in [run 38475701679](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38475701679);
PR #42's final CI run [38475141913](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38475141913)
also passed. The validated current-main artifact was fetched before the separate
R2 upload. Both public `deployment.json` responses identify the same commit, and
full verification passed on [furyremastered.com](https://furyremastered.com/)
and the [Workers origin](https://atv-offroad-fury-2.champtoday.workers.dev/).
Verification matched 43 authored files and 453 served objects by hash, size and
MIME. The approved R2 package contains 454 objects totaling 343,700,282 bytes;
two objects changed (the inspected gameplay screenshot and gallery), with no
runtime-content changes. The gallery snapshot at verification contains 179
entries and 138 screenshots. The screenshot caption remains explicit that it is
a host Chromium Metal gameplay capture, not a public-browser or physical-phone
image.

Public Chromium checks passed no-disc startup, original title/controller/pause
flows, 390px touch driving and reload, and all nine map loading/driving routes
with terrain and vegetation. They reported zero JavaScript or WebGL errors. In a
two-second public active window the game rendered 120 frames at about 60 FPS,
advanced 119 physics steps, submitted 14,520 draws and cleared the HUD zero
times. Paused and synthetic blur-visible windows each recorded zero RAF callbacks,
draws, HUD clears and physics steps; focus, driving and WebGL context restoration
woke rendering successfully. A separate public held-touch Nac Nac route passed
wreck separation, ground contact, automatic recovery and resumed driving.

The public feature retry passed late join with 13 remote draws per client, host
recovery, nearby spawn at about 10 feet, the 342×64 mobile Players hit target,
owner contacts (10 and 10), 20 train samples with at most 0.908207 feet relative
error, both head-on Front wrecks with peer replication, and departure cleanup.
An initial run timed out because its fixture clicked Leave and immediately
closed the host page before the explicit-close handshake finished. The test now
waits for owner state to clear and its WebSocket to close, and waits for the guest
room code to clear before destroying the host page. This resolved the harness
ordering assumption against the existing 30-second recovery grace; no production
transport code changed. Follow-up public transport-v3 checks passed: driving
recorded 170 messages / 22,914 bytes over 10.01 seconds, using 135- and 117-byte
packets; both suspended riders sent two poses in roughly 11 seconds and stayed
fresh, with the tested quad/rider pose sets retained. Late join restored the
host at zero positional error, resume delivered 41 messages in 2.515 seconds,
and a 5.518-second solo interval sent no snapshots. This confirms existing
transport behavior; it is not a new rendering gain.

The public four-browser test also passed with quads 1/14/26/2. Every client drew
three peers in 39 draws with three engine voices; all four drivers' movement
reached every observer, and explicit owner leave cleared guest codes, remotes,
audio, impacts and models with zero page or HTTP errors. These multiplayer
telemetry samples are not used as rendering CPU/FPS evidence. The separate
abrupt-owner-close recovery-grace regression also passed: a copied-invite two-rig
session cleared the guest after abrupt host closure within the existing 45-second
test window, covering the 30-second recovery grace. It rendered 13 remote draws
and one engine voice per client, with 9.953 feet of remote movement. The
four-browser screenshot remains private and was not published.
These public routes verify deployment and the listed behaviors; they do not
establish physical-phone behavior, whole-game parity, main-thread CPU savings or
battery/thermal gains. Detailed release evidence is in
`research/DEPLOYMENT.md` and the corresponding text-only dev-log entry.

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
| Publicly verified | Conservative terrain and scenery frustum culling | 45/45 camera/map pixel comparisons exact; dedicated render-wall medians lower in all nine maps; main CI and both-origin public verification passed |
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
| Integration and release | PR #42 and main commit `124b67bd` | Final PR/main CI, both-origin application/R2 verification, all-map routes, lifecycle, wreck and multiplayer public checks passed; no CPU/FPS/battery gain is claimed |

The approved gameplay screenshot remains stored in ignored
`private/reports/devlog-images/` and was uploaded separately with the validated
R2 package; screenshot files never enter GitHub source. Its public gallery caption
identifies the actual capture as host Chromium Metal at 390×844/DPR3. No screenshot
from the separate four-browser run was approved or published.

## Phone-oriented main-thread pass — 2026-10-15

Method: host Chromium (Metal) emulating a phone, 390×844 at DPR3 with touch,
and CDP CPU throttling ×4. The CPU profiler sampled at 200μs for 8s of held-
throttle driving. These results identify main-thread hot spots; they are not
physical-phone, GPU or battery measurements.

| Hot spot (8s sample) | Before | After | Change |
| --- | ---: | ---: | --- |
| `gl.getParameter` (Supercross race, sx07) | 1,134ms | 0ms | WebGL state shadow |
| Forced layout `clientWidth` (race) | 136ms | 0ms | ResizeObserver canvas size |
| Main-thread idle (race, sx07) | 5,058ms | 6,108ms | ≈13% of the main thread freed |
| `drawEcosystem` (Freestyle, Caribou) | 608ms | 506ms | exact cell culling |
| Vegetation placements scanned per frame (Caribou / Bluewater / nat03) | 3,713 / 1,361 / 2,612 | 1,909 / 626 / 1,199 | same visible counts |

- **WebGL state shadow (`shadowGlState`).** The ambient-effect, stream,
  weather and wheel renderers saved and restored about ten bindings with
  `gl.getParameter`/`isEnabled` every frame. These synchronous queries stall the
  GPU pipeline, and the cost grows on phones. The shadow intercepts the
  matching setters and answers those exact enums. Every other query still goes
  to the context, and the shadow resets on context restore. Unit-tested;
  ambient, weather, wheel and wet-particle pixel gates still pass.
- **Observed sizes.** The game canvas and HUD surface sizes come from a
  `ResizeObserver` instead of per-frame `clientWidth`/`getBoundingClientRect`.
  Those reads forced layout after the race overlay updated its text.
- **Vegetation cell culling.** Visibility cells entirely behind the camera
  (projected depth ≤ −maxRadius), or beyond fog plus maxRadius, are skipped.
  Every placement in them failed the existing per-placement tests, so
  selection, near/far phase state and draw order are unchanged.
- **Adaptive render resolution (`RenderScale`).** On high-DPR devices, only the
  WebGL canvas pixel ratio steps between 1.5 and 1.0 in 0.25 increments.
  Below about 42 FPS for 1.5s steps down; 6s at about 57 FPS or better steps
  back up. A failed recovery doubles the wait (up to 60s). Isolated hitches
  never lower it. Physics (60Hz worker), input and the HUD are unaffected.
  Diagnostics report `renderRatio`. Its benefit on GPU-bound phones still
  needs physical-device confirmation.

Remaining phone candidates: per-frame race JSON is about 37KB, about 21KB of
which is CPU bone matrices (parse ≈3% of the throttled main thread). A race
worker step costs about 2ms (p95 3.2ms) on the host for five physical quads,
which a phone may multiply several times. Vegetation selection and sorting
remain the largest main-thread renderer cost.

## Graphics presets (2026-10-16)

`tools/benchmark_graphics_presets.mjs` rides straight ahead from the spawn with
the throttle held: 2 s warm-up, then an 8 s window, one fresh page per row, one
host Mac (Chromium Metal). **Phone** is 390×844 at DPR3 with touch and CDP CPU
throttling ×4 (canvas 585×1266 at RenderScale 1.5); **desktop** is 1280×720 at
DPR1 unthrottled. "Callback" is main-thread time inside requestAnimationFrame
callbacks (render plus HUD/minimap); "busy" is CDP `TaskDuration` per wall second;
draws count every WebGL draw call per frame. Baseline is the main build at `7a346ab`
(before presets). Headless Chromium is not vsync-locked here
(about 75 callbacks/s with a bimodal 11/26 ms interval in every row, baseline
included), so frame intervals are not a phone frame-rate measurement.

| Profile | Map | Build / preset | Callback mean / p95 (ms) | Busy | Draws mean (max) | RenderScale min |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Phone | Caribou (env01) | baseline | 0.87 / 2.4 | 0.27 | 100.0 (110) | 1.5 |
| Phone | Caribou | Original | 0.89 / 2.5 | 0.29 | 100.2 (110) | 1.5 |
| Phone | Caribou | Enhanced | 1.11 / 2.8 | 0.35 | 101.0 (111) | 1.5 |
| Phone | Caribou | High | 0.77 / 2.1 | 0.22 | 101.4 (112) | 1.5 |
| Phone | Supercross 1 (sx01) | baseline | 0.67 / 2.0 | 0.23 | 204.5 (263) | 1.5 |
| Phone | Supercross 1 | Original | 0.56 / 1.7 | 0.19 | 204.8 (263) | 1.5 |
| Phone | Supercross 1 | Enhanced | 0.71 / 2.0 | 0.22 | 206.0 (264) | 1.5 |
| Phone | Supercross 1 | High | 0.57 / 1.6 | 0.17 | 205.8 (263) | 1.5 |
| Phone | Water (env04) | baseline | 0.74 / 2.0 | 0.26 | 96.6 (99) | 1.5 |
| Phone | Water | Original | 0.87 / 2.3 | 0.32 | 96.7 (99) | 1.5 |
| Phone | Water | Enhanced | 0.70 / 1.9 | 0.23 | 97.6 (100) | 1.5 |
| Phone | Water | High | 0.64 / 1.7 | 0.21 | 97.6 (100) | 1.5 |
| Desktop | Caribou | baseline | 0.25 / 0.7 | 0.08 | 141.4 (187) | — |
| Desktop | Caribou | Original | 0.20 / 0.5 | 0.07 | 140.3 (187) | — |
| Desktop | Caribou | Enhanced | 0.21 / 0.5 | 0.07 | 141.3 (188) | — |
| Desktop | Caribou | High | 0.21 / 0.5 | 0.06 | 141.1 (188) | — |
| Desktop | Supercross 1 | baseline | 0.21 / 0.5 | 0.07 | 258.0 (468) | — |
| Desktop | Supercross 1 | Original | 0.16 / 0.4 | 0.05 | 256.7 (468) | — |
| Desktop | Supercross 1 | Enhanced | 0.19 / 0.5 | 0.06 | 259.7 (469) | — |
| Desktop | Supercross 1 | High | 0.18 / 0.5 | 0.06 | 256.8 (469) | — |
| Desktop | Water | baseline | 0.21 / 0.5 | 0.08 | 101.5 (105) | — |
| Desktop | Water | Original | 0.19 / 0.5 | 0.07 | 101.5 (105) | — |
| Desktop | Water | Enhanced | 0.25 / 0.6 | 0.09 | 102.4 (106) | — |
| Desktop | Water | High | 0.21 / 0.5 | 0.07 | 102.3 (106) | — |

Enhanced and High add exactly one draw (the contact shadows) and about 19 KB of
GPU buffers; the dirt overlays add one texture fetch only on vehicle pixels.
Run-to-run noise between single 8 s windows (±0.2 ms callback, ±0.08 busy) is
larger than any preset difference, and no row lowered RenderScale. Anisotropic
filtering is GPU sampling cost that these CPU-side numbers cannot show; the
emulated phone runs on the host GPU, so physical-phone GPU cost is unmeasured.
Reports: `work/verification/graphics-presets/{candidate,baseline}/benchmark.json`.

### Graphics presets, second batch (2026-10-16)

Same method and host as above, with tire tracks, lingering dust, wheel
re-projection, water Fresnel (all Enhanced/High) and the High-only vehicle
shadow map added. Draws are per frame; the shadow-map pass accounts for the
~13 extra High draws (player quad and rider chunks; more with nearby CPU or
remote riders).

| Profile | Map | Preset | Callback mean / p95 (ms) | Busy | Draws mean (max) | RenderScale min |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Phone | Caribou | Original | 0.79 / 2.1 | 0.24 | 100.2 (110) | 1.5 |
| Phone | Caribou | Enhanced | 0.65 / 1.8 | 0.19 | 103.4 (113) | 1.5 |
| Phone | Caribou | High | 0.68 / 1.9 | 0.20 | 116.2 (126) | 1.5 |
| Phone | Supercross 1 | Original | 0.51 / 1.5 | 0.17 | 204.9 (263) | 1.5 |
| Phone | Supercross 1 | Enhanced | 0.58 / 1.7 | 0.18 | 207.8 (265) | 1.5 |
| Phone | Supercross 1 | High | 0.63 / 1.8 | 0.19 | 220.6 (278) | 1.5 |
| Phone | Water | Original | 0.53 / 1.5 | 0.19 | 96.7 (99) | 1.5 |
| Phone | Water | Enhanced | 0.68 / 1.9 | 0.23 | 98.9 (102) | 1.5 |
| Phone | Water | High | 0.97 / 2.5 | 0.33 | 112.4 (114) | 1.5 |
| Desktop | Caribou | Original / Enhanced / High | 0.26 / 0.29 / 0.30 | 0.09 | 139.7 / 144.3 / 156.8 | — |
| Desktop | Supercross 1 | Original / Enhanced / High | 0.21 / 0.24 / 0.25 | 0.07–0.08 | 260.5 / 262.7 / 276.7 | — |
| Desktop | Water | Original / Enhanced / High | 0.24 / 0.25 / 0.27 | 0.09 | 102.3 / 104.5 / 117.2 | — |

Enhanced adds 2–3 draws (contact shadows, tire tracks, dust) and stays within
single-window noise of Original on the throttled phone emulation; no row lowered
RenderScale. GPU memory: tracks 151 KB (Enhanced) / 605 KB (High), dust 1–4 KB,
shadow map 16 MB (High only). The 2048² depth pass and four-tap comparisons are
GPU work these host measurements cannot price on a phone GPU, which is why Auto
keeps phones on Enhanced (blob shadow only). Report:
`work/verification/graphics-presets/pr2/benchmark.json`.

## Load path and decoder traffic (2026-10-17)

Method: host Chromium (Metal), `play.html?drive=1&environment=env01` deep link,
fresh in-memory profile (cold), a local server with the live 218-record content
manifest. "Phone network" throttles CDP to 25 Mbps / 70 ms RTT; "×4 CPU" is CDP
CPU throttling. The local server does not compress app files (production serves
brotli), so app JS/WASM bytes are pessimistic; content blobs are not compressed in
either. Baseline is main `bb7171d`. These are host simulations, not phone results.

What slowed startup on the baseline:

- **Serial downloads.** Each archive read was awaited before the next request.
  `terrainFromContent` awaited the whole audio pipeline (bank download, every
  sample decode, soundtrack and menu tunes) before requesting the course.
- **Title screen gated on the sound bank.** `Frontend.init` awaited the soundtrack
  catalog and menu tunes, i.e. all 18.7 MB of `AUDIO.RES`, before the title.
- **Loading screen previews.** Opening the loading screen fetched all six course
  preview archives (≈2.5 MB) and decoded every track photo.
- **Decoder copies.** Every archive parse/extract (ops 2/3) structured-cloned the
  whole archive to the worker and copied it into WASM: 2,412 MB over 2,427 calls
  to reach a ride, 630 ms of main-thread `postMessage` at ×4 CPU.
- **Whole sound bank per ride.** A freestyle ride plays a few hundred KB of the
  bank (one quad's engine set, impacts, venue sounds), but downloaded all of it:
  engines for all 26 quads (9.8 MB), race stingers (4 MB), rider vocals (3.3 MB).
- **Repeat visits** revalidated every `?v=<commit>` app file
  (`max-age=0, must-revalidate`).
- **Race frames** read `#toolbar.getBoundingClientRect()` after the race overlay
  rewrote its timers, forcing a layout every frame; CPU and remote riders
  allocated flattened bone arrays and re-uploaded identical bone/model uniforms for
  every chunk.

Changes: ride archives are prefetched together (the bank at low fetch priority);
menu music loads behind the title; the loading screen decodes only its own photo
(ride links skip the course-list photos); archives stay resident in Rust memory
under a key (48 MB LRU, reused archives only) so an extraction posts a key;
whole-chunk reads share the verified cached bytes; the sound bank is opened by
directory and a member is inflated from only the 24 KiB blocks it spans (wrapped
as a one-member container for the existing Rust extractor); the content package
splits `AUDIO.RES` into 256 KiB chunks (same compressed bytes); app files stamped
with the deployed commit are served `immutable`; the toolbar edge is measured on
resize; rider bones reuse buffers and upload once per quad/rider part.

| Cold `drive=1` ride ready | Baseline | This build, live package | + chunked sound bank |
| --- | ---: | ---: | ---: |
| Phone network | 16.3 s | 13.0 s | 8.9 s |
| Unthrottled (page clock) | 2.45 s | 1.49 s | — |
| ×4 CPU, unthrottled network | 5.5 s | 3.7 s | — |
| Decoder bytes posted | 2,412 MB | 49 MB | — |
| Main-thread `postMessage`, ×4 CPU | 630 ms | 40 ms | — |
| `AUDIO.RES` bytes downloaded | 18.7 MB | 18.7 MB | 5.1 MB |

Every one of the bank's 652 members, and all members of `AUDIOTBL.RES`,
`RIDER_MT.RES`, `E01_FR.RES` and `BIKE250.RES`, extracts byte-identically
through the block path versus whole-archive extraction. Rider vocals warm after the
ride is ready and race stingers during race load, so no sound waits on the
network mid-ride. With today's single-blob package the reader still works and
downloads the bank once. The chunked package must publish only after this client
is live: the previous client fetches a record's chunks one at a time.

Considered and not changed: chunking `RIDER_MT.RES` cut its bytes 3.7→1.6 MB but
its member reads become serial after the terrain, so the ride was no faster
(9.06 s vs 8.94 s) and race CPU outfits would add serial fetches. Starting the ride
before sounds finish would save ~4 s more on a cold slow link but changes the
"ride starts with sound" behavior that content tests assert. A warm visit with a
real disk profile already reuses content from the HTTP cache (1.1 MB over the
network), so no Cache Storage layer was added. WASM is 829 KB brotli; physics
batches average about 1 ms on the host during a four-CPU race.

## Rust engine: water grid GPU time (desktop host, 2026-10-21)

The Rust engine dropped the browser's 12,000/8,000-vertex water-grid cap and draws
the recovered dense-row count (00190aa0; RUST_ENGINE.md "Fidelity follow-up: water
grid rows"). Its cost was timed with `fury --gpu-frames` (wgpu timestamp queries
around the scene pass, median of 240 back-to-back frames, median of five runs) on an
Apple M4 under native Metal, against a temporary build with the cap restored.

| View | Size | Original preset: native rows / browser cap (ms) |
| --- | --- | --- |
| NAT01 sea (water fills the frame) | 1280x720 | 2.07 / 1.14 (+0.93) |
| NAT01 sea | 390x844 | 0.88 / 0.55 (+0.33) |
| Wet venue starts (ENV04/05/07/08, FREE04) | 1280x720 | +0.25 to +0.27 |
| Wet venue starts | 390x844 | +0.10 to +0.12 |

High adds about the same absolute amount. These are host GPU numbers, not phone
results; the portrait size is a viewport, not a device. The vertex count triples in
portrait (8,000 to 24,500), so a phone measurement is needed before the native rows
reach play.html; any phone cap would be an explicit option. Details and the full
table are in RUST_ENGINE.md.

## Rust engine: full Free Ride room (host, 2026-10-21)

`tools/test_engine_freeride_full.mjs`: one Rust browser and 19 synthetic riders at
20 Hz in FREERD01 on a local relay (one Mac, headless Chromium/Metal, 1280x720).
In 12 one-second samples it drew all 19 riders, at a median of 79 fps, 13.4 ms
frames and 6.9 ms main-thread CPU. It received about 52.5 KB/s.

Hitches: a 192 ms frame while six remote engine banks decoded on the main thread,
and two 36 ms seconds of unknown cause. These are host numbers, not phone results;
details are in RUST_ENGINE.md (Multiplayer).

Follow-up (same host, 10 Hz bots, measured after the player's own loading): remote
models and engine banks now load as frame tasks, polled about 3 ms per frame.
Resampling kernels, rider motions, outfits, the vehicle pipeline and texture
views are shared, and there is at most one upload per frame. Arrival seconds:
worst frames 25-31 ms (were 16-53 ms), load slices up to 13.6 ms (were 16-23 ms),
and uploads up to 7.7 ms (the first was 33-39 ms). The player's own bank output is
bit-identical. Two 32-36 fps seconds remained, with engine CPU at 7 ms. On this
heavily loaded host they are outside the engine's main-thread work. Round trip
with 1 s pings: median 7 ms, p95 47 ms.

## Rust engine: desktop render scale, Auto tier and loading (host, 2026-10-22)

Desktop builds now follow the browser's render-scale policy where the GPU backend
scales a surface smaller than the window: Metal (the drawable is stretched over the
layer) and DX12 (`DXGI_SCALING_STRETCH`). There the surface renders at most 1.5
backing pixels per logical pixel and `RenderScale` steps it down by 0.25 to 1.0 after
1.5 s of slow frames while riding, exactly as on the web; a 2x Retina window of
1280x720 points renders 1920x1080. Vulkan and GL swap chains must match the window,
so Linux (and Windows on Vulkan) keep the native size and no adaptive step. The UI
keeps laying out in logical pixels.

Desktop Auto graphics comes from the GPU adapter (`desktop_tier` in atv-app):
discrete GPUs, and Apple silicon's integrated GPU, get High; other integrated,
virtual and software adapters get Enhanced, and so does any machine with four or
fewer cores (the browser's low-power rule). The Apple M4 host resolves High.

Ride loads and room joins no longer block the desktop event loop: `spawn`ed loads
are polled between events (16 ms slices in menus, 4 ms while riding), relay POSTs and
the admission wait run off-thread, and the venue/ride loaders yield between their
long decode steps (the web resumes at once, so its behaviour is unchanged). Measured
with `fury --autodrive ... --bench` on ENV01 (Apple M4, Metal, release, warm disk
cache; bench `longest_load_ms` / `load_loop_gap_ms`): loads of 2.25-2.94 s whose
longest event-loop turn was 279-359 ms (the start-of-ride scene partition in
`Ride::start`). Before, the whole load ran inside one blocking call (a first cold run
took 6.8 s), during which the window could not answer the OS. These are host-only
measurements, not results from other desktops.

## Phone menus and loading (Rust engine, host emulation, 2026-10-29)

Reported on a physical iPhone: menus visibly choppy, especially at first, and
slow map loads, unlike the retired JavaScript game. Everything below is **host
emulation** (Apple M4, headless Playwright Chromium/ANGLE Metal and WebKit,
390x844 CSS at DPR 3 with touch, CDP CPU throttling for Chromium); none of it
is a physical-phone measurement. `tools/benchmark_phone.mjs` produces every
row (fresh browser context per scenario, cold HTTP cache, local
`tools/serve_web.py` with the private content package, no compression). It
wraps `requestAnimationFrame` to time each engine callback, counts WebGL draws
per frame, timestamps the loader's `#status` stages and records Chromium long
tasks. Baseline: main `74bbe9f`; candidate: branch `perf/phone-menus-loading`.
The JavaScript reference is `tools/js_reference.py serve` (pinned `8e1b94f`).
Reports: `work/verification/phone-perf/*.json`.

### Causes found

- **Every animated menu page was laid out from scratch every frame.** The
  title's Press Start pulse value was part of every page's `Screen`, so the
  page cache (`Front::render_page`) missed on each frame of any animating page
  (turntable, wordmark sway) and ran the whole taffy layout: about 1 ms per
  frame unthrottled on the main menu and 3.5 ms in the garage (CPU profile).
- **Loads ran as one long main-thread task.** The web build is single-threaded
  and `yield_frame` only re-polled at once (a microtask), so the venue load
  froze the page for 12.9 s at x4 (the loader drew 15 frames in 17 s). The
  JavaScript game decoded in a Web Worker and kept its main thread free.
- **Main-thread decode work per env01 ride (unthrottled CPU profile, 3.75 s
  total):** terrain-layer IPU decode 1.07 s, sky tiles plus their Enhanced
  smoothing 0.84 s, scene-collision plant re-heighting 0.26 s (each terrain
  contact scanned every leaf), inflate 0.48 s, SHA-256 chunk checks 0.20 s,
  shader compiles 0.2 s, texture uploads 0.27 s; the ride's sound bank
  (resampling 0.34 s) ran in the first riding seconds.
- **First tap in the menus:** the menu tune (two VAG channels, decoded and
  resampled) and the first rows of course photos decode on the page's thread;
  host headless Chromium also spends 0.4 s creating the `AudioContext` there
  (not measured on a phone).

### Changes

- The title is laid out once at a fixed pulse and only its Press Start
  overlay's alpha is retinted each frame (`front::retint_pulse`); other pages
  keep a constant pulse, so an animating page reuses its laid-out display list.
- `atv_content::yield_frame` (used by `atv_game::yield_frame`) hands the
  thread back to the browser through a `MessageChannel` task once 8 ms of work
  has run since the last real yield; the loader paints and input is handled
  between load steps. Work and order are unchanged.
- A decode pool (`atv_content::pool`, `web/decode-worker.js`): 2-4 module Web
  Workers (cores minus one) instantiate the page's own compiled WebAssembly
  module and run `atv_game::offload::run` jobs: IPUM terrain layers and sky
  tiles (with smoothing), the ride's sound-bank VAGs (decode + resample), the
  menu tune, the course photos, and SHA-256 chunk checks while the pool runs.
  Each job runs the same Rust function as inline decoding, so outputs are
  byte-identical; without workers (old shell, failure) jobs run inline. The
  pool starts with a ride load and stops after 20 s idle. Desktop runs the same
  jobs on scoped threads.
- Terrain contacts find their leaf through a 16-cell block index
  (`LeafIndex`), falling back to the scan if the leaf list does not match it.
- `Still loading? Reload` appears on the boot loader after 20 s and reloads
  without the offline service worker (the menu agent's proposal for a first
  visit that hung for minutes in the iOS Simulator; its cause is not yet
  reproduced on the host).

WebCrypto was tried for the chunk checks and dropped: Chromium hashes on the
page's thread (31 ms per 2 MiB, slower than the wasm hasher); WebKit hashes off
thread.

### Before and after (host Chromium, x4 CPU throttle)

| Scenario | main `74bbe9f` | candidate | JS reference |
| --- | --- | --- | --- |
| env01 `drive=1` link to first ride frame | 19.5 s | 9.8 s | 10.9 s |
| Longest frozen loader gap | 12.9 s | 1.8 s | (DOM loader, worker decode) |
| Loader frames drawn during the load | 15 | 87 | - |
| Load stage "Reading original ENV01" | 11.4 s | 3.2 s | - |
| Load stage "scene collisions" | 2.0 s | 0.7 s | - |
| First 5 s of riding: fps / worst frame | 49 / 473 ms | 56 / 133 ms | 72 / 42 ms |
| Main menu idle: engine callback p50 / busy | 5.4 ms / 0.48 | 2.1 ms / 0.24 | 0.2 ms / 0.12 (DOM) |
| Garage idle: callback p50 / busy | 8.2 ms / 0.67 | 2.5 ms / 0.24 | - |
| Title idle: callback p50 | 4.0 ms | 2.6 ms | 0.1 ms (DOM) |
| Single Player idle: callback p50 | 5.1 ms | 2.1 ms | - |
| First key on the main menu: longest task | 258 ms | 56 ms | 0 |
| Boot: long tasks in the first 10 s | 3 (1,109 ms) | 3 (557 ms) | 0 |

Frame intervals in headless Chromium are not vsync-locked (they alternate
about 11/26 ms in every build, the JavaScript game included), so callback time
and main-thread busy fraction are the comparable numbers. The remaining slow
menu paths are the Select Track and Single Race pages: scrolling the course
list costs 7-30 ms per frame at x4 and opening the page 300 ms (course-photo
blur filters and text rasterization at 3x), and the garage's slide-in
150 ms; they are the next targets.

### UI atlas at 3x (follow-up)

At 3x the Select Track page's labels did not fit the UI's 2048x2048 text and
shape atlas, so opening it rebuilt the atlas four times and re-rasterized
about 1,000 labels (canvas `getImageData` readbacks, ~0.3 s unthrottled per
open). The atlas now grows once to 4096x4096 (64 MiB, only when a frame
overflows it; within the device's texture limit) instead of being rebuilt.
Host Chromium x4, Select Track: labels rasterized per open about 1,000 -> 7;
Back/open transition 12 -> 63 fps, engine callback p95 273 -> 18 ms, longest
task 308 -> 231 ms (busy 0.96 -> 0.49). List scrolling (callback p50 7 ms,
p95 30 ms at x4; see "List glides without relayout") and the garage slide-in
(150 ms) remain.

### List glides without relayout (follow-up, #468)

Measured before changing anything: on Select Track each frame of a list glide
laid out the whole page again (about 18 ms at x4) only because the scroll offset
is part of the page's cache key, and uploaded all 268 instances, most of them
cards scrolled out of view. A new offset now moves the laid-out items and hit
boxes of the scrolled element (`layout::ScrollSpan`), and `Renderer::prepare`
skips items outside the current clip, so rows out of view cost no instance or
rasterized label. Off-clip items were never visible, so nothing drawn changes.
`pageLayouts` in `?uiDiagnostics=1` counts full layouts.

Host Chromium x4 emulation (Apple M4, 390x844 DPR 3 touch, `?uiDiagnostics=1`,
3 runs each, same day, not vsync-locked; physical phones not measured). The long
tasks count over a burst of 10 steps (6 at 2 steps/s, 10 keys on Championships):

| Select Track | Before | After |
| --- | --- | --- |
| Glide frame prepare p50, 2 steps/s | 20-22 ms | 7-8 ms |
| Long tasks, 2 steps/s | 2, 1, 0 | 0, 0, 0 |
| Long tasks, 10 taps/s | 6, 10, 6 | 0, 0, 1 |
| Long tasks, first 10 keys/s after opening | 8, 9, 10 | 10, 7, 10 |
| Instances drawn | 268 | 94 |
| Labels rasterized by the end of that key burst | ~725 | ~285 |

Championships, 10 keys: long tasks 3, 2 -> 1, 1. The first key burst is not
reliably better: it varies widely on this shared host (a later single run gave 2
long tasks, then 0 on a second burst), and the atlas can now grow during it
(62 ms upload, once) rather than on opening. A selection step itself still
costs a real layout (about 19 ms at x4) and upload (about 11 ms), plus the course
artwork's two upload frames (13-23 ms each); those are what remain.

### Terrain mip chains on the decode workers (follow-up)

The terrain layers' box-filtered mip chains (`atv_core::texture::downsample`,
moved there from atv-render unchanged) are now produced by the same decode job
as each layer, so the main thread only uploads them. Host Chromium x4: the
"Building the scenery textures" stage 1.82 -> 0.80 s (env01) and 0.99 s (env08);
longest load task 1.82 -> 0.83 s; longest frozen loader gap 1.8 -> 1.3 s.


## Decoded course cache (Rust engine, web, host, 2026-11-03)

Issue #452: a course the player has loaded before should open without
repeating its decode work. All numbers are **host** measurements (Apple M4,
headless Playwright Chromium, 1280x720, `/play.html?venue=env01`, local
`tools/serve_web.py` with the private package unless noted); none is a
physical-phone result. "Load" is the engine's own ride-load time
(`prepare`, `?uiDiagnostics=1` `decodedCache.loadMs`).

### Measured first

- **Network is already gone on a second load.** Production
  (`furyremastered.com`, main `d5ac468`, x4 CPU throttle): first load 3.3 s
  link-to-ride, later loads 1.1-1.15 s; the offline worker's content cache
  and the HTTP cache serve every chunk. The local server (`no-store`, on
  loopback) shows the same second-load time, so what remains is CPU work.
- **Second-load main-thread profile (x4):** inflating archive members was
  the largest single item (about 20%: the course archive read by the venue
  and again by the ride, the rider motions archive whose 78 motions re-inflate
  shared blocks, the rider materials); then sky-tile mip chains built at
  upload, the dense terrain heightfield (built twice), scene text parsing,
  model-texture decodes and mesh conversion. The decode jobs (terrain layers
  with mips, sky tiles, the ride's sound bank) run on the workers, which CDP
  throttling does not slow, so the host understates their phone cost.

### Strategy

Cache Storage, not IndexedDB or OPFS: it is reachable from the page thread
through `web-sys` without a wrapper crate, stores large binary bodies
efficiently, and is cleared with the site's other data. OPFS synchronous
handles exist only in workers, and IndexedDB would add a transaction layer
for plain key-to-bytes entries. `atv_content::decoded`:

- one cache, `atv-decoded-v1` (not a `fury-` name, which the offline worker
  deletes when unknown); entries named by the content package (a hash of
  `manifest.json`), the container format and a caller key; a new package
  or container format clears the cache;
- job results (`atv_game::offload::many`: IPUM terrain layers and sky tiles,
  VAG sound-bank PCM, PST menu photos) keyed by `offload::FORMAT`, the job
  kind/argument and a 128-bit fingerprint of the exact input;
- expanded archives (`Resource::expand`, the same archive stored
  uncompressed, so a member read is a copy) for the course archive (with its
  race overlay), the rider, `/ANIMSRDR.RES` and `/RIDER/RIDER_MT.RES`,
  keyed by the package's chunk digests; produced on a decode worker after the
  load finishes, never on the page's thread;
- every entry carries magic, format, its own key, length and a checksum,
  checked on each read; a damaged entry is deleted and decoded again;
- a least-recently-used index in `localStorage` bounds the total to 192 MiB
  or a quarter of the origin's free quota; writes wait until no load is
  running; quota and storage errors only count, and a missing `caches`
  (insecure page, some private windows) or `?decodedCache=0` turns it off;
- desktop builds read local files and keep the uncached path (no-op).

Independently of the cache, the venue now builds the dense terrain grid once
and shares it between the height lookups and the renderer (identical data).

### Before and after (ENV01, same browser context, warm HTTP cache)

| | warm, `decodedCache=0` | warm, cached |
| --- | --- | --- |
| Load, unthrottled | 1.34-1.38 s | 0.79-0.83 s (-40%) |
| Load, x4 CPU throttle | 4.0 s | 2.95-2.99 s (-26%) |

The x4 figures were taken while the shared host ran about four times slower
than in the morning's x4 run (the uncached main build also measured 4.7 s
then instead of 1.0 s), so only same-session pairs are compared. One ENV01
visit stores 165 entries, 63 MB. A cold first load costs about 0.2 s more
engine load time at x4 (fingerprints, checksums and copying results out),
within the noise of the whole link-to-ride time. The browser test
`tools/test_engine_decoded_cache.mjs` repeats the comparison (it requires a
cached load at least 10% faster than the uncached warm load, every decode a
hit, and a damaged entry rejected) and runs against production with
`--url https://furyremastered.com`.

Remaining second-load work is conversion the cache does not yet cover:
sky-tile mip chains at upload (`atv_render::sky`), scene text parsing,
model-texture decodes in `materials`/`ride::textures` (main thread) and mesh
packets, plus shader compiles and uploads. Those are the next candidates.

## Menu course preload (Rust engine, web, host, 2026-11-04)

Issue #453: while the menus rest on a course, prepare it so Start is faster.
All numbers are **host** measurements (Apple M4, headless Playwright Chromium,
1280x720, local `tools/serve_web.py`, ENV01, a fresh browser context each run);
none is a physical-phone result. x4 is CDP CPU throttling, which does not slow
the decode workers, so it understates a phone's worker cost.

`atv_game::preload` follows `Frontend::next_course`: Select Track, the race
setup, the Single Player row leading to either, and the Championship row's next
round. After the selection has rested for 800 ms it downloads the archives at
low fetch priority and fills the decoded cache of #452 (expanded course and
rider archives, terrain layers with mips, sky tiles). Limits:

- a selection change, leaving the page or Start abandons the run at its next
  step; a new run waits for the last job of the abandoned one, so one course
  runs at a time. Job batches are two images, so an abandoned run ends quickly.
- inflating, decoding and scene parsing run on the workers; with no worker pool
  it does not run. A hidden page waits.
- it refuses `saveData`, an `effectiveType` of 3g or slower and `deviceMemory`
  of 2 GB or less. A page prepares at most 6 courses from scratch, 1 on a
  cellular `connection.type`. These report nothing on Safari and Firefox, where
  no limit but the cache budget applies.
- it evicts only entries from earlier visits, never ones this page used, so
  browsing cannot push out the course just ridden. A course that does not fit
  is skipped.
- a finished course leaves a marker listing its entries; if they are all still
  stored, returning to it reads and downloads nothing but the small player files.
- desktop has no decoded cache, so the preload is a no-op there and its load
  path is unchanged. `?preload=0` turns it off.

Start, ENV01, local server (so little network time is included):

| | no preload, cold | after a preload |
| --- | --- | --- |
| Load, unthrottled | 1.81-1.88 s | 1.01-1.05 s (-44%) |
| Key press to ride, unthrottled | 1.97-2.48 s | 1.18-1.22 s |
| Load, x4 CPU throttle | 5.50-5.62 s | 3.91-4.01 s (-29%) |

The preload itself took about 1.5 s (2.5-3.1 s at x4). The page's long tasks in
that window are the menu's own start-up art except for two it adds, 50-74 ms at
x4; unthrottled it adds none over 50 ms. `tools/test_engine_menu_preload.mjs`
repeats the comparison and fails on a long task of 100 ms or more during a run.

Production's cold first load also spends time downloading, which the preload
removes; that gain is not in this table.

### Sound bank and race CPU quads (issue #515, host, 2026-11-13)

After the course and the player's files, a run now also prepares:

- **the ride's sound bank**: the samples `audio::bank::load` decodes up front
  (engine, impacts, landing, chassis, drive-on and the venue's world sounds,
  read from the venue's Free Ride archive as the bank does), plus a race's
  lamps and IntOut stingers (`ride::race::RACE_SAMPLES`), as `VAG_PCM` jobs at
  the output rate (48 kHz on the web). Crowd venues' crowd samples, train,
  rain and actor loops are left to the ride. A motorcycle's bank still decodes
  on the page's thread uncached, so nothing is prepared for it. Samples are
  shared between venues, so the budget check counts only the decodes not
  stored yet (`bank::pcm_bytes`, from each VAG header). ENV01 stored 15.2 MB
  for a Free Ride bank and 45.9 MB for a race (the stingers are long); a
  second venue adds only its world sounds.
- **every quad the race's CPU roster can draw**: the roster is drawn at Start
  from a fresh seed, so the run prepares the whole pool
  (`race_roster::candidates`, the ATV groups of 00210d20 for the player's quad
  or the course's `ChampQuadGroup`): downloads and texture decodes. ENV01 with
  the default ATV: 9 quads, 99 textures, 3.7 MB, checked against an 8 MB
  budget.

Each part has its own finished-marker, so returning to a course reads nothing
again. The selection-change, page-hidden, data-saver, slow-connection,
low-memory and session limits are unchanged; the run takes longer (ENV01
race: about 3.0-3.4 s instead of 1.7 s).

Two ride-side changes make the preload's results usable and are output
identical: the race load decodes CPU quad textures with
`textures::load_jobs` (worker PST jobs, read from the cache) instead of on
the page's thread, and `Bank::load_samples` (race samples, train, rain,
actors) decodes as cached `VAG_PCM` jobs instead of one sample per frame on
the page's thread.

ENV01 National race from the race setup, default ATV, local server
(`tools/test_engine_menu_preload_ride.mjs`, host only, no phone measured):

| | before #515, after a preload | after #515, after a preload |
| --- | --- | --- |
| Race load | 1.81-1.91 s | 1.65-1.81 s |
| First frame to sounds ready | 2.09-2.19 s | 0.58-0.66 s |
| Decoded-cache misses from Start to sounds ready | 124 | 0 |

A cold race (no preload) now has its sounds ready 1.8-2.1 s after the first
frame (2.2-2.4 s before), from the off-thread race samples. A Free Ride's
sounds were ready 0.70 s after the first frame after a preload before #515
and 0.38-0.47 s now, with no cache miss. The CPU quads' share of the race
load is small on the host (local downloads, small textures); on production
the preload also removes their downloads.

Still not preloaded: Freestyle objective and mini-game overlays. A load after
a preload remains slower than a cached `?venue=` load (0.80 s); the cause was
not investigated.

## Garage vehicle load (Rust engine, web, host x4 emulation, 2026-11-03)

Issue #469. Host Chromium with x4 CPU throttle, 390x844 DPR 3, touch; a settled
step to a new vehicle in a fresh browser context, three or four steps each.
Emulation only: these are not physical-phone results.

| Per settled step | Motorcycle before | Motorcycle after | ATV before | ATV after |
| --- | --- | --- | --- | --- |
| Slowest long task | 506-546 ms | 130-164 ms | 55-126 ms | 63-65 ms |
| All long tasks | 724-812 ms | 164-193 ms | 55-126 ms | 63-115 ms |
| Tap to rider or vehicle built | 944-1199 ms | 563-704 ms | 356-548 ms | 375-424 ms |

Phase timing of the motorcycle before: about 130 ms for the riding `.mot` members,
130 ms for the catalog motions and 38 ms for configs the menu never uses; 70-120 ms
for each of the bike's and rider's textures; about 45 ms for each preview build.

- `motorcycle::Selection` loads the meshes, rest pose and four selection clips only.
- `ride::load::expanded_archive` inflates the archive as a job and shares the ride's
  cache of expanded archives; `ride::textures::load_jobs` decodes the PSTs as jobs.
  Both give the same result as the full loaders
  (`crates/atv-game/tests/garage_vehicle_load.rs`).
- At most one preview builds a frame, and a build no longer copies its texture set.

Still open: the vehicle's preview build takes about 110 ms in the frame right after a
load and about 47 ms when the same model is rebuilt later; the extra time is not
explained yet. An ATV's slowest task is still its preview build. A separate 500-600 ms
task follows the first tap in a fresh browser process, before any vehicle load, and
this change does not touch it.

## Remastered rain (host, 2026-11-12)

The authored Remastered rain pass ([WEATHER.md](WEATHER.md)) adds three
instanced draws (3,200 streaks, 64 splash rings, 20 lens beads; 6,568
triangles) and three small buffer writes per frame. Every number here is from
one Apple M4 Mac that other builds and browsers were using at the same time;
none is a phone result.

**GPU.** `fury --gpu-frames 240` (native Metal, median scene-pass time of 240
back-to-back frames) on the held starting ride frame of a raining load under
Remastered, with the pass built and with it skipped by a measurement-only
switch that is not in the tree. Six alternating runs each; median (min–max)
in ms:

| Venue | Size | With the pass | Without it |
| --- | --- | ---: | ---: |
| Freeport (ENV04) | 1280×720 | 4.38 (3.77–6.15) | 4.31 (3.12–7.65) |
| NAT06 | 1280×720 | 10.31 (9.49–14.11) | 10.20 (9.41–11.35) |
| SX13 | 1280×720 | 14.55 (11.28–17.56) | 14.47 (12.95–16.62) |
| Freeport | 585×1266 | 2.44 (2.06–4.09) | 2.26 (1.84–3.87) |
| NAT06 | 585×1266 | 5.90 (4.64–7.56) | 5.60 (5.20–7.36) |
| SX13 | 585×1266 | 8.54 (6.20–11.02) | 9.27 (7.38–11.06) |

The medians differ by −0.7 to +0.3 ms, less than the spread between runs, so
the pass costs at most a few tenths of a millisecond here and this host
cannot resolve it further. 585×1266 is the canvas a 390×844 DPR3 phone gets
at render scale 1.5; it is a size, not a device.

**CPU.** `play.html?autodrive=…&profile=1` (630 frames riding straight on
NAT06, headless Chromium/Metal, median of three alternating runs). The
effects prepare and encode marks, which hold the rain pass's work, per frame:

| Profile | Remastered, dry | Remastered, raining | High, dry | High, raining |
| --- | ---: | ---: | ---: | ---: |
| Desktop 1280×720 | 0.025 ms | 0.043 ms | 0.029 ms | 0.027 ms |
| Phone emulation 390×844 DPR3, CPU ×4 | 0.175 ms | 0.328 ms | 0.125 ms | 0.157 ms |

Whole-frame render CPU moved less than its noise (3.56 to 3.65 ms desktop,
21.7 to 23.7 ms throttled). A raining load also draws the venue's own rain
model, so these differences are an upper bound for the authored pass.

**Not from this pass.** The venue's own placed rain model is the expensive
part of a raining NAT06 or SX13 in every preset: on NAT06 the High scene pass
measured 3.21 ms dry and 7.56 ms raining at 1280×720 (medians of eight runs;
1.49 and 4.58 ms at 585×1266), and Remastered 5.92 and 10.97 ms. That is the
original geometry, drawn as on main, and is tracked separately.

## Venue rain cards (#589, host, 2026-11-13)

The venue's own rain model, not the Remastered pass above, was the expensive
part of a raining NAT06 or SX13 in every preset. Facts from the loaded
scene: NAT06 places the model 120 times (240 draws across two LOD levels;
the LOD 0 card has 72 triangles), material `rain`, microprogram
`TexTransformPrelitCompressed`, alpha blending, GEQUAL alpha test with
reference 120 and failure method 1 (FB_ONLY). The renderer therefore drew
each card twice: an accepted pass with depth write, then the alpha-failure
pass blended without depth.

The 64×128 rain texture has 38 non-zero alpha texels (values 40, 72 and 97);
the cards' vertex alpha is 0, 31 or 63. Decoded texture and vertex alpha stay
in GS units (128 is unity) and the cutoff is the PSM byte over 255, so the
shader's comparison is the GS comparison and there is no units error. The
highest modulated alpha is 97 × 63 / 128 ≈ 48, far under 120: the accepted
pass can pass no fragment, on the PS2 as here, yet it ran the full fragment
shader over every card. Nearly every fragment of the failure pass has alpha
exactly 0 and leaves the framebuffer unchanged, but it still paid for
lighting and both shadow lookups.

Changes (`scenery.rs`, `scenery.wgsl`), all producing the same pixels:

- At load, a chunk whose highest texture alpha (all ATM frames), scaled by
  its highest vertex alpha when prelit, stays more than one byte under its
  cutoff skips its accepted pass. Filtering, mipmaps and interpolation only
  average, so they cannot exceed those maxima.
- The world shadow map (Remastered) skips chunks whose texture alpha cannot
  reach its cutout (the cutoff, or 0.25).
- The blended failure pass, which writes no depth, discards fragments whose
  alpha is exactly 0 before lighting and shadow taps. Alpha blending and the
  additive mode leave the destination exactly as it was for such a fragment.

**Same pixels.** `fury --capture` before and after, one warm-up run each
(the very first capture from the new build differed by one level in 12
pixels, and its repeat runs match the old binary exactly; the cause is
assumed to be first-run timing, not the change): NAT06 and SX13 raining in
High and Remastered, NAT06 dry in High, ENV01 dry in High and Remastered,
ENV05 dry in High and SX01 dry in Remastered, each at 1280×720 and 585×1266.
All 18 pairs have 0 differing pixels.

**GPU.** `fury --gpu-frames 240` on one Apple M4 Mac shared with other work,
alternating runs, median (min–max) scene-pass ms. Host numbers, not phones:

| Venue | Preset | Size | Dry | Raining before | Raining after |
| --- | --- | --- | ---: | ---: | ---: |
| NAT06 | High | 1280×720 | 2.26 (1.82–4.91) | 9.00 (6.32–10.13) | 4.27 (2.32–7.25) |
| NAT06 | High | 585×1266 | 1.48 (1.21–1.49) | 5.13 (3.28–5.68) | 1.85 (1.49–2.53) |
| SX13 | High | 1280×720 | 6.67 (4.93–8.33) | 13.71 (9.24–14.32) | 9.20 (9.00–10.36) |
| SX13 | Remastered | 1280×720 | 9.46 (8.60–10.28) | 19.57 (18.34–20.08) | 11.13 (10.11–11.55) |

Seven runs each at 1280×720 High, five otherwise. Dry frames are unchanged
(NAT06 High 2.24 after). Earlier probes on the same scene attributed the
cost roughly evenly to the two passes (either alone about 7 ms of the 9).
What remains is the failure pass rasterizing the large, mostly transparent
cards and shading the few streak texels; tighter card geometry would cut it
further but changes the meshes, so it was not done here.

## Retained menu layout (#511, host, 2026-11-14)

Re-measured the redesigned menus on `58da9aee` using the validated deployed
artifact, then the local candidate: Chromium, 4x CPU throttle, 390x844 DPR3,
touch, three fresh contexts, ten Down keys with 100 ms between driver calls
and one second settling. `test_engine_menu_long_frames.mjs` records long tasks
and each changed course frame's artwork/layout/upload timings. Host only;
these are not phone results and the driver is not a precise 10 Hz input clock.

The selection still rebuilt a Taffy tree for the entire page. The frontend now
retains its background and content trees. Changed text metrics and styles dirty
the affected nodes; changed structure rebuilds the tree. Color/shadow changes
repaint using retained geometry. Rasterization, artwork, clipping and hit testing
keep their existing paths. A unit comparison checks identical display lists,
hits and scroll spans against fresh layout after paint, text, width and child
changes. This is an authored renderer optimization, with no new recovered-game
behavior claim.

| Ten-key burst plus settling, three runs | Before | After |
| --- | --- | --- |
| Changed-course layout median per run | 29.2, 27.2, 26.8 ms | 10.5, 12.9, 11.7 ms |
| Long-task count | 12, 13, 14 | 9, 13, 9 |

Residual: changed-course uploads still have approximately 11–114 ms samples,
and the fresh-context scenario still includes 467–587 ms tasks. The reduction
is in layout; this does not close #511 or establish physical-device performance.
