# The original mini-games ("Games")

ATV Offroad Fury 2 has four mini-games: King of the Hills, Tag, Hockey and
Treasure Hunt (event types 9-12). This note records the exact-build evidence
for the shared Games framework and the two games the port plays, King of the
Hills and Treasure Hunt, and what remains open. Tag and Hockey plug into the
same framework separately.

## Evidence

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

### Treasure Hunt (`QuadzillaEventTreasure.cpp`, vtable `0045e4e0`)

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

### King of the Hills (`QuadzillaEventHillKing.cpp`, vtable `004578b0`)

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

## Port

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
  courses are marked and refused with the store's lines; Tag and Hockey
  are listed but not playable yet.
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

## Adaptations and open items

- **One player, no opponents.** The original is split-screen or online only;
  the port plays it alone (scores, ranking and results work for more
  riders, but nothing else rides).
- **Freeride scene.** The port rides the Freeride scene with the game's
  mode TOY; the game scenes (`env05kh.scn`, Treasure Hunt's Enduro scenes),
  the `bomb.mcf` treasures with their sparks and explosion, the `kohflag`
  flags, `kohbase` and spotlights are not drawn; screen tags and minimap
  markers stand in for them. `iconhit.vag` is not played.
- **Trigger contact.** The original tests the ATV's collision body against
  the box; the port tests the ATV origin against the box grown by 3 ft
  (1.5 ft vertically). `Box` is read as half extents.
- **Start countdown.** The original runs the shared 3-2-1 start; the port
  holds the ATV for 3 s with a number in the panel.
- **Hill HUD.** The per-hill score pills are not drawn; each hill's tag
  shows its number in its owner's colour.
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
These are host Chromium checks with a private test package.
