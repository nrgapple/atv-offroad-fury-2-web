# Combined scene candidate bridge

The baseline strategy is to connect the recovered USA v2.00 loader, spatial
partition, shape-distance providers and shared candidate-cache policy without
changing their ordering or inserting a substitute vegetation collider.
`scene_collision::SceneCollision` supplies that bridge. Vehicle/WASM wiring and
actual-map/public verification are separate integration gates; this module alone
does not establish live collision parity.

## Construction and registration

`SceneCollision::new` receives original partition configuration, successful
ordinary `StaticObstacle` placements in loader order, an optional decoded ESB,
a force-height flag, and a terrain-height service. The ordinary registration stage
precedes every ESB placement. One `StaticPartition` contains both classes; its
iterator can interleave them. Separate prop and plant query results must not be
concatenated or distance-sorted.

Ordinary registration uses `StaticObstacle::world_bounds`, while its distance
provider uses the transformed local root center and the **local** half-extent
diagonal. Scaling the placement changes registration bounds but does not resize
that distance sphere. Plants use the greatest partition X/Z extent divided by
65,536 for packed coordinates, with independently rounded 65,536/extent for
height requantization. Packed positions do not add partition minima. Tagged or
forced resampling receives the current packed XYZ position, requantizes sampled Y
through signed32 truncation/unsigned16 wrapping, then registers the resulting
bounds. Untagged placements are not sampled when force-height is false.

Every placement registers, including species with no configured proxy.
`RegisteredVegetation` exposes its final coordinates, height, width radius,
registration bounds and configured proxy count. A proxy count is metadata; it
does not mean the mesh/cylinder/sphere/box proxy has been implemented.

`from_terrain` supplies the recovered `Terrain::contact` height. If no cell
exists, it retains the supplied packed Y before requantization, matching
`0016ac10`'s early-return behavior. The main ecosystem call `00200900` passes
proxy-enable one and force-height zero to `00129640`; tagged ESB plants still
resample. Callers should pass false for that loading path.

## Consumers and state

`cached_query` owns one `CandidateCache(40,50)` shared by quad, active wreck rider
and recovery.
It receives a world query center, the caller's native sphere radius and an
ordinary eligibility predicate. Refresh uses the combined tree's native query
at forty-foot half extents; reuse updates distances without changing membership
or order. Plants use horizontal separation minus width radius, with provider
query radius zero. Ordinary nodes use their root sphere separation, also with
provider radius zero. Both classes consume the same 256-entry capacity.

`CachedQuery::candidates` exposes all retained typed nodes and their distances.
`strictly_within` applies the native quad/recovery strict sphere-radius gate;
`static_selection` provides the corresponding ordered ordinary obstacle view.
Vegetation remains visible in the retained list and is never silently converted
into ordinary mesh collisions. `vegetation_blocks_checkpoint` implements
`001ea050`'s plant exclusion over that retained list: separation must be strictly
below twenty, proxy count must be nonzero, and base X/Z proximity squared must be
at most four hundred. Altitude and actual proxy intersection do not enter this
checkpoint gate. Call it with the recovery query at the candidate and radius20.

The normal quad caller must transform its saved local center every frame and
supply its **fixed bind-bound half-extent diagonal** radius. `001e1fd0` obtains
those bounds through `00167a00` before the later Model sphere-bounds flag change;
recomputing a radius from animated aggregate bounds, COM or the AR ray hull
changes native cache invalidation. The adapter does not infer that radius.

`set_ignore_vegetation` changes collector field `+82c`. It does not force a
refresh. A false-to-true change during reuse retains existing plants; a true
refresh removes them, and a subsequent false reuse does not restore them.
This field is distinct from actor query bit `bf.0`, including the boundary
physical-query flag controlling vegetation proxy expansion. Do not automatically
couple those two flags without verifying the original caller.

`direct_query` runs the combined partition separately and never modifies the
shared cache. It supplies ordered spatial candidates and caller ordinary
eligibility; actor sphere/ray overlap, callbacks and physical proxy expansion
remain the consumer's responsibility. Wreck camera uses its own direct bounds.
The rider initializer `001ee580` supplies no quad count/list. Constructor
`00193720` and setup `00193830` leave query `+70`/`+74`/`+78` zero, so an
unbound actor initially takes `00193b40`'s direct `0011f2c0` branch. This is
initialization state, not the ordinary attached rider's lasting query contract.
In the normal attachment branch of `001e4c90` (quad entity flag `+8 & 0x10`
clear), `001ee630` installs quad `+3b0` at rider `+50` and query `+78`.
`00193b40` checks `+78` before its count/list and direct branches. Attached,
airborne, ground and swimming wreck phases therefore all use `0011fd30` with
that shared cache; `001f0740` recovery retains the binding. Phase changes do not
select the direct query path.

The bound cached-rider contract uses inclusive distance and current ordinary
`0011eeb0` eligibility before geometry/callback dispatch. It must not substitute
a strict quad trim or direct query. Attachment adds the special `MUD` category
to the rider actor's exclusion list. Vegetation proxy expansion separately
requires actor query bit `bf.0` clear. The authored active-wreck animator now
borrows the Vehicle's exact scene cache, including its initial attached wreck
phase and subsequent separated phases. Ordinary riding does not yet run this
rider collider update. Direct fallback remains available when no scene binding
is supplied. Special `MUD` metadata transport, dynamic registration and
exceptional object/category filters remain separate work.

## Evidence and limits

The preexisting mixed-tree replay has 30 four-frame sequences (120 frames,
1,779,241 original instructions). Its adapter membership/order, radius and
numeric distance comparisons pass, but **it retains zero vegetation entries**
throughout those frames. Its randomized plants were too far from the queries;
that fixture establishes ordinary behavior with vegetation registered, not
positive retained-plant filtering or interleaving. Earlier wording in
`SCENE_COLLISION.md` about that fixture's vegetation exclusion should be read
with this limitation.

A targeted variation executes the same original partition/collector routines
with nearby plants, smaller nearby ordinary footprints, and collector flags
false → true → true → false across sphere radii three → twenty → three → three.
All 120 frames match the bridge's membership/order, radius and distance bits
after 1,779,297 original instructions. They include 464 retained plant entries
and 210 ordinary entries appearing after plants, establishing mixed ordering and
flag changes during refresh/reuse. This remains finite synthetic input; node
allocation services preserve native fields, and physical vegetation proxy
expansion is not exercised.

The bridge also matches all 600 preexisting complete vegetation registration
cases (153,400 instructions): 1,800 final packed coordinates and 3,600 registered
bounds components match bit for bit. Those cases supply raw reads, terrain
heights and tree-registration services, while the original coordinate assignment
and species-dimension code executes. The 27-case source loader-stage replay
(11,232 instructions) passes ordinary-before-ecosystem callback order. An
additional eight source `00200900` calls (352 instructions) verify the
proxy-enable/force-height arguments; allocation, constructor, string comparison
and `00129640` are services in that narrow call-site proof.

A narrow rider caller replay executes 8,808 original instructions through
`001ee580`/`00193720`/`00193830`, normal `001e4c90`/`001ee630` attachment,
`001ef690`/`00193b40` in all four wreck phases, and `001f0740` recovery.
It checks initializer-only direct selection, the shared cache pointer's exact
provenance, cached selection in attached/separated phases, and binding retained
after recovery. Allocation, actor construction, point transforms, geometry
dispatch, force/integration and registration/filter/animation services are
explicit hooks. The separate 40-case complete `0011fd30` replay (13,392
instructions) verifies inclusive distance, current ordinary eligibility and
`MUD` exclusion, plus vegetation query-bit gating; the collector and geometry
tests are services there. These prove caller/dispatch contracts, not a complete
coupled rider scene query or original gameplay capture.

Core regressions cover shared quad → recovery → quad membership and distinct
consumer trims; exact refresh equality; mixed original order; root-diagonal
versus transformed bounds; altitude-independent plant distance; source-order
height resampling and proxyless registration; missing terrain cells; strict
separation/inclusive checkpoint proximity; shared capacity; and independent
direct queries. These checks do not establish loaded-map candidate equivalence,
physical vegetation collision, complete category eligibility or whole-game
parity. Source/probe data stays under ignored `work/scene-integration-probe/`,
`work/physics/` and `private/reports/wreck/`.

## Live quad and recovery gate (released as `f27869c`)

The WASM transport now reloads the original ESB bytes after terrain initialization
and passes recovered Environment partition metadata with collision placements.
`Vehicle` registers the combined scene once, retains the fixed bind AABB-center
query sphere, and uses the original ordered static view for wheel support and
chassis callbacks. The same cache services recovery at radius twenty; configured
plant proxies reject unsafe checkpoints through the native clearance rule.
The debug overlay exposes cached/selected prop and plant counts. Registration
and checkpoint exclusion do not implement physical plant-proxy contacts.

The combined host build passes 245 core + 2 WASM tests and strict Clippy. Two
Vehicle regressions compare empty-cache and legacy physics across 120 flat plus
120 slope steps, and exercise actual settled driving checkpoints near configured
and proxyless plants, including the strict separation and inclusive circle edges.
An uninterrupted ENV01–ENV08 load/drive run confirms the live cache and finite
physics with no JavaScript/WebGL errors. Clean landing, failed Nac Nac separation/
contact/recovery/driving, uneven turning, normal ENV04 boundary launch/recovery,
one-handed mobile and desktop/portrait diagnostics pass. The same paths passed
publicly on `f27869c`, CI `38210131335`; HTTPS verification matched 26 authored
files and 317 served runtime/gallery objects before its verification entry.
This is live integration, not loaded-scene original replay equivalence. Rider and
camera consumers were separate follow-up gates described below and in
[camera evidence](CAMERA.md). Physical proxies and complete category/dynamic
behavior remain unfinished.

## Active wreck rider cache component

`Vehicle::scene_collision_mut` supplies the same owned scene/cache to
`Animator::step_with_scene` after the quad update. No cache is cloned. All active
wreck phases, including initial attached separation, use the current thirteen
spokes' bounds center and half-extent diagonal as their actor sphere. Selection
is inclusive at the actor radius, and current explicit ordinary-index exclusions
are applied both during refresh and before geometry on reuse. Default transported
ordinary static nodes have the recovered enabled/queryable setup; exceptional
category/object-list metadata remains unavailable. The sixty-record contact
capacity and existing per-object ray reset/order remain intact. Recovery/new
wreck entry do not replace Vehicle scene membership. The normal riding rider
collider update is not connected by this active-wreck gate.

Source `00327360` radius instructions `00327798`–`003277a8` accumulate
`(X²+Y²)+Z²` before square root. Five hundred native thirteen-spoke endpoint-copy
and preparation cases execute 1,112,500 instructions; the authored center/radius
helper matches every compared binary32 bit. The bound-result reset service is a
hook, and endpoints are supplied already in world space; this does not repeat
animated-node transform or full geometry/force parity claims.

The native `001e5260` boundary block writes `bf.0` on both quad `+97c` and the
rider actor at `(((quad+884)+24c)+40)+38`, then clears both after its strict
one-second gate when game `+674` permits. Twelve focused native cases execute
1,508 instructions and verify paired writes/late clearing. The bridge passes
that paired boundary flag to the rider without modifying collector `+82c`.
Initial attachment's global `+674` policy remains unverified; the port retains
its preceding default policy. Configured plant proxies are exposed as unsupported
in rider diagnostics when expansion is permitted, and generate no physical
contacts. Their presence still consumes shared cache capacity.

Four new core regressions check 120-frame static prop contact/motion equivalence
against direct fallback through detach, recovery query and repeated wreck entry;
current ordinary exclusion on cache reuse and the resulting shared membership
after rider refresh; exact-radius inclusion; and plant-query-bit changes without
cache membership loss. All sixteen wreck animator tests pass, including existing
dry/wet and body-cache regressions. The actual-content 1,694-pose/five-direction
recovery probe passes with both direct fallback and the shared-cache path, with
finite cached actor spheres at every active frame. That cached variant uses
original rider clips/terrain and an empty ordinary/vegetation partition; static
prop contact/cached equivalence is covered by the synthetic core fixture, not
an original loaded prop scene in that variant. Both WASM tests pass. Public browser validation and
complete original coupled rider/cache/geometry replay remain subsequent gates.
`MUD` exclusions are supported through explicit verified metadata, but current
command49 omits placement names and the original game `+584` base-name provenance;
this gate does not infer them from a collision filename.

## Combined rider/camera host browser gate

The combined build passes 251 core + 2 WASM tests, strict Clippy/formatting,
39 JavaScript tests, 10 release tests, four Worker tests, setup and repository
hygiene. The serial real-touch Nac Nac route confirms cached finite rider bounds
through attached, detached and grounded wreck frames; its camera reports direct
finite scene bounds and an executed obstruction query. The normal ENV04 edge
route confirms shared rider queries and paired actor vegetation bits through
launch, late clearing and recovery. Clean preload landing, one uninterrupted
ENV01–ENV08 load/drive run, diagnostics and saved one-handed touch checks pass.

The full-batch landing capture contains 201 riding frames, zero skipped, minimum
clearance -0.39306640625ft. It is a different trace from preceding releases and
does not establish improvement or resolve clipping. Three simultaneous host
startup runs timed out with no browser errors; a fresh single startup and the
serial suite passed. Host concurrency remains unverified. These are authored
browser integration checks pending public release, not original loaded-scene
candidate replay or whole-game/hardware parity. Physical plant proxies, normal
riding rider collision, MUD metadata and full custom/dynamic eligibility remain
incomplete. Wreck-camera source and direct-query evidence is in [CAMERA.md](CAMERA.md).
