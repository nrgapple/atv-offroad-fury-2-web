# Original weather selection and browser cost

Weather is selected from the actual common and mode TOY documents. The initial
147-SCN census found no `[Weather]` sections; that result alone does not establish
inactive weather. A broader SCN/TOY census finds 17 Weather declarations across
the source archives. Legacy SCN `[Rain]`/`ChanceOfRain` declarations are not read
by the recovered Weather loader at 00208cc0.

| Venue/mode resources | Original Weather |
| --- | --- |
| Freeport/ENV04 common TOY | Rain, chance 1, `sx13rain.slt` |
| NAT06 common TOY | Rain, chance 0.9, `nat06rain.slt` |
| SX13 common TOY | Rain, chance 0.8, `sx13rain.slt` |
| Caribou/ENV01 National, Enduro large/small, King of the Hill, Tag mode TOYs | Snow, chance 1, four camera-relative rings |
| Caribou/ENV01 Freestyle TOY | No Weather declaration; snowy terrain is not evidence of falling snow |

All other inspected shipped SCN/common/mode TOYs omit Weather. These are source
resource declarations, not observations from a PS2 framebuffer.

00209dd0 selects Weather once during level activation. Its strict dry condition
is `chance < binary32(rand) * 2^-31`; equality selects Weather, including the
very rare zero-RNG endpoint at chance zero. The dry branch removes matching SLT
models. 00214d20 changes chance to zero when native player count exceeds one.
The browser supplies one per-load RNG sample and the lobby's player count;
its independent RNG and online-to-native player-count mapping are adaptations.
Native game-wide random ordering and the original multiplayer event scheduling
remain unrecovered. ATV reset and GL restoration retain the selected scene.

The three rain declarations have no Weather particle-emitter list or snow-ring
flag, and their SLT name is not `screenrain`. Their wet branch therefore retains
the original placed rain geometry and assigns the Rain type; it does not create
a camera-relative WeatherParticleEmitter or ScreenRain object. Their original
prelit cards scroll V at 2. Freeport also places a Type 8 raindrops emitter and
Type 12 fog emitter independently of Weather. The ambient-particle component
owns the placed Type 8 effect.

The active Snow branch generates 4/8/16/32 emitters (60 total), at the source
ring heights/radii, enables camera-facing hemispheres and moves enabled emitters
relative to the active camera (00209dd0/00209310). Its Snow-specific VU birth,
packed updates, fading and render callback remain unconnected. Original ScreenRain
lens/world sprites and framebuffer-copy droplets are also unconnected; none of
these inspected map Weather blocks selects ScreenRain. Rain audio initialization
at 00213bf0 requests `rain.vag` with looping flag 0x81; live playback remains a
separate follow-up at this checkpoint. No generic storm was added to dry maps.

## Host component gate

The authored Rust policy and WASM operation 120 read common/mode TOYs, preserve
first-key selection, choose rain once and remove dry-choice geometry before GPU
construction. Two synthetic regressions cover original defaults, equality,
legacy Rain exclusion and multiplayer suppression. An ignored finite interpreter
executes 35 actual 00209dd0 selections (2,072 EE instructions) with controlled RNG
and string services. Active/dry decisions and binary32 samples match Rust exactly.
This covers the selection branch, not whole engine callbacks or hardware output.

All three shipped rain materials use alpha reference 120 and FB_ONLY failure.
Batching every accepted fragment before every failed fragment could change
ordering when cards overlap. The browser instead retains its previous per-card
accepted/failed draw order and caches identical material uniforms, texture and
VAO bindings across consecutive draws. Each placement still uses its source
matrix, colors, texture, scrolling, fog, alpha predicate and depth behavior.
No extra particles, full-screen postprocessing, visibility reduction, new buffer,
texture or per-frame texture/mesh upload is introduced by this optimization.
The previous browser order is preserved; native PS2 transparency queue parity is
not established.

The deterministic original-resource inspection gate compares the prior authored
renderer with this renderer in desktop and 390×844 portrait. Its test-only start
is derived from an original rain placement near the source field center, with
ordinary physics and a fixed 90-step clock; it is not a normal spawn/hardware
capture. Desktop has 30 visible rain placements/60 original draws, with one
material update replacing 30; portrait has 27/54, with one update replacing 27.
All 2,073,600 desktop framebuffer bytes and 1,316,640 portrait bytes match exactly.
This is identical visual output with 29/26 fewer complete material updates per
frame, not a measured physical-phone FPS gain. Test hooks and source-derived
fixtures stay private; `tools/test_weather.mjs` is an opt-in host/public gate.

Weather selection, rain rendering optimization, ambient placed emitters,
Snow/ScreenRain, rain audio and complete PS2 weather fidelity are separate gates.
Public verification is pending integration and release.
