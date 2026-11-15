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
- **Countdown.** The picked card, 4 s, beeping its last three seconds with
  the start lamps' beep (`beep01.vag`). The round then loads, holds on its
  own grid and starts with the race's 3-2-1-GO lamps (which play GO).
- **Podium.** Top three on steps, values counting up, a gold flash for the
  winner, the rest listed below. Each verified rider's league points count
  up beside the name (green `+35` chips); this rider's strip shows `Lv n`,
  the level bar filling, `+35 XP` and `◆ Tier` (`▲` on a new tier). A new
  level pulses `LEVEL UP!` with a flash and confetti once the count ends.
  **Rematch** (the default) / **Leave** for 8 s; everyone's choice ends it
  after 3 s. Leave queues the rider into another room straight away.
- Late joiners watch the round ("Next round") and play the next one.
- **Join a friend.** Multiplayer → Friends: a friend in a Quick Play room
  shows `● Name · Join`; Join takes a seat in that room in any phase (a
  rider who arrives mid-round watches until the next one). A full room
  falls back to the ordinary queue.
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
| CPU fill | race field to 6, at most 4 CPUs | spec; 4 is the protocol's CPU limit |
| CPU difficulty | Expert when the room's rated riders average an ordinal of 20 or more, else Normal | judgment call, below |
| Rating split | only between rooms that are both still in their lobby | judgment call, below |

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
- **Rated CPUs.** At the vote's end the round's CPU difficulty comes from
  the hidden ratings (`Players::rating`, research/ONLINE_IDENTITY.md) of
  the room's verified riders: their mean ordinal (mu − 3 sigma) of 20 or
  more gives Expert, anything else Normal (the original's default; the
  room race has only these two). Unverified riders do not count; a room
  with no rated rider is Normal. Judgment: a new rider is at 0 and an
  established average rider settles near 15–20 as sigma shrinks, so Expert
  needs riders proven clearly above the 25 start. Ratings refresh from the
  player store every second and when a rider is admitted; they move only
  when the record file is read (every 15 s).
- **Splitting by rating.** A claim carries the rider's token (in the body);
  when two or more rooms of the playlist are still in their lobby with a
  seat free (formed in the same wait window), the rider goes to the one
  whose rated riders' mean ordinal is nearest the rider's (an unrated room
  counts as 0). Otherwise rooms are never split: the lowest room between
  rounds is taken, as before, so a quiet server keeps one room. Judgment:
  splitting a single room would leave riders waiting alone.
- **Podium rewards.** When a round ends the server works out what each
  verified rider earned with the league rules (`league::match_points`:
  10 for taking part, 5 for each rider behind, 10 for a win, 0 for a
  leaver), from the rider's stored XP and week, and sends it (kind 25
  `Rewards`) to the builds that said they take it. It counts nothing
  itself: the record file's reader adds the same points within 15 s
  (`Players::rewards` is a preview of `Players::ingest`, unit-tested
  against it). Two matches of one rider within 15 s would show the second
  from slightly old XP. Unverified rounds, CPUs and riders without a
  verified id get none.
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
- Kind 25 `Rewards` (server → riders that said `signal::REWARDS`, once per
  podium): the round number and, for each verified rider, peer, flags
  (LEVEL_UP, TIER_UP), points, lifetime XP, level, the XP where that level
  and the next start, tier and week points. Gated because a Quick Play
  build before it closes its session on an unknown kind: the claim answers
  `"rewards": true`, and only then does the client add `REWARDS` (bit 2) to
  the optional second HELLO byte (servers before it refuse the bit).
- The claim body also takes `"token"` (the rider's token, for the rating
  split) and `"room"` (a friend's `QP…` room of the same playlist, seated
  in any phase). `POST /multiplayer/friends` with `"quickplay": true`
  (builds that join Quick Play) lists a friend's Quick Play room code;
  older builds still get none, so they never offer a Join they cannot do.
- Kind 23 `Vote` (client → server, on change and every 2 s): round number,
  card or Random or none, rematch (undecided/stay/leave), ready, and the
  sender's own round trip for the others' ping bars. A room without a loop
  ignores it.
- All three are bounded and validated on encode and decode; truncated, padded
  or out-of-range packets are refused (`atv-net` `quickplay` tests).

## Code

- `crates/atv-net/src/quickplay.rs`: `RoomLoop`, `Vote`, flags, playlists
  and pools, the `QP` code prefix.
- `crates/atv-server/src/engine/quickplay.rs`: the loop (`Loop`), checks,
  podium, records and CPU difficulty;
  `Players::rewards` (`players.rs`) the podium rewards, sent from the room
  task (`rooms.rs`); the loop unit-tested as a pure state machine;
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
- Test hooks: `?quickplay=race|stunts` queues straight from a link;
  `&quickplayRoom=QPR00001` tries that room first (Friends → Join).
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
  a lone racer gets a field of five, not six. Wave 2 (#670) found 8 human
  seats unsafe without a protocol step (below).

- One load a round for races, King of the Hills and Treasure Hunt: when
  the countdown moves the room to the round's course, the client keeps
  riding where it is (`QuickPlay::defers`, `Net::defer_course`) and the
  round's own start loads the course once as the race or game. Freestyle
  rides the moved course (its start does not reload). If the move comes
  before the client has seen the countdown, or the round never starts,
  the course loads as before (twice, or once afterwards).
- The countdown's beeps need `beep01.vag`, which room rides now load even
  off a race (`World::lamps`); GO is left to the round's own lamps a
  moment later, since the round still loads and holds on its grid.
- Ping is each rider's own report, not measured by the server.
- The rating thresholds (ordinal 20 for Expert, nearest mean for the
  split) are judgment calls, not tuned on real play; ratings move only in
  rooms with two or more registered humans, so most riders stay near 0.
- **8 human seats: not built, not safe in this protocol.** Race CPUs take
  peer ids 4–7 (`CPU_PEER = RACE_RIDERS`) in `Cpus`, `Progress`, poses and
  podiums, and the room game state (kind 13) carries fixed four-slot
  arrays (`grid`, `stunts`, scores, the `players` mask). Private rooms
  share these packets with older builds (protocol 6 is compatible), and
  every Quick Play build since #663 decodes them the same way, so seating
  humans 4–7 means new CPU ids and a new game-state encoding that older
  builds refuse (closing their session) or misread. It needs a protocol
  version step (or a gated second encoding in rooms of new builds only),
  not a widening; left for a later slice.
- The speed cap and drift allowance are authored guesses, not measured on
  slow Chromebooks; a very laggy rider could be refused.
- Not measured on production or physical phones.

## Private rooms (slice C)

Not built here. Slice C reuses kinds 22/23 for private rooms. A private
room's riders can be older builds, so there the capability travels in an
optional **second HELLO data byte, bit 1 `signal::LOOP`**: the server sends
`RoomLoop` only to sockets that said it. The codec and the server already
accept a one- or two-byte HELLO (second byte at most `LOOP`;
`crates/atv-net/tests/protocol.rs`, the engine's Quick Play test); no
build sends it yet. The host keeps authority, `Vote::ready` becomes
ready-up, the vote and `Vote::rematch` drive the host's next round, and
seats and spectators show on the same panel. The private room's loop would
be a host-run variant of `engine/quickplay.rs` (no records or server checks
needed).
