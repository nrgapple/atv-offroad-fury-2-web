# Online identity, friends, stats and the weekly league (port addition)

Slice D of the modern multiplayer epic (#631, issue #635;
research/MULTIPLAYER_EXPERIENCE.md is the umbrella spec). Nothing here is
recovered behavior: the PS2 game's online play used its own servers and
accounts (MULTIPLAYER_ORIGINAL.md). It is authored for the port, adds no
packet kinds and keeps the original menus as they are.

Code: `crates/atv-server/src/players.rs` (store, friends, presence, stats,
ingest, persistence) with `players/codes.rs` (secrets, ids, tokens, friend
codes), `players/rating.rs` (OpenSkill), `players/league.rs` (weeks, tiers,
points, levels) and `players/routes.rs` (HTTP); `crates/atv-app/src/online.rs`
(registration, requests, socket token); `crates/atv-ui/src/front/controller/
online_page.rs` (pages); the Pass Key carry in `crates/atv-ui/src/cloud.rs`
and the `rider` field of `atv_core::cloud_save::Save`.

## Identity

- **Secret.** The client makes 16 random bytes (Web Crypto, `getrandom` on
  desktop) the first time the rider uses something online: an Online page,
  Free Ride, or hosting or joining a room. It is kept as 32 hex digits under
  `fury-rider-secret` in the settings (localStorage on the web, desktop's
  settings file). Nothing is sent before that first use.
- **Player id (`pid`).** The server derives it as the first 16 bytes of
  SHA-256(`fury-rider:` + secret), in hex. It never stores the secret. The id
  does not depend on the server's signing key, so losing the key only means
  new tokens, not new riders.
- **Token.** `POST /multiplayer/players/register {secret, name}` answers
  `{token, code, created, expires}`. The token is
  `r1.<pid>.<issued seconds>.<HMAC-SHA256 hex>`, signed with
  `FURY_STATE_DIR/players.key` (32 random bytes, made at first start), and is
  good for 30 days. Clients register again at every start, so they never hold
  an expired one; a 401 from any route makes them register again.
- **Sockets.** The room WebSocket takes `?rider=<token>` next to `?key=`. The
  server verifies it; an unverifiable token is ignored (the rider still plays,
  without presence). Other server code reads the verified id with
  `Players::pid(conn)`. This replaces the unverified `?pid=` that Quick Play
  planned to read before this slice landed.
- **Friend code.** 8 symbols shown as `XXXX-XXXX`, from Crockford base32
  without 0, 1, A and E (Crockford already leaves out I, L, O and U), so codes
  cannot be misread and do not spell words. Typing ignores case, spaces and
  dashes. It is public, separate from the Pass Key (a secret), and **New Code**
  replaces it: the old code stops working at once and friends stay friends.
- **Name.** The multiplayer name, through the shared-room filter
  (`atv_net::offensive_name`), at most 16 characters; a blank or refused name
  shows as "Rider". It is updated at every registration.

### Following the Pass Key

The secret travels in the Pass Key save as `rider`, so the rider's friends and
stats follow the save to another device. It is wrapped: `r1:` + the 16 secret
bytes XOR the first 16 bytes of SHA-256(`fury-rider-wrap:` + Pass Key) + a
4-byte check (SHA-256 of `fury-rider-check:` + key + secret). A share code's
snapshot (`/api/share`) holds the same field, but without the Pass Key it
cannot be unwrapped, so loading someone's share code never hands over their
online rider. Rules:

- `local_save` adds the wrapped secret when the device has both a secret and a
  Pass Key; the next sync check pushes it.
- In a merge the online save's `rider` wins (`merge`: remote, else local), and
  writing a save adopts the secret it carries when it unwraps with this
  device's key. Entering a Pass Key on a second device therefore makes it the
  same online rider. The second device's earlier rider, if it had one, is
  dropped from that device (its server record ages out).
- The app compares the stored secret with the registered one every 10 s and
  registers again when a Pass Key brought another.
- Older builds ignore the field and do not merge it; a save they write back
  loses it until a newer build pushes again.

## Routes (HTTP only)

All are JSON POSTs under the usual Origin and CORS rules; the token travels in
the body, never in a URL. Answers are `no-store`. Errors are `{error}` with
400 (bad input), 401 (`Register again`), 404 (unknown code or rider), 409
(friends full, blocked), 413, 429 (the room-claim allowance per address, for
register, add and other writes) and 503 (rider cap reached). Room claims and
`/multiplayer/version` advertise `"identity": true`.

| Route | Body | Answer |
| --- | --- | --- |
| `/multiplayer/players/register` | `secret`, `name` | `token`, `code`, `created`, `expires` |
| `/multiplayer/players/me` | `token` | `code`, `name`, `hidden`, `level`, `xp`, `levelXp`, `nextXp`, `league {tier, tierIndex, points, next, endsMs}`, `stats {races, wins, podiums, best[]}`, `friends`, `requests` |
| `/multiplayer/players/settings` | `token`, `hidden?`, `name?` | as `me` |
| `/multiplayer/players/code` | `token` | `code` (new) |
| `/multiplayer/friends` | `token` | `friends[]` (`id, name, level, tier, online, room {kind, code}`), `requests[]`, `sent[]`, `recent[]` (`id, name, ago, state`), `blocked[]`, `max` |
| `/multiplayer/friends/add` | `token`, `code` or `id` (a recent rider) | `state`: `sent` or `friends` |
| `/multiplayer/friends/{accept,decline,cancel,remove,block,unblock}` | `token`, `id` | `state` |
| `/multiplayer/league` | `token?` | `week`, `endsMs`, `riders`, `top[]` (20: `rank, name, points, level, tier, me`), `me {rank, top}` |

`id`s are per-viewer references (12 hex digits of an HMAC of viewer and
rider), so no list hands out a stable player id. `POST /multiplayer/freeride`
also takes `{"room": "FREERDnn"}`: a friend's Join tries that shared room
first, then the usual lowest room with space.

## Friends, recent riders, presence and Join

- **Mutual.** A request by code or from Recent riders; a request crossing one
  from the other side makes friends at once. At most 100 friends; at most 50
  pending requests each way (the oldest goes).
- **Recent riders.** The last 20 humans met within 7 days, newest first:
  everyone verified in the same room when a verified socket joins, and every
  pair of registered riders in a verified match record. One-tap Add. Rows show
  `✓` friend, `…` sent, `+` asked.
- **Block.** Ends a friendship and requests both ways, removes the rider from
  Recent, and stops them meeting you in rooms. A blocked rider's request
  answers `sent` but never arrives, and they never learn of the block. At
  most 200 blocks. `Players::blocks(a, b)` is there for quick chat (E).
- **Presence** is friends only. A friend is online while a verified socket is
  open or their client called the API within 90 s (the Friends page refreshes
  every 20 s). **Appear Offline** hides the dot and the room from everyone.
  A friend's row carries their room's kind (`freeride`, `private`,
  `quickplay`) and, for Free Ride and private rooms, the code.
- **Join.** Free Ride: a Free Ride claim asking for the friend's shard. A
  private room: the usual Join Game by code (the room still holds four).
  Quick Play rooms show as Quick Play without Join; joining one waits for
  Quick Play (wave 2, as the umbrella spec plans).

## Stats, hidden rating and league

The server reads `FURY_STATE_DIR/matches.jsonl` (written by Quick Play, A;
contract in MULTIPLAYER_EXPERIENCE.md) every 15 s from its saved offset, whole
lines only (a partial last line waits; a shorter file is read from the start;
lines over 16 KiB are skipped). Only records with `verified: true` count, and
only riders whose `pid` is a registered rider and who are not CPUs.

- **Stats:** races, wins (place 1 with more than one rider), podiums (top
  three), and one best value per mode and map (at most 64): the lowest time
  for timed races (`playlist` `race`, mode other than 4), the highest score
  otherwise. A rider who left or did not finish gets a race but no win,
  podium or best.
- **Hidden rating:** OpenSkill's Plackett-Luce model (Weng and Lin 2011,
  Algorithm 4; arXiv 2401.05451), our own implementation with mu 25,
  sigma 25/3, beta sigma/2, kappa 0.0001, matching openskill.js for a 1v1.
  Ranks are the finishing places; riders who did not finish tie behind every
  finisher, and leavers tie last. CPUs never take part, so a match updates
  ratings only with two or more registered humans. New riders start with high
  uncertainty. `Players::rating(pid)` returns `{mu, sigma}` and `ordinal()`
  (mu − 3 sigma) for CPU difficulty and room splitting. Players never see it.
- **League points** per verified match: 10 for taking part, 5 for each rider
  placed behind (CPUs too, at most 19), 10 for a win against anyone; 0 for a
  leaver. Weeks start Monday 00:00 UTC. Points only add up, so the tier
  (Bronze 0, Silver 100, Gold 250, Platinum 500, Diamond 900, Champion 1400)
  only goes up within a week; a new week starts at Bronze. A late record from
  an earlier week adds XP but never reopens that week.
- **Levels:** lifetime XP is the sum of league points; level L needs
  50·L·(L−1) XP (100 more for each level).
- **Weekly board:** the top 20 by points, and the rider's own place as a
  rank and "top N%" (rounded up).

## In the game

- **Profile Editor → Online**: the card (Friend Code, `Level n` with a
  `▰▱` bar and XP, League `◆ Tier · points`, Next Tier bar, Week Ends, Races,
  Wins, Podiums) and rows Friends (`· ● n` online, `· +n` requests), League,
  Copy Code, New Code, Appear Offline: On/Off.
- **Multiplayer → Friends** (and the card's Friends row): Add Code,
  `Recent · n`, requests `+ Name`, friends `● Name · Join` / `○ Name`, sent
  `… Name`, blocked `⊘ Name`. A rider's page has one obvious first action
  (Join, Accept or Add Friend), then Remove/Decline/Block.
- **Add Code**: a text field plus the controller's symbol row (Left/Right),
  Delete Symbol and Send. **League**: the weekly top 10 and "You #n · Top n%".
- Labels are 1–3 words, details are numbers and icons, and rows use the
  existing front-page rows (48 px or more on touch). Answers show as one
  short notice ("Sent ✓", "Friends ✓", "Code copied ✓").

## What is stored, and why

On the server, per rider, in `FURY_STATE_DIR/players.json`: the player id
(a hash, not the secret), friend code, display name, created and last-seen
times, Appear Offline, friend, request and block lists (player ids),
recent riders (ids and times, 7 days), races, wins, podiums, best values,
rating, XP and this week's points. Presence and the rooms riders are in are
kept in memory only. No email, location, age, IP address or device id is
stored by this feature (admission limits keep per-address counters in memory
as before).

Riders unseen for 30 days with no friends and no races, and every rider
unseen for a year, are dropped when the server starts. At most 50,000 riders
are kept (new ones are refused with 503 past it).

### Privacy notice (text for the site's privacy page)

> **Online play.** When you first play online, your game makes a random
> secret on your device and the game server gives you a friend code. The
> server keeps your rider name, friend code, friends, the riders you met in
> the last week, your online results and your level and league. It does not
> know who you are: there is no email, no account, and no location. Only your
> friends can see when you are online, and **Appear Offline** hides you from
> them too. You can block anyone, and **New Code** stops your old friend code
> working. Your secret is saved with your Pass Key, so keep your Pass Key
> private; a share code does not include it. Riders who stop playing are
> deleted after 30 days, or a year if they have friends or results.

## Persistence and backups

- `players.json` is written atomically (temporary file and rename) every
  60 s when something changed, and at shutdown; `players.key` once. An
  unreadable file is renamed `players.unreadable` and the server starts
  empty rather than overwriting it.
- Memory: lists are capped (100 friends, 50 requests each way, 200 blocks,
  20 recent, 64 bests), so a rider is well under 1 KiB typically and a few
  KiB at the caps; 50,000 riders stay within tens of MB on the 1 GB host.
- Deploys keep the state directory: releases live under
  `/opt/fury-server/releases` and `deploy/deploy.sh` switches the `current`
  link, while state lives in `/var/lib/fury-server/state` (systemd
  `StateDirectory`, `FURY_STATE_DIR` in `deploy/env.example`), which no
  deploy step removes.
- **Backup gap.** GAME_SERVER.md said the host keeps nothing that needs a
  backup; that is no longer true. Losing the droplet loses friends, stats
  and the league (riders keep their secret and get a new record and code on
  the next registration; losing only `players.key` just reissues tokens).
  Proposal, not provisioned: a nightly systemd timer on the droplet copies
  `players.json` (gzip, about a few MB) to a private R2 bucket with a
  write-only token, keeping 14 days, plus a weekly DigitalOcean snapshot.
  A restore is "stop, copy file, start". The owner decides; nothing was set
  up.

## Verification

- Unit tests (`cargo test -p atv-server players`): codes and tokens
  (alphabet, parsing, forgery, expiry, per-viewer references), friends
  (mutual, crossing requests, decline, cancel, remove, caps), blocks
  (silent requests, no meeting), presence (friends only, Appear Offline,
  menus window, room kinds), recent riders (rooms, 7 days, 20 newest), new
  codes, ingest (stats, bests, unverified, CPUs, unknown ids, leavers),
  OpenSkill (openskill.js 1v1 values, ties, upsets), league (Monday UTC
  boundaries, tiers, points, levels, late records), state save and load
  (caps, idle drop, bad file), and tailing (whole lines, rotation).
- Integration (`crates/atv-server/tests/socket.rs`): register two riders,
  add and accept, a verified socket's presence and Free Ride shard, Join
  into that shard, recent riders, Appear Offline, the league route, and a
  restart that keeps riders, friends and the key.
- Client: `cargo test -p atv-core cloud_save` (the `rider` field and merge),
  `cargo test -p atv-ui cloud online` (wrapping, Pass Key carry, share codes
  carry nothing; bars, spans, codes), `cargo test -p atv-app online`
  (socket URL).
- Browser: `tools/test_engine_online_friends.mjs` (multiplayer group, local
  server): register, Online card, add by code, accept, presence with Join,
  Join into the same Free Ride room, phone layout.

## Limits and follow-ups

- Join into Quick Play waits for Quick Play's rooms (wave 2).
- Stats, rating and league count only Quick Play's verified records; private
  rooms and Free Ride add nothing. Until Quick Play writes `matches.jsonl`,
  stats stay at zero in production.
- Presence is per server process (one droplet); a second region would need
  shared presence.
- The weekly board sorts every rider with points this week on each request
  (fine at today's numbers; cache it if the board gets hot).
- The level bar and league badge are text glyphs (`▰▱`, `◆`) in the existing
  details table, not new art; a podium/XP count-up on results belongs to the
  Quick Play results screen (wave 2).
- Physical phones were not checked; the phone layout was checked in a
  390×844 browser context only.
