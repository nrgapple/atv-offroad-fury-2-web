# Model selection, original menu music and course loading

The responsive Rust menu emits separate ATV and rider inspection rectangles.
The browser GPU draws decoded original models inside them, with pointer and
keyboard rotation. Portrait places the cards above the four selection rows;
landscape places them beside the rows. All rows retain at least 44 physical
pixels. Keyboard/controller up/down select rows, left/right cycle quads, and
accept activates the selected row. Pointer selection follows the gold row state.
The garage serves Single Player and Multiplayer; the same IDs enter gameplay.

ATV previews use the selected in-race quad and material archive. The separate
original menu rider models are recovered from PAGE07.RES: `mrider_ui.psg` and
`frider_ui.psg`, each with 32 nodes. These differ from the 18/27-node riding rigs.
The original body breathing clips, seven male idle clips and five female idle
clips are sampled by `atv-core::menu_preview`. Their state lives outside the
ride simulation in the WASM bridge. Menu sampling cannot replace the gameplay
rider, animator or physics. The browser only supplies a monotonic clock and
uploads the Rust matrices, at a maximum requested rate of about 30 Hz.

The UI catalog names an eighth male idle using the same VUE as idle seven; the
runtime package keeps the seven unique clips. Ordered breathing/idle playback
is an authored adaptation: native random selection, delay, transition/blending
callbacks and whole-menu parity remain unverified. Preview lighting is simple
and does not recreate original reflection/specular passes. Seven original base
textures provide the default outfit; the shirt export material maps to its
original default shader texture. The unbound mouth material retains a fallback.
Female hair and outfit customization are outside this helmeted rider selector.

`AUDIO/UI/UIMUSIC.AUD` names four dedicated UI tunes. Each is stored as left/right
mono VAG samples `uitune1l/r` through `uitune4l/r` in AUDIO.RES. The browser decodes
these with Rust, assembles stereo PCM and loops the first tune using its original
VAG sample loop markers. A single menu voice runs after user interaction; engines
remain silent. Music preferences control its on/off and volume. Pause suspends it,
while Pause → Music previews the 21-track racing soundtrack instead. Native
menu-tune selection/mixing behavior and a listening comparison remain open.

Course loading is a Rust page, with the selected environment's original two-part
track photograph, course name and actual decoder stages. Its layout adapts to
portrait and landscape, including direct-start and shared-game routes. It yields
a paint before resource decoding; it does not invent a download percentage.

Required menu meshes, body MOTs, seven base PSTs and the UI audio definition are
compacted into PAGE07.RES. Only the eight required UI tune samples are added to
the compact audio bank. The new private package contains 107 records and 307
validated manifest/blob files. Source bytes, reports and screenshots remain under
ignored private storage and publish only through the separately approved R2 path.
Older manifests retain usable model previews and racing soundtrack fallback while
the new resource manifest publishes; the dedicated tunes/idle clips require that
R2 update. GitHub and CI contain only authored code and shareable metadata.

## Host verification

`tools/test_menu_models.mjs` uses actual WASM, original hosted resources and GPU
rendering. It covers all 26 quads; both menu rigs; changing breathing pixels;
390×844, 320×568, 844×390, 740×360, 1280×720 and 768×1024; minimum target size,
model/control separation and overflow; pointer/keyboard rotation; keyboard and
controller navigation; wrap and rapid changes; preview context recovery; selected
quad/rider loading into a course; pause and return-to-menu; zero JS/HTTP/GL errors.
The existing frontend regression passes title/menu/controller navigation, all
nine selector routes, Tioga driving, pause/Controls and 390px Caribou reload.
The actual-WASM soundtrack gate passes all 21 songs, 63 chunks, maximum 65,536
bytes per decode and zero underruns. Source fixture tests cover playlist wrapping,
invalid clocks, compact-package exclusions and single-voice stereo/menu transitions.

These are host Chromium checks, not physical iOS/Android/controller evidence,
native UI scheduling equivalence or a verified public release. Public application
and R2 publication are separate gates after the merged main artifact passes CI.
