# Placed environmental effects

Strategy: append original common and mode TOY emitter records, resolve their
source attachments, translate each observed emitter family separately in Rust,
and adapt original atlas sprite packets to bounded WebGL instance draws. Keep
original resources in the separate R2 runtime package. A declaration, active
simulation and original-game equivalence are separate gates.

The all-venue FR/TR/SO census finds 105 raw emitter sections and 435 actor
animation records across 40 venues. Native NumberOfEmitters indexes 80 declarations: NAT06's
13 additional cloud sections and Training's 12 objective sections are dormant.
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
unresolved model name/count, including Arlington's moving Smoke.slt actor.
A moving actor is not replaced with a phantom static emitter.

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

Hat effects (type9), fog (type12), moving attachments and other actor animation families
remain explicit diagnostics at this component gate. Visibility callback scheduling, lighting, GS transparency ordering/fog precision
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
gate owned by the release coordinator. Source fog12 remains unconnected.

The same configured camera-relative Snow manager now steps in motorcycle
batches using their existing camera position/target. The motorcycle
Caribou Race gate passes at desktop/390px: 42,569/46,540 Snow pixels, 60 rings,
27 active emitters, one draw, 508,016 GPU bytes, and exact frozen restoration.
Its physical vehicle reset preserves the scene clock. Snow resolves the scene
GPU material/texture when a motorcycle's tire atlas is present; it does not use
that vehicle atlas as Snow. Original ATV inputs and supplied scene atlas are
unchanged.
