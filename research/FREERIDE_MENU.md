# Free Ride as the main menu (port addition)

Issue #710, owner-approved 2026-11-16. Not recovered behavior: the PS2 game
opens on a static main menu (PAGE01, research/MENU_FLOW.md) and has no shared
world.

## Behavior

- **Boot.** Past the title (Press Start), the page joins Free Ride exactly
  as Main Menu → Multiplayer → Free Ride does (the saved ATV, rider and
  name; a shared-room claim, then the load of whichever venue the room is
  on in the server's rotation, research/FREERIDE_HUB.md, behind the course
  loader). Nothing here assumes Arlington.
- **The menu over the world.** Once the ride is ready, the Main Menu opens
  over it as the in-game overlay draws every page over a ride
  (`Overlays.pb [Main Menu]`: the near-black rounded panel, the title strip,
  plain rows with the olive-gold capsule). Its rows are the frontend main
  menu's, in its order (Quick Play when offered, Single Player, Multiplayer,
  Training Course, Profile Editor, Hall of Fame, Options, Dev log on the
  web). The frontend's silver stage (FURY wordmark, rider and ATV) is not
  drawn: the live world is the picture. The panel keeps to the left (on a
  phone held upright, the top) so the world shows beside it.
- **Hop in** is the one new control: the overlay's navigation pill a size
  up, at the screen's right (bottom right on a phone held upright, at least
  56 px tall). It is the first row: selected when the menu opens, so Enter /
  A rides at once; Up from the first menu row reaches it and Down from the
  last wraps to it. Back (Escape, B/Y) and Start also ride. A click or tap on
  the world outside the panel and Hop in rides too.
- **Autopilot while the menu is open.** The player's own rider does not sit
  idle: a free ride's roaming CPU (`atv_core::roam_cpu` with the
  `atv_core::game_cpu` driver, skill and stuck rules, exactly as Game
  Settings' Free Ride CPU riders roam; research/CPU_MINIGAMES.md "Free
  roam") rides it from where it is. It is the same ride stepped with the
  CPU's controls instead of the player's (an online ride keeps stepping
  under its menus, research/MULTIPLAYER.md), so the room sees it riding
  normally and the camera keeps following it. Hop in hands that same ride
  back where it stands, at its speed (no respawn or teleport); the next
  steps take the player's input. Pause gives it to the autopilot again,
  over Options too. A stuck autopilot recovers as a roaming CPU does
  (turned in place, or its Reset after the stall time).
- **Pause** in this ride opens the same Main Menu (not the Free Ride pause
  rows; Change ATV and Rider is reached through Single Player → Garage).
- **Options** opens over the world: the ride's Options pages (Pause →
  Options, so without the main menu's Jukebox and Credits rows); Back
  returns to the Main Menu.
- **Any other row** leaves the room and hands the row to the frontend's own
  main menu, which runs it as if picked there (Single Player's page, Quick
  Play, Training Course's ride and so on). Whenever the frontend comes back
  to the main menu (Back from a page, Quit to Main Menu, a results
  Continue that unwinds there), the world is joined again.
- **Fallback.** The static main menu shows, exactly as before and without a
  notice, when the game server has not answered the Free Ride count
  (`/multiplayer/freeride/riders`, asked on the title and the main menu every
  15 s; offline or unreachable), when the claim or the load fails, or when
  the room closes under the rider (that one keeps the room's reason on the
  notice line). After a failure the main menu stays static for two minutes
  before it tries again.
- **Game Settings → Start in: Free Ride / Menu** (key `atv-start-in`,
  default Free Ride). Menu restores the original static main menu.

## Implementation

- `atv_core::ui_menu` page `home` ("Main Menu"): `resume` labelled Hop in,
  then the host's `home_rows`. Game Settings gains `start-in`.
- `atv_ui::menu`: `PauseMenu::home(rows)`; its rows other than Hop in,
  Options and Dev log return `Event::Home(action)`; the layout draws Hop in
  beside the panel (`SHEET` is the panel's hit id for the outside tap).
- `atv_app::game_cpu::Pilot`: the roaming CPU's brain on the player's
  `Riding` (`Riding::pilot`), used by the online paused-step branch while
  the menu is open; the venue's roam map is read with the ride
  (`World::pilot`).
- `atv_app::home`: when the world may open (a fresh server answer, no recent
  failure), and the hooks in `front_frame` (`home_check`), `Message::Ready`
  (open the menu, pause the rider), `Message::Failed`, `room_ended` and
  `home_choice` (leave, then `Front::choose`).
- Browser tests open on the static menu: `tools/browser/harness.mjs` saves
  `atv-start-in=menu` in every context unless the test saved a choice.
  `tools/test_engine_freeride_menu.mjs` (multiplayer group) covers the boot,
  the autopilot moving the rider under the menu, Hop in handing it over in
  place (then coasting without input and riding on the player's throttle),
  Pause returning it to the autopilot, the outside click, Single Player and
  Back, the phone layout, an aborted game server and Start in: Menu.

## Not done / unverified

- Against the production game server (venue rotation, a full room): only a
  local fury-server was used.
- The autopilot avoids no room riders (it roams as a lone CPU does) and its
  route quality is the roaming CPU's; checked on one venue on a local server.
- Free Ride's own pause rows (Players, Change ATV and Rider, Jukebox, Copy
  invite link) are not on this menu, nor are the main menu Options' Jukebox
  and Credits.
