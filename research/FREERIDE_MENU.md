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
- **Hop in.** Bottom right beside Back on every page with Back (not the
  title, the garage or the split-screen seats), the frontend's gold pill
  with an arrow, at least 48 px tall. It is also the last keyboard/pad
  choice after the rows, Start hops in, and a click or tap on the world on
  the stage hops in. Pressed while the world still loads it waits and reads
  "Loading…", then rides once ready. Multiplayer → Free Ride while the world
  is there also hops in. Hop in hands the same ride to the player where it
  stands (no respawn or teleport); the menus go and the ride's HUD shows.
- **Pause** in that ride is the ride's own pause menu; its last row reads
  Quit to Main Menu and brings the menus back over the world, the autopilot
  riding again. Change ATV and Rider holds the room in the garage as any
  Free Ride does; from then it is an ordinary Free Ride.
- **Other modes.** Starting any other ride or room (Single Player modes,
  Quick Play, Training, Host/Join, Split Screen) leaves the world first, as
  leaving Free Ride does; back on the menus it loads again behind them.
- **Fallback.** No world, no caption and no Hop in, so the menus are exactly
  as before, when the game server has not answered (offline, unreachable),
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
  `hints` draws Hop in; `Event::HopIn`.
- `atv_app`: `home.rs` keeps the states (loading, world, live, waiting, the
  probe and failures). `home_check` starts the load from the frontend frame;
  `home_ready` hands over the surface; `step` runs the frontend's input and
  layout, then the world's ride frame, which draws the frontend where the
  HUD would; `home_hop_in` runs the usual ride start (`ride_started`);
  `home_menu` and `home_leave` return or end it. `game_cpu::Pilot` is the
  autopilot on `Riding::pilot`.
- Browser tests open with Start in: Menu (`tools/browser/harness.mjs` saves
  `atv-start-in=menu` in every context unless the test saved a choice).
  `tools/test_engine_freeride_menu.mjs` (multiplayer group) covers the boot
  without the loader, the crossfade under the frontend layout, the
  autopilot, Single Player keeping the world, Hop in and the handover, Pause
  → Quit to Main Menu, a click on the world, the phone layout, an aborted
  game server and Start in: Menu.

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
