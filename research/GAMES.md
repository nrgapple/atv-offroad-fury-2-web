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
  the Coonce Arena session described below;
  Tag is listed but not playable yet.
- `atv-app/src/games.rs` loads the game archive's mode TOY, rides the
  venue's Freeride scene, places the ATV at the TOY's `Start1` (else the
  venue start), holds it for a 3 s start countdown, samples position,
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
  the port plays it alone (scores, ranking and results work for more
  riders, but nothing else rides).
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
  game archive carries is the Freeride one. Treasure Hunt still rides the Freeride scene: its `bomb.mcf`
  treasures are mode-TOY keyframed animations the port does not draw, and
  the sparks and `bomb_explosion.emt` bursts are not emitted; screen tags
  and minimap markers stand in for the treasures.
- **Sounds.** The start countdown plays the race start's `beep01`/`go`
  (`002e79a0`); each treasure plays `iconhit.vag` (`002d0d70`), which the
  packager now adds to the hosted `AUDIO.RES`. No capture sound was found
  for King of the Hills.
- **Trigger contact.** The original tests the ATV's collision body against
  the box; the port tests the ATV origin against the box grown by 3 ft
  (1.5 ft vertically). `Box` is read as half extents.
- **Start countdown.** The original runs the shared 3-2-1 start lights
  (Overlays.pb object 10); the port holds the ATV for 3 s with the number
  in its panel and the start beeps, not the light sprites.
- **Hill HUD** (`QuadzillaHillScoreHud`). `002eac20` builds one
  `koh_pillbkg` widget per hill (30 x 31 with a 10 x 11 inset, white,
  alpha 0x80) and one text per hill whose callback (`002df040`) prints
  "%d" of the hill record's +4: the stunt score that took the hill, i.e.
  the score to beat (`00220b50` stores it with the owner and the owner's
  colour at +8). `002203d0` places each pill at the hill's position raised
  12 ft; every frame `002207e0` projects it and shows the pill only when
  it is on screen and a ray from the camera reaches it (`00169b60`),
  hiding it otherwise (`002eb0b0`). The port draws a rounded pill in the
  owner's colour (white unowned) with the score to beat ("0" before the
  first capture) at the same raised point, only while it projects inside
  the view (off-screen hills are hidden, not pinned to the edge like rider
  tags), and keeps the hills on the minimap. Not ported: the occlusion
  ray (a hill behind terrain still shows its pill) and the
  `koh_pillbkg` artwork itself (a drawn pill stands in).
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
  contact radius. The team marker quads, team rider textures, hockey
  camera and hat emitters are not drawn.
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
  within 3 units. While carried, a random 25 % chance on some condition
  of the carrier's ATV knocks it loose 2 units up with 3 units/s up (the
  condition, rider +0x30 flag 0x40 against a time threshold, is
  unidentified).
- **Gain `00246310`.** "You are it!" (1.6 s) for the carrier, "%s has the
  ball!" (2 s) for the others, the beep; the carrier's ATV gets a
  handicap (+0x178 +0x68 = −0.34, reset on drop; meaning unverified).
- **Drop `00246000`.** The longest possession keeps the larger of itself
  and the current one; the current one resets.
- **Ball `00246da0`, `00246500`, `00246ec0`.** A carried ball resting on
  the ground for 2.5 s is lost ("You have lost the ball!"); after 5 s the
  ball is thrown back into play toward the nearest open ground. A ball
  past the course boundary (110-unit margin) is thrown toward the map
  centre at 170.47 units/s and 112.15 units/s up.
- **End `00247160`.** Riders are sorted by score (a second pass orders
  equal scores by a comparator not yet read) and numbered from first
  place; the TagOver menu shows the result.

### Port

- `atv_core::tag`: the possession rules, scoring, immunity, re-grab
  guard, wreck drop, ground loss and respawn, time-out and standings,
  plus an authored `Ball` (toss, homing catch, carry, bounce, throw-in)
  with unit tests.
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
- **Courses.** The original's Tag rides its fourteen `*_TG.RES` course
  archives (their own scene variants; the mode TOY only sets a start).
  They are not in the hosted package, so the port rides the room's
  Freestyle venue instead. Hosting them would mean adding the fourteen
  archives to `tools/package_content.py` (each a full environment copy)
  and a Tag course list in room setup; the ball model `tagball.psg`
  ships only there, so the ball shows as a screen tag.
- Not implemented: single-player Tag (the original has no CPU riders),
  the carrier handicap, the random knock-loose rule, the boundary
  throw-in toward the map centre (the ball is thrown back toward the
  host), Tag's time-limit option and tie order.

## Uncertainty

- Team colours: team 0 is read as Red and team 1 as Blue from the
  `Hockey_Red_Default`/`Hockey_Blue_Default` order; the team select page
  was not traced.
- The goal triggers' `Box` values are read as half extents (the box then
  spans the ice to the crossbar).
- Tag's tie order, the knock-loose condition and the carrier handicap are
  unread.
