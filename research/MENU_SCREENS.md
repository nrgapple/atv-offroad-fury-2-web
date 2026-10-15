# HD frontend screens

The title, main, single-player, multiplayer, lobby, garage, course, race and
loading pages are browser-native HTML screens (`web/menu-screens.js` and
`web/menu-screens.css`). Rust still owns the supported actions, their labels and
values, and the keyboard/controller choice order through `ui_menu::layout`.
The browser owns responsive layout, which matches the existing Pause panel split.
No frontend page draws into the menu canvas any more; it is still sized on each
layout because transitions and browser tests read it.

## Original reference

The composition was taken from the original pages, rebuilt privately from the
exact-build page layouts, textures and fonts (reference renders and notes stay in
ignored `private/reports/original-menu-reference/`). No PS2 emulator or BIOS was
used; these are approximate composites, not captures.

| Original element | Observation | HD treatment |
| --- | --- | --- |
| Backdrop | Full-screen 640×448 red-tinted scenery per page | PAGE01 `bk_mainmenu` blurred behind every screen; course pages use the focused course photo, grayscaled and multiplied with red |
| Stage panel | Light-gray panel sweeping in from the right with a large curved edge and soft shadow | Vector silver panel (skewed rounded edge); on portrait it sits above the controls with a curved lower edge |
| Title | Bold condensed italic over a white-to-transparent strip | Same, in a system condensed face |
| Rows | Gold bullet; focused row grows into a glossy gold pill running off the left edge | Two-tone chrome-gold bar with a hard highlight split, small 5px corners and short hard shadows; no outline boxes or soft blurs |
| Help | Dark translucent rounded box describing the focused row | Plain text with a gold rule |
| Hints | Black pill with PS2 cross/triangle glyphs | Console glyphs dropped (players use touch, mouse, keyboard and modern pads); a plain Back control keeps that corner |
| Select ATV & Rider | ATV and Rider are rows on one page beside a live 3D model | One page: ATV row with previous/next, Male/Female rider toggle, a list of every vehicle, and the ATV and rider models together on the stage |
| Select Track | Left rows, 384×256 track photo on the right | Event-type tabs (Enduro, Stunt, Nationals, Supercross, Arena), a course list with photo thumbnails and a large preview with Ride |
| Pause | Near-black translucent rounded panel, italic title strip, gold pill | Same, as one compact list over the frozen ride |
| 3D previews | Lit original models on the gray panel | Stronger key/fill lighting with a mild saturation and contrast grade, so pale rider suits read on silver |

Original stats sliders, gear rows, profile pages and the four-player colour
columns are not reproduced; the port has no matching systems yet.

## Artwork limits

Course photographs are 384×256 with black or transparent mats from their page
placement. The browser joins the two halves once, trims near-black edge rows
and columns, and serves a blob URL. They are scaled smoothly, never presented as
new high-resolution art. Panels, pills, bullets and glyphs are redrawn as vectors
from sampled colours instead of stretching the small original textures.

## Responsive rules

- Landscape: controls left, stage right. Portrait (and widths up to 700px): stage
  on top, controls below, hint row across the bottom, inside the safe-area stage.
- Mouse hover selects only on real pointer movement, so reflow under a resting
  pointer never steals keyboard/controller selection.
- Every target is at least 44 CSS px. Long lists (courses, vehicles) scroll inside
  their column; the rest of the page never scrolls horizontally.
- Very short portrait and landscape phones drop help text and secondary labels
  before shrinking targets.
- The viewport helper releases its previous `--app-width/height` before measuring,
  so live narrowing and rotation shrink the layout on mobile browsers.

## Verification

Host Chromium only: `tools/test_frontend.mjs` and `tools/test_menu_models.mjs`
pass (six viewports including 390×844 and 320×568, all 26 quads, rider changes by
touch, keyboard and controller, model/control separation, loading and driving).
Physical phones, Safari/WebKit and exact original appearance are not verified.
