## Public direct-play and lobby URL release (2026-10-13)

PR #44 merged as `e4bfd91aab5351f284d850fd4d039ab888a887b0` and passed
[main CI/deploy 38479468364](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38479468364).
Public `furyremastered.com` identifies that commit. The validated current-main
artifact was fetched before publication; the separately approved R2 package
validated 459 objects and uploaded two changes (host screenshot/gallery).
Full public HTTPS verification matched 44 authored static files and all 458
served R2 objects by hash/size/MIME, with 186 entries and 143 screenshots.

The actual public Chromium Metal URL gate passes root query forwarding, Tioga
ATV14/female/name encoding/copy/reload, deterministic default ATV1 Vancouver,
motorcycle34, ATV2/female two-lap/CPU0 Race, 390px prepared/manual entry, invalid
and duplicate links with no admission/gameplay, missing room recovery, automatic
Vancouver creation and copied late join. The host14/male and guest26/female each
draw the peer (14/12 draws); the guest's env01 URL cannot override room map9.
Prepared creation applies Tioga without starting. An injected missing quad resource
returns to a usable lobby with an error and Leave; ordinary visits reach title.
No file-selection step or page errors occur. See [URL contract](research/PLAY_LINKS.md).

The inspected public screenshot records this deployed commit. Its metadata and
PNG follow the next validated metadata checkpoint's CI/R2 publication. These
are automated browser observations on one Mac with synthetic focus, not physical
phone, geographic-latency or full original-game equivalence. Multiplayer remains
ATV Freeride; Race remains Caribou National; sound/fullscreen need browser gestures.

## Shareable URL setup — host checkpoint (2026-10-13)

`feat/direct-play-links` adds deterministic URL setup/direct entry for all nine
Freestyle maps, ATV1–26 and motorcycle27–46, rider/name, supported Race laps/CPU,
room joining and explicit lobby creation. Existing drive/environment aliases and
room-only invites work. `play=0` prepares setup without loading/admission; URL
values override saved selections. Pause and Race setup copy reproducible links;
lobby invites automatically join and omit owner capabilities/the inviter name.
The room's map remains authoritative and host admission precedes atomic map/start.
Malformed/conflicting/unsupported setups return a clear menu notice; failures
recover to usable setup/lobby menus. See [contract/strategy](research/PLAY_LINKS.md).

Host checks pass: `make check` (110 JS plus workspace Rust/fmt/strict Clippy),
`make verify`, web/relay builds, 35 release tests (one expected skip), 19 Worker
tests and repository hygiene. Actual hosted-content/Rust-relay Chromium Metal
checks pass root forwarding, Tioga/ATV14female/copy/reload, default ATV1 Vancouver,
motorcycle34, quad2female 2-lap/CPU0 Race, 390px setup, invalid no-network links,
missing room recovery, direct create Vancouver + guest26female late join with
host map precedence, and prepare/create Tioga without auto-start. Existing
frontend/controller/mobile routes and copied-invite movement/audio/cleanup pass.
Host screenshots stay private pending validated main CI/R2/public verification.
Physical phone/audio-autoplay/geographic behavior and full original game parity
are not established; URL setup only exposes currently implemented gameplay.

## Multiplayer efficiency and public browser verification (2026-10-13)

Main `8c0ace5b8c46a91bf2a50d49bd192a639d23b38b` passed GitHub CI/deploy run
[38464469566](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38464469566)
for `furyremastered.com` and the workers.dev app. The Workers verifier matched
42 static files and 452 served R2 objects. The public gallery contains 173
entries and 137 screenshots.

At [furyremastered.com](https://furyremastered.com/), the public compact v3
probe sent 142 packets / 19,134 bytes over 10.0125 seconds (14.18227 Hz), with
135- and 117-byte packets. Payload bytes were 94.6656% below the same-count
2,526-byte male v2 baseline. Two synthetic suspended clients each sent two
135-byte updates in about 11.02 seconds and remained fresh. Late join restored
the host root with zero positional error and 13 remote draws; resume passed; a
5.5091-second solo interval sent no poses.

Four actual public browsers using quads 1/14/26/2 each rendered three remote
actors in 39 GPU draws and played three engine voices. All riders were observed
moving by the other clients; host-end cleanup removed remote models and voices
without HTTP or page errors. Custom-code late admission, minimap labels,
close-1012 recovery with train time preserved, nearby spawn and its 342×64
mobile target, owner contacts [2, 10], both head-on wreck notifications, owner
reload, and departure cleanup passed. Across 20 train samples, maximum
deck-relative error was 0.8996261966 feet.

The four-client performance sample is from one Mac; its timing is not
geographically representative. Physical phones, complete PS2 parity,
server-authoritative contacts, racing rules and host migration remain
unverified. See [protocol and measurements](research/MULTIPLAYER_EFFICIENCY.md)
and [multiplayer evidence](research/MULTIPLAYER.md).

A separate copied-invite male/female run also passed movement and cleanup with
13 draws per client and no page errors. A real touch failed airborne Nac Nac
produced 117 trace frames (51 detached rider, 14 impact-audio), advanced crash
serial 0→1 with changing spatial pan, and recovered remotely to Riding. See
`work/verification/efficiency-public-wreck.log`.

## Multiplayer efficiency — tested host checkpoint (2026-10-13)

Branch `feat/multiplayer-efficiency` implements the five user-authorized changes:
compact v3 snapshots/changed-block deltas, adaptive/idle/solo suppression, local
recovered rider/linkage reconstruction and independent remote crash presentation,
bounded rig/GPU caches, and byte/message/WASM-operation measurements. Local
physics remains unchanged; online menu physics remains live. Actual suspended
simulations use slow fresh presence and preserve the host train freeze.

Typical independent state is 135 bytes, versus v2's 2,526/2,994 bytes. The final
10.0114s host driving sample sent 133 packets / 17,901 bytes: 94.672% smaller
per observed message against a same-count male v2 baseline. Both suspended
clients sent two updates / 270 bytes in about 11s; solo about 5.5s sent zero poses;
late join reproduced the suspended host root with zero measured distance.
Positions remain exact binary32, with bounded smaller fields and exact escapes
for rare extreme motion. Remote crash joint timing is intentionally independent,
as requested; authoritative detached root and recovery still replicate.

Host checks pass: 356 core, 33 net, 6 relay, 15 web and 4 release-crate tests;
95 JS tests; formatting/strict Clippy; setup/hygiene; 35 Python release tests
(one intended skip); 12 Worker tests; WASM/relay builds; copied invite, four
clients/models, source touch crash/impact/recovery, train support/contacts,
late join/spawn/mobile hit target, host reconnect/reload and frontend regression.
Four clients each render three remote actors in 39 draws with three engine
voices; every rider moves for every observer and host-end clears all caches.
See [protocol, measurements and limits](research/MULTIPLAYER_EFFICIENCY.md).

Current-main CI, automatic deployment and public/R2 gallery verification remain
the release gate. Old open tabs must reload for v3. This is host evidence with
synthetic focus/visibility, not physical-device, geographic, large-load or
production-bill validation. Existing approved runtime content is reused; source
assets and screenshots remain outside GitHub/CI.

## Minimap projection, rider tracking and zoom — host checkpoint (2026-10-13)

The old map used +X screen-right, opposite the actual overhead camera basis
(−X screen-right, +Z screen-up). Terrain sampling, static scenery, crop center
and marker heading now share the camera's handedness. GPS follows the same
presented frame as the 3D vehicle, including motorcycle interpolation; during
native or enhanced separation it follows the rendered rider root instead of
the ATV. The independent browser observer compares the aerial crop/center and
bearing against actual GPU actor transforms. Native wreck tracking passes 113
samples; enhanced wreck tracking passes 111, with zero mismatches. Moving ATV
and interpolated motorcycle checks also pass.

Pause → HUD settings → Minimap configuration now includes a persisted 25–200%
zoom control with a 50% default (1,536ft across, twice the previous range).
Screen size remains separate, with an adaptive scale bar above MPH. The two
sliders fit portrait/landscape layouts with 44px targets. Chromium checks four
viewports; WebKit checks phone portrait and landscape. Shape/corner/zoom/size
reuse the one 1024px raster and add no scene draws or pixel storage. The 390×844
DPR3 host cost check observes 0.102–0.109ms per update, bounded 20Hz, identical 471
scene draws On/Off, and zero retained map storage or updates when Off. This is
a host Mac observation with concurrent browser activity, not physical-phone
performance or an FPS improvement claim.

make check (350 core Rust tests plus workspace; 84 JS), make verify, make web,
31 release tests (one intentionally skipped queue case), 10 Worker tests and
touch orientation/lifecycle checks pass. New screenshot/devlog metadata stays
host-only until passing main CI, domain/PWA PR 32's separate publication and our
own validated R2/public verification. No game data/screenshots enter source or
CI. See [minimap strategy and limitations](research/MINIMAP.md).

## Minimap cached-file consistency correction — 2026-10-13

The user reproduced a padded dark square map in the old mid-right position,
plus duplicate standalone speed, after the combined GPS release. Serving prior
CSS/HUD modules with the new minimap reproduces that presentation. The release
packager now versions all local CSS/JS references, transitive imports, Worker
startup and WASM with the release commit. The real returning-browser regression
retains the old HTTP cache and verifies the fresh release renders a circle in
the top-right with zero padding/border and no duplicate speed. Freestyle's
standalone speed now follows explicit minimap Off, including during map baking
and temporary visibility suspension. This reproduces a cause; it does not
inspect the user's physical browser cache. Public verification follows main CI.

## Full-lock motorcycle hand reach — host follow-up (2026-10-13)

Public inward-bank/standing-menu checks passed, but a later Sano Mini full-lock
turn exposed a0.25177 ft normal grip gap. Normal MX source-pose blending now
accounts for the shared rigid arm solver's reach, selecting a feasible blend
toward the original forward riding/air pose before applying hand/foot constraints.
The source clips and joint lengths are preserved; this is an authored integration
adaptation, not recovered native MX pose-controller logic. All720 flat full-input
source frames, four sustained full-lock GPU touch profiles, all20 driving choices
and airborne cross-up pass host checks. Publication is pending. See
[the follow-up evidence and limitations](research/MOTORCYCLES.md).

## ATV Fury 2 home-screen identity — host checkpoint (2026-10-13)

The landing page and game page now share the authored ATV Fury 2 identity: a
custom vector browser icon, 180px Apple touch icon, 512px app icon, and standalone
web-app manifest. The app label is **ATV Fury 2**, with `/play.html` as its start
route. The SVG is authored source; its raster variants are text-encoded authored
art decoded only into ignored build output. `tools/test_app_identity.mjs`
confirms both pages' title/icon/manifest links and successful icon/manifest
serving with expected MIME types and dimensions. These assets contain no game
disc material. This is host verification pending current-main CI/deployment and
public verification; physical iOS Home Screen behavior is still untested, and
an existing saved shortcut can require removal/re-addition to refresh its icon.

## Garage and Freestyle selections persist — host checkpoint (2026-10-13)

Garage vehicle and rider choices are saved locally and restored after refresh.
The first Freestyle environment picker restores the last successfully started
course. Browsing another preview, a failed load, or selecting a Race course does
not overwrite that Freestyle choice. Supported Race setup restores the saved
vehicle/rider selection. The menu regression passes ATV/rider and motorcycle
refresh, Tioga Freestyle restore, preview browsing, Race preservation and zero
browser errors. Three focused frontend cases cover valid selection persistence,
unavailable storage and load failure. The actual-menu check is host-only; no
public deployment or physical-device verification is claimed.

## Enhanced one-handed second-touch controls — host checkpoint (2026-10-13)

The saved **One-handed mobile: On/Off** preference and existing Off behavior are
preserved. In On mode, the first touch anywhere on the game surface remains the
primary steering/lean and drive/brace control. A second touch independently
requests ground brake/reverse or, in the air, one of eight directional stunts or
clockwise Cat Nac/counterclockwise Cordova. Secondary release resumes the held
primary's forward drive/brace; primary release clears both. Further gameplay
contacts are ignored, and Pause/Camera retain separate hit areas. See
[touch behavior and evidence](research/TOUCH.md).

The final enhanced mobile Chromium report passes portrait 390×844 and landscape
844×390 ground reverse, primary-axis preservation, secondary release and
cancellation, with no errors or touch camera orbit. A natural Caribou hop passes
Nac Nac, Cat Nac and Cordova selection while primary drive/brace continues and
the secondary stunt releases independently. Normal one-handed/touch/UI and
mobile utility checks also pass. `make check` (349 core, 25 net, 3 relay, 4
tools, 13 web and 79 JS), `make verify`, the WASM build, repository hygiene,
29 release checks (one skipped) and 8 Worker checks pass. This is a host-only
checkpoint pending current-main CI, deployment and public verification. Physical
iOS Safari/controller behavior and full original stunt/game parity remain open.
The inspected capture shows Nac Nac during the touch session; it is stored only
in ignored private screenshot storage until the validated release flow.

## Combined minimap/speed and corner configuration — 2026-10-13

Branch `feat/minimap-instrument` revises the GPS to default circular/top-right,
with square and bottom-left options under HUD settings → Minimap configuration.
It renders Rust speed/preload inside the map; the original speed block is
suppressed whenever Freestyle GPS is enabled. Off/Race retain original speed,
and the original stunt HUD stays active. Shape/size/corner reuse the cached
aerial raster, with no additional scene draws. Host tests pass four viewports,
both shapes/corners, toolbar clearance, 44px controls, persistence, no duplicate
speed pixels, actual preload and Off fallback. Timing/lifecycle/drive gates pass;
combined map updates cost 0.068–0.142ms mean CPU on one Mac, not phone hardware.
The final footer has no backdrop: outlined MPH sits near the bottom edge and
the small 200 ft map-scale readout is directly above. Tap/click switches persisted
north-up/heading-up without rebaking; held driving plus a second-finger toggle,
drag-away cancellation and keyboard activation pass. The final heading-up phone
viewport gate measures 0.077–0.125ms mean CPU/update, 471 unchanged scene draws
and zero retained map pixels/updates when Off.
See [minimap methods and limits](research/MINIMAP.md). The selected screenshot
is a host capture; CI/public delivery are verified separately.

## Motorcycle inward bank and standing menu — host correction (2026-10-13)

The user found outward lean/static garage poses in the previous correction.
Those earlier finite/turn/contact gates did not establish correct banking.
The recovered ground-bank orientation approach now controls normal ground roll;
source pose steering, physical fork steering and shared grip/peg constraints agree.
Actual rendered Mini/125/250/500 touch checks pass in both directions, with
1,061 exact paired Off/On steps and 2,455 GPU contacts. Original MX standing
breathing/three idle clips play for all20 garage selections through the existing
ATV menu animation engine; the original ATV bank returns correctly. The source
Sano50 Mini has ~15-inch tires, versus ~29-inch full-size125 front tires; source
geometry is retained and the Mini label makes the distinction explicit.
The severe500 Vancouver wreck/separation/breakaway/recovery gate passes1,297
paired steps. Native whole-controller and whole-physics parity remain open;
see [motorcycle evidence](research/MOTORCYCLES.md). Publication is pending.

## Freestyle GPS minimap checkpoint — 2026-10-13

New SSD worktree `atv-offroad-fury-2-minimap`, branch `feat/freestyle-minimap`,
adds a borderless orthographic north-up aerial GPS from recovered terrain/static
scenery. Pause → HUD settings persists On/Off and 96–224px size with screen caps.
One 1024px cached raster, <=20Hz small blits, no extra scene draws; Off cancels
baking and shrinks both canvases. All nine actual course loading/driving gates,
settings/reload/rotation/pause/blur/context restoration and normal frontend/touch
regressions pass. Serial host map updates cost 0.05–0.09ms; course baking 45–53ms
is spread across short slices, with observed maximum 2.4ms. Phone-sized Chromium
runs are not physical-phone performance measurements. See
[implementation, measurements and limits](research/MINIMAP.md).
The inspected portrait capture is a host image; production identity and gallery
publication are verified separately through current-main GitHub CI/R2.

## Motorcycle correction — public gameplay verified (2026-10-13)

The correction to public `8e1aa77` is documented in
[research/MOTORCYCLES.md](research/MOTORCYCLES.md). Touch steering now follows
screen direction; the source Mini rider bank, exact packed MOT angle lookup,
inclined fork axis and shared limb constraints correct rider fit. Continuous
pose/camera blending and batch render interpolation address movement snaps.
Garage IDs 27–46 offer 16 source designs/20 clearly labeled choices. Four source
class profiles remain shared across brands. Original ATV solver is unchanged.
Public build `4fbb399` passes actual Mini/125/500 touch steering and rendered
grip/peg contact (762 paired steps/1,760 GPU marker comparisons), all20 choices
(1,543 paired steps), Honda race and mild bike wreck/recovery (308 paired steps).
CI 38399468481 passed and deployed. The host severe MX500 breakaway/recovery
passes 4,607 paired steps. Final current-main metadata/gallery publication and
full R2 HTTPS verification follow the validated-artifact release flow.
No complete original-game physics equivalence is claimed.

# Implementation handoff

## Fury Remastered domain setup (2026-10-13)

The intended primary public HTTPS address is https://furyremastered.com/.
Both apex and www custom domains target the existing Cloudflare Worker. The www
route redirects to the apex; the previous workers.dev address remains supported.
The explicit multiplayer origin allowlist covers both playable origins. Domain
routes and origin settings are persisted in wrangler.jsonc, and main CI verifies
both addresses. See research/DEPLOYMENT.md for the final public verification.

## Original CPU avoidance, wreck recovery and jump tricks — tested host checkpoint

Implementation `eeadf09`, integrated with UI/main `2ef33ab` as `bb9d1bee`,
connects source-correlated peer/scenery/vegetation avoidance, independent physical
CPU wreck/detached-rider recovery, low-air terrain-normal feedback, and National
TDF-airtime-gated original stunt selection. Recovery cannot count teleport distance
or award a checkpoint/lap. See [racing evidence and remaining gates](research/RACING.md).

The final 65.08s browser run observes 5 natural wrecks/5 recoveries, 8 original
stunt starts, resumed racing, finite matrices and no errors/reset progress awards.
ATV and motorcycle 34 race regression, mobile held-drive utilities and UI transitions
pass with portrait/landscape coverage. Rust/JS, fmt/strict Clippy, WASM, setup,
release/Worker checks pass. Private bounded original-instruction comparisons pass
9,000 cases with zero output-bit differences under the finite-binary32 VU model.
This checkpoint is host-only pending validated-main/public release verification.

Rider-to-rider impulses, original global RNG/actor ordering/math tables, roster
quad tuning, remaining AI flags/preload helper, native grid/progression/finish
rules and physical phone/Safari verification remain open.

## Previous original CPU driver checkpoint

Branch `feat/original-race-ai` replaces the preview's constant-speed route motion
with four independent recovered Vehicle solvers, normal rider controllers and
ATV linkage matrices. Exact-build National driver decisions now supply steering,
lookahead, lane slew, TDF target speeds, throttle/brake deadband and ahead-only
leader speed limiting. The field is YOU plus up to four CPUs; guessed difficulty
speeds are removed. Ordered segment distance fixes start-line wrap in the leader
gap. Current starting slots, checkpoint radii and race event rules remain authored.

The merged host gate passes 339 core, 25 protocol, 3 relay, 13 bridge, 4
release-tool Rust and 69 JS tests, formatting/strict Clippy, WASM, source/setup
and repository hygiene, 29 release checks (one skipped) and 8 Worker tests. Private finite-binary32 comparisons pass 1,500 cases
executing 124,410 original instructions with zero output-bit differences for
leader speed, pedal deadband and lateral slew/clamp. This is selected-routine
proof, not full-driver or PS2 replay equivalence.

Caribou browser checks pass 1280×720, 390×844 and 844×390 setup/driving/rotation,
countdown, pause, restart and touch cancellation. Independent CPUs roll wheels,
compress suspension and publish 27 quad/18 rider transforms. A 45.05-second
simulation run covers 2,435–2,466 feet per CPU, braking/coasting, ahead speed
limiting, turns and elevation changes, with actual CPU GPU draws and no JS errors.
Frontend/Freestyle/controller/portrait regression passes. Results UI remains a
separate render-only fixture. A separate natural CPU run reaches the provisional
finish radius for all four racers by 120.06s; native finish-line parity is unverified.

A mobile utility regression also fixes real second-finger Pause/Camera taps while
one-finger drive is held: the browser suppresses the secondary compatibility
click, so utility touch release now invokes the existing button action once.
Portrait/landscape checks cover independent driving capture, no stray throttle,
drag-away/cancel, pause/freeze, neutral resume, 44px hit areas and mouse/keyboard.
Physical iOS Safari and phone hardware remain unverified.

Main motorcycle integration is preserved: its separate authored CPU route
preview remains explicitly distinct from original ATV CPU vehicle simulation.
Quad and motorcycle model 34 race regression both pass.

This previous driver checkpoint was publicly verified at main `7c8c591`.
The newer checkpoint above connects avoidance, AI tricks, CPU wreck/recovery,
low-air feedback and shared dynamic scene inputs. Rider-to-rider collisions,
native grid placement, separate roster quads/costumes and original RNG/math
remain open. The grid helper 001f8410 is recovered privately. Source exports,
probes, content and screenshots remain ignored. See research/RACING.md.

## Playable motorcycles checkpoint — 2026-10-13

The motorcycle branch now connects garage choices 27–34 to the distinct two-wheel
Rust/WASM controller, source Race/Pro 50/125/250/500 profiles, assembled source
parts/materials, THQ rider motion, engine/clutch/impact samples, dust/roost/exhaust,
mud and detached-rider recovery with optional funny crash presentation. This
is authored source-tuned behavior, not whole native-step parity or a complete
decompilation. Multiplayer stays ATV-only. See research/MOTORCYCLES.md and
tools/test_motorcycles.mjs / tools/test_motorcycle_crash.mjs. The four compact
/MX/BIKE*.RES resources are generated only into ignored private hosted-content
by tools/package_motorcycles.py; publish through the validated R2 workflow.

Temporary ignored work/host-target and work/host-web avoid observed SSD open/
hardlink/clone stalls; existing SSD source/build/private symlinks remain intact.
The SSD UUID is verified before use. Host test server is not a remote handoff.

## Deployment recovery — 2026-10-13

The experimental local release queue was cut over before completing public
end-to-end validation. The worker is now stopped and disabled in launchd;
`make release` refuses production submission. GitHub Actions build/deploy and its
production environment history are being restored. Follow
`research/CI_CHECKPOINTS.md` for ordinary checkpoint delivery. Queue development
must stay isolated until successful end-to-end validation and a user-approved
cutover. This recovery checkpoint does not claim a completed public deployment.

## Historical motorcycle recovery checkpoint (superseded above)

The isolated motorcycle worktree starts at `c895ba4` and preserves all concurrent
gameplay lanes. The supplied newer disc is MX vs. ATV Unleashed USA, SLUS_211.04
v1.02. Its source media/module/decompiler observations remain private. The new
Rust PSR reader matches all 20,391 directory records against an independent
parser. A source-verified memory mapping adds the exact game module to private
Ghidra analysis; candidate function boundaries still require review.

Rust PSG v5 decoding now recovers the standard/50cc bike rigs and seven Suzuki
body parts. Original parser instruction replay agrees on 42 geometry headers
across 6,846 instructions. The optional funny-crash presentation accepts the
motorcycle tire markers and carries geometry children below the nearest detached
part, tested on a synthetic bike hierarchy. Existing ATV severe-crash rendering/
recovery passes 855 paired WASM steps with identical baseline gameplay/camera.

322 core/25 net/3 relay/13 bridge/69 JS tests, formatting, Clippy, WASM release,
setup/hygiene and 18 release tests pass. The inspected dev-log image is a static
flat-shaded resource inspection, not an assembled bike in the game. No motorcycles
are yet selectable or drivable, and handling, rider/stunts, source audio/effects,
crash/recovery and public R2 integration remain outstanding. No new source content
has been published. Continue the integration gates in [motorcycles](research/MOTORCYCLES.md);
do not substitute quad handling or claim full decompilation.


## Caribou race preview — public release verified

The worktree race slice adds Single Player → Race → Caribou Peak National,
1–5 laps, 0/3/5 CPU riders and three difficulty speeds. Original `env01natE.tdf`
decodes 252 unique closed route points; AIRiders.pb supplies YUKARI, PANZER,
MCNABB, BALS and ROBERTS. Rust owns countdown, ordered checkpoints, laps,
current/best/total timing, rank, finish and CPU motion. The browser adds radar,
next-point direction, wrong-way text, results and retry. The original National
archive is added only to the separate ignored R2 package, never GitHub. Older
public manifests hide Race until that archive is available.

Host Chromium passes setup and driving at 1280×720, 390×844 and 844×390;
44px setup actions, rotation, countdown input lock, pause, CPU motion and
restart; existing Freestyle/menu/controller/portrait regression passes.
Results UI uses a separate render-only fixture; full course laps are not
claimed as browser replays. Synthetic Rust tests cover finish/shortcut rules.
See research/RACING.md for verification updates. The original implementation request skipped deployment waits; the user then
requested publication. Public availability, CI and content/gallery upload now
pass for af3e6b5, CI 38333554805.

The race slice now integrates main `c895ba4`. The combined gate passes 325 core,
25 protocol, 3 relay, 13 bridge and 69 JS tests, strict Clippy/formatting,
18 release/content tests, 6 Worker tests, WASM, setup/hygiene, race play,
frontend and all-26-model regressions. Race ABI 95/96 preserves multiplayer
80–85; race data accompanies the existing RawValue simulation batch transport.
Temporary native/WASM caches avoided host SSD clone stalls; static browser
artifacts remain on the SSD. Public/CI verification now passes for af3e6b5: 33 authored files, 416 served
objects, 108 runtime records and public race play at all three aspect ratios.
The referenced race portrait remains an honestly labeled host capture.

This is a race preview, not recovered original racing: CPU actors follow the
route with distinct deterministic speeds and shared neutral ATV/rider meshes.
Original AI decisions/physics, rider-to-rider collisions, costume diversity,
Lead timing, HUD toggles, other courses and progression remain incomplete.


## Ordered vegetation: tested host release candidate

The isolated map branch connects original distant atlases and source-correlated
near/far phase, alpha/depth and retained-orientation behavior on all nine maps.
One shared atlas uses two back-to-front billboard batches; total vegetation
draws are 8–17/map. Rust prepares stable 64² visibility cells and direct species
lookup; bounded browser selection matches a full scan, reducing starting-view
placement checks by about 69–92%. The current browser visibility envelope remains
provisional. Native billboard per-row fog/depth packing, callback order, exact
camera/viewport mapping and GS/VU hardware behavior remain open.

Host source-color comparisons pass 1,800 RGBA samples with zero byte differences;
1,800 vertex comparisons have maximum error 0.0009765625 and exact UVs. All nine
map driving and near/distant material gates, Vancouver desktop/390px, FREE05
multiplayer and shared shader regressions pass. 317 core/25 net/3 relay/13 bridge/
68 JS tests, formatting, Clippy, setup, 17 release and 6 Worker tests pass. The
fresh ENV08 hero is inspected and private until this checkpoint passes current
main CI/deployment. See research/MAP_FIDELITY.md and the host-only dev-log entry.
No Mistakes is disabled; normal verification and origin/GitHub CI apply.

The preceding public release is `e1afc2cb984db40a06ebf1138c56c895578c9280`,
including the d9596d9 vegetation/transport proof and default-off funny crashes.
Its responsible release chat reports CI 38320324826/deployment success, 32 authored
files, 410 served objects, 106 runtime records and 124 gallery entries/104 images.
The following near-material public proof remains release-scoped to d9596d9.
## Model selection and original menu presentation: host checkpoint

An isolated worktree adds all-26-quad inspection cards, original 32-node male/female
menu rigs, original breathing/idle MOT sampling in an independent Rust runtime,
dedicated stereo UI tunes and full Rust course loading with original track photos.
The menu gate passes six viewport sizes, 44px controls, both input navigation paths,
rotation, rapid selection, context restoration and selected-model course loading.
Frontend and all-21-song regressions pass. The staged private R2 package has 107
records/307 validated files; source resources and screenshots remain excluded
from Git/CI. Native idle scheduling/transition rules, lighting/specular fidelity,
outfits, hair and physical phones remain open. See research/MENU_SELECTION.md.
Public application and R2 checks are pending the merged main CI release; the user
requests delivery without waiting for deployments. Preserve other implementation
lanes and use origin rather than the disabled No Mistakes remote.


## Impact-triggered funny crashes: host verification

The user requested Funny crashes On by default. Fresh browsers and unavailable
storage now start On; an existing saved Off remains Off. Pause toggles persist
through reload and restore original crash presentation when disabled.

Mechanical breakup now observes actual inward ATV contact speed, independently
of rider separation and travel speed. Terrain/suspension, static obstacle and
moving-obstacle contacts feed a read-only diagnostic; no native forces, wreck
classification, recovery, RNG or camera logic change. An airborne rider can
ragdoll while the ATV remains intact. Contacts of at least 25 ft/s shed front
wheels; at least 50 ft/s shed all seven supported parts. Later impacts can
escalate damage without duplicating parts. Recovery restores the intact model.

Host severe-input verification records three intact-ATV aerial ragdoll frames,
then actual obstacle contacts at 27.420 ft/s (two parts) and 50.855 ft/s (seven).
All parts reached actual GPU draws and recovery; 1,199 paired WASM steps have
zero compared gameplay/camera mismatches. Actual-touch ATV26/female at 390px
passes pause, Off/On during wreck, recovery and driving, with 439 paired steps
and zero mismatches. Explicit Off passes the original touch wreck route.
Fresh defaults, saved Off/On across reload and blocked-storage session toggles
pass at 844×390. `make check` passes: 327 core, 3 web, 25 net, 13 relay,
69 JavaScript tests, formatting and strict Clippy. WASM build and `make verify`
pass. This is host evidence, pending current-main CI/public checks.

## Historical initial optional funny-crashes host checkpoint

The default-off Pause → Funny crashes option runs an authored Rust presentation
layer on actual wreck/separation events: joint-limited rider tumbling and bouncing
original wheels, handlebars, swingarm and rear axle. Local rider only; native
vehicle/rider collision, score, recovery and camera remain unchanged. Recovery
clears effects and restores intact models. Arbitrary chassis fracture and debris
contacts against props/water are not implemented. See research/CRASH_EFFECTS.md.

Host desktop ATV01/male and 390px ATV26/female failure routes pass rendered poses,
pause, Off/On during a wreck and resumed driving. Mirrored exact-input WASM
comparisons cover 415 + 430 ordinary and 762 severe-crash steps with zero compared
gameplay/camera/water/train/wheel-field mismatches after excluding effects and the
echoed option. The severe driving/stunt-input gate displays all seven pieces and
recovers; the normal failure sheds two front wheels. Baseline Off also passes.
312 core, 3 web, workspace net/relay and 68 JS tests, formatting, strict Clippy,
WASM build and make verify pass. Host screenshots are inspected and recorded in
the dev log, with publication pending validated main CI and the R2 gallery upload.
This evidence is an optional presentation component, not original-game parity.

## Latest verified public near-material release

Public main `d9596d937907395440b61e78be61b2045d987d4d` (PR #12) passed CI
`38318057407` and is deployed. Deployment identity matches the validated
artifact. Canonical R2 retains 106 records and 306 references; the current upload
validates 408 approved objects totaling 310,198,438 bytes. HTTPS verification
matches all 32 authored files and 407 served objects by hash, size and MIME. The
public gallery has 120 entries and 101 screenshots.

The public near-vegetation material gate passes all nine maps; 600 actual-GPU
RGBA comparisons have zero byte differences. Public transport validation passes
1,200 steps with exact sequential frame and camera decimal outputs. No production
timing benchmark was run. The public route regression passes all nine maps.
Vancouver desktop/390px passes with 78 drawn placements, 43 colliders, 37,649
plants, 123 wheel-support frames, 31 body-contact frames, takeoff at step 19,
later landing, ATM frame 1 and the default-unlocked route. Public touch input
matches batched/sequential behavior for 124 steps, including shifted release and
cancellation. A public copied-link FREE05 pair passes with 13 remote draws per
client, male/female rigs, one voice per client, host cleanup and a measured
4.9307901763-foot remote distance. No public production timing benchmark was
run; the transport microbenchmark is host-only. These are bounded component and
route checks, not full-scene or PS2 equivalence. The selected ENV07 screenshot is
inspected but remains in private storage until a later validated current-main
artifact supports a gallery update. Weather callbacks, distant vegetation/LOD,
point lights, shadows and native GS/full-frame behavior remain open.

## Previous verified public fog/reflection release (e18a371e)

Public main `e18a371e81b471137dd5fb1121b05d454584b1d0` passed CI
`38312395498`. The validated artifact matches 32 authored files and all 405
served objects. The canonical R2 package contains 106 records, 306 referenced
files and 406 approved objects across three changes; the public gallery has 118
entries and 99 screenshots. HTTPS/hash and public browser gates pass.

Public ENV01/ENV04/FREE05 fog checks match 3,000 GPU coverage samples with zero
byte differences; the tested screen-affine interpolation sample passes at 55.
ENV01 reflections draw 88 groups/275 instances with 141,770 contributing pixels;
FREE05 draws three groups/78 instances with 236,380 pixels. Across both maps,
2,304 GPU comparisons have maximum UV error 7.15e-7. All nine maps pass. Public
materials report 4,618 rain, 73,970 lava, 6,335 waterflow and 115,571 ski-jump
contributing pixels, with pause/context retention. Four wet-map and 390px water
checks, dry-map behavior, Vancouver desktop/390px contact traces, original
frontend routes, and copied-link FREE05 multiplayer pass. The Vancouver traces
draw all 78 placements with 43 colliders, 123 wheel-support frames, 31
body-contact frames, takeoff at step 19 and later landing. The public multiplayer
pair draws 13 remote models with both rigs, audio and movement.

This verifies authored browser components and routes, not complete map/game
parity or PS2 hardware behavior. Fog packing remains source-correlated; native
terrain/water GS precision is unverified. The later d9596d9 release adds
near-vegetation material and bounded transport checks, but does not complete
source tree lighting. Global Sun/frame behavior, distant transitions, LOD, other
lighting, shadows, lens behavior, weather callbacks, remaining modes and
progression remain open. See [deployment evidence](research/DEPLOYMENT.md),
[map evidence](research/MAP_FIDELITY.md) and the public
[dev log](https://atv-offroad-fury-2.champtoday.workers.dev/devlog.html).

## Historical public Vancouver Ridge map checkpoint (f585991)

Public main `f585991579f8c44b098d6b137b1679eb8050e3fd` passed CI
`38302626466`; HTTPS verification matches all 31 authored files and 399 served
objects. The R2 package validates 400 approved objects, with six incremental
changes totaling 305,224,223 bytes. The public gallery has 110 entries and 93
screenshots. Vancouver Ridge/FREE05 is connected to Single Player → Freestyle
under its original SPStunt6 name and two-part thumbnail, unlocked by default.

Public desktop and 390px traces each cover the original terrain, 37,649 snow
plants, 78 source scenery definitions/75 drawn, 43 placed colliders, 123
wheel-support frames, 31 body-contact frames, flight at step 19, later ground
contact, and drag-light frame 1. The bounded ramp route uses normal throttle
without injected velocity, force or wreck state. The public ENV01–ENV08 plus
FREE05 regression, source-material inspection and original title/menu/controller/
touch/pause routes pass without JavaScript or GL errors or a disc prompt. These
are browser component and route checks, not full map, hardware or game parity.
Three Vancouver ice-reflection overlays, exact fog, 92 light/lens behaviors,
projected terrain/vegetation LOD, shadows, weather, remaining modes and progression
remain open. See [map evidence and limits](research/MAP_FIDELITY.md) and the
[deployment record](research/DEPLOYMENT.md). Preserve the separate multiplayer
integration lane. The newly inspected desktop public-ride screenshot is referenced
in the local log but remains in ignored private storage pending the next validated
R2 gallery upload.

## Historical host reflection checkpoint, later released in e18a371e

An uncommitted renderer candidate now draws ENV01's 88 original reflection groups
across 275 instances and FREE05's three groups across all 78 instances, with no
skipped groups. Host inspection records 152,862 and 236,566 contributing pixels.
The updated Vancouver desktop and 390px traces draw all 78 placements with zero
skipped groups; both have 43 colliders, 123 wheel-support frames and 31
body-contact frames, plus takeoff at step 19, later ground contact, animation and
menu checks. Expanded GPU transform feedback compares 768 original arithmetic
cases across identity, translated/rotated placement and rotated child-node
transforms: 2,304 GPU comparisons across 5,376 interpreted instructions, with
maximum UV difference 7.152557373046875e-7. The test supplies a pre-normalized
finite local view direction and does not emulate PS2 accumulators, reciprocal
behavior or the full pipeline. `make check` passes 308 core tests, 22 network
tests, 11 relay tests, 3 bridge tests and 66 JavaScript tests. `make verify`,
`make web`, 17 release tests and 6 Worker tests also pass. All nine maps pass
terrain-following/vegetation checks; material tests report 2,488 rain, 73,978
lava, 6,329 waterflow and 115,571 ski-jump contributing pixels with pause/context
retention. Original menu/controller/touch/pause routes, Tioga drive and Caribou
reload pass without a disc prompt or browser errors. This earlier host
reflection evidence was subsequently integrated and passed public verification
in e18a371e; this f585991 checkpoint predates it. See
[map-fidelity evidence](research/MAP_FIDELITY.md).

## Historical host fog checkpoint, later released in e18a371e

The integrated host candidate now reads source Fog Visibility and Haziness for
ENV01/ENV04 (range 0–768, haziness 1) and FREE05 (352–768, haziness 0.5),
with ENV01/ENV04 source haziness 1.2 clamped by Rust `min(1)` to 1, and following
fog end for the view far clip. Compressed
prelit materials use source-correlated 8-bit GS fog packing and the per-PSM
fogging switch; the water base pass uses its source switch. The shared GPU
coverage path uses screen-affine interpolation rather than the earlier guessed
exponential curve. On the host GPU, 3,000 coverage samples match with zero byte
difference; a triangle sample is 55 versus 55.25 affine and 96.25 perspective.

Native range/coefficient checks cover 600 scalar cases and 35,393 original EE
instructions with zero compared binary32 mismatches. The 3,000 packing samples
are source-correlated expressions with finite distance supplied, not complete
VU runs or hardware replay. The fog-integrated reflection test passes 2,304 GPU
comparisons with maximum UV error 7.15e-7. Nine-map, materials, water, Vancouver
desktop/390px contact and frontend regressions pass; `make check` passes 309
core, 22 network, 11 relay, 3 bridge and 66 JavaScript tests, alongside verify,
web, 17 release and 6 Worker tests. The disassembler's SUB broadcast label was
corrected against execution: SUBAz writes ACC; the reflection XYZ comparisons
are unchanged because the tested operation wrote W only.

This host evidence was later released and publicly verified in e18a371e; it is
absent from the historical `a4d3003`, `cef26f4` and `f585991` releases. The
integrated multiplayer host checks also pass: two-client
FREE05/link-pair routes draw 13 remote models; four actual clients each render
three remotes in 39 draws with three voices, and every movement driver passes.
Host departure clears remote models and voices; cancellation during map loading
passes. Timing was measured in one Mac headless Metal synthetic-focus run and
does not establish device or Internet performance. The combined gate passes
309 core, 25 network, 3 relay, 12 bridge and 68 JavaScript tests, plus 17
release tests, 6 Worker tests, verify, web and hygiene checks. Terrain and
water GS precision, water reflection/froth, source-specific program precision,
active-view overrides, weather callbacks, lighting, shadows, lens effects and
LOD remain open. The host ENV04 screenshot in the dev log is not a public-build
capture or an original PS2 framebuffer. Preserve the separate multiplayer
release lane.

## Verified public ninth-map Rust multiplayer

The merge preserves the public map/scenery release and extends Rust's shared
lobby catalog to `env01`–`env08` plus `free05`. IDs remain stable; map 9 wraps to 1,
and invalid map 10 is rejected. Rust supplies both environment keys and next-map
metadata to browser UI/socket glue. The packet layout and host/identity/rate
controls are unchanged. Two actual public browsers copy/join an invite, cycle to
Vancouver, load matching scenes, draw both rigs, exchange movement and engine
voices, then end cleanly with host departure. A real close-during-load gate keeps
a completed old load on the Multiplayer menu without resurrecting its room or
resuming a disconnected ride. The original eight-map driving,
touch-wreck, water/snow and public model proofs remain separate evidence.

Combined host gates pass 307 core, 25 protocol, 3 relay, 12 WASM and 68 JS tests,
17 release/content and 6 Worker checks, strict formatting/Clippy and 13 live relay
cases. The 106-record runtime package preserves both lanes' required resources.
Main `cef26f4495f0c5dbf6f8398133ea81bf4b78aed3` passes CI `38307658534`.
Public HTTPS matches 31 authored files, the Rust relay identity/WASM hash and
402 served objects (115 log entries/96 screenshots before this verification note).
All 13 public relay cases, copied-link Vancouver play and host-close during loading
pass. Four public clients each draw three remote peers in 39 draws with three
engine voices; movement from all four riders reaches all observers, and host-end
clears guest rooms, audio and GPU caches. A 390px failed-stunt replay records 403
guest frames, 42 detached-rider frames and 14 spatial impact-audio frames, followed
by recovery. No later WIP reflection work is included.

All 26 garage choices pass again against this merged public renderer: 13 male and
13 female selections load correct resources, produce actual animated GPU draws,
drive 37.44–44.76ft and run their exact model engine configuration without browser,
HTTP, worker or WebGL errors. The validated canonical private `hosted-content`
manifest now has 106 records; prior manifests and old blobs are retained.

The four-client 12-second sample uses one Mac, headless Chromium Metal, 1280×896
and synthetic focus: 71.00–75.86 FPS, 25.9–27.2ms frame p95, 2.61–3.11ms render
CPU, 2.1–6.6ms physics round trips and 41–52ms relay RTT. Concurrent validation
workload overlap was not measured; these are bounded results, not device or
Internet guarantees. Shared dynamic-world clocks, racing, player collision,
host migration and server/host physics reconciliation remain unfinished.

## Verified public Rust multiplayer checkpoint

Isolated `feature/rust-multiplayer` adds four-player host-controlled Freeride
lobbies, eight-character share codes/links and an authored Rust Cloudflare relay.
Rust owns packet validation, sender identity, pose interpolation and per-peer
spatial sound selection. Local driving remains immediate in its existing Rust
simulation; this is owner-authoritative presentation, with no player-to-player
collision resolution or competitive race authority yet. See
[network implementation](research/MULTIPLAYER.md), [original multiplayer evidence](research/MULTIPLAYER_ORIGINAL.md)
and [model census](research/MULTIPLAYER_MODELS.md).

The garage adds all 26 recovered in-race quads with their own PHY/AO/AR/linkage
resources, and male 18/female 27-node rigs with their matching 78-motion catalogs.
Original unlock/profile rules are not reconstructed. Networking preserves source
node shear and detached rider poses; state switches/teleports are discontinuities.
Host departure closes the event. Protocol/relay component commits are 4fa8ad0 and
53a0db4; browser integration is b96c702. Public main
`ceee73d44bc69f940729c5fdf20b892e4394bb59` passes CI `38300517282`. No source disc/executable/unused VUT or first-person arms enter
GitHub or the runtime package.

Initial host gates pass 299 core + 22 net + 3 relay + 11 WASM + 66 JS tests, strict formatting/Clippy,
15 release + 5 Worker checks and make verify. Actual two-browser code/link admission,
different models/rigs, remote movement, engine voices and host departure pass.
A hardware Metal touch-driven failed Nac Nac replicates Riding/Airborne/Wreck,
crash serial, independent rider separation, original impact audio and recovery;
local RTT measurements do not establish Internet or physical-phone performance.
The private native probe covers all 26 quads and both rigs’ 3,276 sampled poses.

The checked CI artifact includes the compiled Rust relay and the existing game
Worker hosts its routes and Durable Object, preserving the scoped CI credential. `/multiplayer/version` reports its configured checked
commit/WASM hash. Expanded required resources are staged separately in
`private/reports/multiplayer-content` and must be uploaded from a validated current-main
artifact using `upload_cloudflare_content.py --content`. That package is now public:
396 approved R2 objects, with 59 incremental uploads. HTTPS verification matches
31 authored/static hashes and all 395 served runtime/gallery objects, with 108
log entries and 91 screenshots before the public verification note. The relay's
configured commit/WASM hash matches its checked CI artifact. The original scoped
CI token deploys the combined Worker; host upload authentication uses the existing
Wrangler login (refresh an expired OAuth session with `wrangler whoami`).

Public real-browser verification follows the actual copied invite link, starts
ATV01/male and ATV02/female, draws remote rigs, exchanges movement and spatial
engine sound, and ends cleanly with host departure. A 390px touch-driven failed
Nac Nac produces 112 guest samples across Riding/Airborne/Wreck, 51 detached-rider
frames, 13 original impact-audio frames with changing pan, then recovery. Sampled
round trips are 38–51ms from this host through public Cloudflare, not a promise
about other locations or physical devices. Normal public menus/controller,
Tioga/pause and 390px Caribou reload also pass with no disc prompt or browser errors.

The actual public four-browser gate also passes with ATVs 1/14/26/2 and both rigs.
Each client draws three remote peers (39 draws) and starts three independent engine
voices; each rider's keyboard movement reaches all three observers. Host departure
clears all guest room/pose/audio/impact/GPU caches without browser or HTTP errors.
A 12-second measurement on one Mac with Chromium Metal, 1280×896 and synthetic
focus yields 71.59–74.14 FPS, 26.4–27.2ms frame p95, 2.47–2.92ms render CPU and
2.1–4.3ms physics worker round trips. Relay RTT samples range 37–201ms, ending at
42–47ms. These automation/device/workload conditions do not establish general
Internet, physical-phone or multi-device performance. Repeat with
`tools/test_multiplayer_four.mjs`; original-content garage coverage is repeatable
through `tools/test_player_models.mjs`. Outputs remain in ignored private storage.

All 26 garage choices also pass the public actual-UI single-player probe, with
13 male and 13 female selections. Each loads its own expected model/physics/material
paths, renders finite 27-node quad and 18/27-node rider poses, drives 37.21–45.45ft
under throttle and runs its exact per-model engine configuration/voice. No HTTP,
page, worker or WebGL errors occur. Public representative models 01/14/26 were
inspected; this verifies the connected loading/driving path, not complete original
handling, unlock, controller or physical-phone parity.

The current component gate passes 303 core, 22 protocol, 3 relay, 11 WASM and 66 JS
tests, 16 release/content checks, 6 Worker routing checks, formatting/Clippy and
`make verify`. All 11 live public relay checks pass, including four-slot capacity,
stale command resynchronization, sender identity, malformed-packet isolation,
rate limits and host-end behavior. Preserve the integrated water/snow and UI lanes.

The combined merge against water/snow main cec8a6d passes 303 core, 22 net,
3 relay, 11 WASM and 66 JS tests, plus 15 release/5 Worker checks. Serial
host browser gates retain multiplayer touch-wreck replication, all four wet-map
reset/Swim routes and 24 desktop/390px snow collision approaches.
## Verified public water and Caribou collision checkpoint

Main `b9a3a448d6d19103d63be8e6ec530974297b1870`, CI `38292520844` success,
is public on Cloudflare after PR #5 from `feature/water-snow-collisions` in the
separate managed worktree. The original five-second deep-water reset uses the
preceding cached surface and retained checkpoint. Public ENV04/05/07 routes
expire/reset at 348/349, 345/346 and 337/338 and return above water to the dry bank.
ENV08's shallower route exercises countdown clearing without expiration. All
four wet maps pass water-only rider buoyancy, Swim and recovery; ENV04 passes
at desktop/390px. Existing Caribou ramp and all five ski colliders needed no
new shape/offset: 776 per-face tree rays and 24 public desktop/390px approaches
cover wheel/body response and clean ramp launch/landing at 35.631ft/s upward.

Public HTTPS verification matches 30 authored files and 343 served objects,
with 103 dev-log entries / 90 inspected screenshots before the verification note.
R2 validates 344 objects and uploads only the new approved host snow PNG and
gallery metadata; no runtime resource changes. A first hash fetch resets its
connection; the complete retry passes. Public clean touch landing, Nac Nac
wreck/recovery/driving, title/menu/controller/Tioga/Controls/390px Caribou reload
without a disc prompt, F3/Pause Stats persistence and portrait fit, uninterrupted
ENV01–08 driving and dev-log filters/expandable evidence pass without JS/GL errors.
302 core + 2 WASM tests, 47 JS tests, strict Clippy/fmt, setup, 13 release/4 Worker
checks pass. The snow screenshot is a host capture; no public capture is claimed.
See [water/snow evidence and limits](research/WATER_SNOW_GATE.md). These bounded
source-resource routes and component probes do not prove complete gameplay,
render/wake fidelity, physical-phone behavior or PS2 hardware parity.

## Latest verified public rail checkpoint

`841f11c5d619cac92bc6033fe01e1e2d09cbdcc4` is public, CI 38287671823 success.
All 12 ENV08 curved rail sections now render at original scene placements on
desktop and 390px, with 36,027–49,735 and 6,041–8,366 changed pixels respectively,
versus zero before repair. Native 68 setter cases confirm the static engine
replaces saved PSG root translation/basis; the old browser composed both roots.
The scene-only decoder preserves vehicle/rider/vegetation/train bind decoding,
COL, source scene matrices and terrain heights. See research/RAILS.md.

Public all-eight-map driving loads 1,875 scene instances without unsupported
meshes, JS or GL errors. Normal title/menu, keyboard/controller-to-Tioga,
Controls and 390px Caribou reload pass without a disc picker. Original train
front lift/landing, 65.4826ft/s upward head-on launch, side wreck/recovery and
moving top/wheel contacts retain the same public results as 9ac7365. Original
sound/wheel effects have their separate verified 9ac evidence below. The incoming
f21 original-game UI merge and its devlog capture are preserved; combined
Chromium/WebKit UI and source/package gates pass. Inspection rail views are test
cameras, not a change to the gameplay camera or a complete PS2 renderer replay.

## Verified train and wheel public checkpoint

`9ac73657ba94045aa319d3eaed6d481162f242fd` is public on Cloudflare, CI
38285135155 success. HTTPS verification matches 29 authored files and 334 served
objects (93 devlog entries, 81 inspected screenshots before the verification
entry). Public train tests pass the same four source-route cases as the host:
a rideable front lift/airborne interval/landing, a 65.48ft/s upward head-on launch,
side-body wreck/recovery and top/drop wheel/body contacts. The bridge binds all 11
source collision-enabled cars through original list order, moving velocities,
sequential scene height queries and pose publication. No scripted front-launch
or unconditional side-wreck rule was introduced. See research/TRAIN_CONTACT.md.

Original dust, turning dirt chunks/spray, water and actual MUD roost pass all six
public desktop/390px gates, pause and context restoration without JS/GL errors.
They use at most two draws and 1,031,664 GPU bytes in the tested scene. Mud detection
retains terrain support and excludes MUD from established AR/AO geometry before
nearest-row generation, as proven by 001e92c0/0011eeb0. Native kernel comparisons
and these checks do not establish PS2 GS/VU, physical-phone or full-game parity.
See research/WHEEL_EFFECTS.md.

Four original rider impact samples now play from actual touch wrecks. Public
isolated PCM, native sampled deltas, no overlap/backlog, cached actor position,
recovery and pause/resume pass. Three original train samples retain 11 spatial
loops, near/far gains, pan/Doppler and context restoration. Public title/menu,
controller-to-Tioga, Controls and 390px Caribou reload pass without a disc picker.

The user asks to deploy these updates before repairing missing ENV08 rails; that
ordering is fulfilled. Read-only census finds all 68 track placements loaded.
Native root setters show that 12 curved meshes carry a saved PSG model translation
which must be replaced by scene placement, rather than applied twice. The separate
scene mesh decoder repair is now publicly verified above; terrain/COL/route
transforms are unchanged. Keep normal ATV/rider bind-pose decoding intact.

## Earlier verified public gameplay checkpoint

`0541085b495fd9e8d0a0dae59ba6c8ea186f49cc`, CI `38264831246` success,
is live on the public Cloudflare app. Original placed cylinders now provide
physical chassis, wheel and active wreck-rider contacts. Native constructor,
tree/contact and dispatch comparisons plus 266 core + 2 WASM tests pass; serial
host and public checks cover the real ENV04 tree contact, boundary
launch/separation/recovery, Nac Nac recovery/driving, clean landing, ENV01–08,
diagnostics, saved one-handed input, touch stunts and dev-log filters. HTTPS
verification matches 26 authored files and 323 runtime/gallery objects, with 79
entries and 70 inspected screenshots before the public verification note.

The public tree hit is identical to host step 484: one chassis callback/contact,
five proxy queries and no ordinary candidates. Public boundary recovery covers
1,733 frames, 559 boundary/cached-rider frames, three rider tree-hit frames and
60 flag-suppressed frames with zero plant expansion. A preceding steered public
attempt stalled near a hill; a fresh run passes. The temporary lean-assist test
branch never sent lean input and was removed. Host and public routes are not
identical native gameplay replays. Public landing measures 0.637466ft tire overlap
across 210 riding frames/no skips, versus host 0.393066ft/201; different inputs and
timing prevent causal improvement/regression claims. Tire overlap remains open.

The stranded Riding pose is below the source angle limits, and native chassis
support is permitted. A later native phase audit confirms state0 has no independent
animated rider physics/query; the normal AR chassis/classifier path handles riding
contacts. Do not add a guessed rider query. Replay the complete
classifier/contact inputs before changing automatic recovery. Do not invent an
upright snap or assume wheel contact absence mandates ejection.

The user has promoted visible water and the working Arlington Stream train
(ENV08), and requested more informative one-finger touch feedback. These are
parallel feature lanes, now delivered as the public checkpoints above: water uses original WaterFile,
signatures and material resources; train uses original procedural/VUE routes and
models; mobile feedback preserves existing controls. Feature agents may commit,
push and complete their own tested checkpoints without waiting for every lane.
Coordinate only shared-file staging and current-main publication, keep required
checks and lighter-agent dev-log upkeep, and never publish WIP. Remaining original
priorities—wreck/recovery, stunt fidelity, rider/ATV animation, HUD/modes,
terrain/static collision, terrain LOD and vegetation—remain incomplete.

## Vegetation host gate, subsequently released as 0541085

The authored simulation now expands the original ESB cylinders for chassis,
wheels and the active wreck rider. All eight Freestyle ecosystems contain only
kind-1 cylinder records: 40 definitions and 94,992 placed proxy slots. Original
sixteen-sided open meshes, tree ordering, packed translation, mixed candidate
order and separate supplied/cached-rider gates are preserved. No new runtime
resource is needed. Synthetic contacts verify chassis velocity rejection and
attached rider response; the full gate passes 266 core + 2 WASM tests, 39 JS,
strict formatting/Clippy, setup, 10 release/4 Worker checks and hygiene. Native
component comparisons and their hooks are documented in
[vegetation evidence](research/VEGETATION_COLLISION.md).

The serial host gate passes original ENV01–08 loading/driving, failed Nac Nac
separation/ground/recovery/driving, clean landing, desktop/portrait diagnostics,
saved one-handed controls and touch stunts. A dedicated ENV04 normal-throttle
route has one tree chassis callback/contact at step 484. The normal-keyboard
steered ENV04 boundary route passes launch, paired query flags, rider separation
and inward automatic recovery across 2,126 frames. It records 24 chassis, one
wheel and eight active wreck-rider frames with plant callbacks; 553 frames are
boundary flight and 710 use the cached rider query. Full-batch clean landing
still measures 0.393066ft maximum tire overlap across 201 riding frames, none
skipped. This is not a clipping fix.

The historical straight-throttle ENV04 boundary route now times out after
repeated wreck/deflection; ENV03 also times out after one recovery with a tilted
Riding ATV and no grounded wheels. A first steering-harness attempt reversed
the correction sign; the corrected route passes. Four maps passed before an
ENV05 startup timeout without browser errors on the five-slot host HTTP backlog;
a fresh uninterrupted ENV01–08 run on a 128-slot backlog passes. The latter
also serves 48 simultaneous authored-module requests while private paths remain
404; the prior startup cause is not conclusively isolated. This component is
host-verified and pending public release. Existing mobile controls through
`e6878eb` are retained. Normal riding rider collision, exceptional masks, dynamic
objects, unshipped proxy kinds and full replay remain open. Priority 1 next is
the stranded tilted recovery case and wet-wreck coverage; priority 2 then
addresses original wreck policy and controlled rotation/stunt behavior. No
additional scored flip was established by the read-only source audit.

Preceding verified public gameplay component: `f6353c37684063777cd5d0cff1d87e99a0de615f`,
CI `38212608120` success. The active wreck rider now borrows the Vehicle's
shared scene cache, using its animated thirteen-spoke bounds and inclusive/current
eligibility rules. The wreck camera uses its independent original partition query
and retains the obstruction latch. Native probes match 500 rider spheres,
12 paired boundary flag cases and 120 oblique camera queries; original-content
probes cover 1,694 poses and five recovery directions with an empty scene.
The combined build passes 251 core + 2 WASM tests, strict Clippy/formatting,
39 JS tests, 10 release/4 Worker checks and setup/hygiene. Serial host checks pass
clean landing, failed Nac Nac separation/ground/recovery/driving, paired boundary
flags and recovery, all eight maps, diagnostics and saved one-handed controls.
Landing overlap remains 0.393066ft across 201 riding frames with none skipped.
Three concurrent host startup runs timed out without browser errors; a fresh
single run and the subsequent serial suite pass, but concurrency is unverified.
Public HTTPS verification matches 26 app files and 319 runtime/gallery objects
(74 entries, 66 screenshots before its verification entry). The serial public
suite passes the same routes, including all eight maps, with no browser/GL errors.
The Nac Nac trace covers 195 cached rider frames and 195 direct camera queries;
the boundary trace covers 490 rider refreshes and retained plants. No physical
rider-prop contact or camera latch occurs in these public routes; those response
checks use synthetic core geometry. Public landing overlap is also 0.393066ft
over 201 riding frames with none skipped. Normal riding rider collision, MUD
metadata, custom masks, physical plant proxies, dynamic objects and full original
replay parity remain incomplete. See [scene integration](research/SCENE_INTEGRATION.md)
and [camera](research/CAMERA.md).

Preceding verified public gameplay gate: `f27869c84b1ea09a8425de50b6bdf50935d2ba54`,
CI `38210131335` success. Source-verified spring torque now preserves the
Model inverse-transform order. A single original static/ESB partition and 40/50ft
cache supplies quad wheel/chassis candidates and recovery's plant-clearance gate.
Physical vegetation proxies, exceptional categories/dynamic objects and the
newly recovered rider cache binding remain incomplete. Landing/wreck/slope tests
now record every delivered WASM batch step. The isolated combined build includes
committed mobile controls `179fd92` and passes 245 core + 2 WASM tests, strict
Clippy/formatting, 39 JS tests, release/Worker checks, all eight maps, clean landing,
Nac Nac recovery, uneven turns, boundary launch/recovery, one-handed touch and
desktop/portrait diagnostics. Recorded landing overlap is 0.392776ft across 199
frames; this is not an identical before/after comparison or a clipping fix.
Public HTTPS verification matched 26 authored files and 317 served objects
(72 log entries and 64 screenshots before this verification entry). All eight
maps, clean landing, wreck/recovery, boundary launch/recovery, one-handed mobile,
dev-log filters and desktop/portrait diagnostics passed publicly. Full-batch
public landing capture measured 0.392639ft overlap across 184 riding frames with
none skipped. This is not an identical before/after comparison or a clipping fix.
Rider shared-cache integration followed in `f6353c3` above. See
[coupled landing](research/COUPLED_LANDING.md) and
[scene integration](research/SCENE_INTEGRATION.md).

Verified public continuation `01c9cbb0c8358ba43f589dab1e342c3f746f0a1b`, CI
`38207020177`: terrain normals use the original single-round reciprocal
square root; detached rider water velocity retains the previous integration cache,
including across recovery and another wreck; wheelie accounting/HUD callbacks now
run in the live WASM sequence with the original scoring flag off. Native checks
cover 1,500 terrain queries, 300 water kernels, 64 wreck-cache lifecycle cases and
2,070 wheelie operations. Shared spatial/vegetation loader contracts are documented;
their helpers still do not supply live vegetation contacts.

That combined build passed 233 core + 2 WASM tests, strict Clippy/formatting,
33 JS tests, 10 release and 4 Worker tests, setup/hygiene checks, clean landing,
failed Nac Nac separation/contact/recovery/resumed driving, desktop/portrait F3 and
Pause diagnostics, immediate touch throttle/brake/reverse and one uninterrupted
ENV01–ENV08 load/drive run. Landing tire overlap remains 0.497238ft across 62
riding frames in its host sample. Public HTTPS verification matched 26 authored
files and 313 served objects; public landing, wreck/recovery, touch and one
uninterrupted eight-map run passed. The original RAF clearance samples were
incomplete and are not comparable; later full-batch capture is the measurement
baseline. This is not a clipping fix or complete original gameplay parity. See
[terrain contacts](research/TERRAIN_CONTACT.md), [water](research/WATER.md) and
[HUD](research/HUD.md).

Latest public handling release: `35d328ef61c88619f35291e13fc3193ed9736e15` (CI `38205280162`, success) passes public clean landing, keyboard slope turns, wreck/recovery and immediate touch input. HTTPS matched 26 authored files and 312 served objects (66 entries, 59 screenshots before this verification entry). The public slope route has 148 riding frames, zero wreck frames and 13.915ft elevation variation; this is not an identical before/after comparison. Public geometric clearance is pending; residual clipping and full handling parity remain unclaimed. Boundary and uninterrupted all-eight-map evidence remains `22ab240` below, with those paths unchanged. Terrain contact and rider-water cache audits continue.

Latest public boundary release: `22ab240cce0cea813b482ee20dbc22d5908ec69c` (CI `38204399428`, success). The public ENV04 keyboard route passed inward/upward launch, rider separation, full-ray recovery acceptance and cleared boundary state. HTTPS matched 26 authored files and 310 served objects (63 entries, 57 screenshots before the new verification entry). This release passed one uninterrupted public ENV01–ENV08 loading/driving run without JavaScript/WebGL errors. Wet ENV04/05/07/08 load all four live dynamic channels with finite observed heights; dry maps retain null fields and the -256 sentinel. Water-height connection does not imply rendered water/wakes or full rider-water parity. Callback cadence, vegetation collision, RNG ordering and residual tire overlap remain open.

The public checkpoint restores the original four-point suspension travel-stop actor, missing from the coupled vehicle simulation. User screenshots showed deep burial on landing and cross-slope turns. The original stop removes inward center velocity and force before wheel springs, including during wrecks; it does not snap position upward. Exact instruction probes cover constructor points, slope contacts and actor gates. Public clean-landing measurement reduced deepest visible tire overlap from 1.9423 to 0.4792 feet; residual overlap is still present and full coupled parity remains unproven. Synthetic flat/slope tests, browser clean landing and wreck/recovery pass. A separate host steering trace crosses 14.29 feet of elevation with at most 0.3196 feet of measured riding tire overlap, then hits scenery; this is not an isolated incline-parity test. The wreck camera implements the original ten-foot obstruction ray and activation-long position latch. At that public suspension checkpoint, water heights and spatial candidate ordering were verified components pending live integration.

Verified public follow-up: F3 cycles Off/Performance/Debug, mirrored by a persisted Pause → Stats control. The overlay reports measured render/WASM timing and existing physics diagnostics; browser tests verify keyboard repeat, persistence and portrait layout. Mobile right-side press now requests throttle immediately, with backward drag braking/reversing through the existing Rust direction gate. A source-verified suspension fix uses the Model forward axis for the steep-angle force gate. Clean landing, geometric clearance (0.4971-foot residual), wreck recovery and the expanded touch route pass. The original boundary launch is located and being integrated separately; it is not yet live. See [diagnostic measurement limits](research/DIAGNOSTICS.md).

Current gameplay checkpoint: the user prioritizes wreck/recovery, tricks, ATV/rider animation, HUD, terrain/static collisions, terrain LOD/detail and original vegetation, in that order. The user's rail/vertical-ATV screenshot prompted a coupled movement audit before further visuals. Original wheel rays now support static props; grip/controller inputs keep native outer/substep cadence; engine/drag receive total speed separately from local-forward speed; physical tire axes use the preceding mechanical pose; upright chassis terrain contacts use the native body-up gate. Forced hard-landing controls and the original reverse-chord speed gate are connected. See [movement evidence and remaining gaps](research/MOVEMENT.md).

This checkpoint also connects native stunt scoring and repeat rollback, original HUD fonts/controllers, hard-landing clips, detached rider static contacts/root motion and the two-subject wreck camera. Host browser tests pass a clean preload takeoff/landing and a failed Nac Nac → separation → rider terrain contact → automatic recovery → driving sequence, with the failed score unawarded. Component instruction probes and these browser tests do not prove whole-game or hardware parity. Water force kernels and dormant hand-IK behavior have isolated verification; complete water integration, object camera clearance, dynamic collision, full racing/HUD modes, terrain LOD/visibility and vegetation parity remain unfinished. Runtime assets, native probes and screenshots remain outside Git/CI. The app is authored Rust/WASM, not PS2 emulation.

## Current scope

The setup-only session is historical. On 2026-10-11 the user authorized implementation of ATV Offroad Fury 2 in Rust for browser play. Preserve the original as the baseline, including music and sound; follow with optional modern graphics, genuine 16:9 presentation, animations, and more realistic physics retaining arcade accessibility.

All authored UI should evoke the PlayStation 2 era. Support browser-exposed controllers plus intuitive iOS touch driving/trick gestures. The user explicitly selected Reverse Engineer Anything (REA) as an analysis tool, with CLI versus MCP left to engineering judgment. The latest steering prioritizes the actual fullscreen game and reverse engineering over more workbench or website presentation. See [strategy](research/IMPLEMENTATION.md) and [resources](research/RESOURCES.md).

## Current result

A Rust workspace and fullscreen browser driving reconstruction exist. **This is not the full game or a completed decompilation.** The public production app is now on Cloudflare Workers, with original runtime resources and approved screenshots served separately through its R2 binding. Its Play landing enters the recovered title → Main Menu → Single Player → Freestyle route. ENV01/02/03/05/06/07/08 are public, including Caribou Peak, Tioga Valley and Arlington. The public frontend test passed controller Play, title/menu routes, Tioga driving, pause/Controls and 390px Caribou touch input. The public seven-map test passed loading and movement with terrain detail and vegetation, with no JavaScript or WebGL errors. All 281 initial public runtime/manifest/PNG objects passed size, SHA-256 and MIME checks, and all 21 songs remain in the manifest. The initial Cloudflare release identifies main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`.

The ATV's 27 mechanical nodes, recovered vehicle routines, static type-0 obstacle contacts and single-player chase camera run in authored Rust/WASM. Normal rider pose blends and selected original stunt gestures are connected; full scoring/chaining, crash integration, IK, racing and progression remain unfinished. Tioga's `ruins.col` contains four NaN plane records for zero-area faces; only those faces are omitted while original indices/tree order are preserved. Terrain detail uses the original `_d.pst` layer, and vegetation visibility is provisional. Most original menu callbacks/modes, race HUD, remaining maps and complete PS2 rendering/audio fidelity are still missing. The previous Sites v8 is a historical two-map release. Development inspection remains separate at `/workbench.html`.

| Capability | Evidence and limits |
| --- | --- |
| RES/LSI directory and payload recovery | 487 archives, 58,598 member entries, 91,522 zlib blocks; zero archive failures |
| VAG preview decoding | 192 standalone files (162 mono, 30 stereo), 798 packed entries decoded; SPU2 mixer/resampler/reverb and game scheduling not implemented |
| PST preview decoding | All 934 loading archive PST entries and 13 ATV01 material textures decoded; indexed 4/8-bit mip chains validated |
| Fullscreen renderer | Public Cloudflare: title/menu/Freestyle route, seven environments ENV01/02/03/05/06/07/08; animated 27-node ATV linkage and 18-node normal rider pose blends; recovered single-player follow camera; static type-0 collision query; recovered-art pause panel and revised touch controls. Other original menu callbacks/modes, race HUD and additional maps are not implemented. |
| ELF reconnaissance | 5,599 direct-call target candidates; mixed code/data means these are not verified function identities or boundaries |
| Browser integration | Actual WASM, image pixels, audio playback clock, malformed input, private path isolation, synthetic gamepad, keyboard/focus, narrow layout, simultaneous touch/cancellation, camera cycling, and graphics-context recovery tested |
| Real content in browser | Initial Cloudflare release: main `ad605e1b589720bd9b093ae84c25a753ea65a0fc`, CI `38183457495`; public seven-map, frontend and 281-object HTTPS verification passed. |
| Original fidelity | Component instruction-interpreter comparisons cover selected vehicle, camera and collision routines; no PS2 hardware validation or whole-game replay equivalence yet |

The numerical audit covers entries, not unique assets. Binary decoding and an advancing audio clock do not prove visual/audio fidelity. A stereo reference comparison against FFmpeg differs by up to 70/68 PCM units (left/right); it is not a bit-exact pass. See [format notes](research/FORMATS.md).

## Project and access

- Project: `/Users/nrgapple/Projects/atv-offroad-fury-2-web` on MML (`ssh mac-mini`, user alias `mml`).
- Host: Apple Silicon arm64, macOS 27.0; private GitHub remote https://github.com/nrgapple/atv-offroad-fury-2-web with main CI checks and WASM builds.
- Applicable parent instructions: `/Users/nrgapple/Projects/AGENTS.md`.
- Original download was preserved at `/Users/adamgeiger/Downloads/ATV Offroad Fury 2 (USA) (v2.00).zip` during setup.
- Public HTTPS production: `https://atv-offroad-fury-2.champtoday.workers.dev/`; screenshot log at `/devlog.html`. GitHub main checks/builds deploy authored application assets. A dedicated R2 bucket holds only validated runtime content and approved screenshots, uploaded separately from the private host at completed checkpoints. The publication heartbeat is retired; releases follow `research/CI_CHECKPOINTS.md`. See `research/DEPLOYMENT.md` for current status and release evidence. The ISO, BIOS, raw boot executable, unrelated disc data and reverse-engineering output remain private.
- `make serve` is a foreground development process, not a persistent service. The server serves an explicit static-file allowlist, never `private/`, `references/`, or the repository root.

## Exact source build

| Item | Observed value |
| --- | --- |
| Disc boot file | `SCUS_972.11` |
| `SYSTEM.CNF` version / video mode | `2.00` / `NTSC` |
| ISO bytes | `4,487,446,528` |
| ELF bytes | `3,543,492` |
| Architecture | ELF32 little-endian MIPS |
| Entry point | `0x00100008` |
| Named symbols/functions | `0`; symbol/string table sections are empty |
| Disc files | `742` |
| Main load segment | file offset `128`, guest address `0x00100000`, file bytes `3,542,912`, memory bytes `4,348,160` |

ISO SHA-256: `fb710837bbb1852f53706df6086476b507b63cc9e522777bf91446ef04cf708c`

ELF SHA-256: `11b80faea827c5550df607d24c8c4f38a9e44cf55e3ecac9e1daeb3aca9d7c49`

Disc inventory: 368 `.RES`, 119 `.LSI`, 192 `.VAG`, 29 `.IRX`, 10 `.PSS`, plus auxiliary files. The original ZIP and ISO remain under `private/source/`. Extract selected files on demand instead of duplicating the entire disc.

## Reproducible commands and evidence

See [README](README.md) for `make verify`, `check`, `web`, `serve`, `audit`, `code-map`, and `browser-check`. Native extraction commands:

```sh
~/.cargo/bin/cargo run --locked --release -p atv-tools -- disc-extract \
  "private/source/ATV Offroad Fury 2 (USA) (v2.00).iso" /MODULES/PADMAN.IRX private/extracted/MODULES/PADMAN.IRX
~/.cargo/bin/cargo run --locked --release -p atv-tools -- list path/to/file.res
~/.cargo/bin/cargo run --locked --release -p atv-tools -- extract path/to/file.res 0 private/extracted/member.bin
~/.cargo/bin/cargo run --locked --release -p atv-tools -- wav path/to/file.vag work/verification/preview.wav
```

Persistent reports (ignored by Git):

- `private/reports/import.json`, `disc-files.json`, `elf.json`: source identity and layout.
- `private/reports/content-audit.json`: all archive members, decode statuses, dimensions, and audio metadata.
- `private/reports/code-candidates.json`: provisional call-site/target reconnaissance.
- `private/reports/audio-oracle.json`: measured FFmpeg comparison differences.

Screenshots and temporary audio comparisons live under `work/verification/`. All bulk build/test outputs use the mounted SSD; preserve the existing symlinks.

## Environment

- Rust `1.99.0`, Cargo at `~/.cargo/bin/cargo`; targets `aarch64-apple-darwin` and `wasm32-unknown-unknown`, rustfmt and Clippy installed.
- Python 3, Git, system Node `v25.8.1`, FFmpeg, Xcode clang, uv, Homebrew, and native Capstone available.
- Bundled Node `v24.19.0`: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node`; use this supported major for REA rather than system Node 25.
- Bundled Playwright module: `/Users/nrgapple/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright`; Chromium installed for browser checks.
- OpenJDK `26.0.2.1` available. REA 6.4.0 CLI rejected the exact ELF's machine-specific MIPS architecture. Direct Ghidra 12.1.4 with Emotion Engine: Reloaded 2.1.38 now imports and decompiles selected routines using `r5900:LE:32:default`; all original load bytes match. See [REA and native analysis evidence](research/REA.md). REA integration remains unresolved; MCP does not fix its processor admission.
- Rust dependencies are `serde`, `serde_json`, and `miniz_oxide`; browser code needs no npm build dependencies. Playwright is only a test dependency.
- PS2Recomp and the first-game reference are research inputs, not project dependencies. Their source has not been incorporated or built as this runtime.

## Next work

The public Cloudflare app and local browser build now load original runtime content automatically through their manifest. `/workbench.html` retains inspection tools. See `research/GEOMETRY.md` for the 26-quad/40-terrain audit and `research/COLLISION.md` for collision coverage. Physics data and exact-build loader evidence are in `private/reports/physics-*.json`; selected pseudocode and instruction bytes are in `private/reports/ghidra/`. Reuse the database through `tools/analyze_ps2.py`; do not repeat imports or mistake observed functions for verified semantics.

Continue from the public Cloudflare Play landing page and title-to-Freestyle route; seven environments are connected. The 59-page UI archives, 120 layouts, fonts, and animation data now feed the route, but most pages and callbacks are not yet implemented; race HUD and other modes remain to be built. Tioga collision loads after a targeted skip of four zero-area faces with NaN planes; see `research/COLLISION.md`. The revised touch controls and one native-gated ramp jump now pass public browser checks. The public v8 landing page keeps only Play and Dev log; Play enters the unchanged game at `/play.html`. ENV03/05/06/07/08 now load and drive locally after fixing unquoted-space parsing, a material UV default and packed `trussbridge.col` signed-16-bit expansion; all seven routes now pass the public Cloudflare regression. The seven-map detail build passed tests with original 64×64 detail textures, moving full-sample patches and batched vegetation. 390px renderer checks passed gamepad, touch cancellation, audio-engine startup and graphics-context restoration; menu navigation passed immediately before the terrain-only shader changes. The ENV01 ecosystem archive lists 37,948 vegetation placements, and the local renderer batches up to 1,500 nearest placements per map into 7–24 draws. The original `_d.pst` RGB+alpha detail layer now blends publicly at recovered world scale with near-pass modulation. Projected-coverage pass selection is still pending; distant WebGL mip averages soften detail, and fog/GS rounding plus vegetation visibility are not yet matched to the original. The terrain detail patch already follows the player; current work extends terrain range from 300 to 864 feet, not all scenery. Next broaden menu/map coverage, recover original spatial-grid obstacle order and remaining dynamic collision types, and extend rider animation with input-driven crouch/look, IK, tricks and crashes, plus races and progression. The single-player follow camera and ATV mechanical linkage now run in the browser; camera clearance against objects, original render-distance policies and whole-game fidelity remain incomplete. Validate each recovered component against original instructions without presenting that result as PS2 hardware or full-game equivalence. REA still needs an explicit R5900 provider profile; direct PS2 Ghidra observations are not REA Evidence.

Keep enhancements behind explicit options after the baseline. The full-screen renderer uses the recovered single-player follow camera and animated ATV mechanical parts, with provisional terrain shading/LOD. Normal riding animation blends are connected in public v5, while crouch/look inputs still use constructor defaults and IK, tricks and crash poses are unfinished. Public v7 adds the recovered title/menu/Freestyle route, two driving scenes, a pause panel, and revised touch controls. The v7 public ramp run demonstrated native preload and airborne wheel contacts; physical iOS behavior and original stunt/scoring systems remain unverified or incomplete. Public v8 serves the minimal Play/Dev log landing page at `/` and routes Play to the unchanged game at `/play.html`. Object-aware camera clearance, moving obstacle collisions, full race systems and original multiplayer/split-screen presentation also remain incomplete. Widescreen framing preserves recovered 4:3 vertical framing while extending the horizontal view as a browser adaptation. The tested synthetic controller and touch paths do not verify physical Bluetooth controllers or iOS Safari. Audio playback is connected, but original game-wide mixer behavior is still unfinished.


Host boundary follow-up: a normal ENV04 keyboard drive now passes edge crossing, inward/upward launch, rider separation and recovery about 1,379 feet inward. The regression requires more than 100 feet of inward return, one launch, query-flag clearing after the strict one-second gate and cleared recovery fields. This is not public yet; checkpoint callback cadence, vegetation collision coverage and full RNG/gameplay parity remain unverified. Host integration gates now pass touch Nac Nac → wreck/separation/ground/recovery/driving, clean preload landing with Debug, and F3/Pause persistence at desktop and portrait sizes; workspace 218 core + 2 WASM tests and strict Clippy pass. Map coverage comes from two runs: ENV01–04 passed before an undiagnosed ENV05 startup timeout; a fresh ENV05–08 run passed. Wet ENV04/05/07/08 load four live dynamic signature channels and finite changing heights; dry maps have a null field and cached -256 sentinel. Visible water surfaces and wakes remain absent, and complete water/rider parity is unclaimed. This build is host-verified and pending public release. See research/BOUNDARY_LAUNCH.md.


Historical host wheel-center checkpoint (subsequently released as `35d328e`): the first wheel velocity/spring substep now retains the preceding cached world center until integration; geometry, torque and the chassis contact solver keep their recovered new-center rules. A 600-case original `001ed330` replay executes 386,315 instructions with zero compared binary32 mismatches, versus 1,813 differing components from the former immediate-center input. Host clean landing, both-direction keyboard cross-slope turning and wreck/recovery pass; 222 core + 2 WASM tests, strict Clippy, formatting and `make verify` pass. Landing minimum clearance is unchanged at -0.4970245ft. Slope clearance is -0.3231964ft before/-0.3015594ft after over 14.457ft elevation, but props and 119 riding/30 wreck frame exclusions prevent isolated slope parity or causal-improvement claims. Three vegetation helper tests are registered but collision is not live. The inspected landing screenshot and evidence are recorded in `research/devlog.json`; no terrain-clipping fix was claimed at this host gate. The subsequent public verification is recorded above. See `research/MOVEMENT.md`.


UI transitions/loading host checkpoint (2026-10-13): authored menu transitions use 180 ms exits and 240 ms entries, with a shared loading presentation for boot, course/model loading, and graphics-context recovery. The course loader presents the recovered preview, course name, status, and animated progress bar. Browser checks cover reduced motion, load failure, rotation, touch at 390×844, 844×390 and 667×375, and resize cancellation with focus retention. The frontend regression also covers desktop/controller Tioga and portrait/Caribou. Validation reported 69 JavaScript tests, five Rust UI tests, formatting, strict atv-core Clippy, `make verify`, repository hygiene, and 29 release tests (one skipped). This remains host-only: these authored timings and visuals do not establish native UI timing/callback behavior, physical iOS/Bluetooth support, or public deployment. Screenshot evidence is in `research/devlog.json`; see `research/UI.md`.

The UI checkpoint also passes the existing Caribou race gate: desktop/portrait/landscape setup, course loading, countdown, pause/resume, touch cancellation and restart, with no JavaScript errors.

## Multiplayer visibility, contacts and train synchronization — host checkpoint

`feat/multiplayer-improvements` adds active-ride late admission, numbered player
markers on the minimap, original CompassArrow pixels/colors with authored overhead
name/distance labels, and Pause → Players → Spawn beside. Fresh grounded peers
are required for spawning; nearby candidates check support, clearance and peers.
Rust owner-local swept contact proxies let quads bump one another. The relay
validates protocol 2 snapshots with host train phase and actual wheel-supported
platform transforms; presentation preserves rider position relative to each car.
Guests follow the host's train clock, which freezes while the host is paused.
This is an authored multiplayer approximation without server physics reconciliation.

Host tests pass actual late joining after Arlington's train has advanced, matching
lobby code, minimap/overhead visibility, mobile roster/spawn, owner contacts and
20 moving-flatbed samples (maximum relative presentation error 0.80435 feet).
The merged-main projection gate confirms remote markers follow the rider's world
pose with the minimap's -X handedness and zoom. Quad 1 on ENV02 and quad 26 on
ENV01 passed 41 and 50 moving samples plus seven tracking samples each, with no
projection errors. Four-browser tests pass quads 1/14/26/2, 39 remote GL draws
and three voices per client, movement to every observer, and host-end cleanup.
Thirteen live relay cases, 403 Rust tests, 91 JavaScript tests (102 including
Worker tests), strict Clippy, setup/hygiene, and 34 release tests (one skipped)
pass against the merge tree based on `origin/main` 106e937. Public delivery
remains pending the validated current-main artifact and separate R2 gallery
verification.
See `research/MULTIPLAYER.md`; physical phones and geographically separated
network behavior remain untested. Original PS2 multiplayer fidelity is not claimed.

Latest multiplayer follow-up: separate Create/Join forms and optional custom
eight-character codes pass the actual browser/relay gates. Labels are 22px white
with black outline, no background. Final host run: 404 Rust/91 JS tests, 14 relay
cases, four-client replication/cleanup, contacts 4/3 and 20 train samples with
0.933ft maximum relative error. Public delivery is pending.

Host checkpoint (2026-10-13): iOS standalone fullscreen passed Chromium/WebKit
automated 390×844 portrait → 844×390 landscape → portrait sizing with synthetic
safe-area signals; HUD, held-touch driving and Pause/Camera stayed usable in
both orientations. The default Caribou chase view lifts uphill (0.2208329 rad;
render pitch −0.3769404 → −0.0361075 rad) and resets when Camera cycles. Optional
Funny Crashes preserves more launch/tumble/rebound: a severe route passed 1,197
paired enhanced/Off WASM steps with zero gameplay/camera mismatches, three
aerial intact-ATV frames, two damage transitions, seven parts and intact
recovery. The motorcycle touch/crash replay passed 302 paired steps with zero
mismatches over 251 observed frames and mild-crash recovery. Host gates passed
416 Rust, 100 JavaScript, 8 build-cache, 35 release
(one opt-in skip), 19 Worker tests, make verify, formatting, strict Clippy and
WASM build. This does not verify physical iOS or public deployment; the uphill
camera and crash effects are browser adaptations. See [iOS/camera/crash
evidence](research/IOS_CAMERA_CRASH.md).

## Public late-join and multiplayer visibility checkpoint

Main `551d7fd890946f18cc8ef8430c12f843138fe758` passed CI 38446510103 and
public verification on `furyremastered.com` and the Workers app. The validated
artifact and deployment identity match. Full HTTPS verification matches 39
authored/static files and 446 served objects; the R2 package contains 447
approved objects totaling 338,648,873 bytes, with three uploads for the label-v2
host capture, Players capture and gallery.

Public two-client checks preserve custom code FV70MYWD, admit a late rider, render
13 remote draws per client, spawn the guest about 10 feet from the host through a
342×64 Players target, record owner-local contacts 5/3 and track a moving flatbed
within 1.29715 feet across 20 samples. Fourteen relay cases pass, including custom
and duplicate-code handling. The relay WASM SHA-256 is
`8776744065b1b42d6ac9faf13034c0c39d7246202ea3476646400b622b634753`.

Four public browsers using quads 1/14/26/2 each render 39 remote draws and three
engine voices; every rider's movement reaches the other observers, and host end
clears guest rooms, models and audio. No HTTP/page errors occurred. These are
automated browser checks, not physical-phone or geographic-latency evidence.
Contacts remain owner-local without server reconciliation, and the shared train
clock freezes when its host pauses. Racing, authority, migration and full-game
parity remain unfinished. See [multiplayer evidence](research/MULTIPLAYER.md) and
[deployment record](research/DEPLOYMENT.md).

## Multiplayer polish checkpoint (2026-10-13)

Player markers now use smaller 18px outlined DOM labels and authored SVG arrows,
with original slot colors; minimap peers are simple colored dots. Edge labels
are clamped to fit mobile screens. The soundtrack remains active through pause
and ride submenus while gameplay voices mute; resume does not suspend audio.
Owner-local proxy impulses now enter the recovered per-quad directional wreck
classifier. The equal-mass contact proxy is still authored, not recovered PS2
pair collision or server-authoritative reconciliation.

Host gates: make check (407 Rust, 94 JS, 8 cache tests), make verify, release
tests (35 run, one opt-in skip), solo and multiplayer pause, and two-client
late join/recovery/reload/train/contact tests. Both owners entered Front wrecks
in a head-on impact and both observers received Wreck state/new crash serial.
Solo pause/settings/resume and host/guest pause preserve song playback with zero
AudioContext suspends. Inspected mobile image is recorded in the dev log.

A limited live production capture saw a 1006 host socket close after 270145ms
message silence and no server-requested lobby closure. Historical telemetry
retrieval was denied by the existing OAuth credential; cause remains unproven.
Sanitized evidence: private/reports/multiplayer-polish-log-review-2026-10-13.json.
Public delivery must follow passing main CI and the normal R2/verification gate.
