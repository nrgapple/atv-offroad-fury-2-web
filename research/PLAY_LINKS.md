# Shareable ride URLs

## Contract and strategy

The authored browser startup parses a bounded setup before loading a course or
opening a room. It uses the existing Rust gameplay/relay paths, with the same
loading presentation and menu recovery. The URL selects supported game setup;
preferences such as controls, sound, graphics, GPS and funny crashes remain local.
No original executable, resource URL, arbitrary spawn, debug command or host
credential can be supplied through this contract.

Links work at the public HTTPS root `https://furyremastered.com/?…` and at
`https://furyremastered.com/play.html?…`. The root forwards the full query/hash.
A recognized setup parameter starts directly by default; add `play=0` to review
or edit the setup without starting or connecting. A visit without setup parameters
opens the title and retains the browser's usual saved choices. Setup links use
explicit deterministic defaults (Caribou, quad 1, male rider, name Rider), overriding
saved selections. Successfully started Single Player selections are saved normally.

## Parameters

| Parameter | Values / default |
| --- | --- |
| `mode` | `single` (default), `multiplayer`; aliases `sp`, `single-player`, `singleplayer`, `mp`, `multi` |
| `activity` | `freestyle` (default), `race`; `freeride` also means Freestyle |
| `map` | `env01`–`env08`, `free05`; defaults to `env01`. Menu course names also work, ignoring case and using spaces or hyphens (e.g. `tioga-valley`) |
| `quad` | 1–26 original ATVs, 27–46 motorcycles; defaults to 1. IDs match the Garage's Vehicle number |
| `rider` | `0` / `male` (default), `1` / `female`; motorcycles use the existing MX rider resource |
| `player` | Rider display name, default `Rider`; 1–32 UTF-8 bytes, no control characters |
| `room` | Eight letters/digits, case insensitive. Implies multiplayer joining unless an explicit action is supplied |
| `action` | Multiplayer `join` (default with room), or `create` / `host`. Create may omit room for a random code, or use room for a custom code |
| `play` | `1` / `true` (default): start/join; `0` / `false`: show setup without network admission/loading |
| `laps` | Race only, 1–5; default 3 |
| `cpu` | Race only, 0, 3 or 4; default 3 |

Compatibility aliases: `environment` for map, `vehicle` / `atv` for quad,
`name` for player, `lobby` for room, `drive` for play. Existing
`?drive=1&environment=env02` and `?room=ABCDEFGH` links continue to work; room-only
links now automatically join. Parameter keys are case sensitive, enum/map values
are case insensitive. Unknown parameters are ignored so cache versions/marketing
queries do not trigger a ride. Duplicate recognized fields (including alias pairs)
are rejected. Invalid values or unsupported combinations show an explanation
in the Main Menu before gameplay or networking begins.

## Examples

- [Tioga, Honda 250EX, female rider](https://furyremastered.com/?mode=single&map=env02&quad=14&rider=female&player=Sam)
- [Vancouver on quad 26](https://furyremastered.com/?map=free05&quad=26)
- [Caribou National, two laps, no CPUs](https://furyremastered.com/?activity=race&map=env01&quad=2&laps=2&cpu=0)
- [Supercross race on sx07 with four CPUs](https://furyremastered.com/?activity=race&map=sx07&cpu=4)
- [Create and start a Vancouver multiplayer ride](https://furyremastered.com/?mode=multiplayer&action=create&map=free05&quad=14&player=Host)
- [Prepare a Tioga ride without starting](https://furyremastered.com/?map=env02&quad=14&rider=1&play=0)

Join format (replace the example with a live room code):
`https://furyremastered.com/?room=ABCDEFGH&quad=2&rider=1&player=Guest`.
Names and other text should be encoded with `URLSearchParams` or normal URL encoding.

**Copy ride link** in Pause captures the actual running Single Player setup, or
an invite for the connected room. The Race setup page also copies its selected
laps/CPU/vehicle/rider setup. **Copy invite link** in the lobby now makes a direct
join URL carrying the room and vehicle/rider setup. Invites omit the inviter's
name so guests can supply their own `player` parameter. Only supported setup
fields are serialized; unrelated current-page query/hash and owner tokens are
never copied. If clipboard access fails, the full URL appears in the notice.

## Lobby authority and limits

A create link starts a new lobby, waits for host admission, then atomically
sets its map and starts the ride. `play=0` prepares the create form; clicking
Create lobby applies the requested map while leaving the lobby unstarted.
Custom codes that already exist fail with a clear message. Opening a random-code
create URL again creates another room; use the generated invite to return/join.

A join link joins an unstarted lobby and waits for its host, or loads a running
ride immediately. The room's map is authoritative, including when a joining
URL requests another supported map. Guests never set the map or start a room.
Missing/full/closed rooms and connection failures recover through the existing
room/menu notices. Owner recovery capabilities remain in same-tab session storage,
never in links. No automatic retry loop repeatedly creates rooms.

Current multiplayer is up to four ATV Freeride riders. Motorcycles and racing
remain Single Player; Race accepts the 32 Environment, National and Supercross
courses (`env01`–`env08`, `nat01`–`nat08`, `sx01`–`sx16`). Links do not add
unimplemented modes, race rules, progression, persistent rooms or host migration.
Audio and fullscreen remain subject to browser user-gesture rules: gameplay can
load directly, while sound unlocks on the first interaction. A fresh navigation
loads the starting setup, not a saved mid-ride position.

## Verification

`web/launch.test.mjs` covers deterministic setup roundtrips, supported IDs, names,
aliases, malformed/duplicate/conflicting queries and invite secret exclusion.
Frontend tests cover applying setup before loading, prepare-only routes, failure
recovery and saving only successful courses. Multiplayer tests check admission
ordering, atomic host map/start and cancellation/guest authority. They require
no disc resources and run in `make check` and GitHub CI.

The opt-in `tools/test_launch_links.mjs` uses actual hosted runtime resources and
the Rust relay. It checks root forwarding, saved-choice overrides, copied-link
reloading, ATV/motorcycle/race starts, 390px setup, invalid-link network exclusion,
missing rooms, automatic creation/late joining, host map authority, prepare/create
and ordinary title navigation. Browser evidence and release identity are recorded
in HANDOFF.md and the dev log. Host Chromium captures do not establish physical
phone or geographically separated multiplayer behavior.

Public verification on 2026-10-13 passes at main `e4bfd91` (PR #44), including
all opt-in launch routes above and injected resource-failure recovery. Main CI
run38479468364 and the public hash verifier pass (44 static/458 served objects).
See the deployment record for screenshot/gallery publication and evidence limits.
