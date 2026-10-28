# Menu flow and navigation

How the original front end moves between pages, and how the port follows it.
Page layouts, art and value rows are in [MENU_SCREENS.md](MENU_SCREENS.md);
this note covers which page each row opens and where Back returns.

## Original evidence

Exact-build USA v2.00 `SCUS_972.11`, bounded reading of private Ghidra exports
(`tools/analyze_ps2.py`) and the page archives' labels
(`private/reports/ui-page-inventory.json`). No emulator, no hardware runs.

- **Pages.** 59 `QuadzillaUIPage*` classes, one per `PAGEnn.RES` archive. A
  page's id is its archive number (Main 1, SinglePlayer 2, MultiPlayer 3,
  MultiPlayerSelect 4, Championship 5, TrackSelect 6, ATVRiderSelect 7,
  MultiATVRiderSelect 8, WaypointEditor 9, HOF 10, Options_Main 13, Jukebox
  14, Settings_Game 15, ControllerSelect 16, ControllerSettings 17, Credits
  18, Profile_Main 19, Profile_NameEntry 21, Profile_Copy 22, Profile_Load 23,
  ProfileView_Menu 24, ProfileStore_Menu 36, Profile_Delete 53, Trophy 54,
  MultiPlayerInetSelect 56, HockeyTeamSelect 57): every GoToPage call below
  passes the archive number of the page whose labels match the row.
- **Input.** Each page overrides vtable slot +0x80 (its button handler):
  0x2a is Cross (accept), 0x29 Triangle (back), 0x25/0x26 up/down. The page
  base (`0025c510`) only plays the move/accept/back sounds.
- **GoToPage** (vtable +0xc4, `0025d760`) pushes a 0x14-byte record onto a
  history in the game state (`0021eb50`: at most ten records at +0x1938,
  count +0x1a00): the page being left, its page set, whether it remembers its
  row, the focused widget group and the selected row.
- **Back** (vtable +0xc8, `0025d8b0`) pops n records (`0021ec30`) and opens the
  popped page; the return flag (+0x1a04) makes that page's activation
  (`0025d430`) select the remembered row instead of its default.
- **Every page's Triangle calls Back(1)** (SinglePlayer `002658b0`,
  MultiPlayer `00265fd0`, MultiPlayerSelect `002670a0`, Options_Main
  `002836a0`, Profile_Main `00289600`, Championship `00267ca0`, HOF
  `00281f20`, Jukebox `002846c0`, Settings_Game `002852b0`, ProfileView_Menu
  `002936c0`, ProfileStore_Menu `002a1310`, ATVRiderSelect `002727f0`,
  TrackSelect `0026ba60`). ProfileView_Main can pop two (`00293ae0`). Main's
  handler (`00264dc0`) has no Triangle case: Back on the main menu does
  nothing. While an alert is up, Triangle is ignored and its No closes it
  back to the page.
- **Events.** Starting an event is GoToPage 0x3c, which pushes the page it
  was started from. Coming back to the front end (`0021ecb0`) pops that
  record and opens it, so a race returns to its ATV & Rider page with Start
  Event selected; with an empty history it opens Main. `0021ed60` unwinds the
  history to a named page (the championship results' "Quit to Championship
  Menu" and "Quit to Main Menu", Overlays.pb `[ChampionshipResults Menu]`),
  and for the Trophy page builds the history Main → Profile Editor → View
  Profile → Trophies → Trophy so Back climbs through them.

### Original page graph

Rows in page order; → the page a row opens (GoToPage argument), ⇠ Back.

| Page | Rows → target | Back |
| --- | --- | --- |
| Main (1) | Single Player → 2, Multiplayer → 3, Training Course → (event), Profile Editor → 19, Hall of Fame → 10, Options → 13 | nothing |
| Single Player (2) | Race → 6 (race mode), Freestyle → 6 (freestyle mode), Championships → 5, Waypoint Editor → 9 (needs a profile) | ⇠ previous |
| Multiplayer (3) | Split Screen → 4, i.LINK Host / Join → 56, LAN / Online → 12 (Connection) | ⇠ previous |
| MultiPlayerSelect (4, Split Screen) | Race, Freestyle, Games → 6 | ⇠ previous |
| Championship (5) | a championship → 7 | ⇠ previous |
| TrackSelect (6) | Next → 7 (one player), 8 (split screen), 57 (Hockey) | ⇠ previous |
| ATVRiderSelect (7) | Start Event → event, Edit Name → 21 | ⇠ previous |
| HOF (10) | event/course/difficulty values | ⇠ previous |
| Options (13) | Jukebox → 14, Game Settings → 15, Controller Settings → 16 (then 17), Credits → 18 | ⇠ previous |
| Jukebox (14), Game Settings (15) | Accept/Cancel → Back | ⇠ previous |
| Profile Editor (19) | Create New Profile → alert then 20, Load → 23, Save Current → 21, View Current → 24, Unlock Items → 36, Copy → 22, Delete → 53 | ⇠ previous |
| View Profile (24) | Overview → 25, Trophies → 26, Records → 27, ATVs → 47, Rider Equipment → 48, Games → 46 | ⇠ previous |
| Profile Store (36) | ATVs → 47, Rider Equipment → 48, Games → 46, Tracks → 37, Cheats → 21 | ⇠ previous |

Not traced: the Profile Store item pages' and Profile_Load/Copy/Delete's own
handlers (their labels show Next/Back), the Connection, LAN and lobby pages,
and ControllerSelect → ControllerSettings.

## Port navigation

`atv-ui::front::controller::navigation` keeps the same kind of history for the
frontend pages, and `atv-ui::menu::controller` one for the Options (Settings)
menu laid over them and over a ride's Pause:

- Opening a page pushes the page being left with its selected row (by action,
  so a row that appears meanwhile, such as Championships after a profile
  loads, does not shift it). Back pops it and selects that row again.
- Opening a page that is already in the history returns to it, dropping the
  pages above (the original's unwinding). A saved profile name returns to the
  Profile Editor, a confirmation's No to the slot list, a finished Freestyle
  championship to the Championship page, each on the row it was left by.
- The course loader stands in for the ride. Coming back from a ride returns to
  the page that started it: Continue after a race to Single Race on Start
  Race, a championship round to the Championship page, a mini-game or Hockey
  to Games, the Training Course to the main menu, a Free Ride or room to its
  menu. Quit to Main Menu unwinds to the main menu on the row that led to the
  ride. A load that fails returns to the page that started it with the error.
- A page opened from outside the menus (a ride link, `?menu=`, `?front=`, a
  ride started from a link) gets its canonical parents as history, so Back
  still climbs to the title.
- Back is the Back row or button, Escape or Backspace, a controller's B (or
  Y), and Android's Back key (sent as Escape). Back on the title does
  nothing; Back on the main menu goes to the title (see deviations).
- Confirmations (profile Yes/No, the Ecosystem Collision warning) and forms
  close back to the page that opened them, on the row that opened them.
- Over a held multiplayer ride, Back from the Garage or Select Track still
  rides again, and Back on the Free Ride Lobby still leaves the room.
- The browser's own Back button and gesture are not connected: they leave
  `play.html` as on any page.

### Port page graph

Rows in page order (port additions in italics). Back returns along the
history; the Back column is where it leads when the page is opened from the
row shown (its canonical parent).

| Page | Rows → page | Back |
| --- | --- | --- |
| Title | Press Start → Main, *Dev log*, *Downloads* (web) | nothing |
| Main Menu | Single Player, Multiplayer, Training Course (ride), Profile Editor, Hall of Fame, Options (overlay), *Dev log* (web) | Title |
| Single Player | Race → Single Race, Freestyle → Select Track, *Freestyle Objectives*, Championships → Championship, *Games*, *Garage* → Select ATV & Rider | Main |
| Single Race | Course, Laps, CPU riders, Difficulty, Start Race (ride), *Copy ride link* | Single Player |
| Select Track | event tabs, courses, Ride (ride) | Single Player (Host Game when hosting, Split Screen in split screen) |
| Championship | a championship (ride), Freestyle championships → Select Objective | Single Player |
| *Freestyle Objectives* | a course → Select Objective | Single Player |
| *Garage* | ATV, rider, Rider Gear → Rider Gear | the page that opened it |
| Multiplayer | Split Screen → Split Screen, Host Game, Join Game, *Arlington Free Ride* (ride) | Main |
| Split Screen | Race → Single Race, Freestyle → Select Track, Screen layout | Multiplayer |
| Host Game / Join Game | name and code, Garage, Select Track / Join | Multiplayer |
| Profile Editor | Create New Profile → name entry, Load / Copy / Delete → slots → Yes/No, View Current Profile → View Profile, Unlock Items → Profile Store | Main |
| View Profile | Overview, Trophies, Records, ATVs, Rider Equipment, Games | Profile Editor |
| Profile Store | ATVs, Rider Equipment, Games, Tracks, Cheats | Profile Editor |
| Hall of Fame | event, course, difficulty | Main |
| Options (main menu) | Jukebox, Game Settings, Controls, *Display*, *Audio*, *Extras*, *Alpha* | Main (closes) |
| Pause | Resume, Restart/Reset, Options (no Jukebox row), Jukebox, … Quit to Main Menu | Resume |

## Deviations kept on purpose

- **Title.** The port opens on a title page (Press Start, Dev log, Downloads)
  and Back on the main menu returns to it; the original main menu ignores
  Triangle. The main menu keeps a Dev log row on the web.
- **Rides start from their setup page.** Freestyle rides from Select Track and
  Single Race from its setup page with the Garage choice; the original goes
  Track Select → Next → ATV & Rider → Start Event. The Garage stays a
  separate Single Player row.
- **Single Player** adds Freestyle Objectives after Freestyle (the original's
  Freestyle Prep Objectives event is an event type inside Track Select) and
  Games after Championships (the original's Games are split-screen only,
  under Multiplayer → Split Screen; the port plays them alone). There is no
  Waypoint Editor.
- **Multiplayer** has no i.LINK or LAN; Host Game and Join Game stand for the
  network games and Arlington Free Ride for Online, last as Online was. Its
  live rider count shows there, and leaving a Free Ride or room returns to
  Multiplayer.
- **Options** is also the ride's Pause → Options (the original Pause has only
  volumes and camera); from Pause it has no Jukebox row because Pause lists
  one. Controller Settings is the port's Controls page (keyboard, controller
  and touch); there is no Credits page; Display, Audio, Extras and Alpha are
  the port's own pages.
- **Quit to Main Menu** from Pause unwinds to the main menu as its label
  says; the original's Quit returns to the page that started the event
  (the results' Continue does that in the port).

## Verification

- Rust: `atv-ui` `front::controller::navigation` tests (Back to the page and
  row, a page reached two ways, ride returns, canonical parents, forms and
  confirmations, the original row order, bounded history), `menu::controller`
  parity tests (Back through Options to the row, the Jukebox leading Options
  from the main menu, Ecosystem Collision warning) and `atv-core` `ui_menu`
  row tests.
- Browser: `tools/test_engine_menu_navigation.mjs` crawls every menu page
  reachable from the title without riding, opens each from its row and
  checks that Back returns to the parent on the same row, with the keyboard
  and a stand-in standard gamepad at 1280×720, and with the keyboard at
  390×844 and 844×390. Host Chromium only; physical phones, controllers and
  the Android build's Back key are not covered.
