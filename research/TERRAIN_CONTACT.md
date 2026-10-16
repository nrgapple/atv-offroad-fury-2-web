# Terrain contact instruction comparison

The full-resolution TR2 contact path now passes bounded original-instruction
comparisons for height, interpolated normal and material. This closes a numerical
terrain-query gap; it does not establish complete vehicle contact parity. The
remaining landing and cross-slope tire overlap is decomposed in
[coupled landing](COUPLED_LANDING.md#residual-overlap-decomposition-2026-10-16).

## Verified correction

The normal normalization sequence in `0016ac10` uses a single reciprocal-square-
root result. The port previously rounded the square root to binary32 and then
rounded its reciprocal separately. It now calculates the reciprocal square root
with a binary64 intermediate and rounds its result once to binary32 before the
component multiplications. The input squared length and all preceding arithmetic
remain binary32 in their recovered order.

The saved 500 synthetic query fixtures originally differed in 346 normal
components, with maximum absolute error `1.1920928955078125e-7`. Height and
material already matched. The corrected query matches all 500 heights, 1,500
normal components and 500 materials. These differences are small; no measured
reduction in rendered tire penetration is claimed for this change.

Two authored uniform packed-normal fixtures now guard both directions of the
extra rounding. A separate test covers a signed tile baseline and zero morph,
which suppresses horizontal normal components while preserving the vertical
component. These tests use synthetic data and require no original resources.

## Query and loader evidence

The private interpreter fetches the actual USA v2.00 `SCUS_972.11` instructions
for `0016ac10`, hierarchy/corner query `00133e00`, and the three TR2 tile
accessors. It executes calls, branch delay slots, scalar binary32 arithmetic and
the used VU macro operations. It rejects unsupported opcodes. Terrain service
results and normalization are not substituted by Rust or host query hooks.

| Comparison | Cases | Original instructions | Result |
| --- | ---: | ---: | --- |
| Replayed synthetic fixtures | 500 | 546,213 | Original outputs reproduced; Rust height, normal and material bits all match |
| Selected original terrain queries across ENV01–ENV08 | 1,000 | 1,097,916 | Rust height, normal and material bits all match |
| Scene-loader argument block and terrain-loader prefix | 100 | 4,400 | Full-resolution mode selected in every case |

The original-terrain cases use 125 queries per environment, retaining selected
leaf height/attribute data and its global cell origin. They include corners,
diagonals, near edges and arbitrary fractional positions. The selected leaves
cover morph values approximately `.6`, `.7`, `.8`, `1`, `1.2` and `1.5`.
Synthesized sparse hierarchies exercise the original two-level global traversal
without loading the game's complete scene. Each query starts with a fresh cache;
repeated-query cache behavior is outside this comparison.

The recovered scene loader `002098b0` supplies zero for the optional sampling
mode argument, and the `00168490` prefix stores it in terrain `+d68`. An
instruction replay checks that caller-to-store path with varied unrelated feature
state and a deliberately nonzero incoming argument register. Logging/string-copy
services are hooked. Thus full resolution is supported by the recovered scene
initialization path, rather than being selected solely for the query fixture.
Optional two-cell query behavior and later runtime changes remain unaudited.

## Reproduction and limits

Private reproducible probes are `work/terrain-contact-probe/oracle.py`,
`actual.py`, `loader.py`, and its Rust comparison binary. Native data and reports
stay in ignored `private/reports/terrain-contact-*.json`.

```sh
python3 work/terrain-contact-probe/oracle.py
python3 work/terrain-contact-probe/actual.py
python3 work/terrain-contact-probe/loader.py
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo run --release \
  --manifest-path work/terrain-contact-probe/Cargo.toml
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo run --release \
  --manifest-path work/terrain-contact-probe/Cargo.toml -- \
  private/reports/terrain-contact-actual-native.json \
  private/reports/terrain-contact-actual-validation.json
~/.cargo/bin/rustfmt --edition 2024 --check crates/atv-core/src/terrain.rs
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo test --locked \
  -p atv-core terrain::tests
CARGO_TARGET_DIR=work/terrain-contact-target ~/.cargo/bin/cargo clippy --locked \
  -p atv-core --all-targets -- -D warnings
```

All eight terrain unit tests and strict core Clippy pass. This is host component
verification; no browser build, public deployment or landing-clearance comparison
was performed for this component gate. The interpreter's finite arithmetic
profile does not prove PS2 hardware rounding, exceptional-value behavior, VU
pipeline timing, full loader state, scene cache lifetime, or whole-frame vehicle
equivalence. The integrated wheel/support/stop scheduling and display-time mesh
pose still require their own coupled evidence. See [movement](MOVEMENT.md).

## Browser ground height

The browser keeps two ground queries with different jobs. Gameplay starts (race
grids and race resets) read `Terrain::clamped_height`: the contact plane above,
with grid coordinates clamped to the world edge. Cameras and other presentation
read `Heightfield::height`, which interpolates the dense `heightfield()` copy
that also builds the visible terrain mesh, exactly as the former JavaScript did.
The two agree within about 5e-4 ft on most maps, but not at tile corners whose
neighboring tiles disagree: FREE04 differs by up to 130 ft at a few such
corners and ENV04 by 0.02 ft. Cameras therefore follow the ground the player
sees, while physics and starts follow the contact ground.
