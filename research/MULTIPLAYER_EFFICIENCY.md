# Multiplayer efficiency implementation and verification evidence

This records the five authorized multiplayer efficiency changes, host
implementation evidence, and bounded public multiplayer verification. This is
not a whole-game or original-PS2 parity result. The browser continues to run the
authored Rust simulation and relay; the changes reduce redundant pose work and
transport while retaining the player-owned motion model.

## Implemented behavior

Protocol v3 replaces per-node pose matrices with compact full snapshots and
changed-block deltas. A compact full snapshot is 135 bytes; a moving-platform
transform adds 20 bytes. A no-change baseline patch is 26 bytes. Position stays
exact binary32. Orientation, velocity, and animation weights are quantized.
When a patch would save no bytes, including when all four blocks change, the
sender uses an independent full snapshot of the same 135-byte size. This avoids
unnecessary baseline reconstruction at the relay; partial patches remain in use.
Delta baselines advance only after an actual send receipt, so a dropped send
cannot make the sender and receiver apply different baselines. The relay caches
canonical compact state in room attachments for late joiners and does not write
per-pose SQL records. It encodes each broadcast once and forwards the encoded
payload. Rust rig caches hold at most eight templates, discard inactive models,
and use monotonically increasing rig revisions to prevent stale async cache
reinitialization. GPU caches also evict inactive models.

The sender suppresses unchanged state, sends no movement pose packets in a solo
room, and adjusts network cadence from motion and visibility. Moving, fast,
airborne, and wrecked riders use up to 20 Hz; ordinary motion uses 10 Hz, riders
more than 300 feet away use 5 Hz, and stationary riders use 1 Hz. Actual browser
suspension uses 0.2 Hz so presence remains fresh; host train synchronization is
2 Hz. The near-distance threshold is under 40 feet and the fast threshold is over
35 feet/second. State changes and resume force prompt state delivery. Presence
freshness uses one second for moving peers, 2.5 seconds for stationary compact
state, and 15 seconds for suspended compact state. The online Pause menu continues neutral-input
simulation and remains live; opening that menu is not treated as browser
suspension and does not select the 0.2 Hz cadence. The established train freeze
behavior remains in actual suspension.

Rust now reconstructs the ATV linkage and rider motion recipe on each receiving
client. Remote Air/Bounce crash clips advance on their own local clocks. The
detached rider root, velocity, crash state, and recovery are replicated, allowing
local motion through the wreck without sending source joint matrices. This is a
presentation approximation, not a claim that source animation blending matches
the original PS2 game.

Counters report bytes, packets, full and delta packets, suppressed sends, and
WASM operation time separately from JavaScript round-trip time. This keeps codec
cost and network timing distinct in diagnostics.

## Measured host checkpoint

The latest archived report is
`private/reports/multiplayer-efficiency-host-v3.json`. It records a host-local
run from dirty development checkout `0787b32`; it is not evidence from current
main or public production.

The final probe ran 10.0114 seconds of actual driving and sent 133 packets
totaling 17,901 bytes at 13.2849 packets/second. Observed packet sizes were 135
and 117 bytes. Payload bytes were 94.6717% below the same-count, 2,526-byte
male-rider v2 baseline (335,958 bytes). Each of two suspended clients sent two
135-byte presence packets in 11.0119 and 11.0137 seconds (about 0.18 Hz), and
both remained fresh. A late join reconstructed the host at exactly zero feet
from its root with 13 remote draws. A 5.5103-second solo interval sent zero pose
packets.

During the driving interval, the encoder/scheduler reported 204 calls, 5.3 ms
cumulative and 0.3 ms maximum; the decoder reported 23 calls, 2.3 ms cumulative
and 0.6 ms maximum. Remote presentation reported 757 calls, 220.6 ms cumulative
and 1.3 ms maximum. These are measured WASM operation counters from one same-host
run, not a general timing or FPS claim. An earlier probe reported 137 packets /
18,459 bytes; the archived v3 report above supersedes that measurement.

The native codec check covered 1,000 cases: root positions remained exact;
quaternion component error stayed at or below 5e-5; angular velocity error stayed
at or below 1/512 radian per second. The 356 core, 33 networking, 15 web, 6
relay, 4 release-crate, and 95 JavaScript tests passed, including 23 multiplayer
tests; the local recipe-reconstruction check passed. The 35 release-tool tests
passed with one intentional skip, and 12 Worker tests passed. `make verify`, `make check`
(formatting, workspace checks, strict Clippy, JavaScript, and cache checks), and
WASM and relay builds passed with the final cache and quantization-escape
changes. Out-of-range quantized values now use an exact binary32 escape rather
than clipping or rejecting valid extreme values.

Two-browser copied-invite checks passed for male/female movement and audio. A
real touch failed-stunt run recorded 119 frames, including 51 detached-rider
frames and 14 impact-audio frames, followed by recovery. The four-browser run
used models 1, 14, 26, and 2; each client recorded 39 GPU draws for its three
remote actors and played three engine voices. Every rider's movement reached
every observer, and host end cleared peer state without page errors.

The final host feature/train/reconnect gate also passed: late join retained the
custom room code; host recovery after close 1012 and owner reload succeeded; the
nearby spawn action and 342×64 mobile target were present; local owner contact
counts were 9 and 40; both head-on crashes generated notifications; and cleanup
completed. Across 20 train samples, maximum relative error was 0.737246 feet.
The final-build measured probe above and the final four-client rerun passed.
Each client recorded 39 GPU draws for three remote actors and three engine
voices; movement reached all observers and host end cleared all clients with no
HTTP or page errors. A 12-second same-host timing sample recorded 71.82–73.76
FPS, 27.0–27.2 ms frame p95, 2.03–2.30 ms render CPU, 2.0–3.2 ms physics round
trip, and 1–5 ms relay round trip. These timings are not geographically
representative. The frontend regression passed the original title/menu,
controller, Tioga, portrait Caribou load, pause, and Controls routes with no file
picker or page errors. All host gates now pass. No current-main CI or public
verification is included in this host-only section; the separately verified
public subset is documented below.

## Public protocol-v3 multiplayer efficiency verification (2026-10-13)

Main `8c0ace5b8c46a91bf2a50d49bd192a639d23b38b` passed GitHub CI and deploy
verification in [run 38464469566](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38464469566),
which checked both [furyremastered.com](https://furyremastered.com/) and
[`atv-offroad-fury-2.champtoday.workers.dev`](https://atv-offroad-fury-2.champtoday.workers.dev/).
A public browser probe on furyremastered.com passed and is recorded in
`private/reports/multiplayer-efficiency-public-v3.json`.

The probe sent 142 packets totaling 19,134 bytes over 10.0125 seconds
(14.18227 packets/second); observed sizes were 135 and 117 bytes. Against the
same-count 2,526-byte male v2 baseline (358,692 bytes), this is 94.6656% fewer
payload bytes. Each synthetic suspended client sent two 135-byte presence
updates over 11.0212 and 11.0226 seconds and remained fresh. Late join restored
the host at zero feet of root-position error with 13 remote draws; active resume
and a 5.5091-second solo interval with zero pose packets also passed.

After a Wrangler refresh, the separate R2 uploader validated 453 approved
objects totaling 342,573,301 bytes and uploaded two changed objects (screenshot
and gallery). The full Workers verifier then passed for this commit, matching 42
static files and 452 served R2 objects; the public gallery has 173 entries and
137 screenshots.

The actual public four-browser run also passed using models 1, 14, 26 and 2.
Each client rendered three remote actors in 39 GPU draws and played three engine
voices; every driver was seen moving by all three observers. Host-end cleanup
cleared remote models and voices on all clients without HTTP or page errors. A
same-Mac timing sample measured 73.41–73.94 FPS, 27.1–27.2 ms frame p95,
2.35–2.56 ms render CPU, 2.0–3.7 ms physics round trip, and 41–63 ms relay round
trip. These timings do not represent geographic latency or physical-phone
performance. The run is recorded in
`private/reports/multiplayer-efficiency-public-four-v3.json`.

The public feature/contact/train/reconnect run also passed. Custom late admission
and minimap labels worked; recovery after close 1012 preserved train-clock
progress from 13.083305 to 14.033291. Nearby spawn and its 342×64 mobile target
passed. Owner-local contacts reached [2, 10]. A head-on impact put both owners
into Front wrecks and both observers received Wreck with crash serials 1 and 2.
Owner reload retained the same room code and peer; departure cleanup passed.
Across 20 train samples, maximum deck-relative error was 0.8996261966 feet.
Evidence is in `work/verification/efficiency-public-features.log`.

A separate public copied-invite test with male/female models 1/2 passed movement
and host-departure cleanup, with 13 draws per client and no page errors. A real
touch failed airborne Nac Nac produced 117 trace frames, including 51 detached
rider frames and 14 impact-audio frames; crash serial advanced 0→1, spatial pan
changed, and automatic recovery returned the remote rider to Riding. See
`work/verification/efficiency-public-wreck.log`.

These are bounded public browser routes on a synthetic host setup. The four-client
timing values are from one Mac, not a geographically distributed latency result.
Physical-phone behavior, original-PS2 animation/physics parity, server-authoritative
contacts, racing rules, and host migration remain unverified.

## Limits and next gates

The measurement is host-only and used a synthetic browser focus/visibility setup.
It does not establish physical-mobile behavior, geographically distributed
latency, large-room load, production bandwidth bills, or general device FPS.
The reduction compares payload bytes per observed message against a same-count
v2 baseline; it is not a complete server-cost or Internet-traffic measurement.
Source animation blends are not whole-PS2 animation equivalence. Protocol v3 is
incompatible with older open tabs, which must reload before joining. Main CI,
public object hashes, transport, four-client, feature, contact, train, and
recovery routes passed as documented above. Full game parity, competitive
authority, and host migration remain outside this checkpoint.
