# Games: the original mini-games

ATV Offroad Fury 2 ships four mini-games ("Games"): Tag, Hockey, King of
the Hills and Treasure Hunt. The manual (p. 17) labels Games "Split Screen
Only": two are open on a new profile and two are bought in the Profile
Store. `0021add0` leaves every event type open except King of the Hills (9)
and Treasure Hunt (12), so Tag and Hockey are the two open games
([PROFILE_STORE.md](PROFILE_STORE.md)). This note records the exact-build
evidence for Hockey and Tag, what the port implements and what remains open.

Evidence comes from private Ghidra exports of the exact USA v2.00
`SCUS_972.11` (bounded reading, no runtime traces), the disc's `Tracks.dat`,
the Coonce Arena and Tag mode archives, and `AUDIO/CROWD/HOCKEY.AUD`.
Addresses are guest addresses in that executable.

## Framework, King of the Hills and Treasure Hunt

The shared framework (`atv_core::games`) and the two bought games. The
Games Prep page sets the time limit to 5 minutes (`002f6060`) whatever
the profile default; Hockey (below) still uses the 120 s profile value.

### Evidence

Private Ghidra exports of the exact USA v2.00 `SCUS_972.11` (bounded reading,
no runtime traces), the course lists in the front end's `Tracks.dat`, the
game archives' mode TOYs, `Overlays.pb` and the UI page inventory. Observed
facts:

- **Where the games live.** Main Menu → Multiplayer → Split Screen →
  `MultiPlayerSelect` (Race, Freestyle, Games). Single Player has no Games
  row; the online lobbies include a "Games Lobby". Games Prep is the shared
  Track Select page in mode 3 (`0026a720`): Game, the course and Time Limit.
- **Time limit.** `002f60b0`/`002f6190` step the limit 2-20 minutes and wrap;
  Games Prep starts it at 5 (`002f6060`); the event reads minutes x 60
  (`0026b7a0`); the page prints it as `%d:00`.
- **Courses.** Tracks.dat lists 14 King of the Hills (`koth`) and 14 Treasure
  Hunt (`th`) courses, the same seven Environment and seven Freestyle venues
  in the same order (Prospect Springs and Tioga Valley open, the rest
  locked), with `RaceTypeMask=1`. Each venue ships a game archive per game
  (`E05_KH.RES`, `E05_TH.RES`, `F01_KH.RES`, ...) repeating its Freeride data
  plus the game's mode TOY (`env05kh.toy`, `env05th.toy`), scene and models.
- **No CPU riders.** No game archive carries a racing line or AI roster, and
  the rules count only the split-screen or online players. The 1 Player
  overlay layouts exist for online play (one player per console).
- **Triggers** (`0023ff80`, `0023fd00`, `00171f90`): `Position`, `Box` (the
  extents about the position), `OneTimeOnly`, `NumberOfActions` and
  `[Trigger%dAction%d] ActionType`. A rider touching a trigger calls its
  action callbacks with 1 once, on the new contact (`00172bc0`); the contact
  ending calls them with 0 (`001728e0`).
- **Ranking** (`00220e70`, `002d0f20`): riders sort by the game's count,
  ties by stunt score (`002fa120`).
- **HUD** (Overlays.pb sets 6 and 7): TIME (event time left), TOKENS
  collected / total or HILLS held / total, SCORE (stunt score), PLACE, the
  compass and GPS, King of the Hill's hill HUD (`QuadzillaHillScoreHud`,
  `002eac20`: a pill per hill at its screen position, coloured by owner) and
  the radar icons `RadarIcon_Flag`/`RadarIcon_Hill`. Signs: "ATV %d
  captured hill %d" (`002dc1b0`, 3 s), "All Treasures Collected" (5 s),
  "More Treasures in" with the countdown.
- **Results** (`002ec310`): `[TreasureOver Menu]` and `[HillKingOver
  Menu]`, both titled "Race Results": places 1-5 with the rider names,
  Tokens Collected or Hills, Stunt Score, and Continue. Pause offers
  "Restart Game" (`0043a9e8`).

#### Treasure Hunt (`QuadzillaEventTreasure.cpp`, vtable `0045e4e0`)

- `002d0750` reads `[TreasureHunt] NumberOfTargets` and `NumberOfGroups`;
  each `[TreasureGroup%d]` names its triggers. A treasure is a one-time
  `Box=4,6,4` trigger whose actions hide its `bomb.mcf` model, run the
  `Treasure` action and start `bomb_explosion.emt`.
- Start (`002d0590`, `002d11e0`, `002d1280`): every treasure is hidden and
  disabled, then group 1 is shown and marked used. Collecting counts only
  after the event starts (+0x218, `002d15c0`).
- Collect (`00208b10` → `002d0210` → `002d0d70`): the rider's count rises,
  the remaining total and the group's remaining fall, `iconhit.vag` plays.
- Update (`002d0a20`): none left → "All Treasures Collected" for 5 s and the
  end (`002d10d0`); an emptied group → a 6 s wait, then a random unused
  group (`0036e7a0`, stepping past used ones).
- The private sweep finds 25 treasures in five groups on most courses
  (Freeport Harbor, Rockbridge Park and San Jacinto Isles: four groups, one
  of ten) and 15 in one group on Tempe, Nassau and Crescent Hills; every
  group name resolves to a treasure trigger.

#### King of the Hills (`QuadzillaEventHillKing.cpp`, vtable `004578b0`)

- `00220640`/`002203d0`: `[KingOfTheHill] NumberOfHills` hills (3-16 per
  course), each a `Box=20,20,20` trigger (Arlington Stream `15,15,2`) with a
  `Hilltop` action (`HillNumber`, `SpecificObject=kohflag.slt`); the flag
  marker sits 12 units above the trigger.
- Rider state (`00220150`): entering a hill with a wheel down makes the
  rider ready; entering in the air does not. Leaving the hill in the air
  while ready arms that hill; leaving any other way clears it. Every step
  (`00220750`) a rider with a wheel down and no stunt open who is not ready
  is cleared.
- A finished stunt chain (vtable +0x184, `00221020`): a crash clears the
  state; a landed chain scores its stunts' points times the chain
  multiplier, and `00220b50` captures the armed hill when that beats the
  hill's score. The previous owner holds one hill fewer, the rider one more,
  the flag takes the rider's colour (red, blue, green, yellow) and "ATV %d
  captured hill %d" shows for 3 s. The game ends on its time limit.
- Restart Game (`00220d10`): every hill unowned at score 0 with a white
  flag.

No Profile Points, records or Hall of Fame writes were found in either
game's end (`002d10d0`, `002210e0`).

### Port

- `atv_core::games` (`crates/atv-core/src/games/`): `GameKind` (all four,
  event types, Tracks.dat lists, archive and mode TOY names), the TOY
  trigger and start parsers, the time limit, the `Rules` trait and the
  shared `Session` (start, clock, signs, ranking, end and restart).
  `treasure_hunt.rs` and `king_of_the_hill.rs` hold the recovered rules with
  unit tests and a private all-course sweep (skipped without the disc).
- Single Player → **Games** (after Championship): the port has no split
  screen, so one player starts the games here. The page has Game, Course,
  Time Limit (2:00-20:00, default 5:00) and Start Game, and works with
  keyboard (Left/Right steps), controller and touch. Locked games and
  courses are marked and refused with the store's lines. Hockey starts
  the Coonce Arena session described below; Tag's Start Game hosts a
  multiplayer room on the chosen Tag course and starts Tag there with the
  page's time limit (Tag "Port").
- `atv-app/src/games.rs` loads the game archive's mode TOY, rides the
  venue's Freeride scene, places the ATV at the TOY's `Start1` (else the
  venue start; ten of the 28 mode TOYs list no starts, and their venue
  start is remembered so Restart Game returns there too), holds it for a
  3 s start countdown, samples position,
  ground contact, the open stunt and finished chains, and opens the results
  (Restart Game, Continue back to Games). Pause → Restart Game restarts in
  place.
- HUD: a top-centre panel (game, TIME, TOKENS/HILLS, PLACE, SCORE, the
  countdown and the current sign); the treasures and hill flags are screen
  tags and minimap markers (hill flags in the owner's colour, white while
  unowned).
- Content: `tools/package_content.py` packages the 28 KH/TH archives as
  small lazily fetched archives of only the members that differ from the
  venue's Freeride archive (about 7-10 KB each). The release owner's next
  R2 upload must include them before the public site can play the games.

### Adaptations and open items

- **One player, no opponents.** The original is split-screen or online only;
  Single Player → Games plays it alone. Multiplayer → Split Screen → Games
  plays King of the Hills, Treasure Hunt and Hockey for two local players
  (SPLIT_SCREEN.md "Slice 3").
- **Scenes and models.** King of the Hills rides its own scene
  (`env05kh.scn`, read with its mode TOY over the Freeride archive), so the
  original `kohbase` bases, `kohflag` flags and spotlights stand at the
  hills; each hill's flag (the nearest `kohflag`, as the Hilltop action's
  `SpecificObject` binds it) takes its owner's colour through a per-model
  colour (`001b4280`) and returns to white on Restart Game. Collision
  matches the original: the ride's static collision and scene partition
  come from the loaded (game) scene, and no game archive ships a COL for
  `kohbase` or `kohflag` (the original skips models without one,
  `00206e90`), so the bases and flags are not solid in either; every COL a
  game archive carries is the Freeride one. Treasure Hunt has no scene of its own (`env05th.toy`, no
  `env05th.scn`): it rides the Freeride scene with its mode TOY read over
  the TH archive. Each `Treasure` trigger's `HideAnimation` names a `bomb`
  animation (`MCF` `bomb.mcf`, `Offset`); the port appends one `bomb.psg`
  per treasure at its offset, loops `bomb.MOT` on it (the Freestyle cherry
  path), shows only the live group's uncollected treasures and hides a
  bomb when collected or during the group wait. A private probe checks all
  14 venues: one bomb per `NumberOfTargets`, and the PSG and MOT ship. The
  screen tags and minimap markers stay. A collect starts that bomb's
  `bomb_explosion.emt` instance (the trigger's `Particles` action) at the
  bomb, drawn as the original's CPU spark streams; the authored
  `bombsparks.emt` is undeclared and absent from every archive
  (AMBIENT_EFFECTS.md "Triggered bursts").
- **Sounds.** The start countdown plays the race start's `beep01`/`go`
  (`002e79a0`); each treasure plays `iconhit.vag` (`002d0d70`), which the
  packager now adds to the hosted `AUDIO.RES`. No capture sound was found
  for King of the Hills.
- **Trigger contact.** The original tests the ATV's collision body against
  the box; the port tests the ATV origin against the box grown by 3 ft
  (1.5 ft vertically). `Box` is read as half extents. Freestyle's target
  reset `002fae60` is the only caller of the trigger body registration
  `00172440` found; Treasure Hunt and King of the Hills do not register
  bodies, so their triggers keep the default collision pairing (which
  bodies that admits was not traced) and the port keeps the origin test
  rather than the Freestyle body set (CHAMPIONSHIPS.md "Trigger bodies").
- **Start countdown.** The original runs the shared 321 LIGHTS overlay
  (Overlays.pb object 10, `002e79a0`/`002e81f0`). The port holds the ATV
  for 3 s with the beeps and draws the race's original lamps from the
  course archive: one more red lamp each second, green at GO for one second
  (the panel's digits show only when the lamp art is missing). The 3 s
  hold starts on the first red lamp, so the original's all-off step is not
  shown.
- **Hill HUD** (`QuadzillaHillScoreHud`). `002eac20` builds one
  `koh_pillbkg` widget per hill (30 x 31 with a 10 x 11 inset, white,
  alpha 0x80) and one text per hill whose callback (`002df040`) prints
  "%d" of the hill record's +4: the stunt score that took the hill, i.e.
  the score to beat (`00220b50` stores it with the owner and the owner's
  colour at +8). `002203d0` places each pill at the hill's position raised
  12 ft; every frame `002207e0` projects it and shows the pill only when
  it is on screen and a ray from the camera reaches it (`00169b60`),
  hiding it otherwise (`002eb0b0`). `002eb0e0` refits the pill to its text
  and recolours it: black (0) while unowned, else the owner's colour from
  the HUD's table (+0x168); the text stays white (+0x178 = 0xffffff).
  `koh_pillbkg.pst` (KH archives only, ScreenObject shader, clamped) is a
  32 x 32 white left cap whose first 10 texels are the arc, which matches
  the 10-texel inset. The port loads it from the game archive and draws a
  left cap, a stretched middle and the cap turned round for the right end,
  pre-tinted per colour with GS alpha doubled (0x80 opaque), white score
  centred ("0" before the first capture), at the same raised point, only while it projects inside
  the view (off-screen hills are hidden, not pinned to the edge like rider
  tags), and keeps the hills on the minimap. The occlusion ray
  (`00169b60` on the world's terrain object) is ported as a terrain line
  of sight from the drawn camera to the pill point, sampled every half
  cell; scene objects do not block it (whether the original's ray tests
  only the heightfield was read from the call, not traced). How the
  widget's draw routine slices the texture was not traced; the 3-slice
  reading follows the texture's shape and the constructor's 30 x 31 /
  10 x 11 values, and the pill height stays the 31 units of the
  constructor rather than `002eb0e0`'s text height − 9.
- **Compass.** Overlay Sets 6 and 7 place object 31 (`QuadzillaCompass`)
  at 70,375; Tag (Set 10) and Hockey (Set 9) have none, and Coonce's
  archive ships no compass art. `002edf00` passes the mode's icon:
  event type 7 (King of the Hills) `RadarIcon_Hill`, 10 (Treasure Hunt)
  `RadarIcon_Flag` (8, `RadarIcon_Ball`, and 0/1, `RadarIcon_Gate`, are
  the other cases). King of the Hills fills the icon list once with every
  scene model whose name contains `hill` (`002e7610`) and colours each by
  its owner (`002e76e0`: participant colours 0-3, white unowned) at the
  start and on every capture. Treasure Hunt refills it with the live
  group's treasures when a group starts and after each pickup
  (`002e7590` from `002d1280`/`002d0d70`). The port draws it from the
  game archive's art: one `radaricon_hill` per hill marker (tinted copies
  per owner slot) and one `radaricon_flag` per live, uncollected treasure,
  all clamped to the rim. Assumptions: hill icons sit at the port's hill
  marker points rather than the `hill` models' origins, and collected
  treasures are dropped from the list (whether `002d17c0`'s group list
  still holds them was not traced).
- **Treasure wait flag.** During the 6 s wait the original sets rider flag
  0x40000 (meaning not traced); the port only hides the treasures.

Checks: `cargo test -p atv-core games` (rules, parsers, time limit, the
private sweep of all 28 courses), `atv-ui`
`games_page_gates_games_and_courses_and_starts_a_game`, and
`tools/test_engine_games.mjs` (content group, needs the KH/TH archives in
the package): locked games and courses refused; Treasure Hunt on Prospect
Springs collected through every group (`gameTest=visit` sets the ATV on
each standing treasure) to "All Treasures Collected" and the results,
Restart Game; King of the Hills on Tioga Valley armed through the real hill
enter/leave and captured by a landed chain the `gameTest=hill` hook
supplies (an ATV placed in the air opens no stunt), Pause → Restart Game,
and Time Up on the 20 s `gameTest=fast` clock with Continue back to Games.
`gameTest=hillride` instead launches the ATV across the hilltop (40 ft/s
with a 12 ft/s lip, a new direction per Restart Game) so it rides off the
hill: that arms the hill through the real trigger leave in three of four
directions, but in host runs no keyboard stunt (J, held 0.45 s after the
arming) opened on the launched ATV, so no chain finished and the capture
from a real stunt remains covered by the unit tests only. A ride placed
by a hook may lack the take-off the stunt system needs; driving there
from the start would need a scripted route per course.
These are host Chromium checks with a private test package.

## Shared facts

- **Courses.** `Tracks.dat` lists one Hockey course (`[hoc1]` Coonce
  Arena, `game01`, open) and fourteen Tag courses (`[tag1]`–`[tag14]`:
  Prospect Springs and Tioga Valley open; Arlington Stream, Rockbridge
  Park, Freeport Harbor, Caribou Peak, Sonora Dunes, Tempe, St. Croix
  Sound, Nassau, Wheeler Junction, Crescent Hills, Vancouver Ridge and San
  Jacinto Isles opened by course unlocks, `00323640`). Both type masks are
  1; the picture resources are `TP_hock` and `TP_tag`.
- **Archives.** Hockey rides `TRACKS/TRACKSA/GAME01/G01_SO.RES` (scene,
  `game01so.toy` with the goal triggers and `[Ball] StartPosition`, the
  puck model and collision hulls, the `HockPlyr1`–`4` team markers, the
  `icon_hockeyred`/`icon_hockeyblue` HUD icons). Each Tag course has its
  own `*_TG.RES` (for example `E05_TG.RES`: the course scene, `tagball.psg`
  with its hulls, the ball radar icon); its mode TOY only sets the default
  start.
- **Riders.** Both events build their riders from the session's players
  (controller slots). Neither event creates CPU riders, and no CPU logic
  for either game was found; the manual's "two players include computer
  riders" applies to races.
- **Time limit.** Settings +0x57c, 120 s on a new profile (`0021add0`);
  the Track Select page has a Time Limit row and the manual gives 2 to 20
  minutes.
- **Results.** The in-game menu pages `TagOver Menu` and `HockeyOver Menu`
  (`002ec310`) follow each game.

## Hockey

Event `QuadzillaEventHockey.cpp`, vtable `00460770`; camera
`QuadzillaHockeyCamera` (`00337e60`, `003383a0`).

- **Setup `00335590`.** Sounds `siren02.vag` (goal), `gameover.vag`,
  `puck1`/`puck2.vag`, `Crowd\hockey.aud`. Two team records at +0x1c4 +
  team × 0x38 (rider count, score, per-rider goal counts, last toucher,
  riders). Each session rider joins the team chosen on the Hockey Team
  Select page (`QuadzillaUIPageHockeyTeamSelect`, `bk_hockey.tga`) and
  gets a `HockPlyr%d` marker quad on the ice. The goals are the scene's
  `goal1` and `goal2` objects; the TOY's two `Goal` triggers (boxes 7 ×
  3.5 × 1.5 half extents at z 437.5 and 330.5) fire
  `HockeyGoalTriggerAction`. `HatTrickEmitters` lists the arena's hat
  emitters (the four `hats*.emt` type 9 emitters in `game01so.toy`).
- **Face-off `00336c90`.** The centre is the goals' midpoint, grounded.
  Riders stand on a 30-unit semicircle: rider *i* of *n* on team *t* at
  angle *t*·π − π/(*n*+1)·(*i*+1) from the rink axis turned −90°, which
  puts team 0 on the goal-2 half and team 1 on the goal-1 half, all
  facing the centre and held. The puck waits 5 units over the centre;
  the countdown is 2 s.
- **Drop `00337150`.** The puck appears at a random point within 10 units
  of the centre, 3–11 units up, pushed at up to 6 units/s with a random
  spin; riders are released; play (state 3).
- **Play `00336200`.** The clock runs only in play and sudden death. Each
  rider contact with the puck makes that rider its team's last toucher. A
  puck moving at 13 units/s or more plays a puck hit. At zero: tied scores
  start **Sudden Death** (5 s banner, play continues); otherwise
  **Game over!** (2 s), the end siren, crowd event 4 (HOCKYEND) and a
  2.5 s pause, then the game ends (`003379a0`).
- **Goal `003368c0`.** The team whose attacking goal is nearer the puck
  scores (nearer goal 2: team 1). The goal is credited to that team's
  last toucher only when the toucher is on it; an own goal credits nobody.
  Every third credited goal is a **Hat trick!** (3 s banner, crowd event 3
  HOCKYWIN, hat emitters, 6 s pause); other goals play crowd event 1 or 2
  (HCHEER01/HCHEER04) and pause 4 s. A team HUD message shows for 4 s,
  last touchers reset and the goal siren plays. After the pause a new
  face-off follows while time remains; in sudden death the game ends.
- **Restart `003377f0`.** Scores and goal counts reset, the clock returns
  to the time limit, face-off.
- **Puck body `00337be0`.** A generic rigid body (`hockeypuck.psg`,
  `hockeypuckm.col`/`hockeypuckf.col`) with parameters 100, 1, 2, 0.2 and
  0.02 and contact values 0.7 and 0.25 (meanings unverified).

### Port

- `atv_core::hockey`: the state machine (face-off, drop, play, sudden
  death, pause, over), goal attribution, hat tricks, crowd/siren/puck
  events, restart, face-off spots and the Coonce Arena goals with unit
  tests for each rule, ties, time-out and goals in the last second.
- `Puck` is an authored flat-ice body: gravity onto the ice, a 3 units/s²
  slide drag, board bounces with restitution 0.7 against the rink outline
  taken from the arena's board placements (x 346.5–421.5, z 334–434,
  10-unit corner chamfers, 14-unit goal mouths opening into 7.5-unit
  pockets), and ATV contact as an infinite-mass push within 3.5 + 1.5
  units. It does not use the original rigid body or the puck hulls.
- Single Player → Games → **Hockey** (Coonce Arena) rides `game01` with the
  garage ATV. The
  human plays Red alone; Blue has no riders (see below). The rider is
  placed and held at its face-off spot, the HUD panel shows "Red n – m
  Blue" with the clock, the original banners ("Red scores!", "Hat
  trick!", "Sudden Death!", "Game over!") and the distance to the puck,
  and the puck draws as its own `hockeypuck.psg` model (a scene model
  posed each frame through the drop gate's posed-model path), plus a
  screen tag and map marker. The goal siren (`siren02.vag`), puck hits
  (`puck1`/`puck2.vag`, 13 units/s), `gameover.vag` and the four
  `Crowd\hockey.aud` reactions play as one-shots. Pause shows Restart
  Game. The end opens the HockeyOver page ("Hockey Results": score,
  winner, goals; Restart Game, Quit to Main Menu).
- `tools/package_content.py` adds the five event samples to the packed
  `AUDIO.RES` and the four loose `AUDIO/CROWD/HOCKEY` clips; the release
  owner must publish the regenerated package before they play publicly
  (older packages leave Hockey silent).
- Browser test `tools/test_engine_hockey_tag.mjs` (content group): face-
  off hold and drop, keyboard driving, a goal (`hockeyGoal=1` slides each
  dropped puck off the rider into the far goal), the pause and next
  face-off, Restart Game, and a 20 s game (`hockeyTest=fast`) to Game
  over!, the HockeyOver page and the result; `ATV_HOCKEY_SOUNDS=1` also
  requires the siren, game-over and crowd one-shots.

### Open

- **No CPU opponents.** The original has no CPU hockey; alone, a human
  plays an empty net. Human opponents play through multiplayer rooms
  (below); CPU skaters (an authored chase of `hockey::cpu_target` with the
  race CPU riders) are not connected.
- The puck model is posed but not physically its rigid body; its drawn
  size is the original model's, larger than the authored 1.5-unit
  contact radius. The team marker quads, team rider textures and hockey
  camera are not drawn. The four `hats*.emt` type9 emitters are
  deactivated at setup, start on a hat trick and stop at the next
  face-off or Restart Game (AMBIENT_EFFECTS.md "Triggered bursts").
- Team Select and the Time Limit row are not shown; the port uses the
  120 s default. The puck model also rests at the `[Ball]` start when
  Coonce Arena is ridden as authored Freestyle.

### Online (authored)

The original plays Games on one console (split screen). The port's
replacement for split screen is its relay room, so a room ridden on Coonce
Arena plays Hockey with human opponents. This is an authored adaptation,
not the original's network play (no original Games protocol was
recovered):

- **Host-owned game.** The room host runs `atv_core::hockey` and the puck
  for the roster in peer order, teams alternating Red/Blue (Team Select
  is not reproduced). Each rider still owns its ATV; the host pushes the
  puck with its own ATV and the others' latest relayed poses, so remote
  touches carry relay latency.
- **`Packet::Game`** (`atv_net::GameState`, codec kind 13): phase and its
  timer, clock, scores, banner, puck position/velocity/spin and a phase
  serial; sent at most every third step (20 Hz) and on every phase
  change. The relay forwards it only from the host of a started private
  room to riders on the map.
- **Guests** mirror the state (score, clock, banners, the puck model),
  face off at their own spot on each new face-off serial, play the siren,
  crowd and game-over sounds on the matching changes, and open Hockey
  Results when the host's game ends. A room with one rider waits at the
  face-off ("Waiting for riders"); a roster change starts a new game.
- Check: `tools/test_engine_multiplayer_hockey.mjs` (relay group: Rust
  host and Rust guest on a local relay; roster, Blue face-off spot, a
  shared goal, the puck within 15 units on both, both reach Hockey
  Results).
- Limits: guests' goal counts and hat-trick credit are only the host's;
  older clients do not understand the new packet; the puck is not
  predicted between state packets.

## Tag

Event `QuadzillaEventTag.cpp`, vtable `00459300`. Tag is ball tag: the
carrier is "it" and scores while holding the ball.

- **Setup `00244ac0`/`00244d20`.** Four rider records (+0x1cc + rider ×
  0x10: current possession, total possession, longest possession, points
  awarded), the carrier and its index, a drop time stamp (ms), the
  `tagball.psg` body and `tagBeep.vag`; each viewport's radar tracks the
  ball.
- **Per frame `00244ef0`.** The carrier's current and total possession
  grow by the frame time; every whole second of total possession is 250
  points. A wrecked carrier drops the ball ("%s dropped the ball!"). For
  2 s after a change of carrier the ball cannot change hands. After that,
  any other upright rider whose body touches the ball or the carrier
  takes it, except the rider who lost it within the last 500 ms. A loose
  ball goes to whoever touches it.
- **Carrying.** A newly gained ball is tossed 3 units over the new
  carrier with its velocity plus 11 units/s up; it homes on the carrier
  (at most 2.3 units a step) and rides on the ATV once it falls back
  within 3 units.
- **Knock loose.** While the ball rides on the carrier's ATV, a trick
  that has just begun knocks it loose with a 25 % chance (a uniform draw
  above 0.75): the ball is set 2 units over the carrier's centre with the
  carrier's velocity plus 3 units/s up and homes back as above. The
  carrier keeps the ball and keeps scoring. The condition reads the
  rider's stunt record (+0x30) flag 0x40, which the stunt event
  accounting `0024da00` sets when it opens a trick event, and the rider
  animation's main stunt clock (+0x884 +0x3fc), which `002da6c0` resets
  to 0 when a trick starts and `002d8ba0` advances in the main-stunt
  mode; the clock must be no more than the frame time, so the draw
  happens in the frame a trick begins.
- **Gain `00246310`.** "You are it!" (1.6 s) for the carrier, "%s has the
  ball!" (2 s) for the others, the beep. The carrier's engine is
  handicapped: racer +0x178 is the engine block that `001e3b80` fills
  from the vehicle's `[Engine]` data (+0x60 TopSpeed in feet/second,
  +0xa0 the garage TopSpeed adjustment); the gain sets the engine effect
  +0x68 to −0.34 and its speed term +0x70 to TopSpeed + adjustment. The
  engine update `001f5b40` adds effect × +0x70 to both ends of its speed
  taper (and effect × +0x6c to horsepower; Tag leaves +0x6c alone), so
  the carrier's top speed falls by 34 %. The drop `00246000` clears +0x68
  and +0x70.
- **Drop `00246000`.** The longest possession keeps the larger of itself
  and the current one; the current one resets.
- **Ball `00246da0`, `00246500`, `00246ec0`.** A carried ball resting on
  the ground for 2.5 s is lost ("You have lost the ball!"); after 5 s the
  ball is thrown back into play. `00246500` casts eighteen level rays at
  20° steps from a random heading (the base vector is rotated about the
  up axis by `0012b550`), each 300 units out from the ball, against the
  world collision (`00169b60`). The nearest hit under 300 units wins, and
  a hit under 100 ends the search. The throw direction is the unit vector
  to the hit plus the up axis, halved (about 45° up toward a level hit);
  each axis is scaled by its own `d + max(40, d × draw)`, `d` the hit
  distance and the draws uniform in [0, 1), and the ball gets a random
  spin of 0–16 on each axis. With no hit the ball returns to the event's
  stored start (+0x220), is put on the ground there (`0016ac10`) and
  raised 10 units.
- **Boundary.** Every frame `0020aec0` tests the ball against the
  heightfield (terrain +0x520/+0x524 tiles of 256 cells, +0x528/+0x52c
  cell sizes): outside when within 110 cells of an edge, or only past
  the edge when world +0x658 is set. That field is the scene's `[Sounds]
  CrowdPresent` (`00209460` defaults it to 0, `00209680` reads the key
  into it; `0020af90` gates the stadium camera flashes on it), so the
  Freestyle stadiums FREE01–FREE03 keep the whole heightfield in play
  where the 110-cell margin would leave a 108-unit square. `00246ec0` then sets the
  ball's velocity to the horizontal unit vector toward the heightfield's
  centre (tiles × 128 cells) times 170.4714 plus 112.152245 up; the
  position is kept.
- **End `00247160`.** Riders are sorted by score, highest first
  (`002fa120`); each run of equal scores is then sorted by rider slot
  (`002fa200` compares the rider indices), and places are numbered from
  1 into the racer's place field (+0x28); the TagOver menu shows the
  result.
- **Time limit.** Games Prep (`0026b7a0`) writes its Time Limit row's
  minutes × 60 to settings +0x57c for every game, and the event setup
  `0020f780` copies +0x57c to the event clock for Tag (and King of the
  Hills/Treasure Hunt). Tag therefore runs 2–20 minutes, 5 by default.

### Port

- `atv_core::tag`: the possession rules, scoring, immunity, re-grab
  guard, wreck drop (which also blocks a take in the same frame, since
  it resets the current possession), ground loss and respawn, the
  knock-loose draw (25 %, a port xorshift instead of the original's
  shared generator), the carrier's engine handicap (`carrier_engine`,
  applied through the vehicle's engine modifiers), the boundary test
  and full-speed throw-in toward the heightfield centre, the tie order
  and Games Prep minutes, plus an authored `Ball` (toss, homing catch,
  carry, bounce, knock, throw-in) with unit tests.
- **Online (authored).** Tag plays in multiplayer rooms, the port's
  replacement for split screen. The room host's Pause → **Play Tag**
  starts a game on the room's course (any venue but Coonce Arena): the
  host runs `atv_core::tag` and the ball for the roster in peer order and
  broadcasts `Packet::Game` (game 2: carrier, scores by slot, clock, ball)
  at 20 Hz and on every change of carrier; guests join the game when its
  state arrives and mirror it. A tag is two ATVs within 7 units centre
  to centre, or a rider within 5 units of a loose ball (authored contact
  sizes). "You are it!" / "%s has the ball!" / "%s dropped the ball!"
  banners and `tagBeep.vag` play; the end opens Tag Results (places by
  score, Restart Game for the host, Continue). Check:
  `tools/test_engine_multiplayer_tag.mjs` (relay group: Pause → Play
  Tag, the host carries and scores, the guest takes the ball with the
  `tagReach=60` test hook, both reach the same results).
- **Courses.** Each `*_TG.RES` repeats its venue's Freeride archive:
  a private comparison finds the eight Environment `envNNtg.scn` scenes
  byte-identical to the Freeride `envNNfr.scn` (the six Freestyle TG
  archives ship no scene), so the Tag "scene variants" are the Freeride
  scenes under the Tag name. What a TG archive adds is the mode TOY
  (`[SceneInfo] DefaultStartPosition`/`Direction`, plus Wheeler
  Junction's snow weather and Arlington Stream's train animations, as in
  the Freeride TOYs), `tagball.psg` with its `tagballm`/`tagballf` hulls
  and `tagball.pst`, and `radaricon_ball.pst`. `tools/package_content.py`
  packages the fourteen archives like the KH/TH ones, as lazily fetched
  archives of only the members that differ from Freeride (13.7–31 KB
  each, about 290 KB together); they are on the public R2 package.
- **Riding a Tag course.** In a room on one of the fourteen venues, the
  host's Pause → Play Tag reloads the room's ride from the venue's TG
  archive (read over Freeride, its mode TOY's start, the ball model) and
  then starts the game; a guest seeing the host's Tag state reloads the
  same way and joins. The ball draws as `tagball.psg`, posed each frame
  at the ball's centre through the posed-model path the Hockey puck
  uses; it keeps its minimap marker but no screen tag. Venues without a
  TG archive (Nationals, Supercross, Training) play with the screen tag
  as before. Riders reload at the one mode-TOY start; how the original
  spreads split-screen riders at Tag's start was not traced, so they
  overlap until they drive off (peer contact separates them).
- **Games page.** Single Player → Games lists Tag with its fourteen
  Tracks.dat courses (Prospect Springs and Tioga Valley open, the rest
  by course unlocks). The original reaches Games only from Multiplayer →
  Split Screen and has no CPU riders, so the port keeps Tag human-only:
  Start Game hosts a private multiplayer room on the chosen venue (the
  invite is shared from Pause as usual) and starts Tag on its Tag course
  with the page's time limit. The host's Pause → Play Tag uses the same
  minutes (`tagMinutes=N` in a link).
- **Rules in rooms.** The host runs the boundary throw-in at the
  original's full speed (a ball carried on an ATV is not thrown) and the
  knock-loose draw for its own tricks; remote riders' trick starts are
  not relayed, so a guest carrier's ball is never knocked loose (that
  needs a guest-to-host game message). Each rider applies the engine
  handicap to its own ATV while it is the carrier (motorcycles, a port
  extra, are not handicapped). The host runs `00246500`'s ground respawn:
  its rays are cast against the heightfield in 1-unit steps (static
  objects and stadium walls are not tested, and the ball has no rigid
  body, so a throw can leave a stadium over its walls until the edge
  throw-in returns it), with the draws from the port's generator; no spin.
- The room's Tag course is resolved from the ride's own content package,
  so a room opened from an invite link (no frontend loaded) also moves to
  the TG archive.
- Check: `tools/test_engine_multiplayer_tag.mjs` (relay group) now moves
  both pages onto `E05_TG.RES` (the TOY start, `tagball.psg`), covers the
  throw-in (`tagTest=throw` starts the ball past the margin), the time
  limit option (`tagMinutes=3`), the host's and then the guest's engine
  handicap, the transfer and the results; `cargo test -p atv-game --test
  tag_courses -- --ignored` (private hosted package) loads all fourteen
  Tag courses: TOY start, ball model drawn, nothing unsupported, and on
  each course's heightfield a ball past the margin (past the edge in the
  CrowdPresent stadiums) is thrown back inside and the ground respawn
  from the start lands inside (eleven courses find a ray hit, FREE02 and
  FREE05 return to the start). `tools/test_engine_games_tag.mjs` (relay
  group) plays the Games page path: Tag bought, a locked Tag course
  refused, Start Game on Prospect Springs with a 3:00 limit hosts the
  room on `E05_TG.RES`, and a guest joining by the code follows onto the
  Tag course into the same running game.

## Uncertainty

- Team colours: team 0 is read as Red and team 1 as Blue from the
  `Hockey_Red_Default`/`Hockey_Blue_Default` order; the team select page
  was not traced.
- The goal triggers' `Box` values are read as half extents (the box then
  spans the ice to the crossbar).
- Tag: the knock-loose draw uses the port's own generator, so its
  sequence differs from the original's shared one; the heightfield extent
  is read as the TR2's cell span (assumed equal to tiles × 256 cells);
  `tagball.psg` is drawn at the ball's authored 1.5-unit contact radius
  centre without the original rigid body or spin. The respawn's base
  ray vector and up axis (`0x4a7b70`/`0x4a7b80`) are filled at run time
  and were read as a level forward vector and +Y from how the throw adds
  the axis to the hit direction; the random heading makes the base
  vector's choice immaterial.
