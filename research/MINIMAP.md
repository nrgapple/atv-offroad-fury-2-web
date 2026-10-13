# Freestyle GPS minimap

Projection/zoom correction strategy (2026-10-13): the live scene camera uses
right = −X with north/up = +Z. The old +X-right map reflected that view. Use
−X-right consistently for terrain raster sampling, scenery and crop coordinates,
and derive marker bearing from the same projected forward vector. Follow the
rendered actor rather than a different simulation/interpolation sample; detached
rider poses take over the GPS center during wrecks. Add persisted 25–200% zoom,
default 50% (1,536 feet across, twice the previous range), independently of size.
Zoom only changes the crop of the same cache; use an adaptive feet scale.

Release-consistency correction strategy (2026-10-13): reproduce the reported
old rectangle/button padding plus duplicated speed by retaining the previous
CSS/HUD module alongside the new minimap module. Version every deployed local
CSS, module import, Worker and WASM reference with the release commit so a
returning browser cannot reuse an earlier file from its stable URL cache.
Keep the audited artifact filenames and hashed original-resource URLs unchanged.
Freestyle standalone speed ownership follows the explicit minimap Off setting,
including during raster generation and temporary visibility suspension.

Implementation strategy (2026-10-13): an optional authored orthographic X/Z
projection of the recovered terrain colors/heights and static scenery, cached
once per course. The GPS window follows the rendered vehicle or detached rider with north (+Z) up and a
heading arrow. It does not alter the gameplay camera or simulation. This is a
browser enhancement, not a recovered PS2 overlay.

A second live scene camera would repeat geometry, vegetation, water and shader
work each frame. Instead use one bounded 1024px CPU aerial raster, generated in
short frame-budget slices, then crop/blit at at most 20Hz into a small canvas.
No second WebGL context, scene traversal per update, GPU readback or new content
request. Pause, hidden tabs, context loss, racing and Off suppress updates. Off
releases raster/backing storage and cancels pending generation; re-enabling
rebuilds once. Resizing changes the HUD footprint, not its geographic range.

Settings live in Pause → HUD settings → Minimap configuration with persisted On/Off, 96–224px size and 25–200% zoom (50% default / 1,536ft across),
responsive caps, shared silver/gold/charcoal tokens and no frame border. The map
accepts a tap to change orientation. Static geometry and terrain are approximate cartography; moving actors,
vegetation, live water, paths beneath bridges and native GPS behavior are not
claimed. Physical phone performance requires device testing; report desktop and
mobile-viewport measurements separately from actual mobile hardware.

## Host verification and cost

The final host gate loads/drives all nine courses with one bake each, GPS visible,
finite Rust poses, bounded pixel storage and zero WebGL/page errors. CPU bake
cost is 44.7–53.0ms per course over 23–27 frame slices; maximum observed slice
is 2.4ms. The 2ms slice target is cooperative, not a hard deadline: allocating or
committing an image and finishing a row/triangle may overrun it on slower devices.
The map appears after the bake, while riding continues.

`tools/test_minimap.mjs` checks actual terrain pixels, border/touch behavior,
keyboard size changes, Off/On, saved preferences/reload, viewport rotation,
pause/blur, graphics-context loss/restoration and actual Rust driving/following.
An initial repeated On/Off series is followed by a final paired stationary
Caribou series at 96/160/224px. One Mac, Chromium Metal, synthetic focus;
390×844 uses DPR3 with map backing resolution capped at DPR1.5. F3 timing enabled.
The final paired mean CPU map-update costs are:

| Requested size | Desktop 1280×720 | Phone viewport 390×844 |
| --- | --- | --- |
| 96px | 0.059ms | 0.052ms |
| 160px | 0.076ms | 0.091ms |
| 224px | 0.052ms | 0.064ms |

These tiny samples have timer/host scheduling noise; larger maps are not claimed
to be faster. Actual rate is 44–45 updates per 2.5s (50ms minimum interval), and
all windows retain 471 3D draws. On/Off FPS spans about 69–78 in the final series,
with no consistent penalty across the paired/repeated runs. Frame p95 remains
26.5–27.2ms. FPS/render-CPU snapshots are rolling 120-frame statistics rather
than isolated GPU timings; they do not establish a universal FPS delta.

The cache is 4MiB RGBA. Visible canvas pixels add approximately 0.035–0.23MiB
at tested sizes; generation temporarily holds another 4MiB ImageData buffer.
This counts authored pixel buffers, not browser/driver copies, compositor costs
or shared original scene data. Off shrinks both canvases, cancels generation and
performs zero map updates, with zero retained authored map pixel storage. Size
changes preserve the existing cache; they only change the output canvas.

Shared menu/control regressions pass keyboard/controller, Tioga driving,
portrait touch/Caribou reload and Chromium 44px utilities/touch cancellation.
Source/setup verification, workspace tests, formatting/strict Clippy, JavaScript,
release allowlists and Worker tests pass. No source asset, PNG or generated
analysis enters GitHub/CI. Screenshot evidence is an inspected host capture;
public release identity and gallery delivery follow the validated main CI path.
Physical iOS/Android, thermals, battery cost and isolated GPU timing remain open.

## Combined corner instrument revision

The revised default is circular and inset 8px plus safe-area offsets at the
top-right. The optional square and bottom-left placement are persisted together
with visibility and size under Pause → HUD settings → Minimap configuration.
The root HUD settings page has only a link to the minimap group and Back.
Narrow (<=360px) utility buttons retain their 44px icon hit targets while hiding
text so a top-right map cannot cover Pause/Camera.

The map now renders the Rust HUD speed text/units and compact preload feedback
inside its footprint. The separate original speed/preload block is suppressed
whenever Freestyle minimap is enabled, including during baking or temporary
hiding. Explicit Off restores it; Race keeps its existing HUD. Stunt
text/scoring remain on the original HUD canvas.
Both shapes put compass/scale/type inside the circular safe region. This is an
authored presentation adaptation, not a change to the underlying HUD clock,
preload, scoring or vehicle physics.

Shape, corner and size changes reuse the raster without rebaking or adding scene
draws. Circle clipping is CSS; speed/preload draw with the existing capped 20Hz
canvas update. Existing pixel-resolution/memory limits and Off behavior apply.

The combined-instrument host timing gate passes settings/reload/rotation/pause/
blur/context restoration and real Rust driving. Paired 96/160/224px windows
measure 0.068–0.142ms mean CPU per update on the desktop viewport and
0.089–0.136ms on the phone viewport, on one Mac with Chromium Metal. Scene draws
remain 471 in every On/Off window, pixel buffers remain within the previous
4.04–4.23MiB range and Off has zero updates/pixel storage. These observations
include timer/scheduling noise and do not establish a physical-phone, GPU,
thermals or causal FPS claim. The small added speed/type work stays inside the
existing 20Hz canvas pass.

The final footer revision removes the speed backdrop, places MPH close to the
bottom edge and moves the small map-scale readout directly above it. Measured
text width/descent chooses a baseline that keeps the black outlines inside the
circle even at 96px; square uses a small bottom inset. No extra pixel buffer,
update loop or scene rendering is introduced.

Tap/click the map to switch north-up and heading-up; the choice persists and is
also in the configuration group. In heading-up, a sqrt(2)-overscanned crop of
the same raster rotates opposite vehicle yaw, the player marker faces up, and
a small north pointer stays below N. Speed/scale lettering remains upright.
The existing toolbar touch-release adapter handles this native button, including
second-finger taps while driving, drag-away and cancel, with no duplicate click
or riding-pointer interruption. No new scene render or raster build is needed.

The final heading-up phone-viewport gate (390×844, DPR3 capped at 1.5) measures
0.077/0.080/0.125ms mean CPU per update at requested 96/160/224px, respectively.
Responsive caps limit the largest displayed diameter to about 164px. Each
2.5-second On window has 44 updates and 471 scene draws; Off has the same scene
draw count with zero map updates and zero retained authored pixel storage.
The largest map buffer total is 4,436,368 bytes. Lifecycle, driving, persistence,
pause/blur and context restoration pass. These are Chromium Metal measurements
on one Mac, not physical-phone or isolated GPU measurements.

## Projection/zoom verification (host, 2026-10-13)

The independent GPU/crop observer passes 113 native-wreck and 111 enhanced-wreck
GPS updates without position/heading/crop mismatch (0.002ft/0.0001rad thresholds).
Moving ATV/interpolated-bike tracking, keyboard zoom and persisted zoom pass.
Chromium validates four viewports and WebKit phone portrait/landscape, including
both shapes/corners, nested settings, 44px targets, speed ownership and cache
reuse. Rust checks both sliders including 390×568 and 667×375.

The 390×844/DPR3 host A/B check observes 0.102–0.109ms per map update at 96/160/224px,
44–45 updates per 2.5s (20Hz cap), identical 471 scene draws On/Off, maximum
4,436,368 retained pixel bytes and zero updates/bytes when Off. Zoom extremes do
not rebake or change storage/draw count. These samples came from a Mac with other
browser checks running; they establish bounded work, not an FPS benefit or
physical-phone performance. Earlier measurements above remain historical.
