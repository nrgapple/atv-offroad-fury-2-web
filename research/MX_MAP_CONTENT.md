# MX venue runtime content

Build locally, without upload:

```
python3 tools/package_mx_maps.py --output private/reports/mx-map-staging
python3 -m unittest discover -s tests -p test_package_mx_maps.py
```

The 56 archives total **195,634,102 bytes** and **24,205 members** in the
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

Private `map-packages.json` provides hashes, byte counts, member counts,
excluded shaders and unresolved references per venue. Missing or unsupported
bindings are retained as evidence, not fabricated. Examples include dynamic
`rendertarget.pst`, light textures, seating textures, and collider names absent
or ambiguous in the directory. Generic `.trn` references can name an existing
platform-specific `.ps2.trn`; some short apparent filename tokens in binary
source data may be false positives. This lexical dependency audit does not
prove every opaque binary reference has been decoded. Runtime fidelity and
rendering acceptance remain the parent's gate. In particular, this checkpoint
does not assert that missing textures/colliders have working replacements.

Optional `--integrate-manifest` updates an existing **staging** manifest. It
preserves other records, replaces only these map paths, assigns virtual sectors
past retained files, verifies blob hashes, and atomically replaces manifest.json
under the shared publication lock. Production hosted-content output is refused
pending runtime acceptance. Bike regeneration uses the same lock and atomic
replacement and preserves map records. No upload is performed by these tools.
