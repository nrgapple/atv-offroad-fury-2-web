# Original UI recovery

## Current status

The host browser build renders and navigates this recovered route:

`Title → Main Menu → Single Player → Freestyle → Environment picker`

Both Caribou Peak (ENV01) and Tioga Valley now enter driving scenes. The end-to-end frontend check passed title-to-Freestyle navigation with keyboard and synthetic controller input; it also checked returning to the menu, Rust-frame pause on Start, Caribou loading from a 390-pixel touch viewport, Escape navigation, and absence of JavaScript errors or horizontal overflow. The renderer regression also passes Caribou. This route and Tioga scene are local-only until a later release; public v5 still opens directly into Caribou Peak.

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

The local screenshots show the title prompt, Main Menu, Single Player choices, and Freestyle picker. The user can move between Caribou Peak (ENV01) and Tioga Valley (ENV02) and load either connected scene. The visible Multiplayer, Training Course, Race, Championships, Waypoint Editor and other options remain menu art/layout; they are not implemented game modes.

## Track coverage and limits

The local route connects two environments: ENV01 / Caribou Peak and ENV02 / Tioga Valley. A `ruins.col` prop collision mesh exposed four NaN triangle-plane records on zero-area faces created by duplicate-coordinate vertices. The targeted parser handling preserves the original tree and triangle indices and stored plane words while skipping just those four degenerate faces; non-finite vertices and other invalid geometry remain rejected. See [collision evidence](COLLISION.md).

The frontend test covers the selected route and input cases above. It does not establish all original menus, callbacks, map coverage, race behavior, or whole-game equivalence. No PS2-hardware comparison has been made. Public v5 predates this route and second connected map; both remain host-only until a subsequent publication is verified.
