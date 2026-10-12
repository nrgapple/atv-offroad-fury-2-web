# Speed, preload and stunt HUD recovery

The browser HUD is an authored Rust controller plus a Canvas bridge using the
original runtime layout sections, CEL glyph metrics and PST atlases. This covers
the supported single-player driving/stunt reconstruction. It does not implement
race state, progression, split-screen layouts or the complete original overlay
system.

## Source and strategy

Evidence is from the exact USA v2.00 `SCUS_972.11` ELF, SHA-256
`11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`.
Cached Ghidra observations were checked against instructions and original data
addresses where the decompiler omitted arguments or floating-point operations.
The code reconstructs controller semantics in Rust; it does not execute the ELF
or depend on a first-game function map.

`crates/atv-core/src/hud.rs` owns speed conversion, preload visibility/decay,
stunt display score caching and text animation. `web/hud.js` loads
`OverlayObject-5` and `OverlayObject-12` from the authorized environment
Freestyle archive, submits the original configuration bytes to Rust, and draws
the original art. Those extracted files remain private/R2 runtime resources;
they are not embedded in the GitHub code.

The preload textures (`preloadblue`, `preloadbkg`, `preloadhighlight`,
`preloadorange`, `preloadblocker3`) occur in each environment mode archive,
including `E01_FR.RES`. The font `tradegothic_bci_neg1fs_32ds` is in
`STARTEVT.RES`. The blue/orange indices are selected by texture name in native
initialization; the remaining transparent blocker icons are not visible in this
supported path.

## Recovered behavior

| Component | Exact-build evidence | Implemented behavior |
| --- | --- | --- |
| Speed | `002dd410`, `002e2030` | Binary32 speed times unit factor, truncate to integer, clamp negative to zero; `%02d` minimum width; original `Mph`/`Kph` capitalization. MPH factor is `0.68182`, converting feet/second. |
| Preload initialization | `002e1f60`, `002e2030`, vtable `0045f180` | Blue and orange initially hidden; background/highlight remain visible. Original alpha and orange height come from the runtime layout. |
| Preload update/render | `002e25b0`, `002e2290` | Reads rider status `+0x50` and strength `+0x90`; displays while status allows preload, retains nonzero strength for two seconds, scales orange height and blue alpha during the final second, hides at strict negative expiry. Zero strength with inactive status hides immediately. Browser input is not a substitute for those rider fields. |
| Name/lifetime | `002d9490`, `001eaa70`, `002e3b70` | Scoring update calls the current head event name repeatedly, while scale phase is idle. Each accepted name refreshes a two-second lifetime. An active chain prevents ordinary expiration. |
| Score | `001eaa70`, `002dd4b0` | Actor caches pending sum and event count on the scoring tick. While active, display truncates the sum; after finalization it multiplies the truncated sum by cached count. Zero displays an empty string. Cached values survive linked-list cleanup. This is separate from committed cumulative points. |
| Success | `001eaad0`, `002e3c70`, `002e3d70`, vtable `0045f080` | Uses cached event count for `COMBO x %d`; animates the second text's scale through up, stay and down phases. Source overrides are 0.05s, 0.25s, 0.05s and target scale 2. |
| Failure | `001eaad0`, `002e3c70`, `002e3e00`, `002e3f60`, `002e3ff0` | Changes both texts toward original failure RGB over 0.25s, fades and shakes for `AlphaOutChangeTime` (1s). Shake adds integer deltas in the native range -3 through +2 to existing positions. |
| Reset | `002e4000`, `002e4490` | Restores captured starting scale/color/alpha and original text offsets, then hides. |

The native scale, color and alpha routines do not snap to the requested endpoint
on a timer boundary. The controller preserves that discrete behavior rather than
replacing it with a normalized easing curve. The same alpha-out duration drives
failure fade and shake; there is no independently authored `ShakeTime` field.

The finalization hook also checkpoints repetition penalties on successful
landing and restores that checkpoint on failure. That behavior belongs to the
stunt scorer and was identified while following the HUD callback. Native
`001ead10` is the current-event completion hook and returns zero without a HUD
pulse. The HUD pulse/failure belongs to chain finalization, not every event
record closure. Native wheelie score has a separate path (`001e5260`,
`001e70e0`); it is not inferred from regular stunt points.

## Wheelie scoring and the original cheat flag

The original settings constructor `0021add0` explicitly initializes
`settings+0x1878` to zero (`0021b8c0`). Cheat dispatcher `0033ede0` toggles that
field for `GETONUP`, with the original enabled/disabled wheelie-for-points
messages. **Normal baseline riding therefore does not award or display wheelie
points.** Physical wheelie assist is a separate recovered rider-controller path;
it must not be used as a substitute scoring gate. The authored wheelie kernel
supports the original flag, which remains false by default. The cheat UI and
session flow are not implemented by this component.

`stunt_score::WheelieState` reconstructs actor fields `+0x914` (armed), `+0x918`
(eligible/display), `+0x91c` (duration), `+0x920` (points), `+0x7fc` (front-contact
grace) and `+0x928` (activity during the current tick). Constructor `001e1b10`
initializes points to 1 and grace to 0.5 seconds. The scoring section at
`001e6450..001e6600` applies these conditions:

- Both front wheels lack native contact, both rear wheels have contact, the
  grace value equals exactly 0.5, body Euler pitch exceeds binary32
  `0x3edf66f1` (approximately 25 degrees), and context `+0x13c` is zero.
- On first qualifying contact state, points become 1 and the attempt arms.
  Duration advances even when the cheat flag is disabled. Previous duration
  must be strictly greater than one second before scoring begins. An enabled
  scoring tick adds `50 * dt`, sets eligible/activity, and refreshes `Wheelie`
  text through the ordinary HUD name callback.
- Front contact clears an unscored armed attempt but retains its duration.
  For an eligible attempt, front contact decrements grace while positive;
  finalization occurs on the next front-contact update after it reaches zero.
  Both-front-airborne states freeze a partially consumed grace value; they do
  not restart scoring or finish the award.

`001e70e0` adds wheelie points directly to the same entity `+0x104` cumulative
score used by regular stunts, using binary32 addition without an integer floor
or combo multiplier. It clears armed, eligible and duration, restores grace to
0.5, and leaves point/activity fields intact. The HUD pulses one success event
only if its overlay is visible; otherwise its animation resets without showing
a new panel. An accepted airborne stunt also finalizes an eligible wheelie,
then explicitly resets/hides the wheelie panel before the stunt name callback
(`002da6c0`).

The wheelie wreck slice in `001e8a20` requests the failure fade when eligible,
credits no pending wheelie points, and performs the same armed/eligible/duration
clear and grace restoration. `002dd4b0` gives eligible wheelie points priority
over the regular cached stunt text, truncating them to an integer. Once
eligibility clears, the display returns to the regular stunt cache. Native
`002e3640` prevents ordinary HUD expiration while wheelie eligibility persists;
its sound/activity condition uses the distinct per-tick `+0x928` field.

The kernel returns ordered `Name`, `Finished { points }` and `Failed`
notifications; `State::credit_wheelie_points` joins awards into the shared
scoreboard. The Canvas bridge needs no wheelie-specific authored artwork.
Vehicle/WASM integration is pending: it must supply native wheel contacts and
body pitch and apply notifications in original callback order. The current
implementation is an isolated kernel and HUD API. The source cheat/enable flag
defaults false, so the normal baseline intentionally awards no wheelie points.

## Presentation and limits

Single-player coordinates retain the original 640×448 frame: speed/preload
anchor `(597,372)` and stunt anchor `(320,348)`. Wide displays extend horizontal
framing and keep the speed/preload anchored to the right edge. Compact portrait
phones move the meter above the driving thumb region and bound text width.
Those are explicit browser presentation adaptations; they do not change score,
physics, preload eligibility or animation clocks.

The authored path omits unimplemented race clocks, rank, laps and radar.
Controller-vibration/sound triggers, participant selection
for other modes, disabled-overlays settings, original text-depth/GS rounding and
multiplayer remain incomplete. Shake uses the original libc generator with a
local seed; full-game shared RNG draw ordering has not been recovered, so its
exact pixel sequence is not a parity claim. Original bitmap rendering and the
mobile placement still require visual comparison against a PS2 reference.

## Verification

Six focused Rust tests cover speed formatting, preload boundaries, scale-phase
name gating, timer endpoint behavior, and truncation-before-multiplication score
retention, and reset preserving parsed tuning/units. The HUD module passed Clippy with warnings denied and JavaScript
syntax validation.

A private finite-binary32 instruction interpreter exercised 600 randomized
preload status/strength/time updates against `002e25b0`/`002e2290`, executing
48,042 original instructions. Visibility, normalized orange height bits and blue
alpha matched the Rust controller in all 600 frames. The probe isolates the
meter with unit source height; it is a selected-component check, not validation
of final Canvas pixels, the PS2 hardware FPU, or whole-game replay equivalence.
Evidence remains in `private/reports/hud-meter-oracle.json`; temporary harnesses
remain under ignored `work/hud-probe/`.

The wheelie comparison covers 2,070 operations and 67,982 original instructions:
native update section `001e6450..001e6604`, full finish routine `001e70e0`, and
the actual wreck wheelie decision/reset slice `001e8c6c..001e8cf0`. Disabled and
enabled flags, exact pitch/previous-duration boundaries, front-contact grace,
airborne pauses, explicit finish and wreck cases produce zero differences in
all six wheelie state fields, shared cumulative score bits and ordered callback
notifications. HUD and parent score-report calls are intercepted; those calls'
rendering, audio and progression are not validated by the probe. Private results
are in `private/reports/wheelie-score-oracle.json`, with the synthetic harness
under `work/hud-probe/wheelie/`. Eight additional Rust tests cover the wheelie
kernel, fractional score merge and HUD callback behavior. This remains a
selected-component instruction comparison, not a PS2 gameplay replay or proof
of full game parity.
