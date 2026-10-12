# Static scenery root placement and the missing Arlington rails

The ENV08 rail gaps came from a static model transform error. All 68 original
track placements decoded and reached the browser renderer: eight `trackcurve`,
four `trackcurveshort`, 52 `trackstraight`, and four `trackstraightshort`.
Their `tracktex` material uses the supported `DiffuseCompressed` base pass.
Neither missing resources nor reflection-shader exclusion caused the gaps.

The twelve curved sections have a translated PSG root. Their stored vertices
are local to that root, but the bind-pose decoder previously included the root
translation in `Vertex.position`. The renderer then applied the SCN placement
to those positions, moving the curves far away from the authored railroad.
Straight sections have identity roots and were unaffected by that composition.

## Original setter evidence

The static model loader `00206ba0` loads the PSG, then calls `001657c0` with the
SCN position and `00164dd0` with the SCN look/up directions. The first setter
replaces root matrix translation; the second replaces its basis while retaining
the newly supplied position. It does not multiply the scene matrix by the stored
root bind matrix. An original-instruction replay of both setters passes all 68
rail placements in 5,916 instructions. Translation words match exactly; basis
components differ from the existing portable scene orientation calculation by
at most 5.96e-8. Matrix dirty propagation is explicitly supplied in this bounded
probe, rather than presented as a whole scene or PS2 renderer replay.

The eight hosted freestyle archives contain 234 placed model definitions and
902 placements with a nonidentity root, counted separately in each environment.
All those affected definitions have one node. Counts by ENV01–08 are
111, 89, 31, 285, 65, 248, 25 and 48 placements. The same original static loader
sets their roots, so the repair applies to static scene models generally.
These counts describe transform admission, not new object placement or complete
visibility/material fidelity.

## Authored repair

`mesh::decode_psg_for_scene` decodes static geometry with an identity root world
matrix. Child matrices compose their original local transforms against that
root. Position and normal streams therefore reach the GPU in root-relative
space, with the SCN matrix applied once. This is equivalent to removing the old
root from bind geometry, without a numerically approximate inverse of a scaled
root matrix. Stored local matrices, bounds, material bindings, UVs, strips and
indices are retained. WASM operation 11 selects this coordinate space with
index 1; its default index 0 continues to return the stored bind pose. Only the
static scene mesh loader requests index 1.

A synthetic PSG test exercises a translated, rotated, nonuniformly scaled root,
a rotated/scaled child, signed strip vertices and normals through both decoders
and the actual scene orientation interface. It verifies root replacement,
preserved child geometry, unchanged indices and the independent bind result.
Vehicles, riders, ecosystem meshes, train geometry, COL contacts, source scene
placements and terrain heights retain their existing paths.

No rail height offset, camera change, texture substitution or geometry patch is
part of the repair. Samples of authored rail-bed edges still intersect terrain;
those source intersections are not raised to make a visually smooth overlay.

## Verification

Six focused mesh tests, 297 core tests and two WASM tests pass, together with
strict Clippy, formatting and a private release WASM build. The opt-in
`tools/test_rails.mjs` derives all twelve curved placements from the loaded
original scene and positions an inspection camera at each one. It measures
framebuffer changes across the actual rail draw, with desktop and 390px views;
source coordinates and assets are not fixtures in Git. The baseline changes
zero pixels for all twelve curves on both sizes. After repair, the desktop
curves change 36,027–49,735 pixels each with zero JavaScript/WebGL errors.
The mobile curves change 6,041–8,366 pixels each, also with zero errors. All
eight environments load and drive without unsupported placed meshes or JS/GL
errors, and the moving full-sample terrain patch remains functional.
These inspection views prove placement and visibility at tested views, not
original gameplay-camera or whole PS2 rendering equivalence. Public delivery
requires the separate current-main deployment, R2 and HTTPS checks.

Private evidence lives under `private/reports/train/`: static-root and rail
censuses, source setter replay, authoritative terrain comparisons and paired
screenshots. The original resources are already in the hosted manifest; no new
runtime content is required.
