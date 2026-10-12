# Rust browser multiplayer implementation

This is an authored four-player Freeride baseline, building on the original
four-player/host-controlled experience documented in [original research](MULTIPLAYER_ORIGINAL.md).
It does not execute PS2 network code or emulate the disc executable.

```mermaid
flowchart LR
    A[Browser A: Rust local simulation] -->|binary snapshot| R[Rust room relay]
    R -->|validated peer state| B[Browser B: Rust interpolation]
    B --> G[WebGL actor / Web Audio sample]
    B -->|its own snapshot| R
    R --> A
```

## Authority and latency

Each player simulates their own quad in Rust at the established local cadence;
controls do not wait for a network round trip. The host owns the lobby's map and
start. A Rust Cloudflare Durable Object authenticates the creator's private token,
assigns participant slots, validates shared binary packets and relays snapshots.
The room code and invite URL contain no host credential. Host departure ends the
room; reconnect/migration, competitive authority and player-to-player collision
resolution are not part of this first component.

Twenty snapshots per second include source mechanical and rider node matrices,
quad/rider world poses and velocities, riding/air/crash states, engine telemetry
and a deduplicated crash serial. QR decomposition retains original suspension
shear. Rust interpolation uses shortest-arc quaternions and an 80ms presentation
buffer, with a 100ms extrapolation cap and a one-second stale marker. State changes,
large teleports, actor changes, slot reuse and lobby epochs reset interpolation.
The design trades a small remote presentation delay for smoother motion; it
cannot promise instant remote response or remove Internet latency. WebSockets
also retain TCP head-of-line blocking. Datagram/WebRTC transport and host/server
physics reconciliation remain later options once actual measurements justify them.

The full 27-node ATV plus male 18/female 27 poses encode to 2,520/2,988 bytes. At the
20Hz target that is 50.4/59.76kB/s per sender before protocol/TLS overhead. Four
participants can receive about 179.28 kB/s with three female peers. Browser transport
drops unsent snapshots when buffered data reaches 32 KiB, allows one outstanding
snapshot/presentation request and bounds incoming work to 64 messages. It never
queues old drive inputs or PCM. Rust handles decoder bounds and server identity;
browser JavaScript is socket, DOM/WebGL and Web Audio API glue.

## Models and sound

The garage exposes all 26 original in-race quads with their own PHY, AO/AR collision,
mechanical linkage and material resources, plus the male/female mesh and matching
78-motion catalogs. This is a browser selection adaptation; original unlock/profile
rules are not reconstructed. Exact 25 labels and unlisted ATV06 are documented in
[model evidence](MULTIPLAYER_MODELS.md). The existing port's default ATV01 is retained.

Remote actors use separate GPU geometry/texture ownership and sender node poses,
including the detached rider's independent world transform during a wreck. Audio
uses the selected original per-quad engine banks and recovered spatial arithmetic
for distance, stereo pan and Doppler. Rust selects samples; Web Audio transports
local PCM. Crash serials drive original body-impact samples at the rider position.
Pause/focus cancels local drive and publishes a stationary pose; stale/left peers
lose voices. Per-rider vocal fidelity, original multiplayer mixer budget, clothing
presets and original PS2 rendering/animation parity remain unverified.

## Verification and deployment

Pure protocol tests use synthetic malformed/bounded packets and clock/sequence
rollover, quaternion/shear reconstruction, discontinuities and stale handling.
Native private-resource probes cover all 26 quads and both rigs’ 3,276 sampled poses.
Rust adapter tests cover roster/model authority, pose lifetime and silent rendering.
Browser bridge tests cover asynchronous session isolation, backpressure, cleanup,
pause and audio lifecycle. Opt-in `tools/test_multiplayer_relay.mjs` exercises the
Rust service with the Rust codec; `tools/test_multiplayer.mjs` exercises actual
WASM/content, two-player link admission, different rigs, movement and departure.
`ATV_MP_WRECK=1` adds a real-touch failed-stunt/detached-rider/recovery check.

GitHub CI packages the authored frontend and compiled Rust Worker as one checked
artifact, then deploys the relay before the static-app service binding. Original
resources and screenshots remain in a separately validated R2-only manifest; the
expanded package is lazy for other quad archives. `/multiplayer/version` identifies
the checked relay commit/WASM hash through configuration, while public browser
checks verify actual socket/game operation. Host checks are not a public release.
