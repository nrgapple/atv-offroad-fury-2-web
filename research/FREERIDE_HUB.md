# Free Ride hub (port addition)

Issue #633, epic #631, umbrella spec research/MULTIPLAYER_EXPERIENCE.md.

The shared Free Ride rooms (`FREERD01`-`FREERD25`, research/MULTIPLAYER.md
"Arlington Free Ride") gain three things to do besides riding around:
challenging a nearby rider, pop-up events and a map rotation. None of it is
recovered behavior; the PS2 game has no persistent shared room. The server
(fury-server, `crates/atv-server/src/engine/hub.rs`) owns every timer and
judges every result; clients show it and send the rider's choices.

## Rules

### Challenge a nearby rider (opt-in)

- A **Challenge** button shows while another hub rider is within 100 ft
  (horizontal) and neither is busy. Keyboard: G.
- The other rider gets **ACCEPT / NO** with an 8 s ring timer (Y / N).
  Nothing happens without an accept: an unanswered offer expires.
- Accepted: both race to a server-picked waypoint (gold ground circle, a
  GOAL tag and map marker, the distance and a countdown). The first within
  30 ft wins; at the time limit it is a draw ("TIME UP").
- Time limit: 15 s plus 1 s per 35 ft to the waypoint, within 30-60 s.
- One minute cooldown per pair after every ask, answered or not.
- The server refuses an ask when the two are more than 160 ft apart (poses
  lag), when either is in a challenge or a running event, while a rotation
  vote is open, within the pair's cooldown, and when the other build has no
  hub ("TOO FAR", "BUSY", "WAIT A BIT").
- Muted or blocked riders: the client declines their offers at once
  without showing them (`Hub::blocked`, a slot mask). Slice E (#636,
  research/QUICK_CHAT.md) owns the block list; until it lands nothing sets
  the mask. Follow-up: E sets `net.hub.blocked` from its list.
- Result toast: "YOU WIN!" +50 or "SO CLOSE" +10 (hub points, this room).

### Pop-up events

- Every 4-6 min (random) the server opens a **join circle** 60-250 ft from
  a random hub rider: a glowing gold ring on the ground, a JOIN tag and map
  marker, and a chip with the event name, the distance and a 45 s
  countdown. Driving into the circle (40 ft) joins; the chip turns to
  "IN · n". Riders who do not join see only the chip and tag.
- The events rotate in order:
  - **Sprint**: first to a waypoint 400-1000 ft from the circle (same
    timing and 30 ft finish as a challenge). Ends when every joiner arrives.
  - **Hill King**: seconds within 35 ft (and 30 ft of height) of the hill,
    the highest spot ridden within 500 ft of the circle; 60 s.
  - **Stunt-Off**: stunt points committed in 60 s, reported by each client
    twice a second at most (self-reported, see Limits).
- Nobody joins: cancelled quietly. One joiner: a **solo score challenge**
  (the same event, scored against itself: a sprint finish 50, Hill King 1
  point a second up to 60, Stunt-Off 1 point per 1,000 up to 100, at least
  10).
- Two or more: top 3 get 100 / 60 / 30, everyone else 10. Every joiner gets
  a toast with the top 3 and their points.
- A rider who leaves drops out of the standings. Live standings go out once
  a second while an event runs.
- No event starts within 2 min (plus the join window) of a rotation.

### Map rotation

- Every 20-25 min (random) the room moves to another venue. A minute
  before, hub riders get **NEXT MAP** with three venues and live counts
  (buttons, or 1-3). Most votes wins; ties and no votes pick at random.
- The whole room moves: the server changes the shared room's map and sends
  the ordinary `Lobby`, which every build, older ones included, follows as
  it follows a host's course change in a private room. Events and
  challenges in progress end; poses wait for each rider's Ready on the new
  venue; waypoint spots start over.
- New arrivals ride the room's current map (the claim's `Welcome` lobby).

### Venues

The rotation offers the eight Environment venues (`env01`-`env08`, map ids
1-8): Caribou Peak, Tioga Valley, Sonora Dunes, Freeport Harbor, Prospect
Springs, Rockbridge Park, San Jacinto Isles and Arlington Stream.

Evidence (facts):

- They are the original's open venues: each ships a Freeride archive
  (`E0n_FR.RES`, `launch.rs` tests), long and short Enduro courses across
  the map (`E0n_ENDL.RES`/`ENDS`, `launch.rs` `RaceVariant::exists`) and
  King of the Hills / Treasure Hunt games on seven of them
  (research/GAMES.md "Courses").
- Private rooms already ride any of the 40 venues through the same
  `SetLobby`/`Lobby` path the rotation uses (research/MULTIPLAYER.md
  "Room Freestyle", `Room::receive`).
- Arlington Stream, the shared rooms' only venue until now, is one of them.

Judgment: Nationals and Supercross are closed race tracks and stadiums,
the stunt venues are arenas (research/FREESTYLE_MAPS.md "Family"), and
Training and Coonce Arena are special; none suits 20 riders roaming, so
they are left out. Arlington's train runs only on Arlington.

Rotation ships **on** (the requested default). Risk assessment: older
builds follow the lobby's map exactly as in private rooms, so nobody is
left on the old venue; the cost is a reload every 20-25 min. Turn it off
with `FURY_HUB_ROTATION_SECS=off` if that proves disruptive.

## Waypoints without terrain

The server has no game content. Waypoints, circles and hills are **spots
riders of this room actually rode through**: every 2 s each moving rider's
position is kept, up to 512 spots per room (random replacement when full),
cleared on a rotation. So a goal lies on ridden ground. With no spot in
range (an empty new room) the farthest spot at least half the minimum
distance is used, else a point in a random direction at the riders'
height, which may be off the course.

## Judging and plausibility

- Positions are the riders' own newest poses on the room's map, read every
  100 ms at server time; distances to goals are horizontal.
- A move faster than 250 ft/s from the last believed position is not
  believed: the last believed one stays the reference until 250 ft/s could
  have covered the jump. A teleport to the goal therefore wins no sooner
  than driving at 250 ft/s would.
- Stunt-Off scores are self-reported: only increases count, reports above
  10,000,000 are refused. A modified client could still inflate them, as in
  room Freestyle (research/MULTIPLAYER.md "Room Freestyle" Limits).

## Protocol (version 6, compatible)

Allocated in research/MULTIPLAYER_EXPERIENCE.md "Shared contracts".

- The Free Ride claim answers `"hub": true`. Only then does a client set
  HELLO bit 128 (`signal::HUB`); an older server would refuse the unknown
  bit. The server sends kinds 26 and 27 only to sockets that said it, in
  shared rooms; hub packets from other sockets or private rooms are
  ignored.
- Kind 26 `Hub` (`atv_net::hub::Hub`): `op`, `serial`, `kind`, `phase`,
  `timer_ms`, `point`, `target`, `radius`, `mask`, `value`, `choice`,
  `maps` + `votes` (at most 3), `rows` (at most 20: slot, value, points,
  finished).
  - Server ops: `RIDERS` (mask of hub riders, on every change, and to each
    newcomer with the event or vote in progress), `EVENT` (phases JOIN,
    RUN, RESULTS, CANCELLED; `choice` bit 1 solo), `VOTE` (options, counts
    and time left; `choice` the winner when it ends).
  - Client ops: `SCORE` (Stunt-Off points), `CAST` (vote option).
- Kind 27 `Challenge` (`atv_net::hub::Challenge`): `op`, `serial`, `peer`
  (the other rider), `timer_ms`, `target`, `detail`. Client: `ASK`,
  `ACCEPT`, `DECLINE`. Server: `OFFER`, `WAIT`, `START`, `RESULT` (`detail`
  the winner or 255 a draw), `CLOSED` (`detail`: declined, expired, busy,
  cooldown, left, unavailable).
- Validation on both sides: known ops, kinds and phases, finite
  coordinates within 1,000,000 ft, radius 0-1000, masks within 20 slots,
  values 0-1e8, at most 3 vote options with matching counts (at most 20
  votes), unique row slots, timers up to 30 min, exact lengths.
  `crates/atv-net/tests/hub.rs` round-trips both kinds at their limits and
  checks forged and truncated bytes.

## Settings

| Env | Meaning | Default |
| --- | --- | --- |
| `FURY_HUB_EVENT_SECS` | seconds between pop-up events, `N` or `MIN-MAX`; `off` | `240-360` |
| `FURY_HUB_JOIN_SECS` | the join window | `45` |
| `FURY_HUB_ROTATION_SECS` | seconds between rotations; `off` | `1200-1500` |

The browser test runs the server with short timers (below).

## Code

- `crates/atv-net/src/hub.rs`: the two packets and their checks.
- `crates/atv-server/src/engine/hub.rs`: the hub state machine (challenges,
  events, rotation, spots, plausibility), unit-tested in `engine/hub/tests.rs`;
  hooks in `engine.rs` (HELLO bit, the two packets, `tick`, the map move),
  `config.rs` (settings) and `http.rs` (`"hub": true`).
- `crates/atv-game/src/multiplayer/mod.rs`: `Serves::hub`, the HELLO bit,
  the bounded `Session::hub` inbox and `hub_packet`.
- `crates/atv-app/src/hub.rs`: the client state (nearby rider, prompts,
  local countdowns, Stunt-Off reports, markers, the ring, keys, test hooks
  `?hubTest=challenge,accept,decline,goal,join`); `multiplayer.rs` feeds it.
- `crates/atv-ui/src/hud/hub.rs`: the HUD (button, prompt with ring timer,
  chip, vote, toast; 48 px targets) and the projected ground ring.

## Verification

- Unit tests: `cargo test -p atv-net --test hub`, `cargo test -p
  atv-server` (hub state machine: accept→race→result, teleport, decline,
  expiry, refusals, cooldown, hill and solo sprint events, stunt-off order,
  vote and move, newcomers; room: hub packets only to hub sockets, older
  builds moved by a rotation), `cargo test -p atv-game` (HELLO bit, inbox).
- Browser: `tools/test_engine_freeride_hub.mjs` (multiplayer group) with
  fury-server started as
  `FURY_HUB_EVENT_SECS=20 FURY_HUB_JOIN_SECS=12 FURY_HUB_ROTATION_SECS=off make server-run`.

## Limits

- Stunt-Off scores are self-reported (above).
- Hub points live only for the session (no profile, no league yet; slice D
  owns online stats).
- Waypoints can fall off the course in a room nobody has ridden yet.
- The ground ring is drawn as projected dots in the HUD layer (no 3D
  geometry): it does not hide behind hills.
- The main menu row still says Arlington Free Ride: the rider count route
  does not carry the rooms' current maps yet (follow-up).
- Gamepad: the HUD buttons take touch, mouse and keys; no pad buttons are
  assigned yet.
