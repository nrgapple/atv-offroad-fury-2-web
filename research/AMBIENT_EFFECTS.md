# Placed environmental effects

Strategy: append original common and mode TOY emitter records, resolve their
source attachments, translate each observed emitter family separately in Rust,
and adapt original atlas sprite packets to bounded WebGL instance draws. Keep
original resources in the separate R2 runtime package. A declaration, active
simulation and original-game equivalence are separate gates.

The all-venue FR/TR/SO census finds 105 raw emitter sections and 435 actor
animation records across 40 venues. Native NumberOfEmitters indexes 80 declarations: NAT06's
13 additional cloud sections and Training's 12 objective sections are dormant
(the lesson rules themselves are in [TRAINING.md](TRAINING.md)).
Declarations can expand to multiple attachments; neither count means active
browser effects. The private report retains common and mode sidecars separately.
FREE06 FR adds 13 dust-wall records, Arlington FR adds locomotive smoke, and
Coonce SO adds four hat emitters. Training objectives and hockey callbacks do
not become implemented by loading those archives.

## Static type0/type2/type8 component

Original `002427b0` appends successive TOY declarations. `00240eb0` reads
TypeID, attachment or position, instance count and StartOn (default true).
`00241120` resolves matching model instances up to that count; `00242120`
constructs each native emitter family and applies its activation. `0019a680`
transforms the authored local attachment offset through the model basis.
The port resolves currently loaded static scene attachments and records each
unresolved model name/count. Unresolved instances become actor placements that
stay dormant until a moving actor supplies its pose. Arlington's Smoke.slt is
the only one in shipped content, so no phantom static emitter appears. Moving
type0 displacement terms, the 128-slot actor reserve and their native gate are
described in [TRAIN.md](TRAIN.md#locomotive-smoke).

Static type8 fire/smoke follows `0019e150`: source rate × dt × 0.6, strict
fractional emission gate with discarded remainder, four lifetime phases,
three atlas sets, source X/Z emission spread and optional mirrors. The static
path derives velocity from emitter movement (zero here), then adds the authored
Y upward noise. It does not apply invented horizontal smoke drift. Type2
waterfall/wheel/duct/spring spray follows `0019d870`: source velocity spread,
random lifetime/growth, speed gate, original sequential atlas frames and mirrors.
`00143aa0` supplies age/growth/movement/gravity/ground bounce and set animation;
`001451a0` supplies the 64/128 packet alpha and fade/nonlinear-growth rules.

Static type0 blowing smoke/dust follows `0019c200`: its authored minimum/maximum
emission, size and lifetime ranges, nonlinear lifetime noise, independent XYZ
velocity noise, atlas frame selection and native fractional accumulator. That
accumulator retains its source behavior below the strict one-particle threshold.
The static birth path does not set type2/type8 random mirror flags. Moving
emitter VU alignment remains unconnected.

Original `0019a680` controls delay, one-shot, random on/off and timed modes;
the constructor's first dirty frame does not emit. Random on/off ranges consume
the same ambient generator as particle births. Strict time boundaries and the
source timed-mode range selection are preserved. Actor visibility/event
callbacks are outside this static route.

A private finite interpreter executes 300 original type2/type8 birth cases:
119,615 EE instructions and 6,216 compared Rust flags/words have zero differences.
The active static attachment gate and original RNG service are supplied. This
is a finite kernel comparison, not full native scheduling or PS2 hardware replay.
The optional authored test reads private cases through ATV_AMBIENT_NATIVE_CASES;
ordinary CI contains no source-derived fixture payload. Another 300 static
type0 cases execute 155,442 instructions with 7,272 compared words/flags and
zero differences. Five hundred timer cases execute 24,425 instructions with
2,000 compared state words and zero differences. Date-based native seeding and
the native shared global service are supplied/initialized in those finite gates;
the browser uses the deterministic independent map generator described below.

The ambient manager uses a separate source RNG generator seeded with 1 for each
loaded map. Native global RNG ordering with wheels, audio and gameplay is not
reconstructed. World particles advance once per fixed simulation step in the
same worker response as vehicle/camera/water/train/material clocks, including
motorcycle batches. Repeated rendering, pause and display culling do not emit
particles. ATV reset preserves their map clock and particles; loading another
map replaces them. CPU sprites are bounded at 600. The browser keeps logically
active emitters running outside view and culls only rendering, with source fog
range, stable descending depth and at most two blend-family instance draws.
An original 128×512 atlas plus instance/corner buffers uses 286,192 GPU bytes.
This is an additional GPU texture copy; decoded atlas bytes share tire sources.

Moving attachments other than type0 smoke and other actor animation
families remain explicit diagnostics at this component gate (type9 hats and
gameplay-started bursts: "Triggered bursts" below). Visibility callback scheduling, lighting, GS transparency ordering/fog precision
and original scene-level manager sharing remain incomplete.

## Static type11 fountain streams

Original `0019f580` emits the ceiling of source rate × dt streams. Six RNG values
per stream drive correlated XYZ velocity spread, lifetime, growth, and rounded
stream length. Authored separation, section thickness, bright head, tail lifetime
and velocity evolution are translated separately from CPU smoke. A finite gate
runs 200 original birth cases: 490,018 EE instructions and 44,460 raw-word
comparisons match. It supplies the active static gate, initialized GPU manager,
original seeded RNG and suppressed flush threshold; it excludes full VU/GS replay.
The reciprocal-square-root rounding matches that finite interpreter contract.

`001d1910` packs signed 16-bit velocity, lifetime, size and growth. The browser
consumes the quantized values, source color/alpha, GroundLevel − 5 and the original
8-column/16-row UV selection, including the native 0.51 V offset. The decoded
128×256 source GPU atlas is reused with wheel resources. The existing recovered
mode2 VU contract (`001ad460` → `003b9740`) supplies ballistic motion, minimum Y
plane, size growth and lifetime alpha. GLSL adapts those equations; no complete
hardware raster/precision comparison is claimed.

The fountain initializes a 400×12 ring, distinct from wheel spray's 900×12 ring.
All ten source attachments run on the fixed simulation clock. New birth packets
upload in contiguous ranges, with at most two updates when the ring wraps; one
instanced draw ages/culls at most 4,800 slots on the GPU. An expired zero-lifetime
birth clears its slot rather than preserving overwritten particles. A bounded
retained typed buffer restores all slots after graphics loss. Fountain-only GPU
storage is 515,120 bytes including atlas and corners; it allocates no CPU sprite
renderer. Logical births remain independent of display distance.

Native shared manager batching with wheels, global RNG order, camera-dependent
fog-plane overrides, moving emitters, and complete VU/GS precision remain outside
this component. The authored packet budget rejects configurations that could
produce more than 4,800 births in the maximum accepted 0.1-second step.

## Triggered bursts: Treasure Hunt bombs, Freestyle cherries, hats, fireworks

Exact-build evidence (private Ghidra exports; bounded reading, no runtime
traces):

- **Per-TOY emitter numbers.** `002427b0` appends every loaded TOY's
  `[ParticleEmitterN]` sections. Before each TOY's sections load,
  `001fe070` stores the count already declared (manager +0x30) at +0x3e4;
  the `Particles` action loader (`0023f3c0`) and Hockey's
  `[HatTrickEmitters]` loader (`00335de0`) add it, so an `EmitterNumber` is
  local to its own TOY. Race fireworks (`002229c4`) read it the same way.
- **The action.** Trigger action type 11 is `Particles` (`0023fb30`) with
  `EmitterNumber`, `InstanceNumber` and `TurnOn` (default on). Its enter
  callback (`0023f460`) queues the instance's start (`0019a3c0`: vtable
  +0x88, +0x80, +0x20) or stop (`0019a420`: +0x84, +0x18) for emitter
  types 0, 1, 2, 8, 9, 11, 12 and 13. Only actions within the trigger's
  `NumberOfActions` load, so Treasure Hunt's authored fourth action (stop
  `bombsparks.emt`) never runs.
- **Start/stop.** +0x88 is `0019ac30`: DelayTime, then by EmitMode the
  OneShotTime (mode 1, clearing the emitted flag), a random on-time (mode 2)
  or TimedTimeOn (mode 3). +0x80 (`0019abc0`) switches the emitter on with a
  zero accumulator; +0x20/+0x18 (`00105d90`/`00105d50`) set or clear its
  active bits. +0x84 (`0019abe0`) switches it off; type9's override
  (`0019f300`) lets a running one-shot finish. The EMT loader (`0019ad90`)
  already sets the mode-1 remaining time, so a `StartOn` one-shot bursts
  once after the constructor's dirty step.
- **Treasure Hunt.** Each `envNNth.toy` declares one type11
  `bomb_explosion.emt` with `AttachToSoultree=bomb.mcf`,
  `NumberOfInstances` = the treasures and `StartOn=F`. `00241120` attaches
  instance N to the Nth keyframed animation whose MCF matches (after the
  static models), so it follows that bomb's model node. Each treasure
  trigger runs `HideAnimation`, `Treasure` and `Particles` (that bomb's
  instance). `bombsparks.emt` is declared in a section beyond
  `NumberOfEmitters` and ships in no archive: no sparks exist.
- **Freestyle targets.** Each `gcp`/`rcp` target TOY declares one static
  type11 `cherryg_explosion.emt`/`cherryr_explosion.emt` per target at the
  trigger position; the target's `Particles` action starts it. The first
  emitter is `StartOn=F` and the others are `StartOn=T`, so those burst once
  as the course loads (usually far from the start); the port keeps that.
- **CPU streams.** The burst EMTs (and every Supercross firework EMT) lack
  `UseGpuParticles` (0x260, default off). Type11 then runs `001a0130`
  instead of the GPU path `0019f580`: the same stream construction, written
  as CPU atlas particles (frame +0x194 advancing per stream within the
  group; growth drawn before life; X/Y mirrors gated by `RandomMirrorAbout*`),
  stopping the whole emission when the manager is full. Both paths emit
  max(EmitRate × dt, `MinParticlesPerFrame`) streams: 50 per step for the
  bursts, 500 for firework explosions. The burst groups are single orange
  (bomb), red and green spark frames of each venue's Particles.prt.
- **Hockey hats.** `game01so.toy` declares four type9 `hats*.emt`
  (continuous, DelayTime 0.5 s, 1 hat/s, 4 s life, GroundLevel 91,
  restitution 0.2) listed under `[HatTrickEmitters]`. `003360f0` builds a
  ParticleGroupController from them; setup (`00335590`) deactivates it, a
  hat trick (`003368c0`) starts it (`0020eb90`) and the pause's end
  (`00336200`) and Restart Game (`003377f0`) stop it (`0020ec20`). Type9
  births (`0019eb60`): the native accumulator, at least
  `MinParticlesPerFrame`; X/Z spread; authored velocity plus Y noise when
  `ApplySpreadToVelocity`; a random group frame (or a forced set); life
  shortened by a shared serial (×1, 0.875, 0.75, 0.625); `AgeAtBirth`.
- **King of the Hills.** No KH TOY declares an emitter or `Particles`
  action, and the capture (`00220b50`) only tints the flag, re-ranks and
  shows the sign: there is no capture effect to draw.

Port: `ambient_effects::plan` tags each placement with its TOY (common,
mode or the objective's target TOY, appended third) and TOY-local number,
and resolves MCF attachments from the loaded TOYs' `[Animations]` in order.
`Effects::trigger` applies the start/stop above. A Treasure Hunt collect
(`Rules::take_triggered`) starts its bomb's instance at the bomb's current
posed position; a Freestyle hit starts its target's emitter; the Hockey
session starts/stops the hat group. Game loads now append the game's own
mode TOY (the original's), not the Freeride TOY the overlay archive also
carries. Type11 without `UseGpuParticles` becomes CPU sprites in a separate
900-slot stream pool (static 600 + 900 = the native 1,500-slot manager), so
a burst cannot starve placed smoke; a full pool stops the burst and counts
one drop. Supercross fireworks take the same CPU path and now draw their
original coloured spark frames instead of tile 0 of the fountain GPU atlas.

A private sweep (`every_treasure_and_target_explosion_resolves`) loads all
14 Treasure Hunt TOYs and 26 target TOYs from their archives: all 581
`Particles` actions name an emitter of their own TOY whose EMT and atlas
group resolve, bomb instance N is the bomb that trigger hides, and each
start draws sprites at its trigger (one ENV04 green target's emitter is
authored 2 ft away). The finite private birth, stream, timer and fog gates
still match unchanged.

Host browser gate `tools/test_engine_toy_explosions.mjs` (content group):
Treasure Hunt collects on Prospect Springs (`gameTest=visit`) and a green
target run (`objectiveTest=targets`) each start a burst whose stream-pool
sprites are drawn additively and end within ParticleLife; Hockey with
`hockeyGoal=3` (three teed drops) reaches a hat trick, the four hat
emitters throw hats after their delay and the next face-off stops them.
State is published as `#ui-state` `ambient` (starts, live per pool, drawn
sprites). On Supercross the pre-race explosions keep the 900-slot stream
pool full while they run (most sparks are far and sub-pixel), as the native
pool would be; the start-gate streams draw as coloured sparks. Screenshots
stay in ignored private storage. These are host checks, not physical-phone
or PS2 comparisons.

Open: the explosion follows the bomb's pose at the collect rather than its
node for the 0.1 s burst; the `001a0130` stale-update skip (+0x1a0 > 0.25 s),
StartingZone/zone gating, MovingEmitter stream scaling and the native shared
pool/global RNG order are not reproduced; the 0x400 forced-set list of
type9 is reduced to its two-set default. No hardware comparison exists.

## Host verification

All 40 venue routes load their actual common/mode FX declarations exactly once,
including NAT05's split terrain header and GAME01's partial mode-sidecar spawn.
Particle placement reads static models independently of gameplay starts. Every
frame is finite and stays within 600 particles/two sprite draws. FREE06 reaches
the cap; dropped births remain visible in diagnostics (94 in the recorded gate). The dedicated
ENV07 natural-camera gate verifies source volcano smoke changes actual pixels
at desktop and 390×844, with bounded draws, pause/resume, ATV reset and graphics
context restoration preserving state. The FREE06 type0 gate uses an explicit test-only start beside a source static
grill attachment; it verifies framebuffer contribution and the same lifecycle
checks on desktop and portrait. The shipped gameplay spawn is unchanged.
The ENV05 fountain gate likewise uses a test-only source attachment start;
it verifies center/directional stream framebuffer contribution, a wrapped
4,800-slot ring, at most two birth uploads, pause/reset, and restored GPU-buffer hashes identical to the retained birth buffer, including
pending uploads after the last worker response. Screenshots remain under ignored
private storage. Browser timings and automated portrait checks do not establish sustained
physical-phone performance or complete native environmental parity.

## Weather integration gate

The combined weather/ambient checkpoint retains both optional batch fields and
advances ambient and source camera Snow on the same successful fixed step.
Component configuration follows physics initialization; context restoration
rebuilds renderer resources from retained simulation/birth state. Reset/pause
input can synchronously clear interpolation, so the current render ends before
reading a cancelled presentation frame.

All 40 routes load source ambient declarations and weather selection metadata
without JS/GL errors. The shorter declaration gate complements the earlier
80-step source catalog gate. Seven real rain lifecycle cases cover active/dry
NAT06 and SX13 selection, Freeport ATV/motorcycle audio, pause/reset and graphics
restoration. Source Caribou Race Snow contributes 36,995 desktop/42,616 portrait
pixels, preserves all frozen Snow-only framebuffer bytes after restoration,
and retains its clock across physical vehicle reset. ENV05 ATV and motorcycle
fountain lifecycle/buffer gates also pass in the combined build.

The 147-record weather content package changes only the repacked AUDIO.RES from
446 to 447 members, adding rain.vag. The earlier map package is insufficient for
Rain audio. These are host integration checks; public delivery is a separate
gate owned by the release coordinator. Freeport fog12 is connected in the later host checkpoint below.

The same configured camera-relative Snow manager now steps in motorcycle
batches using their existing camera position/target. The motorcycle
Caribou Race gate passes at desktop/390px: 42,569/46,540 Snow pixels, 60 rings,
27 active emitters, one draw, 508,016 GPU bytes, and exact frozen restoration.
Its physical vehicle reset preserves the scene clock. Snow resolves the scene
GPU material/texture when a motorcycle's tire atlas is present; it does not use
that vehicle atlas as Snow. Original ATV inputs and supplied scene atlas are
unchanged.


## Persistent Freeport type12 fog volume — host checkpoint

The indexed Freeport common TOY fog declaration now loads its original EMT and
CPU atlas group. `001a1070`/`001a12e0` initialize 200 persistent sprites across the
source MinRange/MaxRange volume, with original random sizes, frame choices,
mirrors and per-sprite oscillator noise. Construction initializes the volume
before its first fixed step; `001a0d00` sets the placement before EMT loading.
EmitRate does not create a continuous smoke plume.
The native maximum of ParticlesInVolume/MinParticlesPerFrame is bounded by the
existing 600-slot CPU pool, with dropped initialization slots reported.

`001a0f30` advances the source triangle oscillator. Its phase is computed before
the strict endpoint clamp, retaining the one-step overshoot and 1/1000 endpoint
offset. The persistent branch of `00143aa0` keeps age fixed and adds phase × noise
to position without a dt multiplier. Source Freeport age/lifetime remains 0.5,
producing 32/128 alpha units. The persistent branch of `001451a0` fades that alpha
with squared XYZ camera distance inside the source 20-foot radius. The renderer
preserves finite float operation order for this distance calculation. Existing
source scene fog, depth sorting and two blend-family draws remain in use.

Private instruction gates pass 60 constructor cases (1,537 particles), 80 clock
sequences (8,000 updates), 300 persistent motion cases and 300 near-alpha cases:
660,486 executed finite EE instructions and 45,181 raw field/state/normalized-alpha
comparisons, with zero differences. The clock gate supplies the base emitter
callback; motion and alpha gates isolate their persistent instruction blocks.
These do not recover full source manager scheduling, shared global/date RNG
ordering, lighting, GS transparency precision or physical-phone performance.
The port retains its independent map RNG seed1 and fixed simulation timestep.
No raw source fixtures or instruction exports are included in Git.

The host Freeport gate uses an explicit test-only start beside the original fog
placement. An isolated fog draw changes 417,711 desktop and 202,553 portrait
pixels; counts vary with capture timing. The 200 fog sprites retain age, size,
atlas frame and mirrors after multiple lifetimes, while their positions follow
the oscillator. Placed rain shares the remaining CPU slots. Rendering uses one
instance draw and 286,192 GPU bytes with no extra fog texture or particle pool.
Pause, vehicle reset and graphics restoration preserve the full source world
frame; resuming continues its clock. All 40 venue routes pass declaration and
weather loading with finite bounded frames. This is a host component gate;
the same lifecycle gate also passes on motorcycles at desktop and portrait
(575,622/271,167 isolated fog pixels in that capture). `make check` passes
437 Rust tests and 135 JavaScript tests (five private Rust instruction gates and
one private JS alpha gate are opt-in); formatting, strict Clippy, WASM build,
`make verify` and 35 release tests (one skipped) pass. Public release remains
owned by the release coordinator. Moving Arlington locomotive smoke was later
connected (TRAIN.md). Callback-gated hockey hats, zone-visibility gating and
other animated actor families remain explicit gaps.
