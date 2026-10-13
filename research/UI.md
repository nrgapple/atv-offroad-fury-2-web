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

## Direct start menu and development log

Public main `667f24bc707f5240d6927c49c65cdf6583728ad5` (CI `38188315671`)
opens directly at the title menu. Root bookmarks use replacement navigation to
`play.html`, preserving their query and fragment without an extra landing page
in Back history. Both title and main menus expose Dev log through Rust-owned
choices/hit regions, keyboard and controller selection, and touch. Public
frontend checks covered those links and return navigation, the eight-map picker,
driving/pause and 390px input with no disc picker or browser errors.

## Settings and original artwork checkpoint

The responsive Rust menu now includes paginated keyboard/controller binding screens and a 21-song selector under Pause. The browser captures keys, buttons, signed axes and combinations; Rust owns their labels, layout, selection bars and hit regions. Music preview keeps physics and engine audio paused. Menu navigation and Back remain available when gameplay actions are remapped. The menu canvas follows device scale up to 3x with an eight-million-pixel allocation cap; recovered bitmap glyphs and pictures retain their source resolution.

The original transparent logo halves from PAGE01 are assembled as artwork inside the Rust layout. A separately packaged QUADINFO thumbnail adds the original ATV to Main/Single Player at its original aspect ratio, without baking buttons or labels into the background. Older manifests omit that optional thumbnail while publication completes. This is still a responsive style reconstruction, not exact native menu behavior or a high-resolution 3D ATV selector. See [menu asset evidence](MENU_ART.md).

## Responsive transitions and shared loading presentation (host checkpoint, 2026-10-13)

The responsive menus now transition with authored 180 ms exit and 240 ms entry timings. Boot, course loading, model loading, and context recovery use a shared loading presentation; course loading includes the recovered preview image, course name, status, and animated progress bar. Reduced-motion behavior, load failure, rotation, touch controls at 390×844, 844×390, and 667×375, and animation cancellation with focus retention on resize were included in the host verification gate. The selected course-loading screenshot is recorded in the development log.

This is an authored browser transition and loading treatment using authored red/silver panels and recovered course/font resources. The transition durations do not establish original-game timing or callback fidelity. Host Chromium checks do not establish physical iOS, Bluetooth controller, or public deployment behavior. See the handoff checkpoint for the complete verification scope.

## Responsive pause and live multiplayer menu (host checkpoint, 2026-10-13)

The pause screen now presents grouped Ride, Preferences, and Session actions in a
sharp browser-native panel. Desktop uses two columns; narrow and short viewports
reflow or scroll while retaining access to Quit. Rust still supplies available
actions and stateful labels, and the menu supports keyboard focus and navigation.
The desktop checkpoint screenshot is in the development log.

Host Chromium checks covered 1280×896, 390×844, 320×568, 844×390, and 667×375.
Bounds and button separation passed, all buttons met the 44-pixel minimum, and
keyboard arrows, Tab, Enter, Controls/minimap routing, scrolling Quit access,
single-player freeze/resume, and neutral-input resume passed. In a two-client
relay run, both host and guest continued simulating and moving with the menu open;
local menu/focus input was neutralized, Controls and Players remained live, and
host departure cleared guest state. The browser menu suppresses local player
indicators while online play continues.

These are host Chromium checks with synthetic focus, not physical-phone or public
deployment verification. Hidden-tab and context-loss suspension remain in effect;
original pause-screen artwork, exact callback behavior, and full-game fidelity
are not established by this checkpoint.

## Public pause-menu verification (2026-10-13)

Public `https://furyremastered.com/` passed the single-player pause layout,
navigation, settings routes, freeze, and neutral-input resume checks across five
viewport sizes. A public two-client session kept both rides simulating with the
menu open: the host advanced 41 steps and observed 3.337 ft of remote movement;
the guest advanced 45 steps and observed 3.715 ft. Controls and Players remained
live, and host departure cleared guest state. Merge
`6f24bc2e48872010d870be8cf33b8a20fc9ae519` passed CI `38449491125`; the public
asset-hash audit matched its deployed package (42 static files and 448 served
objects). The approved desktop screenshot is in the development log. This browser
verification does not cover physical phones or establish original-game fidelity.

## Pause secondary screens polish (host checkpoint, 2026-10-13)

Ride overlays and the active multiplayer Lobby now share a charcoal browser
panel with gold highlights, while Rust continues to supply actions, labels, and
values. Host Chromium covered the six secondary screens—Controls, keyboard
bindings, controller bindings, HUD, Minimap, and Music—at 1280×896, 390×844,
320×568, 844×390, and 667×375. These screens had no horizontal clipping, and
all controls met the 44 CSS pixel minimum while remaining reachable. F7
remapping/defaults, binding pagination, song selection, music toggle and volume,
and slider node identity were checked. Synthetic controller capture remapped
Throttle to button 6 and saved it, Defaults restored the mapping, and D-pad Down
moved DOM focus from Throttle to Brake. Single-player simulation stayed frozen
while soundtrack playback advanced.

A desktop two-client host relay run with browser route overrides kept both rides
moving and simulation live while the Lobby overlay was open; Controls, Players,
Lobby navigation, and leave cleanup worked. This is host-only authored-browser
evidence, not public deployment or physical-device verification. Native pause
fidelity and full-game parity are not established. The dev log attaches the
inspected 390-pixel Music capture; the desktop bindings and portrait Minimap
captures are also retained privately for review.

## Public pause submenu verification (2026-10-13)

Pause submenu polish merged as `c1a92fa8fdb8e7ae364a220d5d642614e85c9f0a` in
PR #49. Main CI and automatic deployment passed in run `38484978361`, and
public `https://furyremastered.com/deployment.json` identified that commit.
The public actual-source pause test covered all six secondary routes at
1280×896, 390×844, 320×568, 844×390, and 667×375. Controller capture/remapping,
Escape cancellation and defaults, button 6 input, D-pad navigation, music
playback, slider stability, single-player freeze/resume, neutral input, and zero
page errors passed.

The public actual-source two-client test covered Controls and Players while the
Lobby remained active. Neutral local input, live physics and peer rendering,
continuing music, and Leave cleanup passed without suspending the AudioContext
or interrupting title music. The host advanced 42 steps and observed 3.328 ft of
guest movement; the guest advanced 41 steps and observed 3.272 ft of host
movement. This is automated browser evidence, not physical-device or full-game
parity verification. At the time of this browser checkpoint, the separate
R2/gallery hash audit and upload of the inspected pause-menu screenshot had not
yet run.
