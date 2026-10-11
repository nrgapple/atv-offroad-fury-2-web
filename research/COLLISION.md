# Original collision mesh recovery

Evidence is from USA v2.00 `SCUS_972.11` and its disc resources. The Rust parser is a format reconstruction, not copied game code. Private audit: `private/reports/collision-audit.json`; temporary probes and extracted examples: `work/mesh/`.

The disc contains 6,544 COL member entries, representing 649 SHA-256-distinct files. 604 use type 0, 38 use type 2, and seven use type 1. Of the 604 type-0 files, 603 have unpacked vertices; `trussbridge.col` uses a different packed flag. All 603 unpacked files satisfy the recovered extent equation, have in-range indices, and have triangle planes agreeing with their referenced vertices (largest finite residual 0.0003121 in original coordinate units). Six contain nonfinite normals or planes. The Rust reader currently rejects these six and accepts the other 597 distinct meshes. It explicitly rejects the unimplemented variants.

## Type-0 unpacked layout

All integer and IEEE float fields are little endian. No game bytes are embedded in the implementation.

| Offset | Data |
| --- | --- |
| 0 | u32 marker 1234 |
| 4 | four bytes `col\0` |
| 8 | u32 version 1 |
| 12 | u32 variant 0 |
| 16 | twelve f32 values: basis and translation, retained in file order |
| 64 | u32 observed zero |
| 68 | u32 vertex count V |
| 72 | u32 broad-phase node count N |
| 76 | u32 triangle count T |
| 80 | u32 vertex encoding flags; only zero supported |
| 84 | V packed XYZ f32 vectors |
| 84 + 12V | u32 vertex-normal presence P (zero or one) |
| 88 + 12V | V XYZ f32 normals when P is one |
| 88 + 12V(1+P) | N records of32 bytes: bound scalar, center XYZ, half-extents XYZ, two u16 children |
| following nodes | T triangle records, each 22 bytes: distance f32, normal XYZ f32, three u16 vertex indices |

Expected file length: `88 + 12V(1+P) + 32N + 22T`. A triangle's stored plane obeys `dot(normal, vertex) + distance = 0`.

The reader verifies exact extents, counts, finite floating-point fields, and indices before returning geometry. Unit tests cover a synthetic indexed triangle, invalid indices, unsupported variants, and every truncated prefix. The release CLI was exercised against all 603 structurally supported original files; the six nonfinite cases were rejected. No original assets are test fixtures in Git.

```sh
cargo run --release -p atv-tools -- collision private/extracted/example.col private/reports/example-mesh.json
```

The returned mesh now feeds the recovered static ray-bundle query and vehicle contact response. See [obstacle behavior and validation](OBSTACLES.md) for implemented stages and remaining scene-ordering limitations.

## Related geometry

The PSG rigid-mesh and TR2 terrain readers now exist; see [geometry evidence](GEOMETRY.md). Collision-response behavior remains separate from successful mesh recovery.

## Tioga Valley degenerate faces

The `ruins.col` prop collision mesh on Tioga Valley (ENV02) contains four triangle-plane records with NaN values: faces 3, 66, 70, and 84. Their referenced vertices have duplicate coordinates, so each face has zero area and no valid plane. The parser preserves the original broad-phase tree, triangle ordering, indices, and stored plane words, but omits only these four degenerate faces from query geometry. Non-finite vertex coordinates and other invalid faces are still rejected. This allows the local Freestyle route to load and drive Tioga without normalizing or rewriting the source records.

The frontend end-to-end test drove Tioga and then switched back to Caribou; the full renderer regression passed on Caribou. This is targeted malformed-input handling; it does not prove all Tioga collision behavior matches the original game or PS2 hardware. Other collision variants and ordering limitations remain as described in [obstacle behavior](OBSTACLES.md).

## Type-2 ray trees and chassis contacts

`decode_ray_tree` now reads type-2 COL files using the exact-build loader at `0032a480`. The sixteen floats at offset16 form the stored matrix. Offset80 holds the query capacity,84 the node count N, and88 the ray count R. Beginning at92, each32-byte node holds one bound scalar, XYZ center, XYZ half-extents, and two u16 child references. The following R records hold start/end XYZ pairs,24 bytes per ray. Exact extent is `92 + 32N + 24R`.

The root is node0. A child with bit15 set selects ray `child & 0x7fff`; otherwise it selects another node. `00194590` visits first child then second and emits each leaf endpoint. The bounded parser validates finite fields, exact extents, child ranges, and an acyclic tree. It preserves traversal order and coincident endpoints; `Vehicle::set_chassis_points` subsequently applies the original first-occurrence deduplication and sixty-point capacity.

All38 distinct type-2 files in the original disc pass the new Rust reader. The selected quad's34 chassis endpoints also match all binary32 coordinates emitted by2,311 interpreted original traversal instructions. Its terrain chassis contacts now feed the recovered static-contact solver; see [contact behavior and validation](CONTACT.md). Type1 and the packed type0 variant remain unsupported. Ray/triangle obstacle intersection, moving-object response, and wreck behavior are not implied by loading a ray tree.
