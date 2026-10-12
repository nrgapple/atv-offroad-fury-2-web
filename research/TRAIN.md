# Arlington Stream train — USA v2.00

The implementation follows the original ENV08 Freestyle procedural route and
named wheel motion. This core checkpoint provides authored Rust route time,
carriage poses and wheel-node poses. The host integration draws the original
runtime meshes; public verification remains pending. Original
resources, native exports, instruction probes and their output stay under ignored
`private/` and `work/`. No disc executable or emulator runs in the browser.

## Binding and implementation strategy

`train::definitions` reads the actual TOY procedural definitions in file order.
This lane binds the Arlington `locomotive.Vue` family: eleven bodies configured
for original collision and the separate noncolliding smoke attachment. Other procedural
toys are outside this component. The engine, tender, coal cars, flatbeds and mail
car use their original PSG resources; repeated models share GPU geometry. The
fifth-node `loco_wheels` model uses its separate named MOT. No synthetic carriage,
route, speed, collider, terrain offset or rail timeline is substituted.

The route clock advances once per fixed simulation step. Per-car lags use the
original integer reader, not floating-point authoring text. The shipped decimal
strings therefore become 15, 71, 120, 174, 227, 281, 334, 387, 439, 491 and 544.
The smoke attachment has zero lag. All twelve procedural definitions use FPS 2.
The authored route has 300 frames from 0 through 299 and a 149.5-second loop.

## Native route and body contracts

The ASCII VUE reader `0011c0b0` stores author translation XYZ as runtime XZY.
Its rotation rows are not the procedural train heading. `0011d8a0/0011de20`
interpolate positions with the original cubic weights and frame-spacing-scaled
neighbor tangents. The first and last segments retain their endpoint tangent
rules. `0011c5e0` estimates each segment length from ten substeps, including its
reciprocal-square-root then reciprocal operation order. `0011d5d0` walks those
lengths to find each lagged time, allowing cars to turn separately around bends.
Negative returned times are wrapped by the position sampler.

`0023be10` reads `LagDistance` with `001421b0/00141fc0`, whose numeric path is
`strtol(base 0)`. `PathIsLevel` and `DriveOnSoultrees` default true, while
`NumberOfTires` defaults zero. The procedural body's own tire-spin loop therefore
does not animate these train wheels: their wheel assembly is the independent
keyframed object.

The original procedural loader `0023d670` passes terrain pointer zero into
`0011b950`. The body uses authored VUE height, with a separate direct scene-object
height override. `0011c840` stores height on the shared route owner; level-path
followers use that owner's height. After the owner obtains the same height on
successive updates, its level-path height is cached. Treating every car as a
separate terrain follower changed the initial body matrices and was corrected
against the native replay. Horizontal displacement drives heading, with the
constructor's 0.5 history blend. The up vector and matrix update retain the native
owner/follower branch timing.

## Wheel motion

The wheel MOT is version 3 with named bones, linear translations and rotation
kind 0. Its eight-byte rotation keys hold time, an unsigned angle byte and three
signed axis bytes. The existing general MOT reader accepts only the twelve-byte
ShortAxis kind, so the train keeps this verified extension in `train_wheels.rs`.
`001cb7e0` constructs a 256-entry sine/cosine table; `001cbb10` multiplies the
signed axes by that table's sine scale without renormalizing them. The source
`001c8830` sampler uses the same quaternion blend kernel already reconstructed
for rider motion. The named channels compose the original five-node hierarchy.

Host sine, cosine and inverse-cosine remain the existing animation backend
limitation: their PS2-library bit parity is not established. The packed decoder
probe explicitly supplies table sine/cosine services; it is not a claim that the
whole PS2 transcendental library was executed.

## Evidence and verification

| Actual instruction probe | Scope | Instructions | Rust comparison |
| --- | --- | ---: | --- |
| `0011c5e0`, `0011d8a0`, `0011de20`, `0011d5d0` | 300 shipped route keys; 600 varied position and lag queries | 2,811,481 | All segment-length, position and lag-time bits match |
| `0011c840`, route/lag and model orientation helpers | 240 fixed frames × 12 route followers | 23,983,339 | All 46,080 compared body-matrix words match |
| `001cb7e0`, `001cbb10` | 1,000 varied packed keys | 168,499 | Native decoder observations with explicitly supplied sine/cosine table services |

The body replay supplies engine zero/up/forward constants, remainder, model
world propagation, direct scene collision as no-hit, scene actor updates and
base-actor callbacks. It executes the actual native body arithmetic and
orientation helpers; it does not establish full scene collision, audio callback
or startup-manager scheduling parity. Probes are a finite-binary32 MIPS/VU
interpreter, not an original-hardware run or a complete PS2 execution environment.

Private reproduction:

```sh
python3 work/train-probe/native_route.py
python3 work/train-probe/native_update.py
python3 work/train-probe/native_packed_rotation.py
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/train-probe/Cargo.toml --bin route --target-dir work/terrain-contact-target
/Users/nrgapple/.cargo/bin/cargo run --manifest-path work/train-probe/Cargo.toml --bin train-probe --target-dir work/terrain-contact-target
/Users/nrgapple/.cargo/bin/cargo test -p atv-core --lib train:: --target-dir work/terrain-contact-target
```

Reports are under `private/reports/train/`. Committed synthetic tests cover
coordinate conversion, cubic/loop behavior, integer truncation and defaults,
independent lagged turns, signed packed-axis handling and all truncated prefixes
of an authored eight-byte MOT fixture. None embeds original game content.

## Integration status and remaining work

WASM operation 67 installs the original train from the existing ENV08 archive;
68 returns its frame or performs a bounded diagnostic step. The regular batch
updates it before vehicle/camera operations and carries each intermediate train
snapshot. `web/train.js` draws the original body and wheel meshes using those
Rust poses. Source definitions and GPU resources survive graphics-context rebuilds;
route time pauses with the existing simulation lifecycle.

The desktop and 390px host gate executes the regular WASM batch and verifies
eleven configured train bodies, five wheel nodes, changing body/wheel poses,
pause/resume and graphics restoration. Train draws change 3,990 desktop and
1,710 portrait pixels in the tested view, with no page or WebGL errors. Original
route/MOT output supplies the test camera placement; no extracted map coordinates
are committed. The renderer submits 97 draws and 3,232 triangles for the twelve
procedural models and wheel assembly, sharing repeated model buffers.

```sh
ATV_TEST_URL=<host-or-public-url> PLAYWRIGHT_MODULE=<playwright-module> node tools/test_train.mjs
```

This is host evidence, not a verified public release. The bridge still supplies no-hit for the
train's scene-height override. Dynamic train collision response, original spatial
loop sounds and particle smoke are unfinished. The smoke PSG is the small original
emitter marker, not a reconstructed smoke effect. These gaps must remain explicit
through host/public verification; moving rendered cars and wheels alone do not
establish complete original train behavior or full handling parity.
