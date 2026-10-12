# Authored Rust multiplayer core

This crate is shared by the WASM game and Rust lobby relay. It implements a new
browser multiplayer baseline, not the original PS2 network protocol. It contains
no recovered game resources.

Version 1 packets begin with `ATVM`, one-byte version and message kind, and a
little-endian `u16` payload byte length. All remaining numbers are little-endian.
The decoder rejects trailing bytes, unsupported versions/enums, nonfinite floats,
invalid transforms, duplicate roster slots, invalid room codes and oversized
allocations. Four players, 64 ATV nodes, 64 rider nodes and 8,192 bytes per packet
are hard limits. Names are at most 32 UTF-8 bytes and exclude control characters.
Room codes contain eight nonambiguous uppercase base-32 characters.

| Kind | Payload |
| --- | --- |
| 1 Join | room code, participant |
| 2 Welcome | assigned peer, room code, lobby |
| 3 Lobby | host, epoch, map, started, participants |
| 4 Snapshot | peer, sequence, sender milliseconds, model IDs, ATV transform/velocities, ATV/rider node poses, rider state/transform/velocity, engine RPM/throttle/speed, crash serial |
| 5 Leave | peer |
| 6 Ping | nonce |
| 7 Pong | nonce |
| 8 SetLobby | expected host epoch, map, started |

A transform is thirteen binary32 values: position XYZ, rotation WXYZ, scale XYZ
and three upper-triangular shear coefficients (XY, XZ, YZ).
Node poses are **model-space**, as in the game's current pose renderer, rather
than hierarchical local transforms. Matrix helpers use QR decomposition to preserve signed scale and source
mechanical linkage shear; perspective and degenerate bases are rejected. Source-body quaternions use WXYZ; transport retains that
order. Position and linear velocity use the game's feet and feet/second units;
angular velocity uses world radians/second. The common 27 ATV + 18 rider pose is
2,520 bytes. A 27-joint female rider makes it 2,988 bytes. At 20 Hz these are about
50 and 60 KB/s per sender before WebSocket/TLS overhead. This baseline replicates
visual poses; it does not introduce shared collision physics or race authority.

The relay must derive sender identity from the authenticated socket/room slot,
check model IDs against its roster and authorize host commands. Syntactic packet
validation is not an authority or anti-cheat policy. Reusing a peer slot requires
clearing that peer's interpolation buffer.

`PoseBuffer::push` validates a snapshot and rejects duplicates/older sequences,
backward sender time and receiver clock regression. Sender sequence/time rollover
is handled with wrapping arithmetic. Each buffer retains at most 32 snapshots.
`sample` maps sender time onto a monotonic receiver clock, interpolates at an 80ms
default delay with shortest-arc normalized quaternion slerp, and extrapolates
linear/world-angular movement for at most 100ms. Stale peers are marked after
one second. Crash/state/model transitions and large teleports snap at their
presentation time rather than blending rider attachment or unrelated meshes.
Audio consumers should mute stale peers and deduplicate crash serials per peer.

Integration API: `encode(&Packet)`, `decode(&[u8])`, `Snapshot::validate`,
`Transform::from_column_major`, `Transform::to_column_major`, `parse_room_code`,
`PoseBuffer::{push,sample,clear}`. Serializable Rust types are provided for control
UI and renderer integration; the real-time wire format stays bounded binary.

Host verification on October 12, 2026: 22 protocol tests and nine adapter tests
pass, including malformed input, clock/sequence rollover, bounded extrapolation,
shortest-arc rotation, source affine shear, roster restrictions, slot reuse and
silent/stale presentation. Strict Clippy, formatting and wasm32 compilation pass.
An ignored native asset probe initializes all 26 original quad vehicle/wreck
configurations, samples all 27-node linkages at three steering settings with
compression, and validates codec round trips. It also samples all 78 selected
clips at 21 times for each 18-node male and 27-node female rig (1,638 poses each)
and initializes both complete rider animators through the existing Rust ABI.
Maximum matrix reconstruction error is 2.69e-7 for quads and 5.37e-7 for riders.
These checks cover resource compatibility and networking presentation transforms;
they do not establish complete gameplay, network latency, browser rendering,
public deployment or original PS2 multiplayer parity. Source assets, the probe
harness and its detailed report stay in ignored private/work storage.
