# Original UI recovery

## Current status

The host browser build renders and navigates this recovered route:

`Title → Main Menu → Single Player → Freestyle → Environment picker`

Both Caribou Peak (ENV01) and Tioga Valley (ENV02) enter driving scenes. Public v6 now includes the title-to-Freestyle route and both maps. Its fresh public-browser test passed title/menu navigation, Tioga driving, controller input, Caribou selection from a 390-pixel touch viewport, pause/return behavior, and zero JavaScript errors. The full renderer regression also passes Caribou. Release identity and public checks are recorded in [deployment notes](DEPLOYMENT.md).

Public v7 adds a pause overlay based on the `[Main Menu]` section of `OVERLAYS.RES`. It shows Resume, Reset ATV, Controls, Fullscreen, Dev log, and Quit to Main Menu over the running scene. Public checks confirmed that pausing freezes the Rust simulation frame and that Resume, Controls/back, and Quit work. The driving toolbar is reduced to Pause and Camera. The panel uses original art but its browser callbacks are authored adaptations, not the original pause callback implementation.

This is a connected route through selected original pages, not a full recreation of the original front end. Other menu callbacks and modes, race HUD, race flow, profiles, championships, and most environments are not yet implemented. The title prompt now sits clearly between the logo and legal text. The browser runs authored Rust/WebAssembly code and is not a PS2 emulator.

## Resource and layout evidence

The resource inventory covers 59 page archives in `/UI/RESOURCE/PAGEA` and `PAGEB`. Rust readers decode all 120 page layouts and all eight original UI fonts. Layout records provide page labels, widget placement, and control groups. CEL records provide glyph UV rectangles and metrics. ATM records provide animated menu textures.

| Evidence | Observation | Limit |
| --- | --- | --- |
| Page archive inventory | 59 page archives; inventory saved privately at `private/reports/ui-page-inventory.json` | These pages are part of the original UI recovery target; most are not yet rendered or connected. |
| Layout and text | 120 layouts and labels decode and feed the local route renderer | Navigation is connected only for title → main → single-player → freestyle → environment selection. |
| Quoted strings | Three malformed quote records are retained and diagnosed. Helper `0x00141ca0` strips boundary quote characters independently and does not check whether the pair is balanced. | Preserve the source records and distinguish this observed helper from validation of the string format. |
| Control groups | Loader `0x00232c30` iterates group and control counts as inclusive maxima (`0..=NumberOfGroups`, `0..=NumberOfControls`); widgets are 1-based. | This loader behavior does not establish every screen transition or focus rule. |
| Fonts | Font loader `0x001ccb50`; all eight original UI fonts decode. | Font decoding alone does not guarantee final alignment or fidelity on every page. |
| Animated menu textures | ATM version 1 has nonzero frame counts (21 for `menu01` and `menu02`), zero flags, then concatenated indexed PST frames. | Full original timing and page behavior remain unverified. |
| Pause overlay assets | `OVERLAYS.RES`'s `[Main Menu]` section supplies 14 panel/navigation icons and three selection stretches; the display also uses original fonts and textures from track archives. | A stray comment at line 2109 lies outside the `[Main Menu]` slice and must be handled before decoding. Reusing art does not establish full pause-screen fidelity or original gameplay callbacks. |

The screenshots show the title prompt, Main Menu, Single Player choices, and Freestyle picker. The user can move between Caribou Peak (ENV01) and Tioga Valley (ENV02) and load either connected scene. The visible Multiplayer, Training Course, Race, Championships, Waypoint Editor and other options remain menu art/layout; they are not implemented game modes.

Public v7's pause test passed on desktop Tioga and a 390-pixel Caribou session: the Rust frame stops while paused, and Resume, Controls/back, and Quit to Main Menu respond. The pause screenshot documents the panel and original selection art. Only Pause and Camera stay on the driving toolbar; Reset ATV and Fullscreen are choices in the pause panel. Public touch checks passed right-half hold/lean/release, cancellation, simultaneous left/right input, brake/reverse/forward transitions, pause/focus, and narrow layout with 44-pixel targets. They suppressed text selection on the game surface while leaving the development log selectable; details are in [touch riding controls](TOUCH.md).

## Track coverage and limits

The route connects two environments: ENV01 / Caribou Peak and ENV02 / Tioga Valley. A `ruins.col` prop collision mesh exposed four NaN triangle-plane records on zero-area faces created by duplicate-coordinate vertices. The targeted parser handling preserves the original tree and triangle indices and stored plane words while skipping just those four degenerate faces; non-finite vertices and other invalid geometry remain rejected. See [collision evidence](COLLISION.md).

The frontend test covers the selected route and input cases above. It does not establish all original menus, callbacks, map coverage, race behavior, or whole-game equivalence. No PS2-hardware comparison has been made. Public v7 contains the route, two maps, pause panel and revised mobile controls. Physical iOS behavior and complete original pause/stunt behavior remain unverified or incomplete.

## Responsive Rust menu reconstruction (2026-10-11)

The authored `ui_menu` module now emits full-viewport draw commands and matching
hit regions through WASM operation 62. A Canvas browser bridge rasterizes those
commands and dispatches input. Curved red/silver frontend panels, charcoal pause
panels, yellow selection bars and the recovered italic/condensed font atlases
replace fixed 640×448 page backgrounds. Title typography is authored text, not
the original logo artwork; original scenic backdrops and animated selection
materials are not yet reproduced. This is a responsive reconstruction of the
original visual style, not pixel-exact native UI rendering.

Only implemented choices are exposed: Main Menu → Single Player → Freestyle,
eight environment previews, and the working pause commands. Unsupported racing,
multiplayer, training, profiles and other modes are absent rather than selectable
placeholders. Fullscreen is omitted where the browser reports it unsupported.
The original track photographs and font glyphs remain separate runtime assets;
no complete menu screenshot or page background is used to render these screens.

Host Chromium validation passes controller and keyboard navigation, all eight
picker entries, Tioga driving, frozen simulation while paused, Controls/back,
390×844 touch selection, Caribou reload, and no browser errors or disc prompt.
Portrait menu bounds fill the viewport and exposed touch targets measure at
least 44 CSS pixels in height. Rust tests cover supported choices, invalid
requests and hit-region bounds at five portrait/landscape sizes. Late worker
layout responses are discarded after a route, selection or viewport change.
The frontend also retains its poll loop correctly across persisted pagehide /
pageshow; physical iOS Safari remains a separate validation target.
