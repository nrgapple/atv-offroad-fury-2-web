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
