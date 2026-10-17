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
| Help | Dark translucent rounded box describing the focused row | Not shown: the original help strings describe modes the port does not have, and no invented descriptions are added |
| Hints | Black pill with PS2 cross/triangle glyphs | Console glyphs dropped (players use touch, mouse, keyboard and modern pads); a plain Back control keeps that corner |
| Select ATV & Rider | ATV and Rider are rows on one page beside a live 3D model | One page: ATV row with previous/next, Male/Female rider toggle, a grouped list of every vehicle (ATVs, Motorcycles), and the ATV and rider models together on the stage. Rider gear opens its own page from a separate outlined button beside Done |
| Select Track | Left rows, 384×256 track photo on the right | Event-type tabs (Enduro, Stunt, Nationals, Supercross, Arena), a course list with photo thumbnails and a large preview with Ride |
| Pause | Near-black translucent rounded panel, italic title strip, gold pill | Same, as one compact list over the frozen ride |
| 3D previews | Lit original models on the gray panel | Drawn opaque. The UI rider textures (`RIDEINFO.RES` `atv2_*_ui`) keep a specular/reflection mask in alpha (about 50/255 on shirt, pants and sleeves); treating it as transparency let the silver panel show through and turned the black jersey gray. Rider materials never alpha-cut |

Original stats sliders, gear rows, profile pages and the four-player colour
columns are not reproduced; the port has no matching systems yet.

## Navigation and value rows

- Value rows (race Course, Laps, CPU riders, Difficulty and every Rider gear row) have real
  previous/next buttons either side of the value on every row; tapping the label
  still steps forward. Left/Right step only value rows, so they never start a race,
  copy a link or leave a page.
- Pause and settings value rows (crashes, cameras, stats, speed rush, minimap,
  one-handed mobile, music on/off and volume) cycle on a tap and step both ways with
  Left/Right; volume stops at 0% and 100% with Left/Right.
- Only keyboard/controller focus uses the gold bar. The garage marks the current
  vehicle with a gold edge instead, and a focused gold pill (Done, Ride) gains a
  white ring.
- The garage's vehicle row is one stop for Up/Down. On Select Track, Q/E,
  Page Up/Page Down and controller shoulder buttons switch the event tabs.
- The pause list separates Ride, Preferences and Session with rules. Pager and footer
  controls are drawn as buttons with direction glyphs on Previous, Next and Back.

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

## Settings from the main menu

Settings, Controls, Display, Audio and Extras opened from the main menu use the
frontend backdrop and keep the original menu tune. The racing soundtrack plays
there only after a song is chosen or music is toggled on the Audio page. From
Pause they keep the ride panel and soundtrack.

## Loading screens

Both loaders follow the originals recovered privately from the disc (front-end
loader `LDSCREEN.RES ld_ui` and the per-course `LdScreen` layout from
`UI/LOADICON/*.LSI icon_info.ini`):

- Boot: "Now Loading..." title strip, logo on red, and a gold bar in a black pill.
  Runtime content is not yet available at boot, so the backdrop is a CSS red and
  no tip is shown.
- Course: "Now Loading..." strip, the course photograph as the action shot, a black
  "Track:" bar and "Tips:" on the gray panel, and the gold bar in a black pill.
  The bar advances with the real loading stages and never moves backwards.
- Tips are the original STARTEVT `etips.txt` riding-technique tips, read from the
  hosted archive at runtime; only their numbers are in source. Tips about profiles,
  championships, editors, mini-games or PS2 buttons are omitted.

The course loader crossfades each venue's six original action shots
(`UI/LOADICON/<group>/<course>.LSI`, frames 02-07, 256x180 pictures) every 2.5 s,
matching `icon_info.ini`'s 15-second loop. Each course's archive (~225 KB) is a
lazy hosted record, so a load fetches only its own shots; Training ships no shots
and keeps the course photograph, which is also the fallback while shots load or
when an older content manifest lacks them. The hosted reader accepts `.LSI`
records. The per-course 640x448 `LDSCREEN.RES` backdrops are not used: the original
loader's panels and bars are painted into those images, which cannot match a
responsive layout. Records ("Fastest Lap", profile points) are not shown because
the port has no saved records.
