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
| 88 + 12V(1+P) | N records of 32 bytes, broad-phase representation not yet interpreted |
| following nodes | T triangle records, each 22 bytes: distance f32, normal XYZ f32, three u16 vertex indices |

Expected file length: `88 + 12V(1+P) + 32N + 22T`. A triangle's stored plane obeys `dot(normal, vertex) + distance = 0`.

The reader verifies exact extents, counts, finite floating-point fields, and indices before returning geometry. Unit tests cover a synthetic indexed triangle, invalid indices, unsupported variants, and every truncated prefix. The release CLI was exercised against all 603 structurally supported original files; the six nonfinite cases were rejected. No original assets are test fixtures in Git.

```sh
cargo run --release -p atv-tools -- collision private/extracted/example.col private/reports/example-mesh.json
```

The returned local mesh supports later contact and ray-query implementation. It does not reproduce the original broad-phase traversal, collision response, suspension, material friction, or physics solver. Scene instance transforms must still be applied correctly.

## Related geometry

The PSG rigid-mesh and TR2 terrain readers now exist; see [geometry evidence](GEOMETRY.md). Collision-response behavior remains separate from successful mesh recovery.
