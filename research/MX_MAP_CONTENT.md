# MX venue runtime content

Build locally, without upload:

```
python3 tools/package_mx_maps.py --output private/reports/mx-map-staging
python3 -m unittest discover -s tests -p test_package_mx_maps.py
```

The 56 archives total **198,032,437 bytes** and **23,786 members** in the
validated staging run. Each packed member is round-trip compared to its source
by `pack_resource`; the independent private test reopens all 56 packages,
checks SHA256 and length, extracts every member, checks terrain presence and
rejects executable extensions. Source build identification uses the existing
motorcycle source hash. No executable bytes enter packages.

`research/mx-map-catalog.json` contains authored integration metadata: `id`,
`source_stem`, `display_name`, `group`, `archive`. IDs are namespaced (`mx_fs01`),
scene stems lowercase, and archive paths uppercase (`/MX/MAPS/FS01.RES`).
Display names come from General/DisplayName in TrackList.res's 56 atylist
members. Group labels summarize the source FS/MC/Nat/SM/SX categories.

Packaging preserves original main-archive supported resources, including
scenes, original TOYs, terrain, models, colliders, material/texture data and
original connected `.ps2.cub` skies. It follows explicit file references,
PSG fixed-width material tables and PSM TextureMap bindings recursively into
the disc directory. Original paths are preserved. Shader `.vuo` files are
excluded, including recursively referenced shaders. Their behavior belongs
in authored Rust. No ISO, executable, module or reverse-engineering output
is packaged. Unrelated archives are not recursively imported.

Private `map-packages.json` now carries a typed `dependency_audit` per venue.
Completeness uses SCN/TOY/PCO configuration fields, PSG material tables and PSM
texture/dependent-material bindings, never filenames guessed from opaque binary
bytes. Every edge records origin, reference, kind, status and resolved path or
candidate evidence. `source_stem` is lowercase in both catalog and report.
The resolver checks exact and relative paths before basename lookup, checks
members already inside the venue, then top-level PSR records and a cross-venue
nested-member inventory. Nested payloads retain their original paths.

Source texture animation convention: PSM TextureMap stems may bind binary
`.atm` files rather than `.pst`. This resolves `seating_blue`, `seating_red`,
`holeshotlight`, `leaderlight1/2`, `triplelight_A/B/C/D`, flags and fountain
bindings. Original ATM bytes are packaged without interpreting them as static
textures. The source `Screen.psm` RenderTarget binding is explicitly
`dynamic_generated_target`; Rust callbacks can consume this marker. Generic
CUB/TRN references resolving to existing `.ps2.cub`/`.ps2.trn` are
`platform_alias`, as are PST-to-ATM animation bindings.

Physics collection metadata lives in each private report row's
`physics_collections`: origin/section, wildcard pattern, source active count,
status, matching model stems and `bindings[].roles`. Each active binding has
independent `actor_whiskers` (M) and `collision_target` (F) references, original
paths and resolution statuses. Source functions 002171b0/00218280 honor the TOY
count gate and wildcard prefix matching; 004922a0 derives both suffixes, not
aliases. Original F/M payloads are included for active matching models.
NAT01 trash/recycling, NAT06 eggs and NAT11 SingleBale resolve both roles.
NAT11's active banner pattern has no matching scene models, so no model binding
is invented. NAT10's twelve authored collection sections have count zero: all
are explicitly inactive, with no derived role bindings. Existing original
payloads are preserved without activating those sections. NAT06's actual TOY
count is ten (three eggs, six wood bodies, Meteor); metadata honors that source
count rather than truncating the collection list to just the audited eggs.

The final typed audit has 15,122 resolved edges, 205 aliases, 3,026 excluded
shader references, 29 generated-target markers, 34 inactive TOY template
markers, six generic-to-physics-role records, four authored texture repairs,
two inactive mailbox collision records and one nonphysical marker. No binding
is left classified as ambiguous. MC09 Ring_Trigger has SCN HasCol=False and is
marked nonphysical; it does not acquire collision.

Four texture bindings have explicit `authored_repair` metadata, with
`repair_target` preserving the source PST name and `source_equivalent=false`:
MC05/NAT05 livery `.dxm` spelling and Nat12's two internal-space spellings.
Unique original target resources are packaged; neither PSM nor PST is renamed
or rewritten. Native source equivalence is not claimed.

Twelve records remain `source_missing`: Anaheim whole poles (FS05/SX09),
whole start/drop gates (Nat08/Nat11/Nat16), Nat12 whole sphinx, Nat07 church
snow, and Nat10 tray-water/runoff. SCN already places the valid segment
colliders separately; those are never substituted for missing whole objects.
Source 00319b40 tolerates individual object-load failure and continues. The
church and tray/runoff physical gaps remain fidelity limitations. NAT10
mailbox sections are classified inactive, not repaired by enabling bodies.

The parent may initially consume F targets as static collision compatibility.
This package establishes payload roles, not movable-body simulation parity;
that dynamics limitation must remain disclosed at runtime integration.

`classify_previous_warnings(previous, current)` accounts for earlier lexical
warnings against typed evidence. The private staging run records this as
`legacy-reference-classification.json`: opaque terrain tokens are false
positives, not dependencies. Unresolved references and source naming candidates
remain explicit fidelity limitations for the parent runtime integration.

Optional `--integrate-manifest` updates an existing **staging** manifest. It
preserves other records, replaces only these map paths, assigns virtual sectors
past retained files, verifies blob hashes, and atomically replaces manifest.json
under the shared publication lock. Production hosted-content output is refused
pending runtime acceptance. Bike regeneration uses the same lock and atomic
replacement and preserves map records. No upload is performed by these tools.
