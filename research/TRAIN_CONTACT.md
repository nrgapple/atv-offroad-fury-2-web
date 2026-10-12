# Moving train contact components — USA v2.00

This gate supplies the authored procedural actor's velocity/contact arithmetic
and the dynamic list used by Arlington's scene. Live train collision integration
and encounter verification remain separate work. Existing original COL resources
provide the train geometry; no synthetic collider or launch trigger is introduced.

## Actual scene provider

`001fef10` has an optimized single-region path when static visibility and collision
search types are both QuadTree. `00201f60` defaults those types to QuadTree, and
the shipped Arlington scene supplies no search-type override. This path attaches
the shared static VisibilityQuadTree through `001a2810`, while `001a2880` attaches
the separate `001abae0` dynamic list with a 1,024-slot allocation block.

Ordinary static placements register with flags1; procedural train actors register
with flags2. `001a2a40` selects region+34 or region+38 accordingly. The combined
iterator `001a8830/001a88f0` exhausts the static provider before the dynamic list.
The list returns every active actor, regardless of supplied spatial bounds.
Eligibility, distance and geometry remain later consumer stages.

`001abc40` supplies a stable slot key, and `001abd20` stores its actor. Movement
reuses that key and `001a23f0` performs no membership update. `001abd60` puts a
removed slot on a LIFO free list. Registration reuses the most recently removed
slot before allocating a new slot. `001abde0` scans slot blocks in allocation
order, skipping removed slots. `train::DynamicList` preserves those semantics.

A generic second QuadTree is supported by other scene configurations; it is not
Arlington's actual dynamic provider. Its separate private comparison is not used
to justify the shipped scene bridge. Candidate-cache refresh/reuse retains its
existing rules: no actor-motion invalidation is added. Eligible nonstatic actors
may be retained beyond the cache radius, and reuse updates their distances without
replacing membership or order.

## Contact ownership

Type2's nearest geometry rows remain cumulative across successful callbacks.
`00193060` visits all current rows and `00194820` assigns **the current callback's**
category (+44) and body (+48) to each newly appended solver record. The nearest
geometry row does not preserve its original body's ownership. Earlier appended
records retain their assigned owner. The global solver capacity remains60.

For example, static → train → static callbacks with one, two and three cumulative
rows append body owners `[none, train, train, none, none, none]`. The train callback
can therefore assign its point velocity to inherited static geometry. The bridge
must attach provenance to newly appended ranges, not to the nearest accumulator.

## Procedural velocity and response

`002db170` retains prior and current model transforms. `002daea0` returns their
translation difference multiplied by the engine's supplied reciprocal step.
`002dafe0` sums cross products of prior/current forward and prior/current up,
then multiplies by that reciprocal step. It does not reconstruct axis-angle speed.
`001953d0` evaluates the target's world angular cross contact arm plus linear
velocity before subtracting it from the querying body's point velocity.

The train constructors retain zero mass, reciprocal mass and inverse inertia.
The zero-inertia inverse `00325ac0` returns false and leaves the initially zero
inverse matrix unchanged. For the querying body, `MotionState::resolve_contact`
reuses the verified contact solver with that relative point velocity and the
moving-target reflection input. `001e3b80` supplies the same configured reflection
to the static and moving-target branches. Native target force/torque side effects
are outside this wrapper; the procedural route pose does not integrate those
forces. This component does not establish complete train interaction parity.

The shipped body PSG roots and COL local transforms are identity. Constructor
`0011b660` → `002dacc0` → `002dad70` initializes both cached transforms to identity.
A separate startup replay executes the actual base-cache update on the first four
route ticks. It retains the original large first-tick velocity from origin to the
route, rather than forcing zero initial velocity. The probe supplies the engine
reciprocal step explicitly as `1/f32(1/60)`. The engine scheduler's production of
that global value is a separate clock contract; getter arithmetic alone does not
prove it. The authored fixed-step runtime uses that same supplied-step convention.

All eight recovered train COL files are type0 with identity local transforms.
`00330420` copies the model world transform and `003302b0` composes it with the
COL local transform. A train callback `0011b650` is empty; no scripted ramp launch
or wreck trigger is established by that callback. Contact shape, solver behavior
and later vehicle wreck decisions require the live encounter gate.

## Native comparisons

| Original instruction scope | Cases | Instructions | Comparison |
| --- | ---: | ---: | --- |
| Dynamic-list constructor, mixed static/list registration, movement, removal, reinsertion and combined iterator | 8 scenes; 96 moves, 24 removals, 32 reinserts | 529,076 | All returned orders match; movement slot keys remain unchanged |
| Type2 callback ownership and 60-record capacity | 12 mixed sequences plus 80 saturated callbacks | 115,531 | All appended geometry tags, categories and body pointers match |
| Procedural linear/angular velocity getters | 1,000 | 155,000 | All 6,000 compared words match |
| Target point velocity, normal impulse and friction with zero-mass procedural target | 500 | 1,221,151 | All 9,000 querying-body/relative-velocity output words match |
| Actual procedural constructor/cache startup and first four route ticks | 4 × 12 bodies | 430,738 tick/getter instructions, constructors additional | All 1,824 pose, prior-pose and velocity words match |

Finite-binary32 instruction probes retain original instructions, reject unknown
opcodes and supply explicitly documented engine/allocation/file services. They
are not original hardware or whole-game execution. Private reports remain under
`private/reports/train/`; no original resources or pseudocode are committed.
Authored tests cover moving-target push, both orientation axes, LIFO slot reuse,
and the 1,024-slot boundary. Route, wheel and existing driving tests remain intact.

Private reproduction uses `work/train-probe/native_list.py`,
`native_ownership.py`, `native_motion.py`, `native_contact.py` and
`native_startup.py`, with Cargo companions `--bin list`, `motion`, `contact` and
`startup`. Core validation:

```sh
cargo test -p atv-core --lib train::
cargo clippy -p atv-core --all-targets -- -D warnings
```
