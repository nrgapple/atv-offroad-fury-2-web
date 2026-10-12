# Original obstacle queries

The Rust `obstacle` module reconstructs the USA v2.00 static ray-bundle query. It uses the original flat face test (`001122a0`), box/box separation tests (`00108cb0`), segment/box test (`00109750`), triangle/box tests (`0010adc0`), tree-pair recursion (`0010c140`), and nearest hit replacement (`0010b800`). These are authored semantic implementations; game instructions and decompiler output stay private.

The type-0 reader now retains the original 32-byte bounding nodes. Loader `00330850` stores each face normal as signed 16-bit components after multiplying by 32767, adding 0.5, and truncating; query code preserves this asymmetric treatment of negative components. Both trees begin at node 0. Bit 15 selects a leaf. Tree traversal splits according to the stored bound scalar, visits the first child before the second, and preserves first-hit ray order when a nearer triangle replaces its result. The leaf test is one-sided and retains the original projected-edge boundary rules, including rejection at its first vertex.

Relative transforms follow `0010c3f0`. The query receives collision placements rather than visual model matrices. Scene loading (`00207a10`, `00206940`, `00206e90`, `00120510`) selects authored collision objects, falls back to visual model sections when no override exists, chooses `COL` before `SLT`, and forces a `.col` extension. Missing collision files are skipped by the original loader.

`Vehicle::set_obstacles` retains the original `ar.col` ray bundle and static meshes. Querying occurs once per outer frame. As in `00193060`, successful object callbacks append the cumulative nearest-ray results, with a sixty-contact solver capacity. Each physics substep projects the current ray endpoints onto the cached contact planes and feeds the recovered static response after chassis/terrain contacts. A synthetic wall test verifies active velocity rejection and penetration correction.

## Validation

The private bounded interpreter runs actual instructions from the verified executable, with finite host binary32 arithmetic. It is not a PS2 hardware emulator.

| Comparison | Cases | Original instructions | Result |
| --- | ---: | ---: | --- |
| Flat ray/triangle, including tilted faces | 1,600 | 310,037 | Hit flags and all returned float bits match |
| Bounding-sphere separation | 600 | 19,200 | All returned float bits match |
| Box/box and segment/box | 2,000 | 318,567 | All overlap results match |
| Triangle/box | 1,200 | 954,671 | All overlap results match |
| Tree-pair traversal and nearest-ray slots | 500 | 2,251,967 | Ray order and all hit output bits match |
| Original ATV01 ray bundle against three original meshes | 18 | 1,383,721 | All 22 hits match, including ray order and float bits |
| Rigid relative transforms | 400 | 76,400 | All XYZ bits match; unused basis-W negative zero is not retained |

The 31 ENV01 static meshes and the quad's separate type-0 fixture pass hierarchy validation. Original assets are not public test fixtures. Synthetic Rust tests cover sidedness, touching/separated bounds, nearest replacement without reordering, and integration with vehicle contact response.

## Remaining fidelity work

The current static scene candidate list follows authored placement order, with the original `001213b0` sphere rejection before tree queries. The ordinary-static spatial registration/traversal and candidate-cache policy now have authored modules and original-instruction comparisons; see [scene candidates](SCENE_COLLISION.md). Live candidate integration, exceptional category filtering and vegetation collision expansion remain to be validated; these can affect cumulative contact order when multiple obstacles overlap. Current integration requires identity COL header transforms, as observed in the selected ATV01 ray bundle and ENV01 meshes. Static material 0 is supported; moving objects, type 1 compounds, packed type 0 geometry, query aggregates used by crash logic, and wreck behavior remain separate work. This is a partial driving reconstruction, not a completed game port.
