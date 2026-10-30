# Local split screen

The user wants local split screen so several people can play on one TV with
several controllers (status.md "Product decisions", 2026-10-27). Every mode
that works split screen must later also work in online rooms. This note
records the original evidence, the port's strategy, what slice 1 delivers
and what is left.

## Original evidence

Observed facts, exact USA v2.00 `SCUS_972.11` and its disc resources:

- **Manual** (printed pp. 16-17, see [MULTIPLAYER_ORIGINAL.md](MULTIPLAYER_ORIGINAL.md)):
  two- to four-player split screen on one console. Two-player split screen
  can include CPU riders (except Duel); three and four players exclude them.
  Games are "Split Screen Only".
- **Menu flow** ([GAMES.md](GAMES.md)): Main Menu → Multiplayer → Split
  Screen → `MultiPlayerSelect` (Race, Freestyle, Games) → TrackSelect →
  `MultiATVRiderSelect` (archive PAGE08), where the players claim their
  seats (see "Players page"). `QuadzillaUIPageControllerSelect` (archive
  PAGE16) is not a split-screen step: it is Options → Controller Settings'
  pad picker (see "Players page").
- **Layout setting**: Options → Game Settings (`Settings_Game`, PAGE15)
  labels "Split Screen Horizontal/Vertical" ([MENU_SCREENS.md](MENU_SCREENS.md)).
- **Cameras**: the follow-camera loader `003029a0` reads `FollowCamPresets.pb`
  sections by a layout enum: `Preset<n>` single, `Preset<n>h` horizontal
  split, `Preset<n>v` vertical split, `Preset<n>q` quarter screen
  ([CAMERA.md](CAMERA.md); decoded privately in
  `private/reports/camera-presets-{horizontal,vertical,quarter}.json`). The
  horizontal chase preset keeps FOV 80 at 20 ft; the vertical one narrows
  to FOV 50 and raises its angle to 1.01 rad at 13.5 ft.

Not traced (assumptions, labelled where used): the split viewport
rectangles and divider, the per-player HUD layout, how the race waits for
the second human (the seat claiming rules were traced later: "Players
page").

## Strategy

Split screen reuses the machinery that already runs several riders:

- **Two full rides.** Each player gets its own `Riding` (full recovered
  physics, stunts, crashes, camera, bindings) on the shared venue and scene.
  Player 2 is drawn through the scene's multiplayer rider slot (`split::PEER`),
  exactly as a room's rider is, and each player is the other's authored
  rider-contact body (`atv_core::peer_contact`, the rooms' rule).
- **Two views.** `SceneRenderer::render_views` prepares and submits the
  scene once per view (so each view's uniforms reach only its own draws)
  into its own viewport/scissor rectangle of one frame, then draws the UI
  once. Speed rush and air blur are full-frame effects and are skipped.
- **Cameras.** Both rides switch to the original split presets
  (`Ride::set_camera_layout`, layout 1 or 2). The single-screen port keeps a
  preset's FOV horizontal at 4:3; a split view keeps it horizontal at its
  reference half-frame aspect (8:3 top/bottom, 2:3 side by side) and keeps
  that vertical framing as the view widens. That reference aspect is an
  assumption (half of the PS2's 4:3 frame).
- **Input.** Player 1 plays the first connected controller and the
  keyboard's left side (its normal bindings); player 2 plays the second
  connected controller and the keyboard's right side (arrows drive, `.` `/`
  or right Shift lean, Numpad 1-3 stunt, Numpad 0 preload, `'` camera,
  Delete reset). Slots follow connection order every frame, so plugging in or
  pulling out a controller moves players to the pads that remain. The
  keyboard halves are a port adaptation (the original seats every player at
  a pad). Either player's Start opens player 1's Pause menu over both views.
- **Races.** Player 1's race holds the CPU field; player 2 races its own
  copy of the course with no CPUs, starting in the grid slot after the
  field's last. Each frame `split::merge_race` adds player 2 to player 1's
  standings (finished riders by time, the rest by race distance) and gives
  both players their combined place. The results wait until both have
  finished. Split races skip the pre-race camera tour so both countdowns
  start together. The CPUs do not steer around player 2, and when player 1
  finishes first player 1's race estimates the CPUs' times then (the
  original waits for every human): both are authored approximations.
- **HUD.** Each player's speedometer, stunt text and race overlays are
  painted for its own view and moved into place. The GPS minimap is hidden
  while split.

## Slice 1 (2026-10-28)

- Main Menu → Multiplayer → **Split Screen** → Race, Freestyle, and
  **Screen: Top and Bottom / Side by Side** (saved as `atv-split-screen`;
  the original keeps it in Game Settings). Freestyle uses the Select Track
  page and Race the Single Race setup (laps, CPUs, difficulty); Back returns
  to Split Screen, and leaving the branch rides one player again.
- `?split=h|v` (web) or `FURY_SPLIT=h|v` (desktop) starts a direct split
  ride (tests, measurements). `#ui-state.split` reports both players.
- Both players use the Garage ATV, rider and gear (player 2's own garage is
  slice 2). Player 2 has no engine sound, wheel dust, rumble, Hall of Fame
  or profile credit yet; player 1's ride owns those. The fine terrain patch
  follows player 1; player 2's view shows the coarser terrain beyond it.

Checks: unit tests for layouts, keyboard halves, controller slots, the
standings merge and the menu branch; `tools/test_engine_split_screen.mjs`
drives both players from the menu (Freestyle top/bottom with keyboard and
two stand-in controllers, then a one-lap side-by-side race to combined
results).

## Slice 2 (2026-10-28)

- **Player 2's own garage.** The Split Screen page adds **Player 2 ATV**
  (cycles the owned original ATVs 1-26; rooms and the player 2 rider slot
  carry ATVs only) and **Player 2 Rider** (Male/Female), saved as
  `atv-split-player2` ("quad,rider"). Player 2's gear and paint stay the
  profile's. The original seats each player at a pad with its own Garage
  choice; these two rows are a port adaptation of that (the per-player
  Garage pages were not traced). `?split2=quad.rider` (web) or
  `FURY_SPLIT2` (desktop) picks it for a direct split ride.
- **Sound.** Player 2's engine and body impacts play through the room
  riders' voices (`audio::remote`), positioned and heard from player 1's
  camera on the one output. The original's split-screen mix was not traced.
- **Wheel effects.** Player 2 has its own wheel dust, chunks and spray
  (a second `WheelEffectsPass`, drawn in both views). Tire tracks and
  Enhanced lingering dust remain player 1's.
- **Rumble.** Each player's impacts rumble its own controller slot
  (desktop: gilrs force feedback on the slot's pad; web: `dual-rumble` on
  the slot's browser pad, so two identical controllers are told apart).
- **Shared race.** Before each frame `split::link` ties the two races:
  player 1's CPUs treat player 2 as a human peer (steering around it, as a
  room's human, and pushing against it through `peer_contact`), player 2
  rides against player 1 and every CPU, and both races' PLACE sign, finish
  sign and race-over music read the combined place. Player 1's race no
  longer ends (and estimates CPU times) at player 1's finish while player 2
  still races (`Race::waiting`; RaceStatus `00248b20` waits for every
  human): the clock runs on and CPUs finish for real.
- **Terrain.** Each player gets a fine terrain patch
  (`terrain_mesh::disjoint`: patches that overlap or touch merge into their
  bounding box so every patch keeps a coarse border to stitch to); the
  patches rebuild when either player leaves 48 cells of its patch. A port
  choice: the original's split-screen terrain LOD was not traced.
- **Graphics loss.** A restored WebGL context rebuilds player 2's rider,
  wheel effects and terrain patch; the fallback reload link
  (`fury-resume`) carries `split=` and `split2=` so a reopened ride is
  still split. Desktop device loss reopens the ride through the kept split
  request, as do Restart Race and direct `?split=` rides.

Checks: unit tests for patch merging, the race wait, query links and the
Player 2 rows; `tools/test_engine_split_screen.mjs` also chooses player 2's
ATV and rider, checks its ride uses them, its wheel effects emit, the
terrain patches, the combined finish place and per-slot rumble on two
identical stand-in controllers.

Host frame time: see "Measurements" below.

## Slice 3 (2026-10-28): Games

The original's Games are "Split Screen Only" (manual p. 17) and live under
Multiplayer → Split Screen → `MultiPlayerSelect` → Games (GAMES.md). Exact-
build evidence used:

- Each game's rules keep a record per rider: King of the Hills
  (`00220150`, `00220b50`: per-rider state, hills held, the capturing
  rider's colour from +0x210 red/blue/green/yellow and "ATV %d captured
  hill %d"), Treasure Hunt (`002d0d70`: the collecting rider's count),
  Hockey (two team records at +0x1c4, each session rider on the team
  chosen on Hockey Team Select) and Tag (four rider records).
- The ranking sorts riders by the game's count, ties by stunt score
  (`00220e70`, `002d0f20`, `002fa120`), and the results pages list places
  1-5 (`002ec310`).
- The game TOYs' `[SceneInfo] NumStartPositions` / `Start%d` /
  `Direction%d` give one start per player slot.

What the port does:

- **Reaching it.** The Split Screen page adds **Games** after Freestyle
  (the original's MultiPlayerSelect order). It opens the same Games page
  (Game, Course, Time Limit, Start Game) with two players marked; Back
  returns to Split Screen, which rides one player again. Locks are the
  profile's, as on Single Player → Games.
- **King of the Hills and Treasure Hunt.** One `atv_core::games::Session`
  runs the rules for two riders (`rules_for`): player 1 is rider 0 and
  player 2 rider 1. Every frame both rides are sampled (position, ground
  contact, open stunt, each player's own finished chain and committed
  stunt score), so hills and treasures count for whoever earned them; a
  captured hill takes the capturer's colour (player 2 blue) on its flag,
  pill and compass icon. Player 1 starts at `Start1`, player 2 at `Start2`.
  The private sweep of all 28 courses finds at least two starts on 18;
  ten mode TOYs list none (KOTH env04, free03, free04, free05; Treasure
  Hunt env02, env04, env08, free01, free03, free05), so player 1 keeps the
  venue's ride start (remembered for Restart Game) and player 2 rides ten
  feet to its right. Both ATVs are held through the 321 lamps and after
  the end; Restart Game puts both back.
- **Hockey.** Player 1 skates for Red and player 2 for Blue, each placed at
  its own face-off spot (`Game::face_off`, the rooms' semicircle rule);
  both ATVs push the puck and both are held at each face-off. A port
  choice: the Hockey Team Select page was not traced, so the teams are the
  rooms' alternation. CPU skaters are still not connected.
- **HUD.** Each view gets its own game panel (game, TIME, TOKENS/HILLS,
  PLACE and its own stunt SCORE; Hockey's team panel names "Player N is
  Red/Blue"), its own compass (Overlay Sets 6/7 object 31, drawn at the
  view's size), its own world tags and hill pills projected with its own
  camera and view aspect, and the start lamps. Signs ("ATV 2 captured
  hill 1", "All Treasures Collected") show in both panels. Where the
  original's split-screen HUD sits per viewport was not traced; the port
  reuses the single-screen layout scaled to each view.
- **Results.** "Race Results" lists both players by place as Player 1 /
  Player 2 (the split race standings' names) with their count and stunt
  score;
  Hockey Results lists each player's goals with the team score.
- **Tag** is slice 4 below. Freestyle objectives are split since "Split
  objectives" below.

Checks: `atv-core` unit tests `split_screen_players_collect_and_rank_separately`
(Treasure Hunt: per-player counts, progress, places and the stunt-score
tie order through `Session`), `split_screen_rules_keep_a_record_per_player`
and the private sweep `every_original_game_course_resolves` (both games'
two-rider rules on all 14 venues each, and their start counts);
the Split Screen menu unit test covers the Games row;
`tools/test_engine_split_games.mjs` (content group) plays Treasure Hunt
top/bottom to "All Treasures Collected" with both players collecting
(`gameTest=visit` alternates players) and results listing both, King of
the Hills side by side where player 2 captures hill 1 through the real
trigger enter/leave (`gameTest=hill2` lands the chain for player 2), and
split Hockey (`hockeyGoal=1` scores player 1's goal; a 0-0 game goes to
sudden death) on its 20 s clock to "Red wins!" and results listing both
players' goals. Host Chromium only; player 2 keyboard halves and second
pads are the slice 1 input path (`test_engine_split_screen.mjs`), not
re-driven in the games test.

## Slice 4 (2026-10-28): Tag

Split Screen → Games → Tag's Start Game plays Tag on this console instead
of hosting a room. Evidence: Tag's four rider records (slice 3 list) and
its `*_TG.RES` courses (GAMES.md "Tag").

- **Course.** The ride loads the venue's Tag course (`*_TG.RES` over the
  Freeride archive: the mode TOY's start and `tagball.psg`), the same scene
  a room's Play Tag reloads to. Player 2 starts ten feet to player 1's
  right (the split Freeride spot), not on the overlapping room start.
- **Rules.** The room code runs it: `TagSession::new_local` is a host
  session with roster `[0, 1]` and no `Net`. Both rides feed
  `atv_core::tag` every step (possession, scoring, knock loose from each
  player's own trick starts, the carrier engine handicap on whichever local
  ATV carries, the boundary throw-in, ground respawn, tie order and the
  Games page time limit); nothing is relayed.
- **HUD.** Each view draws its own Tag panel (clock, its own score, "You
  have the ball" or "Player N has the ball: tag them!"); the carrier
  banner names the player in both views. The ball's minimap marker shows
  in both views.
- **Results.** The results, titled "Race Results" like the disc's
  `[TagOver Menu]` (Overlays.pb `Text1Text`), list Player 1 / Player 2 by
  score with Restart Game (a new game, both players back at the Tag start)
  and the results page's Continue row, which returns to the Games page as
  the other games do;
  Pause → Restart Game restarts too.

Port choices: the original's split Tag start spread and per-viewport HUD
placement were not traced. Check: `tools/test_engine_split_tag.mjs`
(content group) plays Prospect Springs' Tag top/bottom with `tagReach=60`
(both parked players tag each other, so both carry), a 30 s clock
(`tagTest=fast`) to the Tag results naming both players, then Restart Game.
Host Chromium only.

## Three and four players (2026-10-28)

Exact-build evidence (USA v2.00 `SCUS_972.11` and `pause/Overlays.pb`):

- **Which modes.** The manual (pp. 16-17) offers two- to four-player
  split screen for Race, Freestyle and Games; three and four players
  exclude CPU riders, and Duel is two players only (the port has no split
  Duel). The executable's controller alerts name the Multitap ("Controller
  %d or the Multitap (for PlayStation 2) ... has become unplugged") and the
  lobby strings count "(%d/4 players)".
- **Cameras.** `003029a0`'s layout 3 loads the quarter-screen presets
  `Preset<n>q` (decoded privately in `camera-presets-quarter.json`: chase
  FOV 70 at 13.5 ft, angle 1.01 rad).
- **HUD.** Every overlay set in `Overlays.pb` has
  `ScreenLayout-3Player` and `ScreenLayout-4Player` sections beside the
  1-player and two 2-player ones. Their per-player overlays list one
  position per player on the 640x448 frame in quarter order: player 1
  top left, player 2 top right, player 3 bottom left, player 4 bottom right
  (lap time at 20,23 / 615,23 / 20,231 / 615,231; speedometer at 20,155 /
  597,155 / 20,372 / 597,372). The 321 lights and the radar are single,
  centred overlays (256,208 and 251,159). The three-player layout lists the
  same positions for three players and one "CHEESY GPS" overlay (object
  32) with no position; whether that map fills the empty quarter was not
  traced.

What the port does:

- **Menu.** Split Screen adds **Players: 2/3/4** after Games (saved as
  `atv-split-players`). Three and four players hide the two-player
  **Screen** row (they always share quarters) and add **Player 3 ATV/Rider**
  and **Player 4 ATV/Rider** rows (`atv-split-player3`, `atv-split-player4`;
  "quad,rider", like player 2's). `?split=q&splitn=3|4` with
  `split2=`..`split4=quad.rider` (desktop `FURY_SPLIT=q`, `FURY_SPLITN`,
  `FURY_SPLIT2`-`4`) starts a direct ride; the context-loss resume link
  carries them.
- **Views.** `split::Layout::Quad` cuts the frame into four quarters with
  the 2 px dividers, in the original's quarter order. Three players leave
  the bottom-right quarter dark (assumption: see the GPS note above). Each
  view presents at 4:3 (the quarter's reference aspect, so the quarter
  presets' FOV is used as authored).
- **Players.** `Ready.split` holds players 2-4, each a full `Riding`
  (physics, stunts, crashes, camera, bindings) drawn in its own scene
  rider slot (`split::peer`: 250-252), with its own wheel dust/spray pass,
  fine terrain patch (all players' patches merge when they touch), rumble
  on its own pad, and engine/impact voices through the room riders' mixer.
  Every local player is every other's contact body (lower player number
  takes the -1 tie side). Players 2-4's riders are posed in one
  `pose_peers` call per frame (the call hides every slot it is not given;
  posing each player separately had left only the last player's rider
  drawn, so in games players 2 and 3 could not see their own ATV).
- **Input.** Player N plays the Nth connected controller; the keyboard's
  halves stay players 1 and 2 (players 3 and 4 need pads). Any player's
  Start opens player 1's Pause menu over every view.
- **Races.** Three and four players race with no CPUs (manual); the
  Single Race page's CPU count is ignored for them. Each player races its
  own copy of the course from the grid slots after player 1;
  `split::merge_race` adds players 2-4 to player 1's standings and gives
  every race the combined place, field and opponents (PLACE trees, finish
  signs, music). No race is over until every player has finished; results
  list all players.
- **Games.** King of the Hills and Treasure Hunt run one `Session` for all
  players (`rules_for(.., players)`; hills take the capturer's colour from
  the original red/blue/green/yellow table), each player at its own TOY
  `Start%d` (or beside player 1 when the TOY lists fewer). Hockey seats
  players 1 and 3 on Red and 2 and 4 on Blue (the rooms' alternation; Team
  Select was not traced) at their own face-off spots. Tag runs
  `TagSession::new_local` with up to four rider records (the original's
  four Tag records). Each view gets its own game panel, compass and world
  tags; the results list every player.

Port choices and gaps: the empty third-player quarter is dark; Hall of
Fame and profile credit are player 1's; players follow controller connection order (the
Players page, below, now seats them). The
`ScreenLayout` HUD positions, per-seat gear/paint and per-player tire
tracks and dust followed (see "Per-player fidelity").

Checks: `split.rs` unit tests (quarter rects and order, quarter camera
layout, player names and scene slots, Freeride starts for players 3/4, a
four-player standings merge with no CPUs), the Split Screen menu unit test
(Players row cycles 2/3/4, rows for players 3/4, saved), and
`tools/test_engine_split_4p.mjs` (content group, host Chromium, four
stand-in pads): the Players row, four-player Freestyle in quarters with
players 3 and 4 each driving only on their own pad, a one-lap four-player
race with no CPUs to results listing all four, and four-player Treasure
Hunt to results listing all four; three-player King of the Hills, Hockey
and Tag each start in quarter views with players 2 and 3 drawn
(`split.ridersShown`).

## Per-player fidelity (2026-10-29)

Exact-build evidence (USA v2.00 `SCUS_972.11`, `pause/Overlays.pb`):

- **Layouts.** Every overlay set heads its layouts with
  `NumOverlays-ScreenLayout-1Player`, `-2Player-Horiz`, `-2Player-Vertical`,
  `-3Player` and `-4Player` (the executable's format strings
  `%s-ScreenLayout-<layout>` / `NumOverlays-ScreenLayout-<layout>`), and
  `002edf00` builds each `OverlaySet<n>-ScreenLayout-<layout>-Overlay<i>`
  from its `OverlayObject`, `NumOverlay` and `Overlay1`..`Overlay4`
  positions. Split layouts use multiplayer objects: 19 lap time, 20 lap,
  21 score, 23 infotext, 24 stunt text (three and four players; two use
  12), 33 speed meter (three and four players; two use 5), 35 signage
  (`SignageName=Multi-`, the `Multi-Sign-N` sections), 41 place, 31 compass,
  38/39 the horizontal and vertical separators, and the single overlays 10
  (321 lights at 256,208) and 13 (radar). `AlterJustification=2` with
  `Overlay<k>Text<j>Justification` re-justifies the left-hand players' text
  (player 1 and 3 LAP TIME left-aligned at 20,23 / 20,231, player 2 and 4
  right-aligned at 615). Object 33 has `NumText=0`: three- and four-player
  speed meters show the preload/speed bar without the digits. Positions
  differ by set: the Race set puts the quarter meter at 20,155 (left), the
  Stunt and game sets at 285,155 (right side of the quarter).
- **CHEESY GPS.** Object 32 (`Type=CheesyGPS`, string at `0x43c168`) is
  not a map. `002edf00` maps the type to overlay kind 15 and drops it
  (count 0) when a global flag is set; its `Text1Value=25` is the value
  formatter `002dce10`, which prints `"%4.2f, %4.2f, %4.2f"` from the
  player's position (+0x720..0x728). It is a debug coordinate readout at
  10,10, listed in the one-player layout too, so it does not fill the
  three-player layout's empty quarter. The three-player layout lists no
  overlay for that quarter (only the full-length separators 38 and 39).
  What the renderer draws there was not traced; the port keeps it dark.

What the port does:

- **HUD positions.** `atv_ui::hud::assets::split_layouts` reads the split
  layouts of sets 1 (Race), 2 (Stunt: Freestyle), 6-9 (Treasure Hunt, King
  of the Hills, Tag, Hockey) and 11 (Enduro) from the original Overlays.pb,
  merging each overlay's texts and per-player justification overrides into
  its object section. `Meter::paint_seat` (`hud/split.rs`) draws each
  player's speed meter, stunt text, compass and, in races, LAP TIME, LAP,
  SCORE, PLACE, infotext and Multi-Sign signs at its layout position. Each
  player's part of the 640x448 frame (a half or a quarter) maps onto its
  view scaled by height; on wider views left-third items keep the left
  edge, right-third items the right edge and the middle stays centred (the
  single-screen rule), and the top-right view's upper right items sit
  under the port's toolbar. The 321 lights draw once, centred on the
  whole frame (object 10). Two players use the `2Player-Horiz`/`-Vertical`
  layouts the same way. `#ui-state.hud.split` lists each player's drawn
  objects and anchors.
- **Gear and paint per seat.** The Split Screen page adds **Player N
  Gear** (Original and each owned jersey line of the Garage catalog, with
  its matching sleeves and pants) and **Player N Paint** (the Garage's
  paints) after each player's ATV and Rider rows, saved as
  `atv-split-look<n>` ("g0,..,g6,paint"). Each seat's ride is dressed and
  its scene slot painted with its own look. `?look2=..&look4=` (web) or
  `FURY_LOOK2`-`4` (desktop, eight dot-separated numbers) set it for a
  direct split ride; the resume link carries it. A port adaptation: the
  original's per-player Garage pages were not traced, and the gloves,
  boots, helmet and goggles stay Original for players 2-4.
- **Tire tracks and lingering dust per player.** On Enhanced/High each of
  players 2-4 gets its own track builder and dust emitter (and its own
  Heavy mud zones), uploaded to its own `Trails` passes in the renderer and
  drawn in every view after player 1's. `?splitTrails=0` keeps them player
  1's (host measurements).

Not built: records/Hall of Fame and profile credit for players 2-4 need a
product decision (whose profile they credit). The Multi-Sign animation uses
the single-player sign timing. (The split radar and object 24's stunt-text
clock followed in "Players page, radar and stunt text".)

## Players page, radar and stunt text (2026-10-29)

Exact-build evidence (USA v2.00 `SCUS_972.11`, private Ghidra exports of
the functions named; page data from the extracted `UIPages`):

- **ControllerSelect is the controller-settings pad picker.** Its vtable
  (`0045c210`, class string at `0x436720`) overrides three slots. The pad
  handler `00286420` takes Triangle on pad 1 as Back and Cross on pad *n*
  as "configure pad *n*": it stores the pad (game state +0x1a0f) and opens
  page 0x11, ControllerSettings. The update `00286510` shows each of the
  four `Player N` panels in its connected (rotating `controller.slt`, full
  alpha) or unplugged (`controller_alpha.slt`, alpha 96) state from each
  pad's connection query. MENU_FLOW's Options → Controller Settings → 16
  (then 17) is this path. The port's Controller Settings already picks the
  pad (CONTROLS.md), so the page is not reproduced.
- **MultiATVRiderSelect is the split-screen claiming page.** Vtable
  `0045c810` (class string `0x436900`). The pad handler `0027a0a0` maps
  each pad to its own player (`0025bb10`). A player that has not joined
  takes only START (0x32), which joins it (`0027e780`: joined flag
  +0x554[n], ready +0xa64[n] cleared, focus on its first row). A joined
  player's nine rows per panel are ATV (1), Rider (2), Gear (3), Helmet
  (4), Goggles (5), Gloves (6), Boots (7), the ATV stats row (8) and its
  ready control (9); D-pad up/down (0x25/0x26) move, left/right
  (0x23/0x24) change the value, and locked choices refuse with the error
  sound. START on a joined player marks it ready (`0027eb70`: sound 9,
  ready flag, panel state 3) when its choices are valid; START again takes
  it back. While ready, only START reaches the panel. Triangle on pad 1
  saves player 1's look to the profile and leaves the page; other pads'
  Triangle does nothing offline. Page entry (`00278c50`) joins player 1
  only, and an unplugged pad's panel shows state 6.
- **Start rule.** The update `0027d300` counts joined players; with fewer
  than two the Start/Force Start group is hidden. `0027e230` returns the
  joined count only when player 1 has joined and every joined player is
  ready; above one, `0027e360` starts the event (GoToPage 0x3c) and copies
  each joined player's ATV, rider and seven gear slots into consecutive
  rider slots, so the seated players are compacted in pad order, and
  stores the count (+0x10).
- **Radar.** Overlay Set 1 (Race) lists object 13 (`Type=Radar`) once per
  layout: `1Player` 20,305, `2Player-Horiz` 20,165, `2Player-Vertical`
  256,305, `3Player`/`4Player` 251,159. With `NumOverlay = 1` it is a
  single radar the whole frame shares, at the divider or the views'
  crossing. No game set (6-9), Freestyle (2) or Enduro (11) lists it. The
  radar constructor `002e6160` adds a racer per rider: CPUs (rider index
  ≥ the human count, offline) white 0xffffff, humans 0 red 0xff0000, 1 blue
  0xff, 2 green 0xff00, 3 yellow 0xffff00 (`002e6390` stores the colour
  on the `RadarIcon_Racer` icon).
- **Stunt text.** Object 24 (three- and four-player stunt text) lists the
  same ScaleUp 0.05, ScaleStay 0.25, ScaleDown 0.05, ColorChange 0.25 and
  AlphaOut 1.0 s as object 12, with ScaleSize 1.5 instead of 2.0 and text
  scales 0.7/0.8 with a 40-unit second line.

What the port does:

- **Players page** (`atv_ui::front::controller::seats_page`, page
  `splitseats`, title "Players"). Split Screen's Start (Freestyle's course
  Enter, Start Race, Start Game, Hockey and Tag) no longer rides at once:
  the event waits on Players. Controller *n* claims seat *n* (connection
  order): START or × joins, then readies or un-readies; the D-pad moves its
  row (marked "> ") and changes ATV, Rider, Gear (the jersey line with its
  sleeves and pants), Helmet, Goggles, Gloves, Boots and Paint; ○/△ takes
  ready back. The first controller, the keyboard, mouse and touch drive
  the page's rows like any menu (every seat's rows are listed, so a
  keyboard-only group can seat and dress everyone). When player 1 and every
  joined player are ready (two or more), the event starts with the joined
  players in pad order (players 1 and 4 ride as two players). Choices are
  saved per seat (`atv-split-player<n>`, `atv-split-look<n>`), the seated
  count to `atv-split-players`.
- **Radar.** Split races draw the race TOY's radar map once at its
  layout's object 13 position on the whole frame (`Meter::
  paint_split_radar`), at native size like the single-screen radar, with
  every racer's dot: CPUs white, players red, blue, green and yellow (tinted
  `radaricon_racer` copies). The Race overlays' Radar toggle hides it.
  `#ui-state.hud.splitRadar` reports its top-left corner.
- **Stunt text.** Three and four players use object 24's clock
  (`atv_core::hud::Config::quarter`, ScaleSize 1.5) for every player; two
  players keep object 12's. The text scales and offsets were already
  object 24's.

Port adaptations and uncertainty: the page opens with the last session's
player count joined (the original joins only player 1), players 2-4 can
leave with ○/△ when not ready, a Paint row follows Boots (the port's
paints), and **Start Event** (the original's "Force Start" label, whose
trigger was not traced) starts with the joined players without waiting
for ready. The ATV stats row (8) and the Profile Name widgets are not
reproduced; player 1 keeps its Garage selection (its rows are the Garage
pages). The original panels' art (`multi_P<n>_panelcover`, arrows) is not
used: the page uses the port's menu rows. The radar's draw order of the
racers and whether it scales in split were not traced (native size,
player 1 on top). Per-player audio mixing was not investigated this pass.

Checks: `cargo test -p atv-ui` (`split_events_wait_on_the_players_page_
until_seated`, `the_split_radar_is_one_overlay_over_the_views_crossing`,
`everyone_joined_and_ready_starts_the_event`), `cargo test -p atv-core hud`
(`quarter_stunt_text_pulses_to_object_24s_scale`), the new
`tools/test_engine_split_seats.mjs` (content group, four stand-in pads:
player 3 joins and changes its helmet on its own pad, players ready on
their pads and rows, three players ride; players 1 and 4 seat as two) and
the radar assertions in `test_engine_split_screen.mjs` (two players side
by side) and `test_engine_split_4p.mjs` (four players).

## Split objectives and championships (2026-10-29)

Exact-build evidence (USA v2.00 `SCUS_972.11`, private Ghidra exports of
the functions named; bounded reading, no runtime traces):

- **No split championships.** Main Menu (`00264dc0`) sets the multiplayer
  flag (game state +0x1a08) to 1 only for Multiplayer (page 3); Single
  Player and Profile Editor clear it. The Championship page (5) is opened
  only by Single Player (`002658b0`, menu kind +0x1a0c = 4).
  MultiPlayerSelect (`002670a0`, Split Screen) has three rows, each to
  TrackSelect (6): Race (event mode 7, kind +0x1a11 = 1), Freestyle (mode
  2, kind 2) and Games (mode 3, kind 3). No multiplayer path reaches a
  series, its points table or the championship results, so the port adds
  none.
- **Split event types.** The TrackSelect type cycler (`0026d010`, also
  run on page entry in `0026a720`) skips Duel unless the multiplayer flag
  is set and, online only, skips Objectives, Lap Attack and Duel (strings
  `0x433c60/68/78`). With Tracks.dat's masks (race tracks 103 = Freeride,
  Duel, Ghost/Lap Attack, Practice, Single Race; stunt tracks 88 =
  Freeride, Classic, Objectives) offline split screen offers **Freestyle
  Objectives** and the race types Duel, Lap Attack and Practice. TrackSelect
  then opens MultiATVRiderSelect (8) when the flag is set (`0026ba60`).
- **One objective for every player.** The objective classes keep
  per-player state: CherryPick's hit (`002fab40`, vtable `0045f550` +0x7c)
  counts targets per player (+0xac[p]), adds the bonus points to that
  player's score and the bonus seconds to the one clock, and removes the
  target's compass icon from all four players' compasses. The targets'
  triggers are registered for every player's quad (`002fae60`), so a
  target goes to the first player to touch it. Combos are listed per
  player (`002fc050`, 0x6c bytes a player). The Stunt objective keeps one
  level counter (`002fb720` +0xb0): a level's sign shows once, on the view
  of the player whose score passed it.
- **Ranking.** After each change the event ranks the players (vtable
  +0x9c): `002fb380` by targets then score, `002fbbb0` by score,
  `002fc450` by combos then score; the comparator `002fa120` orders by
  score, highest first (`002fa0d0`/`002fa190`, the target and combo
  comparators, are inferred from their use, not exported). Each player's
  place is stored (+0x28).
- **End.** The last target ends a target run: every player with a target
  completed it and the players with the most targets win (+0x4c; +0xa4
  marks a tie). A player's last combo ends a combo run with that player
  the winner. Point levels complete only with one human (`002fb720`
  checks the human count +0x84 == 1), so a split stunt run plays to the
  clock. At time out (vtable +0x98: `002fb230`, `002fb960`, `002fc2e0`)
  the leader wins with every player tied with it on targets/combos and
  score. `002fd980` adds the whole seconds left x 100 to each completed
  player's score.
- **No saving.** `002fce00` saves completion bits and pays Profile Points
  only with one human; with more it writes each player's Freestyle record
  (`00332c50`), which the port leaves out (records for players 2-4 await
  the product decision).

What the port does (`atv_core::freestyle::split::SplitRun`):

- **Split Screen → Freestyle Objectives** (after Freestyle) opens the
  standalone Freestyle Objectives course list (the courses with an OB
  archive) and Select Objective; an objective waits on the Players page
  like every split event, then rides with the seated players.
- Every player starts at the objective's `DefaultStartPosition`, players
  2-4 beside player 1 as at a split Freeride start (a port choice). Each
  player's ATV takes targets through the same trigger-body test as one
  player; the hit's sound, explosion and cherry follow the shared target.
  Combos, stunt scores and signs are each player's own; the clock, level
  counter and targets are shared, with the recovered end and winner rules.
- **HUD.** One objective panel (the original's split objective overlay
  layout was not traced): the objective, the shared clock, the shared
  progress and each player's figure in place order ("P2 3 targets · P1 2
  targets"), the leader's score and the newest sign with its player's
  name. Every view draws its own target compass of the untouched targets:
  a split target objective uses Overlay Set 5 (`Type=CherryPick`), whose
  split layouts list the compass (object 31) once per player, at the same
  positions as Set 2's (2Player-Horiz 65,172 / 65,392; quarters 60,175,
  370,175, 60,395, 370,395), so each player's compass turns with its own
  ATV at its layout position. The floating "Target" tags over untouched
  targets are a port presentation (the original draws only the 3D
  targets); each view now tags them from its own camera, as player 1's
  view does (a port choice). `#ui-state.toolbar.hud.split` lists object 31
  per seat and `.viewTags` the tags each view drew.
- **Results.** "Congratulations, Player n!" for one winner, "Tie: Player 1
  and Player 2!" for a shared win (the original's `002ddc50` line with the
  seat names), then the objective and each player's place, figure and
  score; Restart Objective and Continue as in one-player objectives.
  Nothing is saved (no completion bits, Profile Points or records).
- `#ui-state.ride.objective.split` reports the players, clock, which
  player took each target, scores, outcomes, places and winners;
  `.summary` the results text.

Not built, with reasons: the original's split **Race event types** Duel
(two players, no CPUs) and Lap Attack (ghost): the port has no
single-player Duel/Lap Attack either, so they belong with that work.
Practice is built for one player and split screen (research/RACING.md
"Practice event type").
Championships stay single-player, as in the original.

## Split audio (2026-10-29)

Exact-build evidence: the renderer camera is the listener (`00215e30`);
the positional-sound setup `001742f0` loops over the listener list (count
+100, entries +0x6c) and picks the nearest by a strict distance
comparison before computing volume, pan and Doppler (AUDIO.md). One
listener per view is the reading for split screen (each view's camera
registers one); the engines' voice allocation per human in split was not
traced.

What the port does: players 2-4's engines and body impacts (the room
riders' voices, `audio::remote`, up to six engine voices) are each placed
for the **nearest** of every player's camera, so each player hears its
own engine at its own view's distance and pan
(`RemoteAudio::frame` takes the listener list; `spatial_audio::command`
already implements `001742f0`'s choice). Player 1's positional sounds use
the same list (`RideAudio::views`, players 2-4's cameras after player 1's
own camera listener): rider vocals, chassis and drive-on sounds and body
impacts, the venue's Soultree emitters (range test and placement: the
`001c12e0` reading "nearest listener closer than 1.2 x MaxDistance", now
over every view; `world_sounds::World::step` takes the listener list) and
the train/actor sounds (`SceneInput::views`). `spatial_audio::nearest`
reports the chosen listener (`#ui-state.ride.listeners` = [listeners,
index nearest player 1's rider]). Player 1's engine loop, ambient
cycles, rain and race stingers are unpositioned 2D voices in the port
(as in one player), so no listener applies; race CPUs' engines, vocals,
impacts and chassis sounds are heard from the nearest view (AUDIO.md "CPU
riders"). One rider alone, and online rooms, pass a single
listener (views empty), unchanged. Not done: a split-specific mix level
(none was found) and the engines' voice allocation per human in the
original (not traced).

Checks (views and audio): `cargo test -p atv-core spatial_audio
world_sounds` (`nearest_matches_the_command_choice`,
`split_views_hear_soultree_from_the_nearest_camera`); the browser test
below asserts object 31 in both seats, at least one target tag in each
view and `ride.listeners` = [2, 0].

Checks: `cargo test -p atv-core freestyle::split` (shared targets,
end/winner rules, stunt time out, ties, combos), `cargo test -p atv-ui
split_objectives_ride_through_the_players_page`, and
`tools/test_engine_split_objectives.mjs` (content group: two players ride
Prospect Springs' green targets to the end through the real trigger
test, Restart Objective, player 2's engine voice live through the
nearest-listener path (`#ui-state.ride.remoteVoices`), then a timed-out
Score Stunt Points run). The audio check proves the voice plays; its
gains per view were not measured in the browser.

## Measurements

Host only (Apple Silicon Mac mini, headless Chromium on Metal, 1280x720,
ENV01 Freestyle, weather off, both players on the throttle, `?debug=1`
Performance lines after 600 frames; one run each, not phone results):

| Ride | Frame (ms) | Frame p95 (ms) | CPU draw (ms) |
| --- | --- | --- | --- |
| One player | 13.0-13.3 | 19.6-24.7 | 7.4-8.8 |
| Split top/bottom (`?split=h&split2=5.1`) | 14.3-19.0 | 24.3-29.8 | 9.2-9.8 |
| Split side by side (`?split=v&split2=5.1`) | 14.8-14.9 | 20.4-20.9 | 11.5-12.0 |

Split screen draws the scene twice and steps a second ride, so CPU draw
rises by about 1-4 ms on this host; the players rode close together, so the
terrain kept one merged fine patch. The frame cadence is noisy in headless
Chromium. Phones were not measured.

Three and four players (2026-10-29, same host and page, one run each
after 600 frames, every stand-in pad on the throttle, ENV01 Freestyle,
`?split=q&splitn=3|4`; the 1P/2P rows re-measured in the same session for
comparison, so the absolute numbers differ from the table above):

| Ride | Frame (ms) | Frame p95 (ms) | CPU draw (ms) |
| --- | --- | --- | --- |
| One player | 12.0 | 26.7 | 2.9 |
| Two players top/bottom | 12.7 | 27.3 | 3.9 |
| Two players side by side | 12.9 | 27.2 | 3.8 |
| Three players (quarters) | 19.5 | 23.9 | 7.0 |
| Four players (quarters) | 10.9 | 12.8 | 6.9 |

Four views cost about 3 ms more CPU draw than two on this host (the scene
is drawn once per view); the frame cadence is noisy in headless Chromium
under the shared host's load (the three-player frame time is not
meaningfully worse than four). Host only; phones were not measured.

Per-player tire tracks and lingering dust (2026-10-29, same host and
page, High graphics, ENV01, four players on the throttle, about 12 s after
the start, `?splitTrails=0` keeping them player 1's; one run per row, the
shared host was busy with other builds):

| Ride | Frame (ms) | Frame p95 (ms) | CPU draw (ms) |
| --- | --- | --- | --- |
| Four players, trails player 1's | 27.0 / 27.2 / 30.2 | 33.1 / 32.1 / 37.5 | 9.8 / 12.9 / 27.0 |
| Four players, trails per player | 32.3 / 32.6 / 39.7 | 40.9 / 39.8 / 51.5 | 11.2 / 11.3 / 15.9 |
| Four players Enhanced, trails player 1's | 42.0 | 55.3 | 24.4 |
| Four players Enhanced, trails per player | 45.0 | 61.3 | 27.6 |

CPU draw differences stay inside this host's run-to-run noise; frame time
was 3-10 ms higher with every player's trails on High (each of four
views draws up to four 3600-segment track buffers and 96-puff dust pools).
Enhanced (600 segments, 24 puffs) and Original (no trails) cost less or
nothing. Host only; phones were not measured, and phones default to the
lighter presets.

## Left for later slices

- The Players page's ATV stats row, Profile Name panels and original
  panel art; the Force Start trigger.
- Hall of Fame and profile credit for players 2-4 (needs a product
  decision: whose profile).
- The original's split Race event types Duel and Lap Attack (Practice
  is built: research/RACING.md "Practice event type").
  Championships are single-player in the original ("Split objectives
  and championships").
- Online parity: every split-screen mode in relay rooms up to the room's
  player cap.
