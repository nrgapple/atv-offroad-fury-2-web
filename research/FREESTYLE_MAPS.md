# All original venues in Freestyle

## Scope and strategy

The source census covers 40 map families: eight ENV, six FREE, eight NAT,
sixteen SX, GAME01 and TRAINING. Previously only ENV01–08 and FREE05 were
connected. The missing 31 venues are not hidden by profile locks alone: their
archives were absent from the hosted manifest and loader catalog.

Connect each venue through its original FR archive, or TR/SO archives for Training and
Coonce Arena. Preserve original terrain dimensions/spacing, source SCN/TOY,
geometry, colliders, sky, materials, wheel effects and available ecosystems.
Expose each unique source venue once, ignoring tracks.dat access/point costs.
Keep existing lobby IDs 1–9 and append IDs 10–40. Shared catalog selection drives
menu persistence, URLs, loading and HUD; Rust independently bounds network IDs.
Publish only required runtime archives/previews in the separate R2 package.
Menus filter against available archives while the separately uploaded manifest
catches up with the checked authored app deployment.

Short Tracks use ENV01, ENV06 and ENV07; Enduro length and minigame records also
reuse venues. These event routes/rules are distinct from adding missing maps.
The original Training preview name has no matching shipped UI PST in the audited
UI archives; retain the course title without inventing an original thumbnail.

| Family | Original venues exposed in Freestyle |
| --- | --- |
| Environments (8) | Caribou Peak, Tioga Valley, Sonora Dunes, Freeport Harbor, Prospect Springs, Rockbridge Park, San Jacinto Isles, Arlington Stream |
| Stunt venues (6) | Tempe, Nassau, Crescent Hills, St. Croix Sound, Vancouver Ridge, Wheeler Junction |
| Nationals (8) | White Sands Raceway, Saguaro Wash, Anaconda Run, Palisades Basin, Newcastle Landing, Hawthorne Mills, Alpine Skyway, Monterey Trail |
| Supercross (16) | Sausalito, Lafayette, Battle Creek, Williamsburg, Washington, Providence, Trenton, Greenfield, Fairbanks, Prescott, Kingston, Amherst, Concord, Ventura, Lincoln, Roanoke |
| Other (2) | Training Course, Coonce Arena |

Source `tracks.dat` names and unique scene keys identify this inventory. Repeated
race, Enduro, Short Track and minigame entries do not imply additional terrain
families. This checkpoint exposes their venues; it does not implement their rules.

## Source edge cases

NAT05's source has a malformed TerrainZone2 header with an inserted assignment.
The loader treats it as an unknown section. Native 002028f0 initializes all ten
material slots through 00202770, then 00202950 writes found sections into their
source-index slots without compacting gaps. The missing slot retains grip,
traction, powerslide and braking 1, drag 0. Grip 001d8e68–001d8ea8 and wheel brake
001ed008–001ed02c directly index the surface byte without a found-count clamp.
Rust now preserves that gap and later slot identities. Tests reject nonfinite
values and ambiguous tables. The eight-zone interface still omits the original
found-section count and ninth/tenth slots; full original initialization replay is
not established by instruction inspection.

SX13 skyline buildings omit LookVector. Static reader 002064f0 supplies the
0044f120 vectors: position zero, look +Z and up +Y. Rust now uses those defaults
only for absent keys, while rejecting nonfinite authored vectors. Source bare
authoring notes are ignored by the scene adapter; the generic PHY parser stays
strict. FREE04's common SCN places one cruise liner and has no static COL meshes;
terrain and water still feed the shared collision queries.

Training's jumbotron images are original 32-bit uncompressed TGA files. The Rust
decoder preserves BGR(A), origin bits and alpha, with bounds/format checks; all
four actual files match independent Pillow RGBA decoding byte for byte. The GPU
probe observes four uploaded source textures and their draw calls. These files
are uniform dark RGB backgrounds (the fourth also has an alpha mask), not the
original live instructional/video screen contents. That dynamic content remains
unimplemented.

Coonce's original SO archive lacks the standard jump HUD images. To use this
hockey venue in authored Freestyle, the required-runtime packager aggregates the
five original ENV01 preload PSTs into the small shared OVERLAYS archive (42,385
bytes). This avoids loading a second full environment for HUD art. The arena's
terrain/scenery remain its own source resources; this is a Freestyle HUD adaptation,
not a recovered hockey-mode HUD or rules implementation.

White Sands uses eleven 4-fps animated water signature frames. All source frames
remain resident in GPU atlases and the Rust source clock selects frame pairs.
See [water evidence and limits](WATER.md): the portable normal blend is connected;
the provenance of the physics global blend weights remains unresolved.

## Environmental gaps

Connected terrain, sky, scenery, vegetation, source fog, prelit/animated UV
materials, ice overlays, water field and tire dirt/wet effects are partial
component implementations. They do not establish original full-scene parity.

- Weather: weather dispatch callbacks and lens droplets are unconnected. Freeport's
  scrolling rain cards are geometry/material animation, not that weather system.
  Source rain chance declarations vary; no all-map snowstorm or day/night cycle
  should be assumed from snow terrain or sky art alone.
- Ambient particle emitters: source common TOYs declare Tioga campfire/smoke
  and mining effects (3), Sonora grill smoke (2), Freeport fog/raindrops (2),
  Prospect fountains (5), Rockbridge campfire/smoke (2), San Jacinto volcano
  smoke (5), and Arlington waterfall/waterwheel/duct/spring spray (27).
  Caribou and Vancouver common TOYs declare none. Wheel particles do not
  run those placed emitters. Source declarations do not prove active runtime
  policy. The other 31 venues need their own emitter census; St. Croix Sound,
  for example, declares three volcano-smoke emitters and 52 animations including
  seagulls. Inspect original dispatch/lifetimes before translating behavior.
- Stadium crowds: reverse-looping original ATMs display their original first
  frame; reverse playback remains unimplemented and reported in diagnostics.
- Moving actors: Arlington trains are connected, while other TOY/VUE animation
  families and their interactions remain incomplete.
- Lighting: base Sun/ambient inputs and prelit colors are partially connected;
  point/spot lights, flares, shadows and complete lighting callbacks remain open.
- Water: projected surface and vehicle water interaction are partial; exact
  reflection/froth behavior remains unverified.
- Terrain/vegetation: exact projected pass choice, LOD, visibility, wind and
  original distant shading remain incomplete.

See MAP_FIDELITY.md for source/instruction/GPU evidence and remaining limits.
Loading all resources is not proof that every prop moves, emits or collides
faithfully. Full racing, training objectives and hockey rules are separate work.

## Verification

Host checkpoint: all 40 unique venues pass one continuous Chromium source-content
loading/driving run, including terrain dimensions/spacing, finite movement,
zero unsupported scenery meshes, bounded original collision caches, batched
vegetation, shared water fields and zero page/GL errors. The roster includes all
eight ENV, six FREE, eight NAT, sixteen SX, Training and Coonce; original IDs
1–9 remain stable and 31 venues were appended. The frontend gate visits every
unlocked choice with source previews, plus desktop/controller Tioga and portrait
Caribou loading/pause/reload. Training retains its source title with no invented
thumbnail. These were host results before public release.

Six Sausalito hay-bale approaches at 390px supply both original wheel support and
body responses (7–85 support frames and 12–104 body-contact frames per route).
Actual Chromium touch events move the ATV. The inspected portrait stadium capture
shows the original stadium, rider and GPS at 16 mph. Two clients select Sausalito
map23; each draws 13 remote elements, replicates 8.18 feet of host movement, and
cleans up when the host leaves. The landscape capture shows the host's 22 mph
view and the P2 Guest marker at 23 feet; the guest vehicle is outside that camera
view. Sixteen relay checks pass, including map41 rejection on a real socket.
These are bounded host browser checks, not physical-phone or original PS2
full-game equivalence.

After rebasing onto current main, setup, 422 Rust tests/formatting/strict
Clippy, 120 JavaScript tests, eight build-cache tests, release tests (35 with one opt-in skip), Worker tests and the authored WASM build pass.
The expanded private package validates 147 runtime records/347 content objects,
350,272,729 resource bytes and no overlapping extents; four existing motorcycle
archives retain their hashes and use virtual LBAs after source resources.
Multiplayer and solo selectors skip archives absent from an older validated
manifest during the separate application/content publication stages.

## Public release verification (2026-10-13)

The all-40 venue app passed main CI/deploy at `3b08d61`
([run 38487888220](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38487888220)).
The required runtime package was published separately through the approved R2
manifest. The package validated 500 objects and uploaded 41 changes; the
manifest lists 147 runtime entries. Public HTTPS hash verification on both
[furyremastered.com](https://furyremastered.com/) and the
[Workers origin](https://atv-offroad-fury-2.champtoday.workers.dev/) matched 44
authored static files and 499 served content objects. The public gallery at the
time contained 195 entries and 149 screenshots. A later `4c0d90c` deployment
changed dev-log metadata only; map code and runtime content stayed the same.

On the deployed public app, a single Chromium sweep loaded and drove all 40
source venues. Every route was ready, all reported zero unsupported scenery
meshes, and the sweep ended without JavaScript or WebGL errors. Public browser
gates also passed the main title/menu and keyboard/controller routes, mobile
touch map selection, Tioga driving, pause, and Caribou reload without a disc
selection step. Six Sausalito/SX01 hay-bale approaches at 390px produced source
wheel-support and body-contact responses; actual touch input moved the ATV.

Two consecutive public two-client Sausalito/SX01 sessions passed with 13 remote
draws per client, remote rider models, movement replication (5.07 ft and
8.16 ft), and cleanup after host departure, without JavaScript or WebGL errors.
An earlier attempt timed out after both simulations initialized while waiting
for remote-render readiness; the cause remains unknown. The inspected public
screenshots in `research/devlog/entries/` show the 390×844 portrait ride and the
landscape host view with a P2 Guest marker; the guest vehicle is outside that
camera view. These are automated Chromium captures, not physical-phone photos.

Public verification proves the listed browser routes and connected venue
content, not full PS2 or whole-game equivalence. Original race rules, Training
objectives, hockey mode, complete weather/ambient behavior, crowd reverse
playback, lighting, water interactions, and remaining rendering fidelity are
still unfinished. Physical-device and geographically separated network behavior
remain unverified.
