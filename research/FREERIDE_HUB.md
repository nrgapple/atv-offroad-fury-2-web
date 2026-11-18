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
  lag), when either is in a challenge or has joined an event, while a rotation
  vote is open, within the pair's cooldown, and when the other build has no
  hub ("TOO FAR", "BUSY", "WAIT A BIT").
- Muted or blocked riders: the client declines their offers at once
  without showing them, and shows no Challenge button for them
  (`Hub::blocked`, a slot mask built each tick from quick chat's mutes and
  block list, research/QUICK_CHAT.md, matched by name as quick chat does).
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
    the highest spot ridden 160-500 ft from the circle; 60 s. Only the run
    counts, and the hill never lies at the circle: riders gather there, so
    its own spot was often the highest, and a rider who only waited held
    the hill (#685). With no spot in range it is a waypoint 160-500 ft off
    (below), at least 80 ft out, clear of the 40 ft circle plus the 35 ft
    hill.
  - **Stunt-Off**: stunt points committed in 60 s, reported by each client
    twice a second at most (self-reported, see Limits).
- Nobody joins: cancelled quietly. One joiner: a **solo score challenge**
  (the same event, scored against itself: a sprint finish 50, Hill King 1
  point a second up to 60, Stunt-Off 1 point per 1,000 up to 100, at least
  10).
- Two or more: top 3 get 100 / 60 / 30, everyone else 10. Only a rider who
  scored (finished the sprint, held the hill, landed stunts) takes a podium
  place, and equal results share one; nobody wins on slot order with
  nothing to show. Every joiner gets a toast with the top 3 and their
  points.
- A rider who leaves drops out of the standings. Live standings go out once
  a second while an event runs.
- No event starts within 2 min (plus the join window) of a rotation.

### Map rotation

- Every 2 hours the room moves to another venue (owner decision
  2026-11-16, #710; it was every 20-25 min). A minute
  before, hub riders get **NEXT MAP** with three venues and live counts
  (buttons, or 1-3). Most votes wins; ties and no votes pick at random.
- The whole room moves: the server changes the shared room's map and sends
  the ordinary `Lobby`, which every build, older ones included, follows as
  it follows a host's course change in a private room. Events and
  challenges in progress end; poses wait for each rider's Ready on the new
  venue; waypoint spots start over.
- New arrivals ride the room's current map (the claim's `Welcome` lobby).
- A shared room that opens (the first rider, or a full room's overflow)
  rides the venue the last room moved to, so riders land together. If a
  rotation fell due while no room was open, it opens on another venue at
  random; Free Ride no longer always starts on Arlington Stream
  (`hub::Venue`). The rider count route reports that venue when no room is
  open.

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
- Arlington Stream, the shared rooms' only venue at first, is one of them.

Judgment: Nationals and Supercross are closed race tracks and stadiums,
the stunt venues are arenas (research/FREESTYLE_MAPS.md "Family"), and
Training and Coonce Arena are special; none suits a room of riders roaming, so
they are left out. Arlington's train runs only on Arlington.

Rotation ships **on** (the requested default). Risk assessment: older
builds follow the lobby's map exactly as in private rooms, so nobody is
left on the old venue; the cost is a reload every 2 hours. Turn it off
with `FURY_HUB_ROTATION_SECS=off` if that proves disruptive.

## Riders list

Issue #742 (owner request 2026-11-17). Port addition: who rides the shared
room shows on the ride HUD at once, without Pause → Players.

- Shown in shared Free Ride rooms (`Net::shared`), right-aligned under a
  top-right GPS map (else at the top right), clear of a narrow view's
  centred hub and Quick Play panels; hidden over race results and in split
  screen.
- No panel or plates: the race HUD's lettering (italic 800 with the
  overlays' dark outline, `classic::hud_style`) on a soft dark shade. The
  room's count in big gold with "RIDING" in overlay silver, then the five
  nearest riders in their colours (paint, else slot; dark colours outlined
  light, as name tags) with their distance in gold ("120 FT", "0.4 MI"
  from a quarter mile), then "+N MORE". Riders without a pose sort last;
  distances sort in 50 ft steps so neighbours do not swap every frame.
  Alone: "JUST YOU".
- A tap or click on the count folds the names away ("RIDING ›") and
  back; the choice is saved (`atv-hud-riders`). Names are not targets:
  mute, block and report stay on Pause → Players.
- Code: `atv_ui::hud::riders`, `Net::riders_view`, `Overlay::prepare`.

## First-time tips

Issue #742. A tip is a gold arrow bobbing (1.4 s) toward a control and two
lines of HUD lettering under it, no box, fading in and out over 0.4 s; a
tap on it does what it points at (`atv_ui::hud::tips`). The chat tip is the
first: research/QUICK_CHAT.md "First-time tip".

## Spawn spots

Issue #710 (owner request 2026-11-16). Port addition, not recovered
behavior: the original's Freeride starts at the scene's one start point.
In a shared Free Ride room riders now start all over the venue
(`atv_game::multiplayer::spots`).

### Candidates (evidence and choice)

- **Source: the long Enduro racing line** `<venue>endle.tdf` in
  `E0n_ENDL.RES` (fact: every Environment venue ships one; the AI path of a
  course laid across the whole map, research/RACING.md "Enduro"). It is
  authored ground riders ride, it carries a direction to face, and a Free
  Ride load already reads `E0n_ENDL.RES` for the venue's terrain materials
  (`launch.rs` `placements`), so the spots cost no extra download.
- Not chosen: Treasure Hunt items and King of the Hill hills live in other
  archives (an extra download) and Arlington has neither (research/GAMES.md);
  hilltops and treasures are often crests or tucked against props. The
  National line (`E0n_NAT.RES`) would be a second download.
- The line is sampled every 80 ft (the first half a spacing in, off the
  start grid); a candidate that fails is tried 9 ft to each side. Kept
  spots lie at least 60 ft apart. Nothing is committed: the list is derived
  from the content at load time and kept for the session (one venue; a
  rotation reads the new venue's).

### Validation (`spots::check`)

Every rule must pass; the reason of the first failure is counted.

| Rule | Test |
| --- | --- |
| In bounds | inside the rideable area: the boundary launch margin (`0020aec0`, `tag::boundary_margin`) less 60 ft, with the whole 20 ft cliff ring inside too |
| On the ground | contact terrain under the spot; no prop surface (deck, roof) between the ground and eye height |
| Dry | no still water over the ground at the spot or 8 ft round it |
| Slope | the 6 ft footprint ring within 0.3 rise over run (about 17°) |
| No cliff | the 20 ft ring within 0.6 (about 31°) of the spot's height |
| Clearance | no compact prop (bounds under 25 ft) or tree within 7 ft between 0.75 and 7 ft up; 16 rays at 1.5 and 4 ft out to 7 ft hit no prop mesh |
| Sky | a vertical ray from 1.5 to 60 ft hits nothing (not under a bridge, roof or canopy) |
| View | of 8 rays at 4 ft out to 50 ft, at least 5 hit no prop or tree and no ground rising faster than 0.5 |
| Facing | the course direction when its view is open, else the reverse, else the nearest open one |

Props are the scene's collision meshes (static, plus the train cars where
they stand at load) and registered vegetation proxies, the same scene the
quad or motorcycle collides with.

### Use

- **Arrival.** A shared-room arrival takes a random spot clear of every
  known rider by 40 ft (else the one farthest from them), placed before its
  first pose. Once the others' poses arrive (the meet step's wait), it moves
  once more only if a rider stands within 40 ft of the spot it has not left.
  Arrivals no longer meet a rider in shared rooms; Players → Spawn beside a
  rider still does.
- **Rotation.** When the whole room moves together each slot takes its own
  spot (the list's start from a seed of the room code and venue, plus the
  slot times n/20), so 20 riders reloading at once never share one.
- **Reset ATV** (the player's, and the main-menu autopilot's stall reset):
  the nearest spot at least 15 ft away and clear of the riders presented by
  40 ft. Judgment: the original's Freeride Reset returns to the one start,
  which in a room spread over the map would pile every reset there and throw
  a rider far from where it rode; its race Reset returns to the last
  on-track point. The nearest valid spot is the closest equivalent of the
  latter. Online rooms have no other CPU riders.
- Unchanged: private rooms, races, room games, single player and local
  split screen; Change ATV and Rider still reloads in place.
- `?freerideSpawn=meet` (tests needing riders together: the hub's challenge
  range, the relay's contact tiers) keeps the meet.

### Verification

- Unit tests (`cargo test -p atv-game --lib spots`): each rule on a
  synthetic world (edge, lake and shore, ramp, ledge, post, long wall,
  roof, deck, walls on three sides, a bank turning the rider round), line
  sampling, side offsets and spacing, arrival choice, rotation slots, the
  Reset choice and the coverage metric.
- Content test (`FURY_CONTENT=private/reports/hosted-content cargo test -p
  atv-game --test freeride_spots -- --ignored --nocapture`): every venue
  yields at least 20 spots covering at least 6 cells of a 4 × 4 grid over
  its rideable area (the spread metric), and an ATV dropped on every spot
  settles riding, dry, on four wheels within 10 ft in 1.5 s.

| Venue | Spots / candidates | Cells of 16 | Main rejections |
| --- | --- | --- | --- |
| env01 | 110 / 152 | 7 | steep 23, cliff 7, boxed in 6 |
| env02 | 150 / 173 | 10 | steep 19, on a prop 3 |
| env03 | 63 / 87 | 8 | steep 22 |
| env04 | 144 / 182 | 9 | steep 22, on a prop 4, water 2 |
| env05 | 107 / 118 | 6 | steep 11 |
| env06 | 102 / 124 | 9 | steep 13, boxed in 7 |
| env07 | 85 / 150 | 9 | steep 26, blocked 18, boxed in 8, water 5 |
| env08 | 101 / 115 | 7 | steep 7, water 4 |

- Browser: `tools/test_engine_freeride_spawns.mjs` (multiplayer group):
  two arrivals at different spots at least 40 ft apart, each seen by the
  other there; with `ATV_SPAWN_ROTATIONS=n` and a short
  `FURY_HUB_ROTATION_SECS`, the same after each room move. Host run
  (2026-11-16, PR build of 5edc4b30, local fury-server with
  `FURY_HUB_ROTATION_SECS=90`, `ATV_SPAWN_ROTATIONS=2`): env06 random spots
  1,875 ft apart, then the room moved to env05 (slot spots 389 ft apart)
  and env02 (440 ft); each rider saw the other at its spot. Screenshots in
  ignored `private/reports/devlog-images/freeride-spawns-710/`.

### Limits

- The cells a venue covers are those its long Enduro crosses (6-10 of 16);
  corners of the map that course never visits get no spots.
- The train's cars are tested where they stand at load: a spot can lie on
  Arlington's track the train later crosses.
- A rider's own Reset can land within 40 ft of a rider whose pose is stale
  or not yet presented.

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
  `maps` + `votes` (at most 3), `rows` (at most 64: slot, value, points,
  finished). `mask` is 64 bits since rooms grew past 20 riders (#710):
  bits 0-31 stay in their old place and bits 32-63 follow the rows only
  when one is set, so builds that take 20 slots (never seated beside a
  slot from 20 up, research/GAME_SERVER.md "Shared room capacity") read
  every hub packet they are sent.
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
  coordinates within 1,000,000 ft, radius 0-1000, masks within 64 slots,
  values 0-1e8, at most 3 vote options with matching counts (at most 64
  votes), unique row slots, timers up to 30 min, exact lengths (a zero
  high mask half is refused, so every packet has one encoding).
  `crates/atv-net/tests/hub.rs` round-trips both kinds at their limits and
  checks forged and truncated bytes.

## Settings

| Env | Meaning | Default |
| --- | --- | --- |
| `FURY_HUB_EVENT_SECS` | seconds between pop-up events, `N` or `MIN-MAX`; `off` | `240-360` |
| `FURY_HUB_JOIN_SECS` | the join window | `45` |
| `FURY_HUB_ROTATION_SECS` | seconds between rotations; `off` | `7200` |

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
  `FURY_HUB_EVENT_SECS=75 FURY_HUB_JOIN_SECS=12 FURY_HUB_ROTATION_SECS=300 make server-run`:
  a challenge, a pop-up event, then a rotation with a third rider on a
  build without the hub (`?hub=0`): all three land on the voted venue and
  see each other.

## Limits

- Stunt-Off scores are self-reported (above).
- Hub points live only for the session (no profile, no league yet; slice D
  owns online stats).
- Waypoints can fall off the course in a room nobody has ridden yet.
- The ground ring is drawn as projected dots in the HUD layer (no 3D
  geometry): it does not hide behind hills.
- Gamepad: the HUD buttons take touch, mouse and keys; no pad buttons are
  assigned yet.
