# Modern multiplayer experience (port addition)

Epic: #631.

Owner request, 2026-11-14: with our own game server live (GAME_SERVER.md),
make online play more modern. That means matchmaking, easier ways into a
game, and both racing and freestyling. Nothing here is recovered behavior.
The PS2 game's online play used its own servers and lobbies
(MULTIPLAYER_ORIGINAL.md). Everything below is authored for the port, and
the original menus stay as they are. Each feature adds rows or pages beside
them.

This note is the umbrella spec. Each slice keeps its own note (listed
below), so parallel work never edits the same document.

## Starting point (observed 2026-11-14)

- **Two ways in.** Private rooms take an 8-character code or an invite link
  and hold four riders. Arlington Free Ride is one tap, on a fixed map, with
  20 riders a room and rooms `FREERD01`–`FREERD25`.
- **Competition needs a host.** Races, King of the Hills, Treasure Hunt and
  Freestyle (with optional CPUs) run only in private rooms, and the host
  starts each one from Pause.
- **Players are already coming.** The server's daily usage for 2026-11-14
  showed a peak of 150 riders, almost all in Free Ride.
- **Missing:** matchmaking, a public room list, friends, chat and identity
  beyond a typed name.
- **Results are trusted from the host.** Race results are host-authoritative.
  World boards check a submitted trace but cannot prove it.

## Product research summary

The full report, with sources, is at
`private/reports/multiplayer-product-research-2026-11-14.md`. The
conclusions we build on are below. [S] means a source supports it; [J] is
judgment for about 150 concurrent players on one server.

- **Few queues, never waiting.**
  - Start with as few playlists as possible, because thin queues get worse
    when split [S].
  - Fill empty seats with CPUs after a short wait instead of making players
    wait for humans. Battlefield 6 and Fall Guys both do this [S].
  - A human takes over a CPU's seat at the next round, not in the middle of
    a race [J].
  - Label CPUs honestly ("CPU"). Never show "0 searching" [J].
- **A short loop between races.** Results with a podium, then a vote
  between 3 maps plus Random, then 3-2-1-GO. The whole gap stays at 30 s or
  less [J].
  - The vote is weighted roulette, as in Mario Kart [S]. Random must really
    be random [S].
  - "Run it back" is the default. The room stays together [S/J].
- **Hidden rating.** Use OpenSkill (Plackett-Luce) for finishing orders of
  more than two players [S]. Players see weekly points and a league that
  only goes up within the week, never a number that can drop. Players were
  angry when Mario Kart 8's visible rating dropped them for a 2nd place [S].
- **Hub events.**
  - Pop-up events with a visible join circle and countdown, every few
    minutes. Forza Horizon 5's Arcade works this way [S].
  - Challenging a nearby rider must be opt-in. Forza Horizon 4's Eliminator
    cannot be declined, which is wrong for kids [S/J].
- **Kids-safe communication.**
  - Preset quick chat, emotes and horn only. No free text and no voice. FTC
    staff accept a "dictionary" chat, and Epic restricts chat for under-13
    accounts [S].
  - Mute, block and one-tap report.
  - Collect no email and no location. Presence defaults to friends only
    (UK Children's Code, high-privacy defaults) [S].
- **Identity without accounts.** A public friend code separate from the
  Pass Key, which is a secret. Mutual friends, a recent-players list and a
  Join button on friends' presence [J].
- **Async competition.**
  - Trackmania-style ghosts: your personal best, medal ghosts and a "rival"
    run just above your rank [S/J].
  - A weekly challenge with percentile badges.
  - Rewards are cosmetic and earned. No loot boxes, no fake timers and no
    streaks that punish a missed day [S].
- **Light anti-cheat for client results.**
  - Plausibility checks: maximum speed, teleports, checkpoint order and a
    lap-time floor.
  - Compare the reported race time with the server's wall clock, as
    Trackmania's clock-slowing cheats require [S/J].

## Owner-approved defaults (2026-11-14: "do it all and the recs you suggested")

- **The server runs public rooms.** In a public Quick Play room no player
  is host: the server owns the loop and decides results from checked
  progress. Private rooms keep host authority.
- **Two playlists.** **Race** rotates races and King of the Hills. **Stunts**
  rotates Freestyle sessions and Treasure Hunt. CPU fill makes a queue with
  one player still start.
- **CPU fill.** A Race room fills to 6 riders with CPUs. A new human takes a
  CPU's seat at the next round. Stunt modes do not use CPUs; a lone rider
  plays a score attack against the room's best.
- **Names.** The existing filter (`atv_net::names`) stays. The name field
  gains a dice button that generates a name from a curated word list.
- **Ratings are hidden.** They pick CPU difficulty and split players
  between rooms only when two rooms form in the same wait window.

## Slices

| Slice | Issue | Note | Owns |
| --- | --- | --- | --- |
| A Quick Play | #632 | research/QUICK_PLAY.md | queue, server-run public rooms, vote/rematch loop, CPU fill, results/podium, server-checked results, ping bars |
| B Free Ride hub | #633 | research/FREERIDE_HUB.md | challenge a nearby rider, pop-up events, Free Ride map rotation |
| C Private lobby | #634 (wave 2) | research/QUICK_PLAY.md section | ready-up, vote, rematch, spectate while waiting, 8 riders, all reusing A's packets |
| D Online identity | #635 | research/ONLINE_IDENTITY.md | friend codes, friends, recent riders, presence and Join, online stats, hidden rating, weekly league, levels |
| E Quick chat & safety | #636 | research/QUICK_CHAT.md | quick-chat wheel, emotes, horn, mute/block/report, name dice |
| F Ghost challenges | #637 | research/GHOST_CHALLENGES.md | medal ghosts, rival ghost, weekly challenge with percentile badges |

## Shared contracts

### Protocol

`VERSION` stays 6. Every addition is compatible:

- New packet kinds go only to a socket whose HELLO flags include the
  slice's bit.
- A room claim's JSON advertises each feature (`"quickplay"`, `"chat"`,
  `"hub"`), and a client shows a feature only when the server answered with
  it.
- Older clients keep working unchanged.

Packet kinds (`atv-net` `codec.rs`) and HELLO flag bits are allocated here.
A slice defines only its own.

| Slice | Packet kinds | HELLO bit |
| --- | --- | --- |
| A (C reuses them) | 22 `RoomLoop` (server→client: phase, timer, vote options, seats, results); 23 `Vote` (client→server: map vote, rematch, ready) | none: only clients that call `POST /multiplayer/quickplay` reach a `QP…` room; private rooms (C) use bit 1 `LOOP` of an optional second HELLO data byte |
| E | 24 `Chat` (quick-chat line, emote or horn id; server rate-limits and relays) | 64 `CHAT` |
| B | 26 `Hub` (event announce/join/results); 27 `Challenge` (request/accept/decline/start) | 128 `HUB` |
| D | none: HTTP routes only | — |

Kind 25 is spare. Bit 32 of the first HELLO byte is `MOTORCYCLES` (#638),
so that byte is full. New flags go in an optional second HELLO data byte.

### Server layout

New logic goes in new modules, with only small hooks in `engine.rs`,
`rooms.rs` and `http.rs`, so parallel branches merge cleanly:

- A: `engine/quickplay.rs` and `queue.rs`
- B: `engine/hub.rs`
- D: `players.rs`
- E: `engine/chat.rs`

### Match records (A writes, D reads)

The server appends one JSON line per finished public match to
`FURY_STATE_DIR/matches.jsonl`:

```json
{"t": 1763130000000, "room": "QP000001", "playlist": "race", "mode": 3,
 "map": "nat01", "verified": true,
 "riders": [{"pid": "opaque-or-null", "name": "Dusty", "cpu": false,
             "place": 1, "value": 183250, "dnf": false, "left": false}]}
```

`value` is the finish time in ms for races and the score for stunt or
point modes. `pid` comes from the connection's player identity:

- Until D lands, it is the socket query parameter `pid` (at most 32
  characters), kept unverified.
- D replaces that with its verified identity resolution, and stats count
  only `verified` matches with a `pid`.

### Identity on the socket

D defines a signed rider identity issued by the game server. It travels as
a socket query parameter, and its persistent secret is stored in the
settings and in the Pass Key cloud save. Other slices read the resolved
`pid`; only D verifies it.

## UI rules (all slices)

The owner asked for engaging play without walls of text.

- **Few words.** A button or row label has 1–3 words. A hint has at most one
  short line. Show numbers, icons, colors and timers instead of sentences.
  No paragraphs on any screen.
- **One obvious action per screen.**
  - Quick Play is one tap from the main menu.
  - Every wait screen shows a live countdown and seats filling up.
- **Juice that stays quick.**
  - 3-2-1-GO with sound.
  - A top-3 podium and +points that count up.
  - A confetti or flash beat on a win or level-up.
  - Every pause in the loop is at most a few seconds and can be skipped
    where it is only one rider's choice.
- **Mobile first.** Touch targets of at least 48 px. Works at 360 px
  portrait and on keyboard, gamepad and touch through the existing front
  controller. Builds on the existing `atv-ui` front pages and style, with
  no new UI framework.
- **Honest and kid-safe.** Mark CPUs as CPU. No pay, loot or fake-urgency
  timers. Streaks never punish a missed day.

## Delivery

- Each slice ships through its own worktree and PR with auto-merge, under
  its own issue.
- A server change deploys with the server's CI job. The client hides a
  feature until the deployed server advertises it, so the merge order
  between site and server does not matter.
- Wave 2 (C, plus integration: levels on A's results screen, friend ghosts,
  Join into Quick Play) starts after A and D merge.
