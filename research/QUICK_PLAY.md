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
  `cpu`, `place`, `value`, `dnf`, `left`). `pid` is the verified player id
  of the rider's socket (`?rider=` token, `Players::verify`,
  research/ONLINE_IDENTITY.md), null for a rider without one and for CPUs;
  CPUs are named `CPU n`. A round the driver never started writes none.
- Restarts keep a Quick Play room's playlist and riders' slots; its loop
  starts again in the lobby.

## Protocol (version 6, compatible)

- No HELLO flag: a `QP…` room is reached only through `POST
  /multiplayer/quickplay`, which only new clients call, so the server sends
  `RoomLoop` to every rider in it and accepts `Vote` from any. (The first
  HELLO flags byte is full: 32 is `MOTORCYCLES`, 64 chat, 128 hub.)
- `POST /multiplayer/quickplay` `{"playlist": "race"|"stunts", "avoid":
  code?}` → `{code, key, quickplay: true, playlist, links, batches,
  frames}`: a seat in the lowest-numbered room of that playlist that is
  between rounds (lobby, vote or podium), else a new room. Rooms are
  `QPR00001`… and `QPS00001`…; the `QP` prefix is reserved (a private room
  cannot take it). Same admission limit as a Free Ride claim. The socket is
  the usual `/multiplayer/rooms/{code}?key=…&rider=…`.
- Kind 22 `RoomLoop` (server → every rider in the room; on change and every 2 s):
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
- CPU difficulty is fixed at Normal and rooms are not split by skill:
  `Players::rating(pid)` (slice D) is available; wiring it in is wave 2.
- The speed cap and drift allowance are authored guesses, not measured on
  slow Chromebooks; a very laggy rider could be refused.
- Not measured on production or physical phones.

## Private rooms (slice C)

Issue #634. A private room runs the same loop and panel with its host in
charge: a **host-run** variant of `engine/quickplay.rs` (`Loop::hosted`).
Nothing here is recovered behavior.

### What a rider sees

- **Lobby.** Seats with ready ticks, ping bars, a gold **HOST** tag and CPU
  tags (only when the host fills with CPUs). Guests get **Ready**. The host
  gets **Start**, a mode button (**Race ▸ / Hills ▸ / Treasure ▸ /
  Stunts ▸**) and, for races, **CPUs ✓ / No CPUs**. A 10 s countdown runs
  once every guest is ready; Start goes at once.
- **Vote.** Three cards of the host's mode and **Random**, 12 s, early when
  everyone voted, vote-weighted roulette as in Quick Play. The first card
  is the room's own track when the mode has it (the original Select Track
  choice; on a rematch, the track just played). The host's **Skip ▸** ends
  the vote at once and the host's own card wins (Random draws one; no
  card: the roulette spins).
- **Countdown, round, podium.** As in Quick Play. Rematch (the default)
  opens the next vote; **Leave** leaves the room (Pause → Leave Game's
  path), not into a queue.
- **Late joiners.** A rider who joins mid-round sees **Next round** and a
  **▸ name** button: a follow camera behind a racer (26 ft back, 10 ft up,
  eased), cycling the racers on each press. Meanwhile its own ATV is
  parked (no input, no simulation steps) and the server relays its pose to
  nobody (`Loop::watching`: in play and not a racer; frames, single poses
  and arrival catch-ups alike), so racers neither draw nor collide with it.
  This holds in Quick Play rooms too. It takes a seat at the next vote.
- **Existing flows stay.** Pause host rows (Select Track, Play Race, the
  games, Freestyle), invite links and room codes are unchanged. A round the
  host starts from Pause is followed by the loop (straight to play, then
  the podium and the vote); its host never restarts it.

### Rules (`Loop::hosted`)

| Rule | Value | Why |
| --- | --- | --- |
| Lobby | until Start, or 10 s after every guest is ready (the host's own Ready does not count; older builds are not waited for) | spec |
| Vote | 12 s, early when every rider that follows voted; Skip ends it | spec |
| Rounds | the host's mode only (`quickplay::mode_pool`) | spec |
| Laps, minutes | the host's Race menu laps and Games Prep minutes | host authority |
| CPU fill | the host's toggle: race field to 6, at most 4 CPUs, Normal | Quick Play's fill |
| Round numbers | above every game serial the host has run | a new round never reuses a serial guests already saw |
| Results | the host's own table (race finish times, game scores); never `verified`; no match records | host authority, no server checks |

- **Driver.** Always the room's host (`Lobby.host`); a host handoff passes
  the loop with the room, as private rooms already pass rounds.
- **Adoption.** A host `GameState` of a loop mode in an active phase (race
  grid to racing, game countdown or play) with a serial the loop has not
  played moves the loop straight to play on the room's map.

### Protocol (compatible)

- The room claim's JSON says `"roomloop": true`; such a client adds the
  second HELLO byte **`signal::LOOP`**. A private room gets a loop when its
  host's socket said it, and drops it when a host that did not is
  connected (an absent host's held slot keeps it). `RoomLoop` goes only to
  sockets that said `LOOP`; older builds in the same room keep today's
  flow. Without the heartbeat for 6 s a client hides the panel.
- `RoomLoop` gains an optional 2-byte tail, **`Hosted`** (mode, flags:
  `CPUS`); `Vote` gains an optional 4-byte tail, **`Hosting`** (mode, flags
  `CPUS`/`START`/`SKIP`, laps 1-5, minutes 1-20), sent by the host only.
  Quick Play packets carry neither, so they are byte for byte as before.
  Start counts only in the lobby and Skip only for the current vote, so a
  late resend never starts or skips the next one.

### Code

- `atv-net` `quickplay.rs`: `Hosted`, `Hosting`, `host_flag`, `mode_pool`.
- `atv-server` `engine/quickplay.rs`: `Loop::hosted`, `hosted_lobby`,
  `adopt`; `engine.rs`: `Room::private_loop`, `sync_private_loop`, the
  `loop_on` socket flag; `http.rs`: `"roomloop"`.
- `atv-ui` `hud/quickplay.rs`: `START`, `MODE`, `CPUS`, `SKIP`, `WATCH`.
- `atv-app` `quick_play.rs`: host choices, `follow`; `lib.rs`: the panel
  from `Multiplayer::private_loop`, the follow camera, Leave.
- Browser gate: `tools/test_engine_private_loop.mjs` (multiplayer group).

### Limits

- Rooms still seat four humans (`RACE_RIDERS`); 8 riders needs the wider
  `GameState` (#670 may widen it).
- The follow camera does not recentre the fine terrain patch on the watched
  rider, so a far-away racer is seen over coarser ground.
- The host's laps and minutes are read from its menus each frame; changing
  them mid-vote changes the coming round.
- Not measured on production or physical phones.
