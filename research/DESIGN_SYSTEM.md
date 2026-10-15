# ATV Offroad Fury 2 UI design system

Use `web/design-system.css` for browser-native app controls. Load it before component styles. It provides semantic color, typography, spacing, surface, focus and target-size tokens, plus the reusable `.atv-control` button. The existing authored Rust menus and original resource-backed HUD are the visual baseline.

## Evidence and visual language

The menu reconstruction in `crates/atv-core/src/ui_menu.rs` retains the recovered condensed Trade Gothic glyphs, red/silver frontend panels and yellow selected rows. Pause uses a translucent charcoal panel with a gray header. The resource-backed speed/preload HUD in `web/hud.js` uses white outlined lettering, dark meter recesses and blue/orange preload artwork. These are the reference for browser additions; the CSS values describe an authored adaptation, not a recovered PS2 theme API.

| Role | CSS token | Value / treatment |
| --- | --- | --- |
| Brand panel | `--atv-red`, `--atv-red-deep` | `#9c1414`, `#210504`; red frontend and restrained utility accents |
| Metal / edge | `--atv-silver`, `--atv-silver-light` | `#b9b9b9`, `#e9e9e9`; silver rim and highlights |
| Instrument recess | `--atv-ink`, `--atv-charcoal`, `--atv-surface` | Dark charcoal with a subtle diagonal bevel |
| Selection / direction | `--atv-gold`, `--atv-gold-light`, `--atv-gold-deep` | Menu-derived gold with a pale-to-dark selected gradient |
| Armed preload | `--atv-orange` | Orange accent adapted from the original preload meter |
| Label / focus | `--atv-text`, `--atv-focus` | Light gray text; pale gold keyboard focus |
| Rhythm | `--atv-space-1/2/3` | 4 / 8 / 12px |
| Mobile target | `--atv-control-hit` | 44px minimum width and height |
| Utility shape | `--atv-control-radius` | 4px, silver rim, restrained red lower edge |
| Touch compass | `--atv-compass-size` | 96px; two-finger drive marker remains 72px |
| Frontend screens | `--atv-gold-pill`, `--atv-gold-bullet`, `--atv-panel-silver`, `--atv-font-display` | Selection pill, row bullet, stage panel and condensed display face for HD menus |

## Typography

Original menus and HUD keep the recovered font atlas through the authorized content manifest. Browser utility labels use the `--atv-font-label` condensed fallback stack, 11px bold italic uppercase lettering and restrained tracking. They do not introduce a packaged original font or claim identical glyphs. Status cues are 9px uppercase with a dark backing; never put paragraphs inside the touch marker.

## Components and states

```html
<link rel="stylesheet" href="design-system.css">
<button class="atv-control" type="button" aria-label="Camera">
  <svg viewBox="0 0 20 20" aria-hidden="true">…</svg>
  <span>Camera</span>
</button>
```

Keep an accessible button name, an optional decorative 16px icon, and a short visible label. Idle uses dark metal; pointer hover and keyboard focus use gold. Pressed uses the menu selection gradient with dark text. Disabled retains native disabled behavior and reduced opacity. Keep the full target at least 44px even when the label is small. Place controls inside safe-area insets and above noninteractive HUD art; hide them when menus open.

The touch compass uses a silver instrument ring, faint crosshair, gold direction line and beveled gold thumb marker. Available preload highlights the ring in gold, armed preload uses orange plus the existing “LIFT FOR PRELOAD” cue, and active preload uses pale gold plus “PRELOADED”. Text communicates the state alongside color. On release the one-finger marker disappears immediately. The original 34px axis projection and input origin stay unchanged; visual clamping keeps edge touches legible. The entire overlay is noninteractive and hidden from assistive technology because it duplicates gesture feedback.

## Adding UI

Reference semantic CSS variables instead of copying colors into new components. Choose red for brand emphasis, gold for selection or direction, and charcoal/silver for gameplay utilities. Keep mobile chrome compact and outside the rider view where possible. Rust owns menu actions, labels, values, choice order, simulation state and scoring; the browser owns responsive menu layout. Frontend screens follow [MENU_SCREENS.md](MENU_SCREENS.md): red scenic backdrop, curved silver stage, gold bullet rows with a gold selection pill, and no outline boxes around selected items. Original HUD resource colors, dimensions and preload timing remain authored by their resource/Rust paths. If a new Rust menu style is required, align it with this evidence and update this file and browser tokens together.

## Safari gesture ownership

Every touch indicator descendant explicitly disables selection and WebKit callouts. While gameplay is active, nonpassive `touchstart`/`touchmove` listeners on the game canvas cancel native gestures; Pointer Events continue driving the existing input path. Listener teardown restores the surface. Menu buttons, toolbar actions and the separately loaded dev log stay outside this gate. This follows the active-listener workaround described in [WebKit’s magnifier report](https://bugs.webkit.org/show_bug.cgi?id=231161#c2); CSS alone has had loupe regressions. This is a mitigation pending verification on the user’s actual iOS version, not a claim that desktop automation can display or validate the native iOS loupe.

## Scope and verification

This checkpoint restyles Pause, Camera and the existing one/two-finger drive marker. It preserves the menu and speedometer rendering and gesture behavior. Automated Chromium screenshots and input checks cover portrait, narrow portrait and landscape layouts, keyboard focus, button actions and marker states. The older extended two-finger route check currently fails a grounded-preload assertion after takeoff, including with the unchanged pre-checkpoint touch module. A short two-finger input gate covers simultaneous drive/steer/lean/cancel. Physical iOS/Android hardware and exact PS2 visual parity remain unverified.

## Public checkpoint

The shared Pause/Camera controls and one/two-finger touch marker are in public main commit `f21f6b50af07ac12db4a39ac9e1a4c03323174b2` (PR #1; CI run [38286720860](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38286720860)). Public Chromium and WebKit mobile UI checks passed at 390×844, 320×568 and 844×390, including 44px targets, focus, camera and pause actions, scoped native touch cancellation and cleanup. Public one-handed input checks also passed saved settings, directional steering, air control and landing. These browser checks do not verify physical iOS loupe behavior or exact PS2 visual parity; the larger two-finger route assertion remains a baseline-reproduced timing failure documented in the dev log.

## Native steering range control

Rust owns the menu labels, layout and navigation choices. The browser input adapter owns localStorage persistence. Bind a native DOM `<input type="range">` over the Rust-provided rectangle for direct pointer and keyboard access, then sync its value through menu redraws without replacing the focused input. Style it with the shared gold accent and focus tokens. The mobile steering response control uses 50–150% in 5% steps, defaults to 85%, and saves one value for both touch modes; 100% preserves the previous response. The sensitivity scales displacement before the original clamp so full lock remains reachable.

### Public steering sensitivity checkpoint

The setting is public in main commit `f5ee8cddd74bc7342cccd80b9700bbf63d9b07a9` (PR #3; CI run [38289087535](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38289087535)). Public Chromium and WebKit checks passed the default and range, three viewport sizes, keyboard/drag focus and reload persistence. Chromium also passed both touch modes, unchanged lean, full lock and cancellation. The fresh 390×844 public capture shows the 85% default. Physical phone response and native iOS loupe behavior remain unverified.

## Model inspection and loading

The Quad and Rider page uses Rust-owned charcoal/silver model cards and the same
gold selection rows, with 44px physical targets across portrait and landscape.
Original models and a Rust-sampled original rider idle occupy the cards; each
view supports drag or focused left/right keys. Keep controls outside those views.
Course loading uses the selected original track photograph and actual loading
stages, with the existing condensed glyph atlas, red panel and silver photo rim.
See [selection and loading evidence](MENU_SELECTION.md) for asset and parity limits.

## Freestyle GPS instrument

The authored aerial minimap uses the recovered course terrain and static scenery
in an orthographic projection, with a small gold heading arrow and silver
north/scale lettering. It has no border or bevel. Its default footprint is
160px, capped at 42% of viewport width and 34% of viewport height. Pause → HUD
settings → Minimap configuration uses recovered menu glyphs, the existing gold range style, 44px
hit regions and persisted visibility/96–224px sizing. Resizing preserves the
geographic range controlled independently by zoom (25–200%, default 50% / 1,536ft across). See [implementation and performance](MINIMAP.md).

### Combined minimap and speed

The minimap defaults to a borderless circle inset 8px plus safe-area offsets in
the top-right corner. A square variant keeps the existing 4px corner radius;
users can switch to bottom-left. Speed uses compact italic outlined silver
lettering without a backdrop near the bottom edge, with a compact source-driven orange/blue
preload indicator. The former map-scale (feet) readout was removed; it only reflected zoom and read as a broken altitude. North and all lettering stay inside the circular safe region.
HUD settings links to a dedicated Minimap configuration group, keeping shape,
corner, size, zoom, visibility and orientation together. Tap the map to toggle
north-up/heading-up while its footer remains upright. <=360px toolbar buttons keep 44px icon
targets and accessible names while hiding labels to leave space for the map.

## In-game HUD

`web/hud.css` gives the ride HUD the same language as the HD menus: the shared
`--atv-font-display` condensed face (`--atv-font-label` now points to it), gold
accents, and short hard text shadows (`--hud-text-shadow`) for legibility over the
3D scene. Race results and the keyboard controls card use the Pause panel's dark
rounded surface, title strip and gold primary button. The canvas-drawn speed
readout and minimap read the same display font. When a large top-right minimap
would meet the labelled toolbar, the toolbar switches to icon-only 44px buttons.
The Keys button and keyboard card are hidden on touch-first devices.
