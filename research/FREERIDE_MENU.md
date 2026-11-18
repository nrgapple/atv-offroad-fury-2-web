# The live Free Ride world behind the menus (port addition)

Issue #710, owner-approved 2026-11-16; reworked the same day after owner
review of the first version (#712), which laid the in-game pause overlay over
the ride and booted through the course loader. Not recovered behavior: the
PS2 game opens on a static main menu (PAGE01, research/MENU_FLOW.md) and has
no shared world.

## Behavior

- **Boot.** Press Start shows the frontend's Main Menu at once, exactly as
  before: the red scenic side, the Main Menu title band, gold bullets and
  selection pill, the status chips, the curved silver stage with the FURY
  REMASTERED wordmark and the vehicle. No course loader and nothing that
  reads as starting an event.
- **The world loads behind it.** Once the game server answers the Free Ride
  count (`/multiplayer/freeride/riders`, asked on the title and the menus),
  Free Ride joins and loads in the background, exactly as Main Menu →
  Multiplayer → Free Ride would (the saved ATV, rider and name; a
  shared-room claim; whichever venue that room is on in the server's
  rotation, research/FREERIDE_HUB.md). Nothing here assumes Arlington.
- **While it loads** (issue #728, owner request 2026-11-16): the stage shows
  the player's own ATV turning on the frontend's showroom turntable, in its
  chosen paint, exactly as the vehicle preview always does, but without the
  vehicle's name under it. No loading text, spinner or progress cue. The
  name is hidden whenever Start in is Free Ride, so it never shows and then
  goes; the plate keeps its space, so nothing moves at the crossfade.
- **Crossfade.** When the world is ready it fades in (0.7 s) where the
  stage's silver showed, inside the same curved sweep (landscape) or the
  top stage with its sweeping lower edge (a phone held upright). The
  wordmark stays on top; the vehicle model and its name fade out. Every
  page that showed the wordmark-and-vehicle showcase (Main Menu, Quick Play,
  Single Player, Multiplayer, Split Screen, Championships, Games, the
  Profile Editor and Profile Store pages and so on) shows the world, so it
  stays continuous through the menus; pages whose stage holds real content
  (a course hero, the garage, profile tables, the Daily Event board) keep it
  over the silver.
- **Backdrop camera** (#728). Under the menus the ride's camera is a
  higher, wider chase instead of the follow camera (`atv_game::ride::view::
  Backdrop`): the eye 14.5 ft above and 34 ft behind the rider, aimed 30 ft
  ahead of it and 3 ft up (about 10° down, so the horizon, terrain and
  other riders sit above the rider in the stage), and the lens 16 % wider
  (focal length × 0.84). It follows the autopilot's rider: the ground
  position exactly, the rider's height eased (0.45 s) so landings do not
  bob it, and the heading eased (1.1 s) from the follow camera's so turns
  do not swing it. Seven samples of the ground between the eye and the
  rider lift the eye until its sightline to the rider clears the terrain by
  4 ft (rising over 0.12 s, settling over 0.9 s), and the eye never goes
  under the ground. A reset that moves the rider over 80 ft jumps the
  framing with it. The world first shows already framed; Hop in blends back
  to the follow camera over 1.1 s (smoothstep), and Quit to Main Menu
  blends to the backdrop the same way, as the stage's off-axis framing
  (`view_shift`) eases. The same framing serves every aspect ratio: the
  stage's centre is the view's (`world_centre`), so 1280×720 and a phone
  held sideways (844×390) both frame the rider in their stage.
- **Caption.** Where the vehicle's name was: one pill in the status bar's
  chip style (dark, rounded, gold condensed labels, white numbers with a
  shadow), `FREE RIDE · <venue> · <N> RIDING`, the venue from the live room
  and the count from the rider-count route (the room's own count until it
  answers). Offline there is no world and no caption. The time to the next
  venue change is not shown.
- **Autopilot.** A free ride's roaming CPU (`atv_core::roam_cpu` with the
  `atv_core::game_cpu` driver, skill and stuck rules, as Game Settings' Free
  Ride CPU riders roam; research/CPU_MINIGAMES.md "Free roam") rides the
  player's rider while the menus show. It is the player's own ride stepped
  with the CPU's controls (an online ride keeps stepping under its menus,
  research/MULTIPLAYER.md), so the room sees it riding normally and the
  camera follows it.
- **Hop in.** A pulsing target on the world in the stage (issue #742,
  owner request 2026-11-17, replacing the gold Hop in pill beside Back,
  which read as a confusing extra button): the touch coach's gold ring
  (touch-hint-pulse, 1.4 s) under the rider with "CLICK TO HOP IN" ("TAP TO
  HOP IN" on touch screens) beside it, at least 48 px tall. A click or tap
  anywhere on the world hops in; the target only says so. It shows once the
  world has crossfaded in, on every page whose stage shows the world. Hop
  in is then also the last keyboard/pad choice after the rows (the target's
  lettering turns gold and its ring stops pulsing while it is chosen), and
  pad Start hops in from any page with Back, also while the world loads
  (it waits, then rides once ready). Multiplayer → Free Ride while the
  world is there also hops in. Hop in hands the same ride to the player
  where it stands (no respawn or teleport); the menus go and the ride's HUD
  shows.
- **Pause** in that ride is the ride's own pause menu; its last row reads
  Quit to Main Menu and brings the menus back over the world, the autopilot
  riding again. Change ATV and Rider holds the room in the garage as any
  Free Ride does; from then it is an ordinary Free Ride.
- **Other modes.** Starting any other ride or room (Single Player modes,
  Quick Play, Training, Host/Join, Split Screen) leaves the world first, as
  leaving Free Ride does; back on the menus it loads again behind them.
- **Fallback.** No world, no caption and no Hop in target, the ATV turning on the
  stage without its name (with Start in: Menu, the name too, as before), when
  the game server has not answered (offline, unreachable),
  when the claim or the load fails, or after a dropped room (which ends on
  the main menu with its reason). After a failure the menus wait two
  minutes before trying again.
- **Game Settings → Start in: Free Ride / Menu** (key `atv-start-in`,
  default Free Ride). Menu never loads the world.

## Implementation

- `atv_render::Gpu::share` gives the background load a surface-less handle
  on the same device while the frontend keeps presenting. Once ready, the
  world takes the frontend's surface (`App::ready`), and
  `Gpu::set_scene_offscreen` keeps its scene in a texture
  (`Gpu::scene_texture`).
- `atv_ui`: `Fill::Scene(image)` paints a box with a frame-sized image
  sampled in screen space (`ui.wgsl`), so the stage's own shapes (the skewed
  rounded sweep, the portrait ellipse) frame the world; the silver is drawn
  over it at the crossfade's opacity. `Renderer::add_texture` registers the
  scene texture as a live image (the layer cache redraws it every frame).
  `front::LiveWorld` carries the image, fade, readiness and caption;
  `front::paint_hop_target` paints the Hop in target each frame over the
  cached page (`atv_app::front` `render_page`); `Event::HopIn`.
- `atv_app`: `home.rs` keeps the states (loading, world, live, waiting, the
  probe and failures). `home_check` starts the load from the frontend frame;
  `home_ready` hands over the surface; `step` runs the frontend's input and
  layout, then the world's ride frame, which draws the frontend where the
  HUD would; `home_hop_in` runs the usual ride start (`ride_started`);
  `home_menu` and `home_leave` return or end it. `game_cpu::Pilot` is the
  autopilot on `Riding::pilot`.
- `Riding::backdrop` (set by `home_ready`, `home_menu` and `home_hop_in`)
  selects the backdrop camera in `Riding::present_view`, after the chase,
  sightline and crash views; `Screen::world_stage` hides the stage's
  vehicle name under Start in: Free Ride. Diagnostics: `home.backdrop`
  (blend, eye, yaw, pitch, clearance) and `front.worldStage`.
- `tools/test_engine_freeride_menu.mjs` measures the backdrop camera's
  height and yaw speed over windows of at least 0.15 s and separately
  bounds any single frame's change (4 ft, 0.35 rad). Per-rAF rates read a
  headless browser's ~20 engine frames a second two or three times too
  fast and failed on hilly venues (#744: 62-67 ft/s measured where the eye
  climbed at most 21 ft/s with no step over 1.7 ft).
- Browser tests open with Start in: Menu (`tools/browser/harness.mjs` saves
  `atv-start-in=menu` in every context unless the test saved a choice).
  `tools/test_engine_freeride_menu.mjs` (multiplayer group) covers the boot
  without the loader, the crossfade under the frontend layout, the
  autopilot, the turning ATV while the world loads and offline, the
  backdrop camera (full share, looking down 0.05 to 0.45 rad, over 6 ft of
  clearance, yaw under 3.5 rad/s and height under 45 ft/s each frame) at
  1280×720 and 844×390, the blend back at Hop in, Single Player keeping the
  world, Hop in and the handover, Pause → Quit to Main Menu, a click on the
  world, the upright phone layout (a reset, by a wreck's recovery or the
  autopilot's stuck rules, cuts the ride's own camera and is not counted
  as a backdrop snap), an aborted game server and Start in:
  Menu.

## Not done / unverified

- Not run against the production game server or a full room; a local
  fury-server only.
- The autopilot steers around no room riders (it roams as a lone CPU does).
- The world renders every frame behind the menus, also on pages whose stage
  covers it; phone battery and frame time behind the menus are unmeasured.
- A lost WebGL context while the menus show over the world is untested (a
  world that finished loading on the lost device is dropped and loads
  again).
- The time until the next venue change is not shown.
- The backdrop's sightline check samples the ground at seven points: a
  narrow spike between samples (a tree, a rock) can still hide the rider
  for a moment. Scenery is not tested against the eye.
- A phone held upright frames the same backdrop in its short top stage:
  there the near dunes fill it, the horizon sits above the stage, and the
  rider's helmet can reach the wordmark's lower edge. Not tuned (#728
  covered 1280×720 and a phone held sideways).
- The backdrop framing was tuned on host screenshots of the venues the
  local server's rotation picked, not on every venue or a physical phone.
