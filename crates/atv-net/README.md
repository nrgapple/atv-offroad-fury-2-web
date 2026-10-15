# Authored Rust multiplayer core

This crate is shared by the WASM game and Rust lobby relay. It implements an
authored browser multiplayer baseline, not the original PS2 network protocol.
It contains no recovered game resources.

## Protocol

Protocol v4 packets begin with `ATVM`, a one-byte version and message kind, and a
little-endian `u16` payload length. Remaining integers are little-endian. The
decoder rejects trailing bytes, unsupported versions/enums, nonfinite floats,
invalid transforms, duplicate roster slots, invalid room codes, and oversized
allocations. Rooms allow four players, at most 64 ATV nodes, 64 rider nodes, and
8,192 bytes per packet. Names are at most 32 UTF-8 bytes and exclude control
characters. Room codes have eight uppercase letters or digits; generated codes
omit ambiguous glyphs. Lobby map IDs are bounded to 1–40, preserving IDs 1–9 with source keys
`env01`–`env08`, then `free05` (Vancouver Ridge). IDs 10–40 append the other five stunt venues,
eight Nationals, sixteen Supercross stadiums, Training and Coonce Arena. `map_environment` and
`next_map` share this Rust catalog.

| Kind | Payload |
| --- | --- |
| 1 Join | room code, participant |
| 2 Welcome | assigned peer, room code, lobby, 16-byte rejoin key (owner only) |
| 3 Lobby | host, epoch, map, started, participants |
| 4 LegacySnapshot | full pose format retained for codec/relay regression fixtures |
| 5 Leave | peer |
| 6 Ping | nonce |
| 7 Pong | nonce, relay wall clock (u64 ms) |
| 8 SetLobby | expected host epoch, map, started |
| 9 CompactSnapshot | independent compact full state |
| 10 SnapshotDelta | changed blocks against an acknowledged baseline |
| 11 Profile | participant: change vehicle/rider without leaving |
| 12 Ready | map the sender has loaded |

Protocol v4 adds drop-in rooms. Every participant's Welcome carries a private
rejoin key; presenting it within 30 seconds of a dropped connection reclaims the
same slot. The relay forwards a rider's poses only after its Ready matches the
room's current map, so a host track change never shows old-map positions. Pong
carries the relay clock, from which every client derives shared world time (the
Arlington train phase) without depending on the host. Lobby `host` is the
current map authority and passes to another rider when the host leaves.

Protocol v3 sends no per-node pose matrices. Compact full snapshots are 135 bytes;
a moving-platform transform adds 20 bytes. Changed-block patches carry only
changed state; a no-change baseline patch is 26 bytes. World positions remain
exact binary32 values. Orientation, velocity, and animation weights are
quantized. On each client, Rust samples its ATV mechanical linkage and rider
animation recipe to reconstruct node poses. Remote Air/Bounce clips advance on
independent local clocks while detached rider root, velocity, crash state, and
recovery are replicated. This is an authored presentation approximation, not
complete PS2 animation or physics equivalence.

When a changed-block patch would save no bytes, such as when all four blocks
change, the sender uses an independent full snapshot of the same 135-byte size.
This avoids relay baseline reconstruction in the tie case; smaller partial
patches remain in use.

Valid values outside a quantized field's representable range use an exact
binary32 escape rather than clipping or rejecting the value.

Delta baselines advance only after a packet send receipt. The relay caches
canonical compact state in room attachments for late joins; it does not write
pose records to SQL. Broadcasts are encoded once and that encoded payload is
forwarded. Bounded Rust rig/GPU caches evict inactive models and track rig
revisions. Older open clients use an incompatible packet schema and must reload
before joining a v4 room.

The sender suppresses unchanged state and sends no movement poses in a solo room.
Cadence adapts to distance and movement, with up to 20 Hz for near/fast/airborne/
wrecked movement, 10 Hz for ordinary movement, 5 Hz beyond 300 feet, 1 Hz while
stationary, and 0.2 Hz during actual browser suspension. The near threshold is
under 40 feet and fast is over 35 feet/second. Presence freshness is one second for moving peers, 2.5 seconds for
stationary compact state, and 15 seconds for suspended compact state. Online
Pause continues neutral-input physics and does not select the suspension
cadence. Host implementation and gate evidence is documented in
[`research/MULTIPLAYER_EFFICIENCY.md`](../../research/MULTIPLAYER_EFFICIENCY.md);
the final-build measured probe and four-client rerun passed. The frontend
regression also passed its title/menu, controller, Tioga, portrait Caribou,
pause, and Controls routes without a file picker or page errors. Current-main CI
and public verification remain pending for this checkpoint.

The relay derives sender identity from the authenticated room slot, checks model
IDs against the roster, and authorizes host commands. Packet syntax validation
does not provide anti-cheat policy. Reusing a peer slot clears its interpolation
buffer.

`PoseBuffer::push` validates snapshots and rejects duplicates or older sequences,
backward sender time, and receiver clock regression. Sequence and sender-time
rollover use wrapping arithmetic. A buffer retains at most 32 snapshots. Sampling
maps sender time to a monotonic receiver clock. `PoseBuffer::present` is the
renderer path: it plays back with an adaptive delay of clamp(interval + 2·jitter
+ 8, 45, 220) ms from running estimates of the sender's packet interval and
arrival jitter, slewed so presentation time never jumps. Continuous segments use
cubic Hermite positions from the reported velocities (jump arcs follow their
parabola) with shortest-arc quaternion slerp. Extrapolation covers up to 250 ms.
When a new packet changes the trajectory, the visual offset decays over ~110 ms
instead of snapping. A `near` factor blends nearby riders toward their current
estimate so what you see lines up with contacts. `sample` keeps the earlier
fixed 80 ms / 100 ms behavior for compatibility. Stale peers are marked after one
second. Crash/state/model transitions and large teleports snap at presentation
time rather than blending rider attachment or unrelated meshes. Audio consumers
mute stale peers and deduplicate crash serials per peer.

Integration API: `encode(&Packet)`, `decode(&[u8])`, `Snapshot::validate`,
`Transform::from_column_major`, `Transform::to_column_major`, `parse_room_code`,
and `PoseBuffer::{push,present,sample,sample_current,clear}`. Serializable Rust types support control UI
and renderer integration; the real-time wire format remains bounded binary.

## Host implementation evidence

The October 13 host-only efficiency run is recorded in
[`research/MULTIPLAYER_EFFICIENCY.md`](../../research/MULTIPLAYER_EFFICIENCY.md).
It measured 133 packets / 17,901 bytes in 10.0114 seconds of driving, 0.2 Hz
fresh presence during suspension, exact late-join root reconstruction, and no
solo pose packets. The codec cases retained exact root positions with bounded
quaternion and angular-velocity quantization error. The host feature/train/
reconnect gate, final measured probe, final four-client rerun, and frontend
regression passed. This evidence is not current-main CI or a public release.

Earlier resource-compatibility checks initialized all 26 quad vehicle/wreck
configurations and sampled the 27-node linkage at three steering settings with
compression. They sampled all 78 selected clips at 21 times for both the 18-node
male and 27-node female rig, then initialized both complete rider animators
through the existing Rust ABI. Maximum matrix reconstruction error was
2.69e-7 for quads and 5.37e-7 for riders. Those checks cover resource
compatibility and presentation transforms; they do not establish complete
gameplay, network latency, browser rendering, public deployment, or original PS2
multiplayer parity. Source assets, the probe harness, and its detailed report
remain in ignored private/work storage.
