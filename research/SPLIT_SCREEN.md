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

## Left for later slices

- Three and four players (quarter presets `Preset<n>q`, no CPUs) and the
  ControllerSelect claiming page; each player's own garage selection.
- Games (Tag, Hockey, King of the Hills, Treasure Hunt), Freestyle
  objectives and championships split screen.
- Player 2 sound, wheel effects and rumble; a per-view terrain patch.
- Online parity: every split-screen mode in relay rooms up to the room's
  player cap.
