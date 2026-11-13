# Graphics presets

Settings → Display → **Graphics** chooses how the browser presents the recovered
game. The Rust menu (`ui_menu`) owns the row and its labels; the choice persists
in `localStorage` as `atv-graphics` (`web/graphics.js`, `GraphicsSettings`).

| Choice | Meaning |
| --- | --- |
| Auto (default) | Enhanced on phones, tablets (including iPadOS's desktop user agent with multi-touch), touch-primary devices without a fine pointer, ≤4 logical cores, ≤4 GB `deviceMemory` or Save-Data; High otherwise. The row shows the resolved preset, e.g. `Graphics: Auto (High)`. |
| Original | The recovered renderer exactly as before presets existed. |
| Enhanced | Original plus the cheap authored layers below, tuned for phones. |
| High | Enhanced with 16× anisotropic filtering, long tire tracks, more lingering dust and a vehicle-only sun shadow map. |
| Remastered | High plus the authored modern look described under [Remastered](#remastered-2026-11-11). Opt-in only: Auto never picks it. |

Nothing in a preset reaches the Rust simulation, input sampling, the 60 Hz step,
multiplayer snapshots or race logic. The browser gate checks that the worker
receives the same input fields in every preset.

## What each preset contains

| Layer | Original | Enhanced | High | Provenance |
| --- | --- | --- | --- | --- |
| ATV render interpolation (player quad, attached/detached rider, Crazy/Gorn poses, CPU racers, chase camera) | Off | On | On | Authored presentation |
| Anisotropic filtering (terrain atlas/detail, scenery, vegetation) | Off (sampler never touched) | 4× | 16× (clamped to the device maximum) | Authored |
| Soft contact shadows (player, detached rider, CPU racers, remote riders) | Off | On | On | Authored; the original has no vehicle shadows |
| Dirt build-up on quad and rider | Off | On | On | Original dirt textures and zone rates; authored reveal, water wash and reset |
| Visual wheel re-projection | Off (recovered wheel placement) | On | On | Authored presentation of the documented one-frame order effect |
| Tire tracks | Off | 600 segments, 15 s | 3,600 segments, 75 s | Original GenerateTireTrack/dirt zone flags; ruts from speed, scuffs from contact slip |
| Vehicle-only sun shadow map | Off | Off | 2048², 48 ft box | Authored |
| Scene-model level of detail | Recovered selection and 130 ft multi-pass cull | Recovered | Finest level, every pass | Original `001b3a90`/`001b2390` (`VISIBILITY.md`); High is the enhancement |
| Water Fresnel | Off (original pass) | On | On | Authored |
| Lingering dust | Off | 24 puffs | 96 puffs | Original GenerateDust flags and terrain texture colours; authored puffs |
| Smooth sky | Off (original decode, bilinear) | On | On | Original CUB tiles; authored deblock, denoise, chroma upsampling and bicubic sampling |
| Motorcycle interpolation and MX mud | Unchanged in every preset | | | Existing behavior (see `MOTORCYCLES.md`) |
| Vibration (Settings → Controls) | Independent of the preset | | | Authored adaptation of the original vibration option |

### ATV render interpolation

`AtvPresentation` (`web/motorcycle-presentation.js`) reuses the motorcycle
approach: each delivered batch of 1–4 physics steps becomes one segment from the
previous frame to the newest, shown over `ticks × 16.67 ms` on the render clock.
It blends origin (quaternion nlerp), quad and rider bone matrices, the detached
rider's world matrix, Crazy/Gorn ragdoll/debris matrices, CPU racer poses
(per racer, yaw on the short arc) and the chase camera. It snaps on resets,
moves over 30 ft, rig changes, attach/detach, crash-presentation changes, camera
preset/mode changes, pauses and gaps over four steps. Each node matrix blends
as a rigid transform (shortest-arc quaternion, per-axis scale, translation);
blending the 16 entries directly shrank fast-spinning wheels mid-interval
(about 30% thinner at 1.6 rad per step, roughly 60 mph), which showed as wheel
flicker in Enhanced/High. The presented frame is a
copy; the Rust frame, HUD, audio, effects and multiplayer sends keep using the
simulation frame. Remote riders already arrive through the `atv-net`
Hermite smoothing sampled every render frame, so they need no second layer.

The segment starts when a batch arrives, so the displayed pose trails the newest
simulation state by up to one batch (16.7–66.7 ms), as with motorcycles.

### Anisotropic filtering

`EXT_texture_filter_anisotropic` with `TEXTURE_MAX_ANISOTROPY_EXT` set on the
terrain array atlas, the terrain detail map, scenery materials (including ATM
frames) and vegetation textures. Sky tiles (see Smooth sky), vehicles, riders and effect atlases
are left alone. Changing the preset updates every registered texture in place;
Original restores 1, which is the sampler default, and the gate confirms the
frame returns to the identical pixels. Context restoration re-registers textures
and re-queries the extension.

### Soft contact shadows

`ContactShadows` (`web/vehicle-shadows.js`) draws every blob in one indexed
draw after scenery and before vegetation: a 7×7 grid per caster draped over the
visible terrain (`render_terrain_height`), multiplying the framebuffer by
`1 − alpha` with polygon offset and no depth writes. The ellipse follows the
vehicle's horizontal axes (66 % of its length, 80 % of its width each side),
spreads with height and fades out by 12 ft of lift; casters beyond 240 ft fade
out. Lift is measured from a learned resting height, so a grounded quad keeps a
full contact shadow on slopes. The detached rider casts a smaller blob under the
pelvis node, including during Crazy/Gorn ragdolls. Capacity is 12 casters
(19,296 bytes of GPU buffers), recreated after context loss. Receivers are
terrain only; ramps, bridges and stadium props built from scenery meshes receive
no blob, and a vehicle high above the terrain on such a prop shows none.

### Dirt build-up

Every original quad material carries a second shader pass whose base texture is a
dirt overlay with `PixelAnimation` (for example `dirt250rm1`, `dirttireside`,
`dirttiretread`, `dirtshock`), and every rider material has one too (`dirtshirt`,
`dirtboot`...). `scenery_material::describe` now names it as `dirt_map`, and the
browser loads it into the same slot the MX mud textures use.

The level follows the source terrain-zone fields `DirtAnimChangeTime` and
`DirtAnimChangeAmount` read from the map's TOY: above 10 mph with grounded
wheels, every ChangeTime seconds the level changes by ChangeAmount of 255
(Caribou dirt: one step per 0.4 s; snow: per 1.0 s; ice: per 1.5 s; zones without
the fields change nothing). The zone under most grounded wheels wins.

Authored parts, labeled as such:

- **Reveal.** The native pixel-animation routine is not recovered. Texels appear
  as the level passes their inverse alpha, so heavily painted dirt shows first,
  then blends by the texel's GS alpha.
- **MUD scenery** (the original MUD collision category) dirties at least one step
  per 0.1 s.
- **Water** washes the quad: each half-submerged wheel removes about 6 % per
  second (fully submerged: clean in about four seconds).
- **Reset** (Reset button/key, new ride) cleans the quad; automatic wreck
  recovery keeps it.
- CPU racers and remote riders stay clean.

### Visual wheel re-projection

[COUPLED_LANDING.md](COUPLED_LANDING.md) explains the residual ~0.22 ft landing
tire overlap: springs run before integration while the frame is drawn after,
and the original visual wheel update rebuilds offsets from the stored
compression without re-checking terrain. The port reproduces that order, so
Original keeps today's wheel placement exactly.

Enhanced/High re-project the drawn wheels after integration. For each wheel
whose bottom (centre minus radius) lies below the visible terrain at its centre,
the presentation compression rises by the overlap divided by the body's
vertical axis component, at most 0.5 ft, and the result is clamped to the
quad's original suspension travel (`Suspension Compression`). The main-thread
render core (`render_quad_pose`, op 144) holds the player's original quad
linkage and re-samples the whole pose, so A-arms, shocks and the swingarm follow
the wheel. It replaces the drawn pose only when re-sampling the simulated
compression reproduces the worker's matrices (within 0.001), and never during
wrecks, Crazy/Gorn presentation or for motorcycles, CPU racers or remote riders.
The simulation, contacts, wheel effects and multiplayer snapshots are unchanged.
Single-point contact remains: on steep cross-slopes the outer tire edge can
still dip slightly (cause 3 in that note).

### Tire tracks

`TireTrackRenderer` (`web/tire-tracks.js`) keeps one fixed ring of quads for all
four wheels (Enhanced 600 segments fading over 15 s of simulation time; High
3,600 over 75 s; 130 KB and 778 KB) and draws them in one call after the contact
shadows. Segments are laid every 0.6 ft of travel while a wheel is grounded, dry
and moving faster than 2 ft/s; strips break in the air, in water, during wrecks
and on jumps over 4 ft. Zones flagged `GenerateTireTrack=T` in the original TOY
(the mud zones) and MUD scenery are soft; other zones whose `DirtAnimChangeTime`
dirties the quad (dirt, snow) are firm; cleaning or unflagged zones leave none.

What a wheel leaves follows its physics contact. `wheel_effects.slip` reports,
per grounded wheel, the sideways contact speed across the tire plane and the
tread speed minus ground speed, in the same contact frame as the tire force
step. Rolling presses a treaded rut: deep in mud at any speed, faint on firm
ground and only above about 8 ft/s. Sideways scrub above 3 ft/s or wheelspin
and lockup above 4 ft/s tear a wider, untreaded scuff that fades back out
when the tire grips again; a gripping turn stays under the threshold (about
2 ft/s measured on env01, while full-lock slides reach 12–30 ft/s). Marks
modulate the lit ground (2× multiply), so dry
scuffs churn darker grooves and lighter ridges, mud smears darker, and
world-space noise breaks marks up instead of one uniform colour. The CPU copy
survives context loss and is re-uploaded. Player only.

### Vehicle-only shadow map (High)

`VehicleShadowMap` (`web/shadow-map.js`) renders the player quad and rider, CPU
racers within 40 ft and remote riders within 40 ft into a 2048² 24-bit depth
texture from the map's directional light (the same direction as the scene's
Lambert term; clamped to at least 11° above the horizon), using a texel-snapped
orthographic box of ±24 ft around the player. Terrain and scenery sample it with
four hardware-compared taps on texture unit 7 (reserved; unit 7 holds a 1×1
comparison texture otherwise) and darken to 55 % inside the shadow, fading at
the box edge. Vehicles and vegetation do not receive it. With the map active the
contact blob drops to 35 % strength. Enhanced keeps only the blob: the map costs
one extra pass of about 13–40 vehicle draws and 16 MB of depth memory.

### Water Fresnel

Enhanced/High compile a second water program variant (`web/water.js`) whose
alpha follows Schlick's approximation (F0 0.02): about 55 % of the original
alpha looking straight down, rising towards opaque reflection at grazing angles,
with the reflection brightened by up to 20 %. The Original variant's shader
source is unchanged. No depth or refraction copies are added.

### Lingering dust

`LingeringDustRenderer` (`web/lingering-dust.js`) emits large, slow, long-lived
puffs (2.8–4.4 s, 2–5 ft growing to about 1.5×) behind the rear wheels while they
are grounded on zones whose original TOY sets `GenerateDust=T`, outside mud and
water, above 15 ft/s. Each puff takes the mean colour of the original terrain
texture layer under the wheel and fades into the fog colour. The pool is capped
(Enhanced 24, High 96) and drawn as one instanced call; the recovered wheel dust
and spray are untouched.

### Smooth sky

Each sky face is 4×4 MPEG-intra (IPUM) tiles of 128² texels, so a phone in
portrait stretches every sky texel over roughly 5–6 device pixels. That exposes
the compression: 8×8 block edges, low-amplitude stripe and dot patterns inside
blocks (most visible as a grid over haze and distant mountains), and 2×2 colour
steps from nearest-neighbour chroma.

Enhanced and High upload a second decode of the same original tiles,
`Image::smoothed_rgba` (`crates/atv-core/src/ipu.rs`, codec op 39):

1. MPEG-4 Annex F deblocking of every interior 8×8 edge on Y, Cb and Cr, using
   each macroblock's quantizer (every observed sky uses scale code 3).
2. A bilateral filter (5×5 luma, 3×3 chroma) whose range falls off over about
   1.6 quantizer steps, so ringing flattens while cloud edges and horizon
   silhouettes keep their contrast.
3. Centred triangle (3/4, 1/4) chroma upsampling instead of repeated samples.

The sky pass then samples with nine-tap Catmull-Rom bicubic (`skyBicubic`)
instead of bilinear. Tiles carry 4-texel overlapping borders (texels 120–127
of one tile match 0–7 of its neighbour), so neither filter reaches a clamped
edge and tile seams stay continuous. Measured natively, smoothing adds about
24 ms per 64-tile sky. Both decodes stay in memory (about 4 MB extra per sky)
so switching presets only re-uploads the textures; Original reuploads the
original bytes and renders identically (`test_graphics_presets.mjs`).

### Vibration

Settings → Controls → **Vibration** (default On, `atv-vibration`). Hard landings
and impacts reuse the existing `ImpactEvents` severity; entering a wreck pulses
at least 70 %. Strength `0.2 + (severity − 18)/55` (silent under 18 ft/s).
A connected controller receives `vibrationActuator.playEffect('dual-rumble')`
scaled by strength (70–300 ms). Touch devices without a controller call
`navigator.vibrate` for pulses of at least 30 % (Android; iOS Safari has no
vibration API). One impact cannot chain pulses; a stronger hit can upgrade.
It is independent of the Graphics preset.

## Remastered (2026-11-11)

Authored, not from the original game. `Graphics::remaster` (Rust engine only)
adds five things on top of High. Original, Enhanced and High do not run any of
it: every shader branch is gated by a uniform that stays 0, and the finish
pass is not created.

- **Terrain sun relief** (`terrain.wgsl` `relief`). The original terrain is
  unlit. Remastered multiplies it by hemisphere ambient plus the venue's
  directional light over the same sum for flat ground, so flat sunlit ground
  keeps its authored brightness, slopes facing the sun brighten and warm, and
  slopes facing away darken. It is applied at 65 % and clamped to 0.55–1.5
  because the terrain textures already carry painted shading.
- **Vegetation wind** (`vegetation.wgsl` `vs_mesh_wind`). Near plant
  meshes lean with a gusting sine, 2 % of a vertex's height up to 0.8 ft.
  Distant billboards stay still. It is a separate vertex entry point on its
  own pipelines: adding the sway to `vs_mesh` behind a uniform changed a few
  tree pixels by one step in Original on Metal. Its clock follows the
  ride's simulation steps since 2026-11-12; before that it only ran in the
  free viewer, so (from the code) plants stood still in a ride.
- **Water sun glint** (`water.wgsl`). A tight specular lobe of the venue sun
  on the wave normals.
- **World shadows** (`world_shadows.rs`, `world_shadow.wgsl`;
  `Graphics::world_shadows`, 2048² in the browser and 4096² on desktop). One
  sun shadow map covers 360 ft either side of the rider, in a light box
  1,440 ft long so hills towards a low sun still cast. The terrain, each
  scene model's finest solid chunks (not gates, flashes, glows or anything
  wider than 400 ft, which keeps backdrops and stadium shells from shading
  the whole course) and every plant mesh within 540 ft cast into it. The
  terrain and scenery receive it: lit models and the ground lose the sun
  term and keep the sky's light, and unlit models, whose light is painted,
  darken by up to 40 % on their sun-facing sides only. The casters do not
  move, so the map is cached and drawn again only when the rider is 108 ft
  from its centre, a new fine terrain patch arrives or a model is hidden.
  Receivers step two texels off the surface along its normal and fade the
  map out over its last 6 %.
- **Finish pass** (`remaster.rs`, `remaster.wgsl`). The frame renders into a
  texture after speed rush and air blur. A soft-knee threshold (0.68) keeps
  its brightest parts at half size, four levels blur down and add back up,
  and the composite adds 42 % of that glow, lifts exposure 6 %, rolls
  highlights off above 0.8, then applies a mild contrast curve, 12 % more
  saturation, warm highlights, a 10 % vignette and one-step dither. The
  frame stays 8-bit: no float targets, so it fits WebGL2.

- **Rain** (`rain.rs`, `rain.wgsl`). On loads whose Weather selected rain
  (Freeport always, NAT06 and SX13 by chance): falling streaks, splash rings
  on the ground and water, and lens beads on the live riding view (see
  [WEATHER.md](WEATHER.md#remastered-rain-authored--2026-11-11-finished-2026-11-12)).

Limits: split screen skips the finish pass and the world shadows (the
relief, wind and glint still apply). The player's and CPU racers' vehicles
and riders receive the world shadows (four taps on the sun term only, added
2026-11-12; parity for the other presets checked on ENV01 and ENV05, and no
capture yet shows a rider standing in shade); remote riders and ghosts do
not. Vehicles cast only the High preset's own vehicle map, not into this
one, and plants cast with their resting rotation and no wind. A
redraw of the map costs one depth pass over the nearby world; its frame time
has not been measured on a phone. Tuned on host captures of ten venues at 960×540;
no phone or frame-time measurement yet, which is why Auto does not select it.

Parity (2026-11-11, macOS Metal, native captures at 960×540): Original,
Enhanced and High on `env01` and `env05` are byte-identical to a build from
before the change.

## Cost

Measured with `tools/benchmark_graphics_presets.mjs`; see
[PERFORMANCE.md](PERFORMANCE.md#graphics-presets-2026-10-16) for the method and
numbers.

## Verification

- `web/graphics.test.mjs`: Auto detection, persistence, cycling, features,
  vibration settings and rumble scaling/rate limits, TOY dirt fields and the
  build-up/wash/reset rules.
- `web/motorcycle-presentation.test.mjs`: ATV interpolation blending and every
  snap condition.
- `crates/atv-core` `scenery_material` and `ui_menu` tests: dirt-pass naming,
  Graphics/Vibration rows and labels.
- `web/graphics.test.mjs` also covers wheel re-projection, the tire-track
  builder, lingering-dust emission and the shadow-map light box.
- `tools/test_engine_remastered.mjs` (content group): Remastered builds and
  draws on WebGL2 without errors, keeps High's settings, and its picture
  differs from High's; `test_engine_settings.mjs` cycles through it.
- `tools/test_engine_remastered_rain.mjs` (content group): the rain pass
  exists only under Remastered on a raining load, runs on the ride's clock
  and beads only the live riding view.
- `tools/test_graphics_presets.mjs` (content group): with
  `ATV_GRAPHICS_BASELINE` pointing at the previous main build, Original is
  pixel-identical on five fixed views per map (spawn camera plus four yaws) at
  390×844 DPR3 on Caribou, Supercross 1 and a water map; Enhanced/High apply
  anisotropy and one shadow draw; High→Original returns identical pixels;
  context loss/restore with High; menu rows and persistence; worker input
  isolation; interpolation and wheel re-projection only outside Original; dirt
  build-up and reset; High tire tracks, capped dust and the shadow map; only
  High renders the shadow map; a Supercross wreck rumbles a connected controller.
