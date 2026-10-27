# Keyframed TOY actors

The venue TOYs place animated props: crows, seagulls, sharks, fish, whales,
monkeys, crocodiles, windmills, blimps, airliners, gliders, twisters, a
lighthouse, a sawmill and the stadium roofs. Each is an `[AnimationN]` record
with `AnimationType=KeyFramed`. The Rust engine now loads, animates and draws
these records on every venue route. Procedural records (the Arlington train)
are described in [TRAIN.md](TRAIN.md). The particle emitters that share the
TOY files are described in [AMBIENT_EFFECTS.md](AMBIENT_EFFECTS.md).

Status: host component. Placement, playback and drawing follow the recovered
loader and update. The actors' looping engine sounds play through the train's
body-attached sound path (see [Loop sounds](#loop-sounds)); their samples need
a content upload before the public site plays them. Trigger-driven
sequences, moving collision and zone visibility are not connected (see
[Not connected](#not-connected)).

## Recovered behavior (USA v2.00)

Addresses are from the exact executable. Facts come from Ghidra exports of
these routines. Inferences are labeled as such.

- **Container.** `0023d9f0` reads `[Animations] NumberOfAnimations` and
  appends records across successive TOY files: the venue's common TOY, then
  the mode TOY. `0023dcb0` sets type 1 for KeyFramed and type 2 for
  Procedural. `0023db90` treats any other type as keyframed.
- **Record reader** (`0023c760`, class `KeyFramedModel`, vtable `00459210`).
  It reads these keys:
  - `ReferenceName` and `NumberOfMotions`.
  - `MotionToPlay%d`.
  - `NumberOfSequences`, default 1.
  - Per sequence: `NumberInSequence%d` (default 1), `MotionSequence%d`
    (comma list, every entry 1 when absent), `ProbabilityList%d` (every entry
    100 when absent), `BlendSequence%d`, `LoopSequence%d`,
    `SequenceActive%d` and `NameOfSequence%d`. Booleans default true.
  - `0023c1e0` reads Offset, Look, Up, Yaw, Pitch and Roll.
  - `0023bfc0` reads MCF, UseCollision (default 1), UseLighting (default 1),
    IsGlobal and StartingZone.

  Values go through the shared TOY reader `00141fc0`: a leading T/F, ON/OFF, a
  leading Y/N, otherwise `strtol` with base 0. `UseBlendedKeys` occurs nowhere
  in the executable. `OneTimeOnly` is read only by the trigger reader
  (`0023ff80`). Neither key affects actors.
- **Model and motions** (`0023cdc0`, `0011e1d0`, `001ca7c0`, `001cb0d0`). The
  `anim\<mcf>.mcf` `[General info]` names the model through `SLTFile`/VUT. The
  PSG stem can differ from the MCF stem: `747_01` uses `747_02`, `blimp_04`
  uses `blimp_06` and `monkey03` uses `monkey02`. Each `[MOTION n]` is entered
  once, by `NAME`, in a global motion library. `MotionToPlay` values are these
  names: seagulls play `crow.mot`. The MOT header overrides the MCF's
  `FrameCount`, duration, rotation encoding and position encoding. The MCF
  keeps `Looping`. The MCF `FPS` is stored but never read during playback:
  time is in seconds and is compared directly with the MOT key times.
- **Placement.** Offset becomes the translation of an engine root node above
  the PSG nodes. `00164dd0` builds the basis from Look (+Z) and Up, with
  X = Up × Look. If Look is absent or zero, `00141150` builds the basis from
  Yaw/Pitch/Roll; no shipped record sets those keys, so it is the identity. The
  node table that MOT channels address starts below the engine root
  (`0013c390`), so the MOT never moves Offset/Look/Up. Version 3 MOTs bind by
  node name. Version 2 MOTs bind by PSG node index. The MOT root translation
  (the crows' flight path, the blimps' circuit) is therefore local to the
  placement.
- **Start.** The current sequence is the first one with `SequenceActive`,
  otherwise sequence 0. The actor is posed at t=0. Construction makes no RNG
  call: every actor starts at phase 0, so identical props move in step. An
  inactive starting sequence holds the t=0 pose, and the actor is still drawn.
- **Update** (`0020d0b0` → `0020cba0` → `0013d7a0`). The update is skipped
  while the game is paused. Time advances by the actor frame dt. Past the
  duration, a looping motion subtracts the duration once. A one-shot motion
  holds its last pose and stops. The sequence logic then restarts it on the
  next frame if the sequence loops; otherwise it stays frozen. Sequence
  changes use a shared per-frame `rand()` roll against `ProbabilityList`,
  blend over `TransitionTime` when `BlendSequence` is set, and cut a
  differing next motion `TransitionTime` before its end. Every shipped actor
  except the SX10 roofs has one motion in one active looping sequence.
- **No gating** in the update. It has no distance, visibility or LOD test and
  no dependence on mode. Drawing and spatial handling go through zone
  registration (`StartingZone`, `0011e520`/`0011e5c0`). No further draw
  distance rule was traced.
- **Triggers.** `AnimationTriggerAction` (`0023dec0`, fired by
  `0020c910`/`0020d260`) clears the target's `SequenceActive[0]` when it links,
  and starts it on contact. `AnimationSequenceChangeTriggerAction`
  (`0020c940`) switches by `NameOfSequence`. `ResetAnimationTriggerAction`
  (`0020d720`) restores the initial state. `HideAnimationTriggerAction`
  (`0020c990`) disables the actor; Treasure Hunt's bombs use this.

## Loop sounds

Facts:

- The keyframed blimps, planes and the helicopter carry
  `NumberOfLoopingSounds` and `[AnimationNLoopSoundM]` sections with
  `SoundResourceFile`, `Looping`, `MinDistance`, `MaxDistance`,
  `StartDisabled` and `AttachToObject`. These are the same keys as the
  procedural train's loops. The executable reads the count and formats
  `%sLoopSound%d` in one shared reader, `00205640` (strings at `0042a0e0`
  and `0042a0f8`). That reader allocates 16-byte elements (constructor
  `00204dd0`) and calls each element's reader through its vtable.
- `AttachToObject` names a node of the actor's PSG: `blimp`, `p_plane`,
  `tour_plane` or `chopper`. Every shipped actor sound names an existing node.
  The train's sounds use `Body`.
- `00173b20` and `00173cc0` allocate a sound emitter (0x1f0 or 0x310 bytes),
  optionally attach it to an object (`001c1840`), and append it to the sound
  manager's emitter list at +0x7c (`001745f0`). The manager update `001736f0`
  walks that list.
- The census below finds 36 actor loops over the 72 routes (each venue's
  common TOY is counted for both its Freeride and race routes). There are
  26 `soultree\b\blimp.vag` loops on 12 blimp models, 6 `soultree\p\plane.vag`
  loops on `p_plane` and `tour_plane`, and 2 `soultree\h\helicptr.vag` loops on
  `chopper`. All loop, none start disabled, and all resolve their node.

Implementation:

- `atv_core::toy_actors` reads the sounds. A sound without a sample or with
  an inverted distance interval is skipped.
- `Actors::sound_emitters` places each attached node at its posed position
  for this step and the step before. A missing node falls back to the actor
  root, but no shipped record needs that fallback.
- `RideAudio` plays these sounds through the train's code path. That path
  uses the original stereo, `MinDistance`/`MaxDistance` attenuation and
  Doppler commands (`spatial_audio::command`), restarts a finished one-shot,
  and keeps loops through Reset and pause.
- The page's audio diagnostics report `actorSounds` (defined, samples
  present and voices playing) and `events.actor`.

Inference: the attenuation is assumed to match the train's. Both use the
same keys and the same shared reader. The element vtable (`00456bc4` family)
and the per-frame setter were not separately decompiled for the actor case.

Packaging: `tools/package_content.py` now adds every keyframed record's
loop sample from each packaged track archive's TOYs to `/AUDIO/AUDIO.RES`
(`selected_actor_loop_names`). The three samples are `soultree/b/blimp.vag`,
`soultree/p/plane.vag` and `soultree/h/helicptr.vag`. They are not in the
current public package (`5dde4eb8…`), so the browser stays silent there until
the package is rebuilt and uploaded. Missing samples are skipped without
error.

## Census (all routes)

`crates/atv-game/tests/toy_actors.rs` loads the 40 Freeride/Training/
Mini-game venue routes and the 32 National races from the private hosted
package, using the same `venue::load_with` path as the browser. It found:

- 724 actors in total. Every MCF, PSG and MOT resolves and every motion binds
  to its model, with 0 unresolved.
- Every actor poses finitely. All of them move over 1.5 s except the SX10
  roofs, whose sequences are inactive.
- No model has more than 19 nodes, below the pose capacity of 32.

When the mode TOY is the common TOY itself (Freeride on National and
Supercross venues), its records are read once.

The loader leaves two kinds of record to the systems that own them:

- Treasure Hunt's bombs, which its trigger rules pose and hide.
- `loco_wheels`, which the train poses.

Routes per venue family:

- ENV02: 26
- ENV03: 30
- ENV04: 1
- ENV05: 28
- ENV06: 19
- ENV07: 36
- ENV08: 2 water wheels
- FREE01: 1
- FREE04: 52
- FREE06: 9
- NAT01–NAT08: 20–26 each
- SX venues: 1–2 each
- GAME01: 2

ENV01, FREE02, FREE03, FREE05, NAT07, SX09, SX11 and Training declare no
keyframed actors. Shared resources are decoded once per venue: FREE04's 52
actors use 5 models and 5 motions.

## Implementation

- `atv_core::toy_actors` contains the record and MCF readers, the starting
  sequence rule and the playback clock: looping wraps, a one-shot motion holds,
  and an inactive sequence holds t=0.
- `atv_game::actors` loads the actors per venue load. It shares models and
  motions and runs the scene clock on the fixed 60 Hz world step. Poses are
  cached for each (model, motion, time) on each step, so the 25 crows of a
  venue cost one MOT sample.
- The `Venue.actors` field holds this state outside the GPU passes, so a
  restored graphics context keeps it.
- `atv-render`'s actor pass merges every model's geometry into one vertex
  buffer and one index buffer. Each visible actor writes one rigid-node pose
  slot. The pass culls actors by their posed world bounds against the view
  frustum.
- The pass uses the scenery material rules: texture, prelit vertex color,
  cutout, GS source alpha, additive or alpha blend, the ATM frame clock for
  the stadium streakers, and GS fog. Opaque passes draw before blended ones.
- The pass draws after the train and before vegetation and water. This order
  puts the river crocodiles under the water surface.
- Pause stops the clock because the ride's world steps stop. ATVs,
  motorcycles, races, Freestyle objectives and mini-games all use the same
  venue load and renderer.

No new runtime resource is needed. Every archive the actors read is already in
the public R2 package with identical bytes.

Cost (host only, not a physical-phone result):

- **Draw calls.** FREE04 from a fixed overview adds 3–4 draws: 81–82 against
  78 on production `da5c0be`. Frame CPU is within noise: 2.27–2.77 ms
  against 2.15–2.41 ms, two alternating runs each, Chromium Metal, M4.
- **Native posing.** Posing every actor with nothing culled takes 20–110 µs
  per step on most venues and 190–370 µs on ENV07. ENV07's cost comes from
  its 19-node monkeys, which share no poses.

## Not connected

- The SX10 stadium roofs' trigger-driven open/close sequences
  (`AnimationTriggerAction`, `ChangeAnimSequence`) are not connected. The roofs
  hold their t=0 pose, as the original does before a trigger fires.
- The actor loop sounds are implemented (see [Loop sounds](#loop-sounds)),
  but their samples await a content upload. The emitter's own per-frame
  setter was not decompiled, so the train's recovered attenuation is reused.
- Moving collision from `UseCollision` is not connected. The windmills,
  sawmill and other actors with this flag load `anim\<mcf>.col` in the
  original and move it with the animation (`0011eca0`/`0011ee00`). Which node
  drives that transform was not decompiled.
- Zone registration and draw (`StartingZone`) are approximated by frustum
  culling. The port has no zone visibility.
- The original advances actors by variable frame dt. The port advances them on
  its fixed 60 Hz step.
- Sequence probability and blending are parsed but not stepped, because no
  active multi-motion sequence ships.
- Lighting uses the provisional prop lighting shared with the train.
