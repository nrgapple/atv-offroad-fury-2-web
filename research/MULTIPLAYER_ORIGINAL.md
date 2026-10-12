# Original multiplayer: evidence and browser baseline

Investigated 2026-10-12 against USA v2.00 `SCUS_972.11`, SHA-256
`11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`.
The canonical identity remains in [HANDOFF.md](../HANDOFF.md); networking behavior
below is partial reconnaissance, not a recovered PS2 protocol.

## Published original behavior

The [USA instruction manual](https://www.videogamemanual.com/PS2/ATV%20Offroad%20Fury%202%20%28USA%29.pdf),
printed pages 16–17 and 36–37, describes four-player split screen, two-system
i.Link host/join, LAN, and two-to-four-player internet sessions. Online requires
a profile and has human riders only; the host's unlocked items determine
availability. Online competition saves neither profile progress nor Hall of Fame
records. Two-player split screen can include AI, except Duel; three/four-player
split screen excludes AI. Page 17 labels the Games option split-screen only;
do not assume every mini-game is online-capable. Pages 21–22 describe race
and freestyle variants, including unrestricted Freeride. Pages 23–24 offer male
and female riders and clothing, helmet, goggles, gloves and boots customization.

[Sony's November 12, 2002 launch announcement](https://sony.mediaroom.com/2002-11-12-ATV-Offroad-Fury-2-Heightens-the-Multi-Player-Gameplay-Experience-as-The-First-Must-Have-Online-Racing-Title-for-PlayStation-2)
independently confirms up to four internet participants over broadband or dial-up
and more than twenty ATVs. It names Tag, Hockey, King of the Hills and Treasure
Hunt, but does not establish which were available online.

## Exact-build observations

ASCII strings in the exact executable provide these research leads. Guest
addresses use the observed load mapping `file_offset - 128 + 0x00100000`.

| String / resource | Observation |
| --- | --- |
| `MediusNetworkConnection`, guest `0x0043dd80` | Medius connection implementation exists; lobby/backend association is plausible, packet protocol unverified. |
| `MGCLNetworkConnection`, guest `0x0043dcf0`; `LanNetworkConnection` | Separate network connection types exist. Their gameplay transport and ownership have not been recovered. |
| `Race Lobby`, `Freestyle Lobby`, `Games Lobby`, guest `0x004353d0`–`0x004353f0` | Three lobby labels exist. Presence alone does not prove every game mode is enabled for internet play. |
| `Under Host Control`, guest `0x00433af0`; host-forced-start text, `0x00434100` | Host controls session options/start. |
| Host-quit/host-left exit text, guest `0x0042b6b0` / `0x0042b6e0` | Original UI explicitly anticipates terminating an event on host loss. No host migration evidence found. |
| `/NETCONF/MEDIUS.INI`, `/MODULES/INET.IRX`, `INETCTL.IRX`, `LIBNETB.IRX` | Disc includes original PS2 online configuration/modules. These are research inputs, never browser runtime resources. |
| Extracted `ui-pages/MultiPlayer/` and lobby panel path string | Original menu art/layout resources exist and can inform browser presentation. |

No verified network send/receive function, message schema, tick cadence,
prediction algorithm, rider replication layout or collision authority is available
from this investigation. Names from the first game's symbol-rich reference are
not proof for this stripped sequel executable. No reference code was copied.

## Authored browser strategy

Start with four-person Freeride rooms, shared eight-character nonambiguous codes
and same-origin links, and host-owned environment selection. Browser Rust/WASM
owns the shared bounded binary protocol and presentation smoothing; a Rust
Cloudflare Durable Object validates identities/messages and relays them over
secure WebSockets. Local driving stays immediate. Remote state contains vehicle
and detached rider transforms, velocities, articulated node poses, crash sequence,
engine RPM and throttle. Host departure ends the room cleanly initially.

This is owner-authoritative driving with host-controlled session settings. It
does not provide server-authoritative physics, competitive cheat resistance,
player-to-player collision resolution, race results or faithful original network
semantics. The baseline can establish practical latency/traffic measurements
before replacing gameplay snapshots with host physics/input prediction or
WebRTC unreliable delivery. Rust use alone cannot eliminate network round-trip
latency or WebSocket head-of-line blocking.

Cloudflare supports [hibernating WebSockets](https://developers.cloudflare.com/durable-objects/best-practices/websockets/)
with socket attachments preserving connection metadata across idle eviction.
The [workers-rs DurableObject API](https://docs.rs/worker/0.8.7/worker/durable/trait.DurableObject.html)
provides Rust fetch/message/close handlers. Version 0.8.7 is selected for both
`worker` and `worker-build` after checking crates.io, 2026-10-12.

## Relay checkpoint verification

The authored `atv-relay` crate builds with `worker-build 0.8.7`, passes three
native tests and strict Clippy. A live local workerd gate exercises eleven behaviors:
origin denial and nonexistent rooms; secure creator reservation; server-stamped
peer identity; four-player capacity; stale host-command resynchronization;
vehicle/rider/wreck/audio snapshot transport; duplicate-sequence suppression;
host-only settings; leave-before-slot-reuse; malformed/model-change/rate-limit
closure; active map/start locking; and host-loss termination without room resurrection. The eleven recorded
check labels group related assertions. Private evidence is
`private/reports/multiplayer-relay-gate.json`.

Socket attachments preserve participant/model identity, sequence and rate-window
metadata. A native serialization roundtrip verifies that state can be hydrated;
actual production hibernation/eviction has not been forced or measured yet.
The relay limits each connection to 120 messages and 1 MiB per second, expires
idle connections after 60 seconds, expires empty reservations, and limits a room
to twelve hours. `hostToken` stays in the creator's memory; invite URLs carry
only the room code. Rider selection uses ID 0 for male and 1 for female.
Once riding starts, the host cannot change the environment or stop/restart the
event in this baseline; denied changes return the current lobby for resynchronization.

The local gate uses Wrangler 4.149.0. The older cached 4.129.0 workerd process
hung on this SDK build; use the checked current Wrangler for repeat verification.
Compatibility date `2026-09-10` is sufficient for the chosen APIs. These results
are **host verification**, not proof of public deployment or remote latency.

Repeat after building the native codec example and starting the local relay:

```sh
cargo build -p atv-relay --example codec
WS_MODULE=/path/to/ws/index.js \
  ATV_RELAY_URL=http://127.0.0.1:4190 \
  ATV_RELAY_ORIGIN=http://127.0.0.1:4186 \
  node tools/test_multiplayer_relay.mjs
```

The test resolves Cargo's actual target directory, so global Cargo configuration
and ignored worktree build symlinks do not need to coincide. Port 4190 is only
an internal test relay; browsers use the combined test origin on 4186. Production
access goes through the existing game's HTTPS Worker service binding.
