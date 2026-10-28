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
  Screen → `MultiPlayerSelect` (Race, Freestyle, Games). The
  `QuadzillaUIPageControllerSelect` page (PAGE17) lets up to four players
  claim a pad by pressing × beside a rotating `controller.slt` model
  ([CONTROLS.md](CONTROLS.md)).
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
the second human, and ControllerSelect's claiming rules beyond the page
text.

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
- **Tag** is not split yet: its Start Game still hosts a multiplayer room
  (GAMES.md "Tag"). Freestyle objectives/target runs are not split either.

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

## Left for later slices

- Three and four players (quarter presets `Preset<n>q`, no CPUs) and the
  ControllerSelect claiming page.
- Player 2's own gear and paint, Hall of Fame and profile credit; its tire
  tracks and lingering dust; a per-player audio mix.
- Tag, Freestyle objectives and championships split screen (King of the
  Hills, Treasure Hunt and Hockey are slice 3).
- Online parity: every split-screen mode in relay rooms up to the room's
  player cap.
