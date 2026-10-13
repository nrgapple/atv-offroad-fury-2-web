# Freestyle GPS minimap

Implementation strategy (2026-10-13): an optional authored orthographic X/Z
projection of the recovered terrain colors/heights and static scenery, cached
once per course. The GPS window follows the vehicle with north (+Z) up and a
heading arrow. It does not alter the gameplay camera or simulation. This is a
browser enhancement, not a recovered PS2 overlay.

A second live scene camera would repeat geometry, vegetation, water and shader
work each frame. Instead use one bounded 1024px CPU aerial raster, generated in
short frame-budget slices, then crop/blit at at most 20Hz into a small canvas.
No second WebGL context, scene traversal per update, GPU readback or new content
request. Pause, hidden tabs, context loss, racing and Off suppress updates. Off
releases raster/backing storage and cancels pending generation; re-enabling
rebuilds once. Resizing changes the HUD footprint, not its geographic range.

Settings live in Pause → HUD settings with persisted On/Off and 96–224px size,
responsive caps, shared silver/gold/charcoal tokens, no frame border and no touch
capture. Static geometry and terrain are approximate cartography; moving actors,
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
