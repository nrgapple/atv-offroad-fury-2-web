# Original map-edge launch

The baseline uses the USA v2.00 `SCUS_972.11` boundary branch in `001e5260`,
with predicate `0020aec0`, wreck entry `001e8a20`/`001efb90`, and recovery
`001e8d10`. `boundary_launch` implements these verified component contracts;
Vehicle, rider and scene integration have host checks; public and full original
gameplay equivalence remain separate verification gates. The
behavior launches the quad toward the map center, rather than teleporting it.

The test runs after the quad's outer physics/child update and before ordinary
impact/angle wreck checks. It requires no active wreck and no existing boundary
flight. Normal bounds use a margin of 110, multiplied separately by WidthScale
and BreadthScale. A coordinate equal to the threshold remains inside. The upper
X threshold is `WidthScale * (TerrainWidth * 256 - margin)`; Z uses the
corresponding breadth values. Altitude, BorderWidth and spatial-grid explicit
minima/maxima do not enter this predicate. The extended margin is -1000 when
`[Sounds] CrowdPresent` is nonzero or native game mode is6. `00209460` defaults
CrowdPresent to0; `00209680` reads that original key through `00141fc0`, accepting
T/F, ON/OFF, Y/N and numeric forms. Mode6 is retained as the
source value without assuming a browser mode label.

On a crossing, the branch normalizes the horizontal direction toward
`(128 * TerrainWidth * WidthScale, currentY,
128 * TerrainBreadth * BreadthScale)`. It derives a flight-time constant as
`2 * sqrt(f32::from_bits(0x40f8727c))`, replaces horizontal velocity with
`950 / flight_time` times that direction, and sets vertical velocity to
`125 / flight_time + 16.1 * flight_time`. These are approximately170.47 and112.15
feet per second respectively. It replaces local angular velocity with random
pitch/yaw and fixed roll0.02; it does not add a force, clamp ride height, or change
the launch origin. Both quad and rider maximum squared speeds change from18225
to125000. The Model position setter receives the same origin.

The branch enables collider query bit0 on quad and rider. The direct scene query
routines `0011f2c0`/`0011f9a0` use that bit to suppress Vegetation expansion;
ordinary CollisionObjects remain eligible. The flight clock starts at0. On
subsequent outer updates, strictly more than one second requests clearing bit0
when game setting+674 is enabled. `0021add0` defaults that setting to1. This is
vegetation-query suppression, not a terrain/static-contact disable. Boundary
flag+808 prevents another launch until recovery clears it; elapsed time is not a
relaunch cooldown. Wreck entry bypasses the global wreck toggle for a boundary
flight and selects the existing random direction path (native kind5).

The four launch-related RNG words are consumed in yaw, pitch, direction, variant
order. The spin scales are0.4 for yaw and0.5 for pitch. The direction selector is
the truncated binary32 value `(word * 2^-31) * 5`. Stock setup registers one
separation clip per direction in order Front, Left, Back, Right, Up. It still
consumes the variant draw. At the binary32 upper endpoint, direction can round
to5 or a single-clip variant can round to1. The original unmatched search falls
through to the final registered SepUp clip; the authored helper preserves that
fallback. The existing local OriginalRandom stream can supply consecutive words
through `Animator::boundary_random_words`; complete game-wide RNG call ordering
and the original first-use RTC seeding remain unverified.

Recovery restores both speed limits and clears boundary flag/clock. Original
checkpoint acceptance also has a boundary override: `001ea4d0` projects candidate
height to terrain and bypasses ordinary wheel/upright/wreck gates while+808 is
set. Its water-height and static-clearance checks still apply. Authored
`checkpoint_candidate` matches432 original wrapper cases and75,496 instructions,
including height mutation before rejection. The Vehicle now evaluates these
gates outside the Riding-only branch and follows with the nine static clearance
rays. It retains the port's once-per-outer-update checkpoint sampling phase;
the original manager callback is now recovered, but its ordering relative to
quad physics and the late water cache remains unverified. Vegetation
clearance/category coverage remains separate work. The existing wreck camera and clips
are reused; a dedicated whole-game boundary camera replay has not been made.

Private probes execute800 original boundary blocks, including `0020aec0`, with
112,681 instructions and380 actual launches. Predicate/gate decisions, clock,
linear/angular velocities, speed-limit writes and flag writes match authored
Rust bits. Another600 complete rider constructor/registration/wreck-selection
cases execute117,364 instructions and match selected clips, including the rare
SepUp fallthrough. Allocation, RNG, entity disable, Model position setter and
wreck-entry service hooks are explicit probe boundaries. A further1,000 selector
prefixes inspect the floating-point direction index. Core regressions cover
scaled exact thresholds, launch direction, one-second strict gate, recovery reset
and RNG endpoints. These are finite instruction-interpreter component checks,
not PS2 hardware or a complete gameplay replay. Probes and generated traces stay
under ignored `work/` and `private/reports/`.

The shared browser simulation consumes the four boundary-related words from the
existing wreck Animator stream, raises the rider speed limit before wreck entry,
and restores both limits on recovery. It preserves the body's existing
integrator histories while replacing velocity, as the native inline writes do.
A host ENV04 normal-keyboard run observed the launch, rider separation and
recovery; it initially exposed the old-edge checkpoint retained by the
Riding-only gate. The new boundary candidate override addresses that measured
failure. Final inward-recovery regression and public verification are recorded
by the delivery checkpoint, not inferred from these component comparisons.

## Checkpoint manager dispatch

The original save callback is reached through Quad adapter vtable `00456054`
virtual `20`: thunk `001eba70` adjusts its receiver from Quad+30 to Quad and
jumps to `001ea860`. The main game constructor `0020f780` maps game mode eleven
to manager state three. Manager tick `00248020` invokes `0024e5a0` when its
enabled flag is set and paused flag clear. For each linked player, adapter
virtual `08` first invokes thunk `001ebad0` → `001e9890`, which copies current
Body-bound Model world translation to player+60 and Model world forward to
player+150. These are direct matrix reads, rather than Quad's cached pose fields.
The checkpoint virtual then receives those vectors only when the player has
active flag two and lacks finished flag one. Track bookkeeping between those
calls does not replace the vectors. Manager postprocessing and its child update
follow the selected state handler.

A private forty-case replay executes 3,526 original instructions through the
manager, both adapter thunks, player updater and save wrapper. It verifies the
active/finished and enabled/paused gates, distinct Model-versus-cached-Quad
position inputs and callback-before-manager-postprocessing order. Terrain,
checkpoint acceptance and manager bookkeeping services are hooks. This does
not yet establish where the separate Quad group update occurs relative to the
manager in the full game frame; the live port still saves after physics and
before its late water-height write. No phase change is inferred from this probe.

Quad+34 is the manager: player registration `00248260` writes its pointer into
adapter+4. Thus the acceptance helper's check at Quad34+64 refers to manager
state two, rather than a player class. The main save wrapper `001ea860` bypasses
the entire acceptance helper for that state. The current freeride mode eleven
uses manager state three, so its gate path remains applicable. Other game modes'
checkpoint contracts require their corresponding manager dispatch before live
integration.
