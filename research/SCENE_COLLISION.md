# Collision scene candidates

The baseline strategy is an authored port of the verified USA v2.00 spatial
registration, traversal and cache semantics. Candidate order affects cumulative
chassis contact rows, the first-row rider-height crash input, wheel support and
recovery. Preserve original registration and iterator order; do not introduce
nearest-distance sorting or a different spatial structure as a parity shortcut.

`scene_partition::StaticPartition` implements the immutable ordinary-static
part of VisibilityQuadTree's base QuadTree methods. Its configuration takes
original X/Z minimum and maximum bounds plus minimum division size. `001618f0`
quantizes the larger dimension to 32,768 units, builds at most twelve division
levels and retains binary32 accumulation order. `00161d50` selects up to four
cells for an object's X/Z bounds. `00162060` appends entries within a cell in
registration order, in blocks of four. The authored vector representation keeps
that ordering without copying native pointer storage. `00162b80` rounds query
bounds to minimum cells; `00162ed0`/`00163080` visit children in source order and
prepend selected cells. `00163120` emits each object's first occurrence. As a
result, returned order can differ from placement order even for static props.

The ordinary static placement loader `00206e90` uses mode `0x0d` and category
represented by `00462d38`. Mode bit `bf.3` is set; queryable bit `bf.2` is set.
CollisionObject and Vegetation class tags are registered at runtime; they are
not literal numeric values from COL headers. Native eligibility `0011eeb0`
requires a different target object, enabled entity bit `31.0`, target queryable
bit, intersection between query accepted-category mask and target categories,
and both objects' explicit object/category filter lists. The current ordinary
static transport does not carry those exceptional filter lists.

The native quad scene sphere is initialized from Model local center/half bounds
returned by `00167a00`, then transformed with the Model world matrix before the
outer query. It is distinct from the AR bundle and COM. Standard COL0 spatial
registration bounds come from its own virtual getter, which returns the
transformed local root center and axis-aligned transformed half extents.

`scene_candidates::CandidateCache` implements `0033e4b0`/`0033e350` policy. The
normal quad uses search half extents forty feet and a cache radius bounded between
forty and fifty feet. The caller supplies original spatial iterator order,
shape-specific distance and category eligibility. Refresh occurs when movement
from the cached center plus query sphere radius reaches the cached radius;
equality refreshes. On reuse, distances change but membership/order are retained.
Ordinary static nodes require distance strictly below cache radius. Nodes with
`bf.3` clear are retained when eligible even beyond it. Vegetation has a separate
path, is optionally excluded and otherwise consumes the same 256-entry capacity.

Query-cell bounds returned by `00162b80` are relative to the partition minimum;
`0033e4b0` compares them directly with world center. The authored modules retain
this original quirk. Default shipped environment bounds recovered from
`00202bd0` have minima zero, so that quirk does not affect their normal setup.
Environment configuration defaults to 96-foot divisions and maxima from terrain
width/breadth, scale and border, with explicit keys allowed to override them.

The scene parser now decodes Environment's `TerrainWidth`, `TerrainBreadth`,
`TerrainWidthScale`, `TerrainBreadthScale`, `TerrainBorderWidth`, explicit
`Xmin`/`Xmax`/`Zmin`/`Zmax`, and `MinDivisionSize`. All eight connected original
scenes use 96-foot divisions; seven span 5,760 feet in both axes, while ENV04 spans
4,608 by 5,760 feet. These values are metadata pending live partition integration.
`StaticObstacle::world_bounds` preserves `00330490`'s center transform and the
half-extent accumulation order. A separate 600-case probe executes 97,200 original
instructions; all 3,600 output component bits match Rust. It includes rotated and
non-orthogonal finite transforms. Probe data remains under ignored
`work/scene-bounds/`.

## Validation

The private bounded interpreter runs exact source instructions with finite
host binary32 arithmetic and explicit integer conversion. This is not a complete
R5900/PS2 floating-point emulator or an original gameplay capture.

| Check | Cases | Original instructions | Result |
| --- | ---: | ---: | --- |
| Initializer, registration and returned query order | 100 scenes, 3,600 registrations, 400 queries | 7,475,365 | Every packed key and returned index sequence matches |
| Candidate refresh/reuse and ordinary eligibility policy | 300 four-frame sequences | 1,308,598 | All cache membership/order, radius and distance bits match |
| Combined native tree → iterator → collector/cache | 30 four-frame sequences | 1,856,111 | All membership/order, radius and distance bits match, including translated minima |

The partition cases use synthetic translated, non-square regions and 16/32/64/128
foot division sizes, overlapping footprints, repeated-cell queries and whole-tree
queries. Allocation hooks preserve native node base fields and block contents;
registration, traversal, deduplication, distance and default collision eligibility
instructions execute. The standalone policy cases supply an ordered iterator and
cell bounds. The combined cases execute that iterator through the real partition.
Private reports are under `private/reports/wreck/scene-*`.

Core regressions retain a native order checkpoint, deduplication, out-of-region
registration/query behavior, X/Z-only partitioning, strict static radius, equality
refresh, reuse order, and vegetation capacity/exclusion semantics.

## Remaining integration and fidelity

The modules provide verified components; they do not establish live scene parity.
Integrate original configuration and per-object bounds, preserve actual loader
order, and use the quad Model sphere for cache invalidation and candidate distance
filtering. Camera direct queries use their segment bounds rather than this quad
cache. Camera mode two with query bit zero set excludes Vegetation expansion in
`0011f2c0`; normal enabled static props remain eligible under default masks.

Dynamic registration/removal, shared-tree concurrency, exceptional category and
object filters, region portals, vegetation collision expansion/distance providers,
and other search methods remain separate. In particular, vegetation geometry in
the renderer is not yet proof of native collision participation. Wreck rider and
recovery paths must use their own native query contract. Validate coupled driving,
rider contact and recovery on actual loaded scenes before claiming complete parity.
# Vegetation recovery exclusion components

The vegetation path is now traced separately from ordinary COL0 placements.
`00200900` initializes the ecosystem with collision proxies enabled. Species
records support a COL0 mesh (kind0), an open sixteen-sided cylinder built from32
side triangles (kind1), a sphere actor (kind2), or a box actor (kind3).
`001280b0` applies packed placement coordinates and signed height/width scale
ratios to those shared proxy actors before querying them. Physical proxy contacts
are still unimplemented; a visible plant model is not evidence of its collider.

`00129640` sets the packed coordinate unit to the greatest terrain world extent
divided by65536. `00129b70` registers each plant in ESB placement order with
center `(packedX*unit, packedY*unit+height/2, packedZ*unit)` and half extents
`(radius,height/2,radius)`. Height-resampled placements quantize their sampled
height back into the original unsigned16-bit coordinate before registration.
The placement/tree registration wrapper has been read; complete combined
ordinary/vegetation loader ordering remains to be validated.

The plant distance provider `0033e2e0` uses horizontal distance minus plant width
radius and query radius, clamped at0. Altitude does not enter this provider.
Candidate-cache refresh/reuse callers pass query radius0 to this distance
provider; query range is handled by subsequent cache/consumer gates.
`vegetation_collision::horizontal_separation` matches600 original cases and
15,000 instructions bit for bit, with no instruction hooks.

Recovery `001ea050` takes the retained cache candidates and requires cached
separation strictly below20. For Vegetation nodes, it checks configured species
proxy count via `00128090`; if any proxy exists, horizontal squared distance
from the packed plant base to the recovery candidate at most400 rejects that
checkpoint. It does not test actual proxy intersection or candidate altitude.
`vegetation_collision::blocks_checkpoint` matches600 complete wrapper cases
and157,058 original instructions, including actual `001280b0` sphere-placement
and dimension helpers. Ordered cache output and buffer clearing are explicit
service hooks. This helper expects already selected native cache candidates;
testing all plants without preserving cache/grid scope would broaden behavior.

Three authored regressions cover the distinct strict cache gate and inclusive
twenty-foot circle, absent proxies, height-independent distance, and coordinate
scaling. The helpers are a component gate, not live vegetation collision or
whole-map recovery parity. Private probes remain under `work/physics/` and
`work/vegetation-checkpoint-probe/`, with generated data under `private/reports/`.
