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

Original stats sliders, gear rows and the four-player colour columns are not
reproduced; the port has no matching systems yet. The Profile Editor and View
Profile pages use the main-menu layout (rows left, ride stage on the editor and
view menus) with a label/value table on the silver panel for the name entry,
profile lists, alerts, Overview, Trophies and Records (`PROFILE.md`).

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

## Game Settings (2026-10-27)

The original Options → Game Settings page (`Settings_Game`, PAGE15 labels:
Split Screen Horizontal/Vertical, Music and Sound FX Volume, Sound
Configuration Stereo / Stereo (Swapped) / Dolby / Mono, Measurement
Standard (U.S. system) / Metric, Display Mode 480p/480i, Ecosystem
Collision On/Off) is Settings → Game Settings in the port, with the rows
that have an effect on a browser or desktop build. Exact-build evidence
(bounded reading of the private Ghidra exports):

- **Measurement.** `0020f780` stores the speed factor at settings +0x564
  from the profile's flag +0x6b0: clear → 1.08 (`0x3f8a3d71`), set →
  0.68182. The speedometer multiplies ft/s by it and labels "Kph" unless
  the factor is 0.68182 ("Mph", `002e2030`). Every jump record line tests
  the factor: the race and Freestyle record hooks (`00332810`,
  `00332e30`) print "New Longest Jump %d %s!" with ft × 0.3048 and
  "Meters", and the Hall of Fame / loading pages (`002585c0`) print "%d %s"
  the same way. The port applies all of these (`hall_of_fame::Units`).
- **Ecosystem Collision** (settings +0x674). Off sets query bit 0 on the
  quad's, rider's and wheels' collision objects at setup (`001e1fd0`,
  `001e4c90`, `001e92c0`); the broad phase (`0011f2c0`, `0011f9a0`) skips
  objects of class "Vegetation" for objects with that bit. The edge launch
  sets the same bit for one second and clears it only when +0x674 is set
  (`001e5260`), so with collision off it stays set. Every race record hook
  returns early when +0x674 is clear (`00324b40`, `00324cc0`, `00324e60`,
  `00324f50`, `003249c0`, `00332810`); Freestyle's (`00332e30`) does not.
  Network play forces it on (`0020f780`, +0x7d8). Turning it off shows the
  Warning alert 0x90 (`0025e120`): "If Ecosystem Collision is turned off
  freestyle / records will still be kept, but race records will not. /
  NOTE: In i.LINK, LAN, and online play, / Ecosystem Collision is always
  on. / Are you sure you wish to turn off / Ecosystem Collision?" with
  Yes/No. The port asks the same, applies the setting to the player's quad
  and every CPU quad (the vehicle's vegetation query flag, the one the
  edge launch already used), keeps it on in multiplayer rooms, and skips
  race records while it is off.
- **Sound Configuration.** Stereo, Stereo (Swapped) and Mono apply to the
  port's mixed output (all effects, the menu tune and the desktop
  soundtrack) and to the web's streamed soundtrack, whose Web Audio graph
  then routes a splitter/merger crossover (Swapped) or a one-channel
  speakers down-mix ((L + R) / 2, Mono); the menus apply it as well as
  rides. Stereo keeps the song playing straight from its element. Dolby is
  the PS2's Pro Logic matrix surround; there is no matrix decoder behind a
  browser or desktop output, so it renders as Stereo.

Authored or not ported: the original keeps Measurement in the profile
(+0x6b0) and saves the settings with it; the port keeps one device-wide
choice like its other settings. Split Screen
(no split screen) and Display Mode (480p/480i) have no counterpart; the
volumes stay on Settings → Audio. Where the original stores +0x674 between
sessions was not traced. The port's motorcycles (not in the original
game) keep their vegetation collision.

Checks: `atv-core` `hall_of_fame` units test (Meters/Kph lines, record
messages), `atv-ui` preference and menu tests, mixer configuration test,
and `tools/test_engine_game_settings.mjs`.

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
