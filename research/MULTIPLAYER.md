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
room; reconnect/migration and competitive authority are not implemented. A new
authored equal-mass contact proxy uses swept six-foot cylinders and each owner
applies only its own response. It is not the original PS2 contact behavior and
the relay does not reconcile physics. The updated packet schema is incompatible
with earlier clients; clients must reload before joining a room on this version.

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

The full 27-node ATV plus male 18/female 27 poses encode to 2,526/2,994-byte
snapshot packets. Attaching a moving-platform transform adds 52 bytes. At the
20Hz target, the base packets are 50.52/59.88 kB/s per sender before protocol/TLS
overhead; a receiver with three female peers receives about 179.64 kB/s. Browser
transport drops unsent snapshots when buffered data reaches 32 KiB, allows one
outstanding snapshot/presentation request and bounds incoming work to 64 messages.
It never queues old drive inputs or PCM. Rust handles decoder bounds and server
identity; browser JavaScript is socket, DOM/WebGL and Web Audio API glue.

## Create and join flow

The authored Multiplayer menu first offers separate **Create lobby** and
**Join lobby** choices. Each form collects a rider name and offers the garage.
Create accepts an optional custom code of exactly eight letters or digits; a
blank code requests a random one, while a duplicate custom code is rejected.
Join requires an eight-character code. Invite links open the Join form with the
code prefilled, leaving the rider to submit the join action. This form flow is
under host-browser recheck in the pending integration build; public delivery
remains pending.

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
While a multiplayer ride is visible, opening the menu or losing focus clears local
drive input and continues neutral-input simulation, so peers receive the moving
rider state. Hidden tabs and context loss still suspend local simulation and mark
outgoing snapshots as paused; stale/left peers lose voices. Per-rider vocal
fidelity, original multiplayer mixer budget, clothing
presets and original PS2 rendering/animation parity remain unverified.

## Original player indicator evidence

The exact USA v2.00 executable's compass initializer at `0x002e68e0`
loads `CompassBkg` and `CompassArrow`, selects the current actor by participant
index, and iterates the actor list to create indicators for other actors. Its
source tint values map participant slots 0–3 to red, blue, green and yellow
(`0xff0000`, `0x0000ff`, `0x00ff00`, `0xffff00`). The original
`textures/compassarrow.pst` is an 8×8 white, alpha-masked arrow pointing up in
its stored orientation; `textures/compassbkg.pst` is a 128×128 compass face.
Both are present in the extracted ENV01 Freeride archive. A decoded, inspected
preview is kept in ignored `work/verification/compassarrow.png`.

This supports the original participant color mapping and compass indicator
art. It does not establish an original overhead marker or nametag. Any browser
marker drawn above remote riders uses authored placement and presentation; it
should be described as such rather than as recovered PS2 placement behavior.

## Host-only multiplayer feature gate

On 2026-10-13, a local two-client browser run admitted a guest more than six
seconds after Arlington Freeride had started, using the same room code; each
client rendered 13 remote draws. The minimap and original compass arrows updated,
and the paused Players screen offered the late guest a nearby-host spawn action.
At 390×844 its measured hitbox was 342×64 pixels at `(24,96)`. The inspected
captures are recorded in the dev log. The follow-up integrated run again passed
late admission and spawn; owner-local contact counters reached 6 and 2. A
20-sample moving flatbed check kept the rider within 0.80435 ft of the
train-relative position. Host departure cleaned up the guests.

Peer indicators now follow remote rider world poses and share the minimap's `-X`
orientation and zoom mapping. The projection gate covered quad 1 on ENV02 with
41 moving and 7 tracking samples and quad 26 on ENV01 with 50 moving and 7
tracking samples; all reported no errors. A four-browser host run selected quads
1, 14, 26 and 2. Each client rendered 39 remote GL draws and three remote engine
voices. Each driver's motion reached the other three observers; host departure
cleared remote models and audio on every guest. The relay gate passed 13 live
cases. The integrated workspace passed 403 Rust tests, 91 JavaScript tests (102
including Worker tests), formatting, strict Clippy, Worker checks, setup and
repository hygiene. The release suite passed 34 tests with one skipped.

These are host checks only. They do not establish public release, physical-phone
input, geographically separated latency, authoritative collision or server-side
physics reconciliation. The equal-mass six-foot swept-cylinder contacts and
overhead player labels are authored browser behavior; the recovered original
compass art and slot colors do not establish original overhead placement. The
shared train animation clock currently freezes when its host pauses. The
integrated main tree was checked against `origin/main` 106e937; public delivery
is pending.

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

GitHub CI packages the authored frontend and compiled Rust relay as one checked
artifact, then deploys both through the existing game Worker. Its deployment entry delegates
room routes to the compiled Rust module and exports the Rust Durable Object. Original
resources and screenshots remain in a separately validated R2-only manifest; the
expanded package is lazy for other quad archives. `/multiplayer/version` identifies
the checked relay commit/WASM hash through configuration, while public browser
checks verify actual socket/game operation. Host checks are not a public release.


## Public integration evidence

Main `ceee73d44bc69f940729c5fdf20b892e4394bb59` and CI `38300517282` are
verified through the production HTTPS app. The relay's identity and all authored
static/R2 object hashes match their checked packages. Eleven live public room
gates cover capacity, authentication/identity, packet validation, rate limits and
cleanup. A real copied-invite two-client session replicates movement, original
spatial engine/impact audio, 390px touch-driven crash, detached rider and recovery.
An actual four-browser session gives each client three remote actors/engine voices
and propagates each rider's movement to all observers; host-end clears every guest.
All 26 garage choices also drive through actual public UI with alternating rigs.

Opt-in browser gates are `tools/test_multiplayer.mjs`,
`tools/test_multiplayer_four.mjs` and `tools/test_player_models.mjs`; original game
content is supplied only by the separate approved manifest or private host server,
never CI. The four-client timing sample uses one Mac, Chromium Metal, 1280×896 and
synthetic focus: 71.59–74.14 FPS, 2.47–2.92ms render CPU, 2.1–4.3ms physics worker
round trips and 37–201ms relay RTT (final 42–47ms). Headless/backend/device/workload
and location matter; these numbers do not establish performance on other devices.
See `DEPLOYMENT.md` and the public dev log for broader checks and limitations.


## Ninth-map integration

Vancouver Ridge (`FREE05`) joins the stable Rust catalog as map ID 9, after
`env01`–`env08`. The Rust protocol/relay share that catalog and reject ID 10;
the WASM adapter derives environment keys and next-map choices for the browser.
This extends supported values without changing packet layout. Public main
`cef26f4495f0c5dbf6f8398133ea81bf4b78aed3` / CI `38307658534` passes actual
copied-link two-browser Vancouver driving/audio/cleanup plus 13 live relay cases,
including all-four map selection and invalid-map rejection. Host closure during
asset loading keeps the completed old load on Multiplayer without reviving its
room or resuming a disconnected ride. All authored files, the Rust relay hash and
402 served objects match checked packages through HTTPS.

The merged renderer also passes four actual public clients, each with three
finite remote actors, 39 remote draws and three engine voices. All four drivers'
movement reaches every observer; host-end clears guest rooms and pose/audio/GPU
caches. A real 390px touch failed stunt records 403 guest frames across riding,
airborne and wreck, including 42 detached-rider frames and 14 impact-audio frames
with changing pan, then recovery. The four-client 12-second sample on one Mac,
headless Chromium Metal, 1280×896 and synthetic focus is 71.00–75.86 FPS,
25.9–27.2ms frame p95, 2.61–3.11ms render CPU, 2.1–6.6ms physics round trip and
41–52ms relay RTT. Concurrent validation workload was not measured; this is not
a general device or Internet performance claim.

Historical note: at the public integration checkpoint above, shared dynamic-world
clocks and server/host physics reconciliation remained future work; that release
relayed rider-owned simulations while scene animations and dynamic objects ran on
each client's local clock. Later host-only checks now cover synchronized train
timing and authored player contacts, but the shared train clock still freezes
when its host pauses. These changes are not a public release. Competitive
authority, host migration and original online/race rules remain unfinished.
