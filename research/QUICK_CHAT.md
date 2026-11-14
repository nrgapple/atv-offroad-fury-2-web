# Quick chat, emotes, horn and safety

Slice E of the modern multiplayer epic (#631, research/MULTIPLAYER_EXPERIENCE.md),
issue #636. A port addition: the PS2 game had no chat of this kind, and the
original menus are unchanged.

## What riders can say

Only presets travel. A `Chat` packet (atv-net kind 24) carries a kind and an
id, three bytes in all, so no free text can be sent and nothing can be spelled
out from letters. Unknown ids fail to decode and close the socket like any
invalid packet.

| Kind | Ids | Shown as |
| --- | --- | --- |
| Line | 0-7 in-ride: Nice!, GG, Wow!, Sorry!, Follow me, Wait up, Race?, Thanks. 8-13 post-race: Rematch?, Close one!, Well raced, Next time!, So fast!, Fun! | a bubble over the sender's rider and a feed line |
| Emote | 0-7: thumbs up, laugh, wow, cool, heart, star, fire, trophy | an icon bubble over the rider (drawn from SVG paths, no emoji font) |
| Horn | 0 | a short synthesized horn and a ring at the sender's quad |

After a race in a room, the wheel swaps to the post-race lines (GG, Rematch?,
Close one!, Well raced, Next time!, So fast!, Fun!, Thanks). All lines are
positive or neutral.

## Limits (server-enforced, mirrored by the client)

- Lines: three in any 5 s, then a fourth starts a 5 s lockout.
- Emotes: one every 2 s. Horn: one every 1 s.
- The horn reaches only riders within 300 ft whose newest pose is on the
  room's map (the server's 60-300 ft interest tier).
- Events over a limit are dropped silently. The server echoes accepted events
  to the sender too, so a rider's own bubble and feed show only what others
  saw. The client keeps the same budget (`atv_net::chat::Limits`) to grey out
  the wheel.

## Protocol

- HELLO flag bit 64 `CHAT`. The server relays chat only to sockets that said
  it, so older builds never see kind 24.
- Room claims say `"chat": true`. A client shows the CHAT button only when its
  server says so, so the site and server can deploy in either order.
- Server: `crates/atv-server/src/engine/chat.rs` (relay, horn range, ring),
  `crates/atv-server/src/reports.rs` (the report route). Client:
  `atv_game::multiplayer::Session::chat_packet`/`take_chat`,
  `crates/atv-app/src/quick_chat.rs`, `crates/atv-ui/src/hud/chat.rs`.

## Controls

- Touch and mouse: the toolbar's CHAT button opens a wheel with eight lines
  round the ring, a page switch to the eight emotes and the horn in the
  middle. Targets are at least 48 px and fit at 360 px portrait. Tapping
  outside closes it.
- Keyboard: T opens the wheel; with it open 1-8 send a line (Shift+1-8 an
  emote). B sounds the horn. T and B stay gameplay keys when the player binds
  them. They were free in every default layout; H stays help.
- Controller: Select opens the wheel (R3, then L3, when the preset binds
  Select, as atv2 and unleashed do). D-pad left and right move round the ten
  targets, A picks and B closes.

## Safety

- **Mute** (per rider, this room): hides that rider's lines, emotes and horn.
- **Mute all chat** (setting `atv-chat-mute-all`): hides every rider's lines
  and emotes and keeps horns, which carry no words.
- **Block** (setting `atv-chat-blocked`, at most 100 riders): hides the
  rider's chat in every room. The list is keyed by the verified player id
  when slice D provides one, otherwise by the shown name (case-folded).
  `quick_chat::is_blocked(name, pid)` (and `QuickChat::blocked` on a room's
  copy) is the API for the Free Ride hub, which declines challenges from
  blocked riders.
- **Report**: one tap with a fixed reason (Name, Griefing, Cheating) sends
  `POST /multiplayer/report` with `{room, key, target, reason}`. The rejoin
  key proves the reporter holds a slot in that room. The server adds both
  shown names and the room's recent chat (a ring of at most 256 events from
  the last two minutes), then appends one JSON line to
  `FURY_STATE_DIR/reports.jsonl`. When that file reaches 4 MiB it becomes
  `reports.1.jsonl`, so at most about 8 MiB is kept. Each address may send
  five reports, then one a minute.
- All of these are on the Players page. In a chat room each rider's row has
  a `Name: Mute/Report` row. It opens that rider's page with Spawn beside,
  Mute, Block and the three report reasons. Mute all chat sits above Back.
  The same page serves Free Ride, private rooms and, once slice A lands,
  Quick Play rooms.

## Privacy

- No address, email, location or free text is stored.
- A report line holds the time, the reason, the room code, both slots and
  shown names, and the room's preset events. A preset event holds a time,
  slot, name, kind and id.
- The ring lives only in the room's memory and is gone when the room ends.
- Blocks and the mute-all choice stay on the device, in settings.

## Name dice

The Host Game and Join Game pages have a Dice button beside the rider-name
field. It makes a curated adjective, a noun and two digits, e.g.
`DustyFox42`: 32 × 32 × 100 names of at most 16 bytes. Every name passes
through `atv_net::offensive_name`, and a flagged roll is rolled again. An
exhaustive check found no flagged combination in the current lists.

## Limitations

- A room's rider pages key blocks by shown name until slice D gives verified
  ids on the wire. A renamed rider is no longer blocked.
- The horn is a synthesized two-tone sound (`atv_game::audio::synth::horn`).
  No original sample fits.
- The rider's own bubble is not drawn over their own quad; their own events
  show in the feed as "You".
- Reports are stored for review only. There is no moderation tooling yet.
