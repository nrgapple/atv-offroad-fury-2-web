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

GPU fountain streams (type11), hat effects
(type9), fog (type12), moving attachments and other actor animation families
remain explicit diagnostics at this component gate. Visibility callback scheduling, lighting, GS transparency ordering/fog precision
and original scene-level manager sharing remain incomplete.

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
Screenshots remain under ignored private storage. Browser timings and automated portrait checks do not establish sustained
physical-phone performance or complete native environmental parity.
