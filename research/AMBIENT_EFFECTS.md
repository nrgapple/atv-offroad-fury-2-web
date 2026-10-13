# Placed environmental effects

Strategy: append original common and mode TOY emitter records, resolve their
source attachments, translate each observed emitter family separately in Rust,
and adapt original atlas sprite packets to bounded WebGL instance draws. Keep
original resources in the separate R2 runtime package. A declaration, active
simulation and original-game equivalence are separate gates.

The all-venue FR/TR/SO census finds 105 emitter declarations and 435 actor
animation records across 40 venues. These are source records, not 105 active
browser effects. The private report retains common and mode sidecars separately.
FREE06 FR adds 13 dust-wall records, Arlington FR adds locomotive smoke, and
Coonce SO adds four hat emitters. Training objectives and hockey callbacks do
not become implemented by loading those archives.

## Static type2/type8 component

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

A private finite interpreter executes 300 original type2/type8 birth cases:
119,615 EE instructions and 6,216 compared Rust flags/words have zero differences.
The active static attachment gate and original RNG service are supplied. This
is a finite kernel comparison, not full native scheduling or PS2 hardware replay.
The optional authored test reads private cases through ATV_AMBIENT_NATIVE_CASES;
ordinary CI contains no source-derived fixture payload.

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

GPU fountain streams (type11), blowing smoke/cloud/dust (type0), hat effects
(type9), fog (type12), moving attachments and other actor animation families
remain explicit diagnostics at this component gate. Timed/random emitter modes,
visibility callback scheduling, lighting, GS transparency ordering/fog precision
and original scene-level manager sharing remain incomplete.

## Host verification

All nine existing map routes load and drive without JS/GL errors. The dedicated
ENV07 natural-camera gate verifies source volcano smoke changes actual pixels
at desktop and 390×844, with bounded draws, pause/resume, ATV reset and graphics
context restoration preserving state. Screenshots remain under ignored private
storage. Browser timings and automated portrait checks do not establish sustained
physical-phone performance or complete native environmental parity.
