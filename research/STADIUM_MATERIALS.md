# Original stadium screens and scenery material passes

The reported Ventura (`SX14`) pale ovals are the stadium lamps' additive glow,
not a screen image. The tall blank-looking background shapes are original
`building08sky14` skyline geometry. Their missing second shader pass removed the
building facade texture. Separately, eight stadiums use compressed animated LED
advertisements that the ATM reader previously rejected.

## Source evidence and implementation

- USA v2.00 `001b51d0` reads the four original ALPHA inputs and packs the GS
  equation. `stlightadditive.psm` uses A=texture, B=zero, C=texture alpha,
  D=framebuffer (`0x48`): `(Cs-0)*As+Cd`. The Rust material reader reports this
  as additive blending; WebGL uses `SRC_ALPHA, ONE`. It retains the original
  lamp mesh and texture rather than drawing opaque glow geometry. Normal
  source-alpha blending remains `0x44` / `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`.
- `001bf430` reads `TotalShaderPasses` and creates each declared pass in order.
  World scenery now loads each original PSM pass, with its own shader, texture,
  UV/texture clock and render state. The two `building08sky14` passes are the
  existing XZ sky reflection followed by `diffuseCompressed` / `building08`.
  Source model dimensions, placement and UVs remain unchanged. A facade may
  still reflect sky or fade into source fog according to its authored alpha.
- Secondary passes use `LEQUAL` to draw the identical geometry at its existing
  depth. Reflection changes UVs, not `gl_Position`. `LESS`, writable depth,
  normal alpha-test state and disabled blending are restored after scenery,
  before trains, vehicles, water and particles. This does not reproduce every
  possible original GS test or the engine's complete global draw scheduling.
- `stbscreen.atm` in `SX05–SX11` and `SX14` contains ten pixel-format-2 frames.
  Each frame has four length-prefixed IPUM mip payloads (16,32,64,128 pixels).
  The reader now checks each frame/mip extent and decodes a selected frame
  through the existing authored PST/IPUM decoder. Original forward-looping
  PSM animation uses the existing source-correlated 10fps texture clock.
  Venue initialization starts at frame0; ATV/motorcycle placement reset preserves
  the current venue clock, as do pause and context restoration.
  The images are original LED advertisements, not an invented video feed.

The bounded pass splitter accepts up to eight declared passes, verifies exactly
one section for each, and gives cached secondary passes distinct keys. This is
limited to world scenery; other model paths retain their existing behavior.
Textures and original advertisement frames stay resident after loading, so
animation changes only the selected GPU texture. No new runtime content or
third-party movie decoder is required. IPUM hardware precision remains subject
to the existing limits documented in [IPUM.md](IPUM.md).

## Verification and scope

Synthetic tests cover compressed ATM frame selection, mip lengths, truncation,
trailing data, later-frame extent errors and invalid frame indices, plus additive
ALPHA metadata and ordered/bounded PSM splitting. Original content is kept out
of Git and CI.

`tools/test_stadium_screens.mjs` checks source lamp and skyline draw groups on
desktop and 390px portrait, with before/after pixel reads and original LED frame
advancement. Its inspection camera, legacy-material overrides and frozen worker
batch replay exist only in the test. Actual menu pause, source placement reset
and WebGL context restoration are tested separately. A read is captured inside
the game render callback, before the browser discards the drawing buffer.

The original-content all40 driving gate additionally rejects any scenery texture
decode error. ENV01/FREE05 reflection, Freeport rain, lava/water UV movement and
pause/context regressions verify the shared loader and draw-state changes.
An independent source census covered 3,088 PSM records in all40 archives,
including 233 multipass declarations: no missing first binding, pass splitter
error or synthetic-key collision. It also confirmed secondary animated/UV
passes remain connected to their distinct source clocks.

At the integrated host gate, all40 driving routes had zero scenery texture
decode errors. Reflection GPU transform feedback compared 768 native cases over
three model/node configurations (2,304 comparisons), maximum UV error
`7.152557373046875e-7`. Original LED frame0 and all ten frames, lamp/facade pixel
contributions, pause/reset/context lifecycle and zero page/GL errors pass for
ATV1 and motorcycle27 on desktop and 390px portrait. An independent WebKit
private candidate override over public HTTPS assets passes the same ATV gate;
it is not a public deployment of the candidate. Source-group contributions
were 3,156/4,193 lamp pixels and 123,475/192,926 facade pixels on the host ATV
desktop/portrait run; counts vary with viewport and capture timing.

These checks establish component behavior and source-derived visual corrections,
not complete GS/VU pixel equivalence, crowd animation recovery, live cameras on
stadium screens, or a completed port. Host evidence is distinct from a public
release; the responsible release agent verifies the merged deployment.
