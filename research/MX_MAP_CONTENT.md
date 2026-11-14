# MX venue runtime content

Build locally, without upload:

```
python3 tools/package_mx_maps.py --output private/reports/mx-map-staging
python3 -m unittest discover -s tests -p test_package_mx_maps.py
```

The 56 archives total **197,915,253 bytes** and **23,752 members** in the
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

The final typed audit has 14,454 resolved edges, 205 aliases, 3,026 excluded
shader references, 29 generated-target markers and 34 inactive TOY template
markers (`NumberOfStaticModelParams=0`, unused `Model1 SLT=?.slt`). It retains
18 ambiguous bindings and 7 actual-missing bindings across venues. The latter
are `ring_trigger.col`, `nat07_churchsnow.col`, `natdropgate.col` (three venues),
`nat10_tray_water.col` and `nat_tray_runnoff.col`.

Ambiguous evidence includes original F/M collider pairs (trash/recycling cans,
dinosaur eggs, mailboxes, single bale), segmented gate/sphinx colliders,
`Anaheim_poleCol.col`, and texture naming variants: `.dxm` in the livery stable
binding and spaces in the Nat12 sandstone/pillar bindings. Related source
assets remain preserved when already present; the audit never silently assigns
one candidate's geometry or texture to another binding. Source `.pco` files
and their connected collider payloads remain available to the runtime.

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
