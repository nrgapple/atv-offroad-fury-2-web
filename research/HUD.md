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

## Presentation and limits

Single-player coordinates retain the original 640×448 frame: speed/preload
anchor `(597,372)` and stunt anchor `(320,348)`. Wide displays extend horizontal
framing and keep the speed/preload anchored to the right edge. Compact portrait
phones move the meter above the driving thumb region and bound text width.
Those are explicit browser presentation adaptations; they do not change score,
physics, preload eligibility or animation clocks.

The authored path omits unimplemented race clocks, rank, laps and radar. Native
wheelie HUD/scoring, controller-vibration/sound triggers, participant selection
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
