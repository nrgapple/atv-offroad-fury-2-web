# Quick Play (port addition)

Issue #632, epic #631, spec research/MULTIPLAYER_EXPERIENCE.md (slice A).
Nothing here is recovered behavior: the PS2 game's online lobbies are not
used (MULTIPLAYER_ORIGINAL.md). Everything below is authored for the port.

## What a rider sees

- **Main Menu → Quick Play** leads the main menu, only once the game server
  says it runs Quick Play (`"quickplay": true` beside the rider count on
  `/multiplayer/freeride/riders`, polled on the main menu and Multiplayer).
- **Two tiles: Race / Stunts.** One tap queues. Race rotates races and King
  of the Hills; Stunts rotates Freestyle and Treasure Hunt.
- **Lobby.** Seats fill with names, CPU tags (Race) and ping bars for the
  others (green under 80 ms, yellow 80–150, red above). A big countdown
  (20 s at most) and one button, **GO**. Everyone's GO starts at once; a
  full room (four riders) starts at once.
- **Vote.** Three track cards (one of each of the playlist's modes, then
  any) and **Random**, with live vote counts and a 12 s countdown that ends
  early when everyone voted. Keys 1–4 pick a card.
- **Countdown.** The picked card, 4 s while the course loads. The round then
  holds on its own grid and starts with the race's 3-2-1-GO lamps.
- **Podium.** Top three on steps, values counting up, a gold flash for the
  winner, the rest listed below. **Rematch** (the default) / **Leave** for
  8 s; everyone's choice ends it after 3 s. Leave queues the rider into
  another room straight away.
- Late joiners watch the round ("Next round") and play the next one.
- Input: touch and mouse tap the buttons (48 px or more); keyboard Tab /
  Enter / 1–4 (the riding keys stay the ride's); a controller's D-pad and
  A (A never presses Leave, since it is also the throttle). The panel fits
  a 360 px portrait screen.

## Rules (server, `crates/atv-server/src/engine/quickplay.rs`)

The server owns the loop; no player is the room's host in the private-room
sense. Phases: lobby → vote → countdown → play → results → vote …

| Rule | Value | Why |
| --- | --- | --- |
| Lobby wait | 20 s from the first rider, at most | spec |
| Vote | 12 s, early when all voted | spec |
| Countdown | 4 s | course load; the round's own 3-2-1 follows |
| Podium | 8 s, 3 s minimum when everyone chose | spec |
| Driver start timeout | 45 s without the driver's round state: back to the vote | authored |
| Race cap | 12 min | authored |
| Game / Freestyle cap | limit + 90 s | authored |
| Race laps | 2 (`raceLaps=N` overrides on the driver for tests) | authored: short rounds |
| Game / Freestyle limit | 2 min (Games Prep's minimum) | authored |
| CPU fill | race field to 6, at most 4 CPUs, Normal | spec; 4 is the protocol's CPU limit |

- **Roulette.** Each vote is a ticket; with no votes every card has one.
  Random draws uniformly from the playlist's whole pool (65 Race rounds:
  every race of every National, Supercross and Environment course, Enduro
  long/short and the three Short Tracks, plus King of the Hills on its 14
  venues; 28 Stunts rounds: Freestyle and Treasure Hunt on the 14 game
  venues). The round just played is not dealt again.
- **The driver.** CPU motion and the King of the Hills / Treasure Hunt
  rules run on the client today (they need physics and course data the
  server does not have), so the server names one connected rider the
  round's **driver**: the lowest seat that follows the loop. It is the
  room's `Lobby.host`, so the existing host code runs the round
  (`RoomRace::host`, `start_room_game`, `start_room_freestyle`) with the
  server's setup: variant, laps, CPU field, difficulty, the grid seed and
  the round number as the game serial. The driver cannot change the map
  (`SetLobby` is refused) and starts only what the server picked. When the
  driver leaves, the next rider becomes driver and the clients' existing
  host handoff runs the round on (`Field::adopt`; CPUs retire, as in
  private rooms).
- **Seats.** Four human seats (peer ids 0–3), because room races and games
  carry four rider slots (`RACE_RIDERS`); CPUs take ids 4–7. A Race room
  is therefore at most 8 riders: 4 humans and 4 CPUs. More humans per room
  needs a wider `GameState` (not built).
- **Server-checked race results.** The server sees every rider's forwarded
  `Progress` and the driver's race state:
  - laps never go back and a reported finish time never changes;
  - race distance may grow at most 180 ft/s (plus 200 ft per check) over
    any second;
  - a finish time is no shorter than the final distance at 180 ft/s;
  - GO on the server clock comes from the driver's countdown (arrival plus
    its timer); a finish time more than 3% (plus 1.5 s) slower or faster
    than the server's clock from GO to the report is refused.
  A refused rider ranks DNF (`REJECTED`). `verified` is set when the server
  timed GO and heard every rider still in the room. CPUs rank on the
  driver's frozen CPU results.
- **Stunt results.** Freestyle ranks on the driver's table, but never above
  a rider's own last report the server forwarded; a disagreement clears
  `verified`. King of the Hills and Treasure Hunt rank on the driver's
  table and are never `verified` (the server cannot check captures).
- **Match records.** Every played round appends one line to
  `FURY_STATE_DIR/matches.jsonl` in the spec's contract (`t`, `room`,
  `playlist`, `mode`, `map`, `verified`, `riders[]` with `pid`, `name`,
  `cpu`, `place`, `value`, `dnf`, `left`). `pid` is the socket's `?pid=`
  (1–32 printable ASCII characters, unverified) until slice D replaces it;
  CPUs are named `CPU n`. A round the driver never started writes none.
- Restarts keep a Quick Play room's playlist and riders' slots; its loop
  starts again in the lobby.

## Protocol (version 6, compatible)

- HELLO second flags byte (`data[1]`), bit 1 `signal::ROOM_LOOP`: the
  build follows the loop (the first byte is full; bit 32 is
  `MOTORCYCLES`). A HELLO is one or two bytes. The
  client sends it only when its claim said `"quickplay": true`.
- `POST /multiplayer/quickplay` `{"playlist": "race"|"stunts", "avoid":
  code?}` → `{code, key, quickplay: true, playlist, links, batches,
  frames}`: a seat in the lowest-numbered room of that playlist that is
  between rounds (lobby, vote or podium), else a new room. Rooms are
  `QPR00001`… and `QPS00001`…; the `QP` prefix is reserved (a private room
  cannot take it). Same admission limit as a Free Ride claim. The socket is
  the usual `/multiplayer/rooms/{code}?key=…` (plus optional `pid`).
- Kind 22 `RoomLoop` (server → ROOM_LOOP builds; on change and every 2 s):
  phase, playlist, phase deadline in server time (`Pong::server_ms`), up to
  3 cards (mode, map, variant), votes per card and Random, the pick, the
  setup (round, number, laps, minutes, CPU field, difficulty, seed), the
  driver, up to 8 seats (peer, flags CPU/VOTED/READY/SPECTATING/LEAVING/
  DRIVER, ping ms) and up to 8 placings (peer, flags CPU/DNF/LEFT/REJECTED,
  value), and `verified`.
- Kind 23 `Vote` (client → server, on change and every 2 s): round number,
  card or Random or none, rematch (undecided/stay/leave), ready, and the
  sender's own round trip for the others' ping bars. A room without a loop
  ignores it.
- Both are bounded and validated on encode and decode; truncated, padded
  or out-of-range packets are refused (`atv-net` `quickplay` tests).

## Code

- `crates/atv-net/src/quickplay.rs`: `RoomLoop`, `Vote`, flags, playlists
  and pools, the `QP` code prefix.
- `crates/atv-server/src/engine/quickplay.rs`: the loop (`Loop`), checks,
  podium and records, unit-tested as a pure state machine;
  `crates/atv-server/src/queue.rs`: the route and the record file. Hooks in
  `engine.rs` (`Room::quickplay`, vote, observed game state, progress and
  claims, the loop step in `tick`), `rooms.rs` (`QuickClaim`, `pid`,
  records) and `http.rs` (route, reserved prefix, `pid`, the advertised
  flag).
- `crates/atv-ui/src/hud/quickplay.rs`: the panel; Main Menu row and the
  Quick Play tiles in `front/controller.rs` and `front/mod.rs`.
- `crates/atv-app/src/quick_play.rs`: loop → panel, votes, driver starts;
  hooks in `lib.rs`, `overlay.rs`, `multiplayer.rs` and `atv-game`'s
  `Session`/`Serves`.
- Test hook: `?quickplay=race|stunts` queues straight from a link.
- Browser gate: `tools/test_engine_quickplay.mjs` (multiplayer group).

## Limits and unverified items

Two accepted deviations from the spec (epic owner, 2026-11-14):

- **A rider drives each round.** The server owns the loop, the vote, the
  map choice, the podium (checked results) and the match records, but one
  connected rider (the driver) starts and simulates the round with the
  existing host code: CPU physics and the King of the Hills / Treasure Hunt
  rules need data the server does not have. Moving them to the server is
  later work.
- **4 humans plus up to 4 CPUs.** Room races and games carry four rider
  slots, so a room seats at most four humans (eight riders with CPUs), and
  a lone racer gets a field of five, not six. Rooms of 8 humans move to
  wave 2 (a wider `GameState`).


- The double load: the countdown moves the room to the round's map, then
  the driver's round start reloads it again as the race or game (as a
  private room's Select Track then Play Race does).
- The 3-2-1-GO with sound is the round's own (race lamps and beeps); the
  loop's countdown screen is silent.
- Ping is each rider's own report, not measured by the server.
- CPU difficulty is fixed at Normal until slice D's hidden rating.
- The speed cap and drift allowance are authored guesses, not measured on
  slow Chromebooks; a very laggy rider could be refused.
- Not measured on production or physical phones.

## Private rooms (slice C)

Not built here. Slice C reuses kinds 22/23 and `ROOM_LOOP` for private
rooms: the host keeps authority, `Vote::ready` becomes ready-up, the vote
and `Vote::rematch` drive the host's next round, and seats/spectators show
on the same panel. The private room's `Loop` would come from a host-run
variant of `engine/quickplay.rs` (no records, no server checks needed).
