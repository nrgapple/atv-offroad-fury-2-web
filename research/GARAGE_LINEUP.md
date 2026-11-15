# Visual vehicle selection

Strategy (2026-11-14): reuse the exact-build QUADINFO portraits for a two-column
vehicle picker and a colored local player lineup. This avoids loading a 3D mesh
and an animated render target for every catalog entry. The selected garage
vehicle retains its live turntable. Player colors follow PAGE08's red, blue,
green and yellow; the responsive two-by-two arrangement is authored.

Evidence: private original-menu-reference/SUMMARY.md and the reconstructed
MultiATVRiderSelect image, plus QUADINFO.RES uiQuad.txt. These composites are
approximate, not original hardware screenshots. QuadN is catalog order, not the
runtime ID: e.g. Quad1 is IngameID 22. Portraits are multi_atvNN indexed by runtime
ID. The source archive contains 25 portraits, with no multi_atv06. Missing art is
explicitly labeled. Motorcycle portraits, remote online-player lineup, and
per-player gear/paint portraits are not supplied by this ATV2 archive. Cards
show stock vehicle artwork, while the selected garage model shows actual paint.

Verification: required CI and focused content browser tests are pending for this
checkpoint. No physical-device or exact original appearance claim.
