# Vegetation physical proxies — USA v2.00

This component reconstructs ESB proxy definitions and their placement state from
`SCUS_972.11`. It does not establish complete game collision or public-release
parity by itself. The scene collector and consumer integration are documented in
`SCENE_COLLISION.md` and `SCENE_INTEGRATION.md`.

## Record decoding and shipped census

The original `00128ca0` loader reads nine file words per proxy: integer kind,
center XYZ, half extents XYZ, radius, height. It expands these into a twelve-word
runtime descriptor with center W and half-extents W set to one. Native species
slots are compacted in increasing file-slot order; proxy records retain source
order within each species. The existing ESB decoder preserves those orders.
`vegetation_proxy::Record::parse` interprets only fields consumed by the selected
constructor. Unused cylinder half-extents words in the real data include NaNs;
rejecting them would exclude valid shipped species.

The eight hosted freeride ESBs have the following actual definitions. Proxy
instances count every placement multiplied by its species' record count, before
any scene-query gate; these are not a count of simultaneous contacts.

| Environment | Species | Plants | Cylinder records | Proxy instances |
| --- | ---: | ---: | ---: | ---: |
| ENV01 | 8 | 37,948 | 6 | 28,913 |
| ENV02 | 15 | 39,641 | 4 | 10,184 |
| ENV03 | 11 | 26,530 | 0 | 0 |
| ENV04 | 9 | 13,816 | 7 | 10,793 |
| ENV05 | 24 | 14,883 | 7 | 5,609 |
| ENV06 | 9 | 38,968 | 5 | 21,205 |
| ENV07 | 7 | 6,617 | 5 | 4,761 |
| ENV08 | 8 | 18,325 | 6 | 13,527 |
| Total | 91 | 196,728 | 40 | 94,992 |

All 40 records are kind 1 cylinders. None are kind 0 model COL meshes, kind 2
spheres, or kind 3 boxes. The first shipped integration therefore needs no new
COL resource or R2 asset. Record kind 0 remains a separate model-COL resolution
requirement for any later resource that uses it; the typed enum alone does not
load that resource. ENV03 plants remain in the partition and candidate lists
although none has a physical proxy. ENV05 Maple has a second record with radius
and height both zero. It still contributes one proxy slot and native geometry;
its presence must not be dropped or replaced with a minimum-sized collider.

Record kinds and constructor runtime tags are different:

| ESB record kind | Constructor | Runtime actor kind |
| --- | --- | ---: |
| 0 model COL mesh | `0032e670`, `0032e900` | 0 |
| 1 cylinder mesh | `0032e670`, `0032eb70` | 0 |
| 2 sphere | `0032d840`, `0032d980` | 5 |
| 3 box | `0032aa50`, `0032ab60` | 4 |

In particular, ESB boxes must not be relabeled runtime kind 6 to enter a different
contact dispatcher. Constructor records for unshipped sphere and box kinds are
represented, but their physical contact parity requires method-specific proof.

## Cylinder construction

`001286f0` constructs 16 top vertices at authored height and 16 bottom vertices
at zero, then adds the local record center. It creates 32 outward side triangles,
two per sector, with no end caps. Sector `i` and `next = (i + 1) & 15` produce
`[i, next, next + 16]` then `[i, next + 16, i + 16]`. The source preserves this
input order before acceleration-tree construction.

The sector angle is `i * 0x3ec90fdb` in binary32. Its strict greater-than-pi wrap
precedes the original `001ab3c0` sine/cosine polynomials. The authored constructor
uses their coefficients and operation order, including separate rounded
multiply and add operations; host trigonometry changes vertex bits. Zero or
signed dimensions are retained. `Cylinder::mesh` returns the 32 vertex and index
arrays; native face normals and tree reordering are the next geometry stage,
covered separately below.

## Per-plant placement

`001280b0` uses final packed unsigned XYZ, each multiplied by the ecosystem's
extent/65536 coordinate unit. It does not resample Y. The scene's registration
has already applied any requested terrain resampling and packed-Y wrap; using
the earlier ESB position again would undo that native quantization.

The routine calls `00127d60` and `00127d20` with signed scale bytes and divides
the resulting width and height by the species' base width and height. Runtime
sphere kind 5 stores the two ratios at actor +128/+12c; runtime box kind 4 stores
them at +138/+13c. Both receive the packed world position.

Runtime mesh kind 0, including every shipped cylinder, **does not apply either
ratio**. Its VU instructions only copy the existing template matrix, replace XYZ
translation, and call virtual +9c. The basis and translation W remain unchanged.
Repeated placement of the shared species proxy replaces translation rather than
accumulating scale. `PlacementTransform` exposes the original ratios and position;
`mesh_matrix` follows this translation-only behavior. The consumer must retain
the native world/inverse pair. A synthesized scaled mesh or capsule would change
the source collision geometry.

## Record and placement verification

All binary execution reports and original-derived data remain ignored under
`private/reports/`; the authored source and this evidence summary contain no
proprietary binary or pseudocode.

- `work/physics/vegetation_proxy_loader_oracle.py` executes complete native
  `00128ca0` for all eight shipped ESBs: 76,649 original instructions, all 40 raw
  records matched to their expanded descriptor and ordered species slot.
  File access, allocation and the base species constructor are explicit service
  hooks; placement and geometry construction are outside this probe.
- `work/physics/vegetation_proxy_constructor_oracle.py` executes native
  `001286f0`, `0032e670` and the full `001ab3c0` polynomial: 240 cases, 763,680
  original instructions. These include all 40 shipped records and 200 synthetic
  finite, signed and zero cases. The Rust companion compares all emitted vertex
  bits and triangle indices exactly. Allocation, base actor/mode/registration
  services and the geometry-tree call boundary are hooked; this constructor
  probe does not claim to verify the tree or contact kernels.
- `work/physics/vegetation_proxy_placement_oracle.py` executes native
  `001280b0` and both signed dimension helpers: 600 runtime 0/4/5 cases, 82,400
  original instructions. The Rust companion matches every packed position,
  stored ratio and translated template-matrix bit. The runtime-mesh virtual +9c
  inverse service is hooked after its input has been captured. This checks
  placement rather than contact dispatch or response.
- Five committed unit regressions cover selected-field validation, the shipped
  zero-cylinder slot/side topology, native polynomial vertices, signed-byte
  ratios with unchanged mesh basis and repeated translation, and runtime tags.

These probes use the actual USA v2.00 instruction bytes in the project's finite
binary32 MIPS/VU interpreter. They establish the tested kernel contracts, not a
complete PS2 execution environment or full gameplay equivalence.

## Native tree and contact kernels

`vegetation_ray::cylinder_collision` prepares the existing `CollisionMesh`
representation from constructor vertices and indices. The original
`0032eb70 → 00119ec0` stage computes flat face normals and planes, applies its
fresh identity transform, then computes triangle centroids. `00119940` builds
31 internal nodes and reorders the 32 face records. Its split helper `00118420`
scores 18 candidate planes on each of three axes using child bounding volumes,
with the original initial choice, stable partition order, strict improvement
rule and balanced fallback. Node bounds use the source minimum 0.01 dimension
for the volume measure. Replacing this with an arbitrary balanced tree would
change first-hit traversal, capacity admission and equal-distance face ties.
The Rust builder retains the native preorder allocation and reordered face
indices. Zero-size proxies keep all nodes and face slots, with zero normals
and no emitted surface contacts.

Chassis actors use runtime type 2: `00329440` delegates type-0 mesh targets to
`0010c3f0`, whose `0010c140` traversal and nearest-hit insertion are the existing
`obstacle::query_bundle` kernel. Wheel and rider actors use runtime type 3:
`00327810` prepares a separate result set per object and traverses the same mesh
using `00117830/001148c0`. The shared `wreck::query_static_segments` preserves
world preparation, independently transformed start/direction, unchanged world
length, the native node prefix mask and nearest-face tie rule. Its callback rows
are emitted in ray-index order. `00193060` reconstructs each endpoint from the
prepared world direction and length; `00194820` enforces the solver row capacity.
Type-2 cumulative results across successive callbacks remain a scene-consumer
responsibility; the cylinder builder does not reset or reorder them.

Both native geometry paths form their inverse inline by transposing the basis
and separately negating the rotated origin. The actual placement instructions
`00128294–001282e0` copy the template matrix and store translation; there is no
hidden VU scale multiplication. The source does not use a general affine inverse
in these kernels. The shipped cylinder template starts with the identity basis;
visual width and height ratios must not be introduced into its physical matrix.

## Inclusive selection and strict segment geometry

The ordinary scene provider accepts touching query/object spheres; plant
admission similarly uses the horizontal actor/plant sphere gate. Those selection
rules do not replace the type-3 dispatcher's own three-dimensional geometry gate.
Both `00327810` and explicit-result helper `00121440` require
`dot(deltaXYZ, deltaXYZ) < (queryRadius + objectRadius)^2`. Exact equality rejects
geometry. The shared Rust segment kernel now applies this previously omitted
strict test, using the native root-center transform and unscaled half-diagonal
radius. `wreck::segment_query_sphere` exposes its query midpoint/half-diagonal
calculation for scene plant admission. This preserves the distinction between
inclusive candidate membership and strict geometry admission for ordinary props,
wheels, rider spokes and wreck-camera segments as well as vegetation.

Runtime tags are method-specific. Type-3 dispatch accepts mesh kinds 0 and 1;
ESB sphere runtime kind 5 and box runtime kind 4 return no hit. Type-2 dispatch
handles mesh kinds 0/1 and has a sphere branch for runtime kind 5, but does not
handle the ESB box's runtime kind 4. Its runtime kind 6 branch is a different
actor and must not be used as a substitute. Sphere geometry, composite-mesh
loading and model-COL resource resolution remain outside this shipped-cylinder
integration; no analytic sphere, capsule or cylinder contact was substituted.

## Tree and contact verification

The following additional probes execute actual USA v2.00 instructions through
the bounded finite-binary32 MIPS/VU interpreter. Their inputs and native exports
remain ignored; committed tests use authored synthetic fixtures only.

| Probe | Cases | Native instructions | Compared outcome |
| --- | ---: | ---: | --- |
| `00119ec0` and complete normal/bounds/split/tree helpers | 24 | 67,204,836 | All 12,072 tree and packed-face words match |
| Type-2 `0010c3f0` plus type-3 prepare, wrapper, tree, callback and row append | 96 | 7,763,806 | All 9,762 result/callback words, counts and order match |
| Type-3 strict wrapper sphere boundary and target-kind dispatch | 40 | 4,442 | Predecessor of distance 2 enters mesh; equality/successor reject; kinds 4/5/6 never enter mesh |

The contact probe includes 216 chassis hits, 627 segment hits and 405 actual
callback rows across capacity 1, 5 and 60. It uses the natively built 31-node
cylinder trees, zero-size geometry, vertical open-cap rays, varied ray ordering,
oblique rigid transforms and translations up to track scale. Type-2 comparison
is in mesh coordinates before the wrapper's world conversion; type-3 comparison
includes world hit positions/normals and the endpoint/point/normal fields stored
by actual `00194820`. Geometry is executed, not hooked. Builder allocation/free
and constructor-array allocation are supplied services; segment memory copying
and clearing are explicit hooks. The strict-boundary probe alone hooks the mesh
callee to no-hit to isolate whether the wrapper enters it.

Reproduction commands on the host, with the private executable present:

```sh
python3 work/vegetation-kernel-probe/native_builder.py
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/vegetation-kernel-probe/Cargo.toml --bin builder --target-dir work/terrain-contact-target
python3 work/vegetation-kernel-probe/native_contacts.py
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/vegetation-kernel-probe/Cargo.toml --bin vegetation-kernel-probe --target-dir work/terrain-contact-target
python3 work/vegetation-kernel-probe/strict_gate.py
```

Private reports are `vegetation-builder-native.json`,
`vegetation-builder-comparison.json`, `vegetation-contact-native.json`,
`vegetation-contact-comparison.json` and `vegetation-strict-gate-native.json`
under `private/reports/`. Three focused cylinder regressions cover a native tree
checkpoint, faceted side contact with open caps, and preserved zero-size slots;
a fourth shared-segment regression covers strict sphere equality plus a real
surface contact. The focused tests and strict core Clippy pass. Live scene
wiring, browser interaction and public deployment require the release gate;
these kernel results alone do not establish full handling parity or a tire
clipping fix.

## Live consumer integration

`SceneCollision::physical_selection` expands supported cylinder meshes in the
original mixed ordinary-node/plant order and species proxy order. Final registered
packed coordinates replace template XYZ translation. The quad and wheel supplied
list uses its existing strict cached-distance trim and `0011f9a0`'s inclusive
horizontal plant-sphere admission. The active wreck rider uses `0011fd30`'s
inclusive cached-distance trim and current ordinary-object exclusions, without
adding the supplied-list plant gate. The boundary query bit skips plant expansion
while retaining the shared cache's plant membership.

The chassis keeps one type-2 nearest accumulator across ordinary objects and
proxies and appends cumulative successful callbacks up to the original 60-row
capacity. Ordinary sphere admission remains separate from vegetation dispatch.
Wheel and active wreck-rider type-3 requests reset nearest per object/proxy and
retain the global row limit. Wheel support preserves the native final-successful
plane override and terrain material. Counters expose queried plant proxies and
successful chassis, wheel and rider callbacks; candidates alone do not imply a
physical collision.

A further 48-case `0011f9a0` native probe executes 23,592 instructions. It checks
horizontal predecessor/equality/successor admission, actor query-bit skipping,
MUD/current exclusion behavior and two proxy slots interleaved between ordinary
objects. Distance, eligibility and plant helpers execute; reset, geometry, matrix
setter and callback services are supplied to isolate dispatch order. The script
and report remain under ignored `work/physics/` and `private/reports/`.

Synthetic integration regressions cover mixed-list ordering, supplied versus
cached-rider plant gates, boundary skipping without removing cache entries,
preserved zero-size/open-cap geometry, cumulative two-proxy chassis callbacks,
chassis velocity/contact response and attached wreck-rider response. A separate
read-only native integration review found no material dispatch mismatch. Normal
riding rider collision, exceptional categories/custom masks, dynamic objects,
unshipped sphere/box/model-COL contacts and full gameplay replay remain unfinished.
No visible vegetation change or tire-clipping fix is inferred from this gate.

Host original-content contact test `tools/test_vegetation_contact.mjs` drives with
normal keyboard throttle, without replacing simulator state. On ENV04, step 484
records one vegetation chassis callback and one obstacle-contact row, five proxy
queries, two selected plant candidates, zero ordinary candidates and zero
unsupported proxies. Physics remains finite and browser/WebGL errors are absent.
The hit is still Riding; this route establishes a physical tree contact, not a
verified tree-triggered wreck or wheel/rider tree impact. The ordinary failed
Nac Nac separation/ground/recovery/driving route passes separately.

Straight-throttle boundary routes now time out on both ENV04 and ENV03. ENV04
records repeated wreck/deflection; ENV03 has no physical plant records and becomes
stranded tilted with no grounded wheels after one recovery. No causal claim is
made about the new vegetation geometry from these routes. Boundary launch
kernels remain source-verified, but a controlled normal-input browser route and
broader recovery investigation are needed. Full-game parity remains unclaimed.

The corrected normal-keyboard steered ENV04 boundary route passes launch,
separation, paired expansion flags and inward automatic recovery across 2,126
frames. Its trace contains 24 chassis-plant callback frames, one wheel-plant
callback frame and eight active wreck-rider plant callback frames, so all three
physical consumers have positive original-content browser coverage. It includes
553 boundary-flight frames and 710 cached rider frames. These trace counts are
not a full collision/gameplay parity claim. The first steering harness used the
wrong geographic yaw correction sign; only the corrected route is passing.
The straight-route limitations above remain recorded.

The serial host build also passes ENV01–08 loading/driving, desktop/portrait
statistics, saved one-handed controls and the touch stunt route. Full-batch clean
landing retains 0.393066ft tire overlap across 201 riding frames with none skipped.
An ENV05 startup timeout on the original five-slot HTTP backlog has no JavaScript
errors; a fresh uninterrupted eight-map run passes on a 128-slot host backlog.
A separate 48-request authored-module burst passes and private/source paths stay
404. This improves host-test admission but does not prove the prior stall's cause
or establish public verification. Production uses Cloudflare's own HTTP serving.
