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
No file-selection step or page errors occur. See [URL contract](PLAY_LINKS.md).

The inspected public screenshot records this deployed commit. Its metadata and
PNG follow the next validated metadata checkpoint's CI/R2 publication. These
are automated browser observations on one Mac with synthetic focus, not physical
phone, geographic-latency or full original-game equivalence. Multiplayer remains
ATV Freeride; Race remains Caribou National; sound/fullscreen need browser gestures.

# Private source repository and public Cloudflare hosting

## Change-aware delivery (2026-10-14)

The active flow is [CI_CHECKPOINTS.md](CI_CHECKPOINTS.md). Documentation skips
application builds/deploys. Dev-log-only publications reuse validated deployed
application bytes and record their `application_commit`; unknown/runtime changes
retain full checks. PR updates cancel superseded builds, main remains serialized,
and trusted main saves Rust build/registry caches. One named owner handles each
combined R2/gallery publication, pinned to a successful **live** release even if
unrelated docs advance main. Milestone receipts separate implementation-ready,
merged, live and gallery-complete; no daemon or No Mistakes cutover is
involved (the experimental release queue was removed). Older release descriptions below describe historical runs.

## Authored asset release keys (2026-10-13)

The minimap regression reproduces old CSS and HUD code remaining in a returning
browser while a new module draws its speed footer. `package_release.py` now
adds the release commit query to local CSS/module/Worker/WASM references,
including transitive module imports. The checksum-verified artifact keeps its
stable allowlisted filenames; original resource URLs and their long-lived
content-addressed caching remain unchanged. Browser cache keys differ by release
without adding requests or work per riding frame. The warm-cache regression
retains old responses and proves the new document loads a consistent module
graph without clearing the browser cache.

## Fury Remastered domain (2026-10-13)

The user registered `furyremastered.com` in the existing Cloudflare account.
Its active zone is `ea0921ec471b2a7de78838cdbaeccb78`. The apex and `www` attach
to the existing `atv-offroad-fury-2` Worker; Cloudflare manages DNS and TLS.
Always Use HTTPS is enabled. The authored Worker redirects `www` to the apex
with HTTP 308, preserving paths and invite queries. The original workers.dev
address remains available. Existing browser saves stay on their original origin.

`wrangler.jsonc` records both provisioned custom-domain routes. CI stages with
`--preserve-custom-domains`, leaving the existing Cloudflare DNS/TLS bindings
attached while updating the Worker code. The deployment credential remains
scoped to the game Worker. `GAME_ORIGIN` and
`CANONICAL_ORIGIN` use `https://furyremastered.com`; `GAME_ORIGINS` explicitly
allows that origin and the existing workers.dev origin in both the JavaScript
gateway and Rust relay. Missing, opaque, HTTP, foreign and lookalike origins remain
rejected. No original content is moved or exposed beyond the existing approved
R2 path allowlist. Main CI deploys and verifies authored files at both addresses.
The first deployment of `277e26f` uploaded the checked application and origin
changes but failed while reapplying custom-domain triggers (Cloudflare error
10002). The bindings remained attached. Omitting their reprovisioning in CI
addresses that deployment failure; the staging regression covers preserved
origins, authored files and untouched source routing configuration. Public
gameplay, multiplayer and full R2 verification evidence follow below.

## Production flow restored (2026-10-13)

GitHub Actions checks/builds and automatically deploys authored application assets.
The validated checkpoint R2 upload remains a separate local release-agent step.
The experimental local Rust queue is stopped and must not replace or block this
production flow until its end-to-end validation passes and the user approves the
cutover. Other chats should follow research/CI_CHECKPOINTS.md.

## Cloudflare migration (2026-10-11)

The user authorized moving production hosting to their existing Cloudflare account.
The authored browser app is now deployed as a dedicated Workers static-assets app:
**public HTTPS** https://atv-offroad-fury-2.champtoday.workers.dev/.
Its dedicated R2 bucket is `atv-offroad-fury-2-content`; the Worker binding serves
only `/content/manifest.json`, SHA-256 named `/content/*.bin` files and approved
`/devlog-images/*.png` paths. R2 itself has no public bucket URL. This application
needs no database. No tax-portal or other shared resources are reused.

Account ID: `92b7e16fd24d5fb585f866f5832a032b`. Wrangler uses the user's existing
local OAuth authentication. The first Cloudflare app deployment uses the successful
current-main CI release at commit `ad605e1b589720bd9b093ae84c25a753ea65a0fc`
(run `38183457495`). Worker version `f1f520a7-f2d1-428d-837d-731935cbf18f`
adds explicit `/` landing routing and gallery HEAD support. Public HTTPS verification passed all 281 served
objects (runtime blobs, manifest and 31 PNG screenshots), checking size, SHA-256 and
MIME type. The public log returned 34 entries with 31 screenshots; deployment
metadata matched the CI commit. The public frontend regression passed the landing
and controller Play flow, original title/menu routes, the seven-map selector, Tioga
driving, pause/Controls and a 390px Caribou touch session with no JavaScript errors.
The public seven-map regression also passed ENV01/02/03/05/06/07/08 loading
and vehicle movement with terrain detail and vegetation, with no JavaScript or
WebGL errors. The prior Sites v8 remains available as a historical release.

The initial separate R2 upload contained 282 approved objects, 261,696,979 bytes.
After committing the Cloudflare checkpoint, incremental publication uploaded only
the new PNG and gallery metadata: 283 objects, 262,140,223 bytes total.
Its manifest describes 50 runtime resources including all 21 music tracks; only
249 referenced content blobs were uploaded, excluding stale private blobs.
`tools/upload_cloudflare_content.py` validates every runtime blob with the existing
manifest allowlist and SHA-256 checks, uploads only referenced PNG checkpoints,
and publishes the manifest/gallery pointers after all blobs succeed. Credentials
remain in memory and are never printed or included in source or CI. Objects and
upload/verification reports stay under ignored private storage. Disc images, BIOS,
raw executable, reverse-engineering output and reference repositories are excluded.

GitHub CI continues to check and package only authored code/WASM and devlog text.
`tools/stage_cloudflare.py` stages that verified artifact for the GitHub Actions production job; production deployment is not a manual local step.
The main-only deployment step uses the scoped `CLOUDFLARE_API_TOKEN` repository
secret, configured on 2026-10-11 after user approval. Its permissions are Individual
Worker Editor for this game Worker and Account Settings Read, with a one-year
expiry. It fails clearly if the secret is absent. The first automatic deployment
passed for main `2b2e6ec901e9825edf0073718b6d4a6b6071d909`, CI run
[`38184194254`](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38184194254),
at 2026-10-11 23:26 UTC. Cloudflare Worker version
`2976a4db-182e-420b-ba58-8d6f76c6da36` serves that commit. Public verification
matched all 23 unchanged authored/static file hashes, the CI devlog metadata,
and all 282 served runtime/manifest/PNG objects. The gallery now contains 35
entries and 32 approved screenshots. This verifies the GitHub push-to-Cloudflare
path. A former scheduled publisher was later retired; R2 content and approved
gallery updates are handled by the release agent at each validated checkpoint.
Original resources and screenshots never enter GitHub or CI; CI attaches the
existing R2 binding without uploading its data.
`/devlog.json` merges screenshot references from the separately uploaded gallery
only when each checkpoint's authored metadata exactly matches the CI revision.
New or edited checkpoint screenshots require another authorized local R2 upload.
The uploader reads checkpoint metadata from the validated current-main commit,
rejects other revisions and skips byte-identical objects recorded by its previous
local upload report. `--force-upload` repairs remote drift; public hash verification
remains required after each content/gallery update.

## Per-checkpoint R2 and gallery publication

GitHub Actions deploys authored application assets after passing main checks. The
release agent does not run a separate local Worker deployment. At each completed
checkpoint, fetch the validated current-main CI artifact without a Site URL
(the `--site-url` option is not used for the Cloudflare gallery workflow):

```sh
python3 tools/fetch_site_release.py
```

Wait for a ready release, then confirm the public `/deployment.json` identifies
its exact commit. Inspect newly approved screenshot pixels and ensure their image
references are in committed `research/devlog/entries/*.json` (and the historical
`research/devlog.json`); release tools aggregate both from the validated commit. Image files remain in
ignored `private/reports/devlog-images/`. After confirming the validated R2 content
package and screenshots, publish them and verify the public result:

```sh
python3 tools/upload_cloudflare_content.py --release <validated-release> --commit <commit>
python3 tools/verify_cloudflare.py --url https://atv-offroad-fury-2.champtoday.workers.dev --commit <commit>
```

The uploader accepts only a validated current-main release and committed gallery
metadata, uploads approved runtime resources and screenshots to the separate R2
bucket, and publishes the manifest/gallery pointers after blobs succeed. The
verifier checks deployment identity, authored/static hashes and every served R2
object's size, SHA-256 and MIME type, plus gallery references. Run relevant public
browser checks after the upload as well; hashes alone do not prove game startup or
behavior. Never publish uncommitted work or uninspected images. The R2 Worker serves
GET/HEAD only; content-addressed blobs use immutable browser caching and
manifests/gallery use revalidation. Do not
claim a Cloudflare CDN edge-cache hit on the workers.dev hostname: the Workers Cache
API is not effective there ([Cloudflare R2 cache documentation](https://developers.cloudflare.com/r2/examples/cache-api/)).
Static application assets use Workers' managed asset storage.

## Eight-environment release (2026-10-11)

Main `e19ec6b4c7f515e6ea3ed7c7aa97810357109f13` deployed automatically via
successful CI `38184851978`. Incremental R2 publication uploaded five changed
objects: Freeport FR/ENDL resources, the approved screenshot, manifest and gallery.
The bucket now has 286 allowlisted objects, 267,330,103 bytes, including its internal
gallery metadata. Public verification matched 23 authored/static hashes, committed
devlog metadata and all 285 served runtime/manifest/PNG objects. The gallery has
36 entries and 33 screenshots; all 21 songs remain available.

The public browser passed all eight ENV01–ENV08 map loads and driving checks,
plus the original menu/controller selector, pause/Controls and 390px touch flow,
without a disc picker or JavaScript/WebGL errors. Freeport reports 356 scenery
instances, 13,816 ecosystem placements in nine batches and no unsupported meshes.
The Arlington environment is available; its animated train remains unfinished.

## Settings, soundtrack and menu thumbnail release (2026-10-12)

Main `0906822adbc585f58e4a7309322f63f4cc3f7b6a` passed GitHub CI
[`38194430965`](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38194430965)
and deployed to the public Worker. Verification matched all 24 authored/static
hashes and all 298 served runtime, manifest and screenshot objects by size,
SHA-256 and MIME. The public gallery contains 50 entries and 45 PNG screenshots.

The public browser verified keyboard and controller binding capture for keys,
buttons and signed axes, actual-WASM driving, persistence after reload, reverse
chords, stunt axes, once-only commands, cancellation on blur/disconnect and the
Resume→Escape path. The 33-step stale-preload cancellation regression passed.
All 21 songs decoded and played through WASM with reads bounded at 65,536 bytes
and zero underruns; paused preview, music off/on and focus handling passed. The
frontend route covered title/menu navigation, keyboard/controller input, Tioga
driving, pause and a 390px Caribou reload. The Main Menu includes the recovered
256×128 ATV thumbnail. These browser checks do not establish physical controller
or iOS compatibility, listening fidelity, full menu-art restoration, full flips
or whole-game parity.

## Coupled handling, wrecks, stunt scoring and HUD (2026-10-12)

Gameplay commit `91b60c4849c6f52299a623caa6c224daa275598c` passed CI
`38197809475` and deployed. The following CI visibility commit
`6d31df367fa5a8f5165541a16df12889b3c4c91c` passed
[`38197954580`](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38197954580)
with identical gameplay code. Public HTTPS checks passed no-disc startup,
all eight map loading/driving routes, a clean preload jump/landing, and a failed
Nac Nac → detached rider contact → automatic recovery → resumed driving. The
failed stunt displays its pending score without awarding it. No JS/GL errors
were observed. Host tests also passed remapped controls and cancellation.

The public verifier matched all 25 authored/static hashes, committed log text and
301 served runtime/gallery objects by size, hash and MIME. There were 53 entries
and 48 screenshots before adding the public verification entry. The separate R2
package contained 302 approved objects, 272,296,009 bytes; four objects changed.
An expired host OAuth token initially blocked screenshot upload; Wrangler refreshed
the existing session and the retry succeeded. No new credential or exposure path
was introduced. The original content remains separate from GitHub and CI.

This checkpoint corrects wheel support on static props, speed/controller cadence,
animated axle feedback and several crash/control gates. It does not establish
complete original handling, race systems, dynamic collisions, water behavior,
camera obstruction, LOD or vegetation parity. See [movement](MOVEMENT.md),
[wrecks](WRECK.md), [animation](ANIMATION.md) and [HUD](HUD.md) for evidence and limits.

## Historical Sites hosting


Source repository: https://github.com/nrgapple/atv-offroad-fury-2-web (private).
The user selected **public HTTPS Sites hosting**, matching the Game Boy project.
Site project ID: `appgprj_6acbf33be01c81919ba8edc04fbcc3d0`.
Live public URL: https://atv-offroad-fury-2.nrgapple.chatgpt.site/.
Tailscale is a development verification path, not the production handoff.
The proposed Mac deployment supervisor was removed before it was ever started.

## GitHub build pipeline

Every push to `main` runs `.github/workflows/ci.yml` on a GitHub-hosted Ubuntu runner.
Pull requests receive the same checks. The workflow checks tracked-file hygiene,
Rust formatting/tests/Clippy, input tests, Python syntax, and release rejection tests.
It builds the browser WASM and uploads a `web-release` artifact with an explicit
static-file allowlist, complete commit metadata, and SHA-256 checksums.

The GitHub repository and CI artifact contain no disc, BIOS, extracted game content,
reference checkout, generated game pseudocode, or screenshots. The user's explicit
authorization for original content hosting applies to the separate public Sites
asset manifest and its content-addressed runtime files; it does not allow any of
those assets to enter GitHub source or CI. Keep the ISO, BIOS, raw boot executable,
unrelated disc files, and reverse-engineering output private. CI's development log
contains authored text; screenshot references are omitted because screenshots stay
outside GitHub. An authorized Sites gallery and runtime-content package are kept in
the Site's separate hosting checkout. `make verify` remains a host check because it
requires the local private disc and reference checkouts. Artifacts are retained for
14 days.

## Sites publication

Public Sites publishing uses the native Sites tools with a fresh source-write
credential kept only in session memory. The Sites source helper pushes the exact
hosting source and packages the static archive, then native tools save and deploy
that version. The Site's source commit is separate from the GitHub source
commit; `/deployment.json` in the built artifact identifies the latter.

The current Sites integration does not expose a durable public GitHub push-to-deploy
hook or a reusable public deployment credential. Its optional publish-on-push
credential applies only to temporary owner-private publication windows. Do not
store these short-lived credentials in GitHub secrets or an on-disk deploy service.

## Authorized runtime-content hosting

The user explicitly authorized extracting the runtime resources required by the
browser build and serving them from the existing public Site so the game can
autoload without asking visitors to select a disc. This is a Site-only content
package, separate from the GitHub source and CI release. The package is generated
under ignored `private/` storage and transferred to the Site's hosting checkout as
content-addressed files referenced by an asset manifest with size and SHA-256
metadata. The manifest, file allowlist, and each blob must be checked before
publication.

This authorization does not include the ISO/disc image, BIOS, raw boot executable,
unrelated disc files, reverse-engineering output or pseudocode, or reference
repositories. None of the extracted runtime resources, manifest blobs, or private
analysis may be added to GitHub source or CI. The native Sites workflow described
here is historical; current runtime-content and gallery publication uses the
per-checkpoint Cloudflare R2 procedure above. Do not claim autoload is complete
until the public build starts without a file-selection step and requests are
verified against the approved manifest.

The former local 15-minute publisher is retired and is not an active workflow.
GitHub Actions deploys authored application assets; the responsible release agent
updates R2 content and approved screenshots at each validated checkpoint using
the procedure above. Public visitors do not depend on a local heartbeat.

The CI-artifact-to-public-Site path is verified for main commit
`0dc5c09890df6bbf8f942309558ad168b7a8857b`; public `/deployment.json` returns
that SHA. Native Sites deployment `appgdep_6acc01ca684081918ec76849748bc642`
succeeded at 2026-10-11 21:38:33 UTC. Public v5 starts without disc selection and
includes the recovered normal rider-animation connection. The full public renderer
regression confirmed 18 animated rider nodes, responsive node matrices, turn/turn/
reverse modes, and no JavaScript errors. The reverse-pose capture shows the distinct
reverse path in the public build. The separate native Rust animator probe produced
1,200 finite poses; that probe was not a browser test.

Public v6 is now deployed at
`https://atv-offroad-fury-2.nrgapple.chatgpt.site/`. The exact CI source commit is
`989214801f4f9a3f33971d3d0c65f91c4f6d816a` (workflow run `38178360227`); its Sites
source commit is `5f2ab1052ff251d2db925ab8286ff156e5ae4328`, app version
`appgver_6279dc34d99c81918ce8d3d609794c73`, and successful deployment
`appgdep_6acc06f11fb881919d5664a2d9472514` at 2026-10-11 22:00:44 UTC. Public
`/deployment.json` was checked against the exact CI commit. The public frontend test
passed the title → menu → Freestyle → Tioga driving route, controller input, Caribou
selection from a 390-pixel touch viewport, pause behavior, and zero JavaScript
errors. The log served 23 entries and 21 PNG screenshots; every image returned
HTTP 200 with `image/png`. V6 includes both connected maps and the recovered menu
route. It does not include the newer local pause overlay or the revised hold/lean/
release touch surface described in [touch riding controls](TOUCH.md); those changes
are pending a later release.

Sites accepted and deployed v7 from source commit
`3a22ed545546c4b25e02df78d83800a66f50b8d1`, app version
`appgver_940dd3fd318c8191a27ddf8a8be8d5f6`, at
`appgdep_6acc0b368b988191b46fdc745f8c3f0e` on 2026-10-11 22:18:43 UTC. Its CI
source commit is `114e074d7b12045a02a56aac8a64be515fa54e1f` (workflow run
`38179702052`). Fresh public verification has now completed:

- Frontend test run `2919` passed original menu routes through Caribou and Tioga,
  plus pause, Resume, Controls, and a 390px layout.
- Touch test run `24045` passed right-half hold/lean/release, cancellation,
  simultaneous input, brake/reverse/forward transitions, pause/focus, and game-only
  selection suppression while leaving the development log selectable.
- Ramp test run `67228` activated native preload and cleared all four wheel contacts
  without injecting state or force. The observed upward velocity changed from 9.29
  feet/second at arming to 25.26 during the active interval. This is one trajectory,
  not a controlled comparison isolating preload from the ramp and other forces.
- Gallery check `98683` confirmed `/deployment.json` identifies commit
  `114e074d7b12045a02a56aac8a64be515fa54e1f`; all 26 entries and 24 PNGs loaded,
  with every screenshot returning HTTP 200 and `image/png`.

The pause-panel screenshot was visually inspected. A separate public ramp screenshot
shows the airborne scene after the marker had expired; it does not show the transient
preload indicator. The local test capture illustrates that indicator. This release
does not establish physical iOS behavior or full original stunt/scoring support.

The local v8 candidate adds a brief `/` landing page with only Play and Dev log; its
plain text explains that the game is rebuilt in Rust rather than emulated. Play
opens the unchanged game at `/play.html`, and the Dev log's Play link returns there.
A small script supports controller navigation between the two links and gamepad A
to activate one. Before Play, the landing loads no WASM, game runtime, or original
content. Final frontend integration test `8826` passed D-pad navigation and gamepad A
activation, landing → Play → title/menu → Tioga/pause, 390px Caribou selection,
and landing → Dev log → Play → title. The landing fits the 390px viewport.
Those test results are the pre-publication host checkpoint; the later public v8
verification is recorded below.

Public v8 is deployed at the Site root. GitHub main commit
`b1e815162c9cc769091ee5933f2e49e4823a5e24` passed CI run `38180312529`; a live
request to `/deployment.json` returned the same commit. Native Sites deployment
succeeded at 2026-10-11 22:28:28 UTC. Public frontend integration passed, and the
public gallery served 28 entries with 26 PNG screenshots, each returning HTTP 200.
The inspected desktop and 390px mobile captures show the short Play/Dev log landing;
Play opens the game at `/play.html`. The small controller-navigation script remains
on the landing page, and the game runtime/content load only after Play. This proves
the landing route and gallery are live; it does not expand the two playable
settlements or imply that the rest of the game is complete.

A local checkpoint added original ecosystem placements to ENV01, ENV02, ENV03,
ENV05, ENV06 and ENV07. The corresponding original ESB placement lists contain 37,948, 39,641, 26,530,
14,883, 38,968 and 6,617 records. Local browser runs entered and drove each map
without JavaScript or WebGL errors. The renderer selects up to 1,500 nearest
placements per scene and batches them into 7–24 draws; this visibility rule is
provisional and is not the PS2 level-of-detail policy. The added ENV03/05/06/07 routes
were local-only and have not been published. At that checkpoint ENV08 was blocked by its packed
`trussbridge.col` collision resource; a later decoder recovery is documented below. The terrain detail patch follows the player; current range work expands terrain sampling
from 300 to 864 feet, not all scenery. The browser now blends the original `_d.pst`
RGB+alpha detail layer at recovered world scale with near-pass modulation. Original
projected-coverage pass selection is still pending, and distant WebGL mips average away
fine detail; this is not a complete PS2 LOD match.

ENV08 (Arlington) now loads and drives locally after recovering the packed
`trussbridge.col` decoder from the original signed-16-bit expansion path. The local
renderer therefore has seven maps; only ENV01 and ENV02 are in public v8. A fresh
capture shows original vegetation and jump obstacles in Arlington. This does not
establish original terrain LOD or visibility fidelity; projected-coverage pass selection
and vegetation visibility remain under study.

The completed prepublication detail build passed the seven-map regression with
original 64×64 detail textures, moving full-sample terrain patches and batched
vegetation. The 390px renderer checks passed gamepad input, touch cancellation,
audio-engine startup and graphics-context restoration. Menu navigation passed
immediately before the terrain-only shader changes. These are local checks; no public
verification is claimed for this build. Original projected-coverage selection,
distant mip behavior, fog, geometry rounding and exact vegetation visibility remain
unmatched.

A separate local touch capture reached Caribou ramp takeoff without injected vehicle
state or forces and exercised the Nac Nac gesture after rear-wheel contact. The trace
had no JavaScript errors. The Nac Nac leg-crossing animation is visible in the air and its held extension persists
through rear-wheel contact. This does not establish trick scoring, chaining, or a
public-build stunt feature.

The no-disc runtime-content path was previously verified on public v4, commit
`76f276315650e29841cb122095654239a03cbbed`. That fresh-browser run loaded 68 terrain
textures, 64 sky tiles, 174 scene instances, ATV/rider resources, 21 engine samples
and the 21-song catalog from the same-origin Site manifest. It exercised driving,
throttle/brake/reset, camera cycling, controller/focus, touch/cancel and graphics-
context recovery. Music played with zero underruns; diagnostics had no JavaScript/
WebGL errors. Its 390px mobile layout had no horizontal overflow and no disc picker.
Public v5 retained that no-disc path, and v6 continues to autoload without disc
selection. V5's public log served 15 entries and 13
screenshot images; a Playwright browser fetched every image successfully with HTTP
200 and `image/png`. V6's public gallery was separately checked as described above.

Public v5 had only ENV01 (Caribou Peak) and did not contain the original title/menu
navigation or map selection. Public v6 now connects the title-to-Freestyle route and
Caribou Peak plus Tioga Valley (ENV02). Other menu callbacks, game modes, race HUD,
and additional playable tracks remain unfinished; the UI inventory and recovered
assets do not imply that every page or game mode is implemented.

The previous public version on commit
`3c9f7bdc72810c23afc2aa461810e911a998f01c` received devlog-only verification; its
autostart and manifest requests were not tested. The preceding release
`5d160a747e9730c9e07f35afda9bac941c15f062` passed a real-disc browser run for
terrain, sky, props, ATV/rider, engine sample scheduling, chassis, throttle/brake/
reset, controller/focus, touch/cancel and graphics-context recovery. That older
check required selecting the ISO and is not the basis for the no-disc result.

The first local autoload checkpoint was verified before public v4: desktop and
390px mobile captures showed original runtime resources loaded through the manifest,
with no JavaScript/WebGL errors, music underruns or mobile overflow. The deployed
v4 and v5 tests independently verified the no-disc flow publicly.

Historical note: the former scheduled publisher ran once on 2026-10-11 21:12 UTC
and skipped because main was unchanged. V4 through v8 used the native Sites
workflow. The scheduled publisher has since been retired; neither Sites publishing
nor that heartbeat is part of current production operations.

## Release commands

```sh
make check
make web
python3 tools/package_release.py --commit "$(git rev-parse HEAD)"
python3 -m unittest discover -s tools -p 'test_release*.py'
```

The current Cloudflare checkpoint workflow is described above. The earlier
unattended Sites publisher and `stage_site.py` instructions are historical and
should not be used for production. The validated fetcher checks current-main CI
success, artifact hashes/exclusions, and that main did not change during retrieval;
only committed metadata from that exact revision is eligible for R2 publication.

`work/release/static/` is the asset-free CI application output for hosting, and
`work/release/release.json` contains commit and file hashes. The packager validates
the application release before returning. Site-only runtime content is packaged
separately under ignored private storage and does not alter the CI artifact. Do not
change either package after its matching Sites source has been pushed and its
archive prepared.

GitHub's [artifact download documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
and [GitHub CLI documentation](https://cli.github.com/manual/gh_run_download)
describe the authenticated CI artifact retrieval used by an updater.

## Suspension travel stops and obstruction camera (2026-10-12)

Gameplay `c0810ab92bb1529a8558a7edaf83d72a558495b2` passed
[CI 38200640787](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38200640787)
and deployed through the existing production job. Public HTTPS verification
matched all 25 authored/static file hashes and 305 served runtime/gallery objects,
with 57 log entries and 52 inspected screenshots. The R2-only upload contains 306
approved objects including its internal gallery metadata, totaling 273140944 bytes.

Public no-disc Chromium checks passed all eight environments, a real-touch clean
preload landing, and failed Nac Nac → rider separation/contact → automatic recovery
→ resumed driving, with no JavaScript/WebGL errors. Comparing transformed tire
vertices to original terrain in the public landing trace found a minimum clearance
of -0.47921753 feet, improved from the earlier -1.9423218 feet. Residual overlap is
explicitly unfinished. The host steering capture covers changing elevation and
turning, but includes a scenery collision and is not an isolated incline proof.
Native instruction comparisons establish the restored travel-stop actor and wreck
camera components; these browser checks do not prove complete original handling.

## Combined scene candidates and spring torque (2026-10-12)

Gameplay `f27869c84b1ea09a8425de50b6bdf50935d2ba54` passed
[CI 38210131335](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38210131335)
and automatic production deployment. The separately uploaded R2 package and
public HTTPS verification matched 26 authored files and 317 served objects,
including 72 dev-log entries and 64 inspected screenshots before the verification
entry. No game content or screenshots entered GitHub or CI.

Public no-disc checks passed all eight maps, clean preload landing, failed Nac Nac
separation/contact/recovery/driving, normal ENV04 boundary launch/recovery, saved
one-handed touch controls, gallery filters and desktop/portrait diagnostics.
The full-batch landing capture includes every delivered simulation step: 184
riding frames, zero skipped, minimum tire clearance -0.39263916015625 feet.
The host run's 199 frames follow a different trace, so this is not an improvement
comparison or a clipping fix. Rider/camera cache consumers, physical plant proxies,
dynamic/categories and complete original-game replay parity remain unfinished.

The release workflow uses validated current-main CI artifacts and explicit
checkpoint R2 uploads. The recurring publication heartbeat is retired.

## Active wreck rider and camera scene queries (2026-10-12)

Gameplay `f6353c37684063777cd5d0cff1d87e99a0de615f` passed
[CI 38212608120](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38212608120)
and automatic production deployment. The explicit R2 checkpoint upload and HTTPS
verification matched 26 authored files and 319 served objects, including 74 log
entries and 66 screenshots before the public verification entry.

Serial public checks passed active wreck rider-cache/direct camera assertions,
boundary launch/recovery and paired query flags, clean landing, all eight maps,
diagnostics, saved one-handed controls and gallery filters. The Nac Nac trace
contains 195 cached rider frames and 195 direct camera queries. Boundary flight
includes 490 rider refreshes and retained plant candidates. These routes have no
physical rider-prop contact or camera latch; synthetic core geometry checks those
responses. Landing overlap remains 0.393066ft across 201 riding frames with none
skipped. Normal-riding rider collision, MUD metadata, custom masks, dynamic actors,
physical plant proxies, global attachment policy and full gameplay parity remain
incomplete. The normal single-player chase source disables optional object queries.

## Vegetation physical-contact public checkpoint (2026-10-12)

Main `0541085b495fd9e8d0a0dae59ba6c8ea186f49cc`, CI `38264831246` succeeds
and identifies the public authored app. The separate R2 package validates 324
approved objects and uploads only two changed objects (one inspected route-context
PNG and gallery metadata). HTTPS checks match 26 authored files and 323 served
objects, with 79 log entries and 70 screenshots before the verification note.

Serial public browser checks pass original-content tree contact, steered ENV04
boundary launch/separation/recovery with paired expansion flags, failed Nac Nac
recovery/driving, clean landing, all eight environments, desktop/portrait
diagnostics, saved one-handed controls, touch stunt input and dev-log filters.
The public tree contact matches host step 484; the boundary trace has three
active wreck-rider plant-hit frames and 60 expansion-suppressed frames. A previous
public steered attempt stalls on a hill; a fresh route passes without the temporary
lean assist sending any input. This is bounded route coverage, not an original
hardware or deterministic full-game replay. Public landing overlap is 0.637466ft
across 210 riding frames/no skips; host is 0.393066ft across 201. These are different
traces, so neither a clipping fix nor a causal regression is claimed.

Visible water, moving Arlington Stream train and one-finger feedback are now
parallel feature lanes with independently owned tested commits/publication. Their
in-progress source and private screenshots are excluded from this release.


## Original train contacts, rider impacts and wheel effects (2026-10-12)

Public commit 9ac73657ba94045aa319d3eaed6d481162f242fd passes CI 38285135155.
The validated current-main artifact and public deployment identity match; R2
publication validates 335 approved objects and uploads 6 changes, including only
the four required original BodyImpact samples in the compact audio bank, approved
wheel captures and metadata. HTTPS verifies 29 authored files and 334 served
objects, with 93 log entries/81 inspected images before the public verification
note. Original data, screenshots and native probes remain excluded from Git/CI.

Public train scenarios match host results: a rideable front lift/air/landing,
a 65.4826ft/s upward head-on launch, side wreck/recovery, and top/drop contacts.
The front effect comes from original collision ramp geometry and moving-target
contact response. Actual touch wrecks produce two original rider clips with
isolated PCM energy, no overlapping voices or queued backlog; cached detached
sound location, recovery and pause/resume pass. All three train samples/11 loops,
near/far stereo/Doppler and GL restoration pass without duplicate starts.

Dry dust/turning dirt, original water and actual MUD effects pass desktop/390px:
changed pixel counts 69/48, 4109/3322 and 1288/1286 respectively. Surface/atlas
selection, pause/resume and context restoration pass at at most two draws and
1,031,664 GPU bytes. Source-correlated kernels do not establish hardware or full
gameplay parity. Normal public title/menu/controller/Tioga/Controls and 390px
Caribou reload pass without file selection or browser errors. Missing curved
rail geometry remains the next separate source placement repair.


## Static root placement and restored curved rails (2026-10-12)

Public 841f11c5d619cac92bc6033fe01e1e2d09cbdcc4 passes CI 38287671823. The
current-main artifact and public deployment identity match. The repair retains
the concurrently merged f21 original-game UI changes, source checks and gallery
entry. R2 validates 339 approved objects and uploads 4 incremental changes after
refreshing the existing OAuth session; no new original runtime content is needed.

All 12 curved rail placements change framebuffer pixels in public desktop/390px
inspection views, 36,027–49,735 and 6,041–8,366 respectively. They previously
changed zero pixels because saved PSG root and SCN placement were composed twice.
The repaired static decoder follows original root replacement, with all scene,
COL, terrain, actor and train transforms otherwise preserved. Original native
setter replay passes 68 placements/5,916 instructions. Public all-eight-map
loading/driving covers 1,875 scene instances with zero unsupported meshes/JS/GL
errors; normal menu/controller/Tioga/Controls and 390px Caribou reload pass with
no disc picker. The four public train collision cases retain the same lift,
launch and wreck/recovery results. See research/RAILS.md for evidence limits.


## Deep water and Caribou ramp coverage (2026-10-12)

Public main `b9a3a448d6d19103d63be8e6ec530974297b1870` succeeds in CI
[38292520844](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38292520844)
after PR #5 from the isolated water/snow branch. The current-main artifact and
public deployment identity match. R2 validates 344 approved objects and uploads
only two changes (inspected host snow PNG and gallery metadata); original runtime
content is unchanged. HTTPS verification matches 30 authored files and 343 served
objects, with 103 entries and 90 screenshots before the verification note. An
initial object fetch has a transient connection reset; the complete retry passes.

Public deep-water routes on ENV04/05/07 consume the original five-second clock
and reset to a dry bank; ENV08 exercises shallower depth/wave clock clearing.
All four wet maps show water-only rider buoyancy/Swim/recovery, also at 390px on
ENV04. All six Caribou ramp/ski placements supply wheel/body contacts in 24
desktop/390px approaches; the clean ramp launch/landing reaches 35.631ft/s upward.
The existing authored collision geometry requires no added shape or offset.

Public clean landing, Nac Nac wreck/recovery/driving, normal title/menu/controller
and mobile map routes without disc selection, diagnostics persistence/layout,
uninterrupted all-eight-map driving and dev-log filters pass without JS/GL errors.
These are bounded original-resource routes and component proofs, not complete
PS2/gameplay/physical-phone parity. See [water/snow evidence](WATER_SNOW_GATE.md).

## Public Vancouver Ridge and nine-map checkpoint (2026-10-12)

Main commit `f585991579f8c44b098d6b137b1679eb8050e3fd` passed successful CI run
[`38302626466`](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38302626466).
The public deployment identity matches that commit. R2 validates 400 approved
objects and uploaded six incremental changes totaling 305,224,223 bytes. HTTPS
verification matches all 31 authored files and 399 served objects. At verification,
the public log contained 110 entries and 93 screenshot references.

The public Single Player → Freestyle route includes FREE05 Vancouver Ridge as the
original SPStunt6 entry, unlocked by default with its original two-part thumbnail.
Desktop and 390px source-derived ramp traces each record 78 model definitions,
75 drawn, 43 solid colliders, 37,649 vegetation placements, 123 wheel-support
frames, 31 body-contact frames, flight beginning at step 19, later ground contact
and drag-light frame 1. The bounded test uses ordinary throttle without injected
velocity, force or wreck state. This does not test every collision face or prove
original hardware behavior.

The uninterrupted public map regression passes ENV01–ENV08 and FREE05, with
terrain following, batched vegetation, no unsupported meshes and zero JS/GL errors.
Material inspection records 2,729 contributing ENV04 rain pixels, 73,978 ENV07
lava pixels, 6,331 ENV08 waterflow pixels and 115,571 FREE05 ski-jump pixels.
Prelit attributes, source UV animation, pause/context phase retention and the
original title/menu/controller/touch/pause routes pass. The frontend test also
reaches Tioga, reloads Caribou at 390px and sees no disc prompt or browser errors.

The newly inspected desktop public-ride capture is recorded in the local dev log,
but its PNG remains in ignored private storage pending the next validated R2
gallery upload. The public screenshot count above predates that entry. Three
Vancouver ice-reflection overlays, exact fog, 92 light/lens behaviors, projected
terrain/vegetation LOD, shadows, weather, other modes and progression remain
unfinished. The public checks establish these authored browser components and
routes, not full-map or original-game parity.


## Rust Freeride multiplayer release (2026-10-12)

Main `ceee73d44bc69f940729c5fdf20b892e4394bb59` passes
[CI 38300517282](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38300517282)
and is verified on the existing public HTTPS game URL. The checked artifact now
contains authored frontend assets and a compiled Rust room relay. The existing
game Worker exports its Rust `LobbyRoom` Durable Object and delegates room routes
to the Rust WorkerEntrypoint; no separate Worker or expanded CI credential is
required. `/multiplayer/version` reports the checked commit and relay WASM hash.

The private expanded R2 package includes 26 quad mesh/PHY/AO/AR/particle archives,
both rider body rigs, their union of 140 used MOT clips, connected materials and
per-model engine banks. Unused first-person arms and unreferenced animation clips
are excluded. The uploader validates 396 approved objects and uploads 59 changes;
HTTPS verification checks all 395 served runtime/manifest/PNG objects plus 31
unchanged authored/static hashes. The gallery initially has 108 entries and 91
screenshots; the public verification note follows separately. Source resources
and screenshots remain outside GitHub and CI. If the existing host OAuth session
has expired, `npx --yes wrangler@4.149.0 whoami` refreshes it before the R2 upload.

Eleven live public Rust relay gates pass, including four-slot capacity, origin
rejection, reserved host identity, stale command resynchronization, immutable
active map/start, validated snapshots, safe slot reuse, malformed sender isolation,
rate limits, model validation and host departure. The real two-browser Metal GPU
gate uses the copied invite link and distinct ATV01/male and ATV02/female rigs.
Remote movement, orientation, spatial engine audio, touch-driven crash/rider
separation/impact sound/recovery and host-end all pass. The crash trace contains
112 guest samples, 51 detached-rider frames and 13 impact-audio frames. Sampled
public round trips are 38–51ms from one host through Cloudflare; no cross-location,
physical-phone or general Internet performance claim is made.

A second actual public gate starts four browser clients with ATVs 1/14/26/2 and
both rider rigs. Each renders three finite remote actors (39 draws) with three
engine voices. Movement from each of the four riders reaches every observer;
host-end clears all guest room, pose, audio, impact and GPU caches. No page errors
or failed HTTP responses occur. `tools/test_multiplayer_four.mjs` repeats this
opt-in original-content check. On a single Mac with Chromium Metal at 1280×896
and synthetic focus, a 12-second diagnostics sample reports 71.59–74.14 FPS,
26.4–27.2ms frame p95, 2.47–2.92ms CPU draw and 2.1–4.3ms physics round trips.
Relay RTT samples are 37–201ms (final 42–47ms). These are bounded measurements,
not claims about other machines, phones, browser backends or locations.

The public actual-UI garage probe selects and drives all 26 models, alternating
13 male/13 female rigs. Expected source paths, animated GPU pose draws, finite
Rust physics, per-model engine configurations and live voices pass without
HTTP/page/worker/WebGL errors. Representative public models 01/14/26 are inspected. The repeatable authored
garage gate is `tools/test_player_models.mjs`. The canonical private
`hosted-content` package is refreshed to this verified 104-record manifest,
preserving earlier blobs and a prior manifest backup; future packages must use
the current content packager so multiplayer resources remain present.
Public original title/menu/controller/Tioga/pause/390px Caribou reload passes
without a file picker or JS/GL errors. This is an owner-authoritative shared
Freeride baseline; racing, player collision, server/host physics reconciliation,
host migration, split-screen and full original game parity remain incomplete.
See [implementation and transport tradeoffs](MULTIPLAYER.md),
[original multiplayer evidence](MULTIPLAYER_ORIGINAL.md) and
[model census](MULTIPLAYER_MODELS.md).

## Combined Vancouver and Rust multiplayer verification (2026-10-12)

Main `cef26f4495f0c5dbf6f8398133ea81bf4b78aed3` passes
[CI 38307658534](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38307658534).
The public HTTPS app and Rust relay identity/WASM hash match that exact artifact.
The 106-record runtime package preserves nine environments, all 26 quads and both
rider catalogs. R2 validation covers 403 approved objects with four changes;
HTTPS checks 31 authored/static files and all 402 served objects, with 115 dev-log
entries and 96 screenshots before the final verification note.

Thirteen live public relay checks pass, including all-four ninth-map selection
and rejection of map 10. The copied-invite Vancouver pair loads matching scenes,
draws both rider rigs, exchanges movement and spatial engine audio, and cleans up
when the host leaves. Actual host closure during guest asset loading also passes.
Four public browsers each draw three peers in 39 remote draws with three engine
voices; every rider's movement reaches every observer, then host-end clears all
guest rooms, models and sound. The 390px failed-stunt replay records 403 guest
frames, 42 detached-rider frames and 14 spatial impact-audio frames, then recovery.
All 26 actual garage selections pass against the merged renderer, alternating
13 male and 13 female rigs: exact assets/physics/engine configurations, finite
Rust state, animated GPU poses, live voices and 37.44–44.76ft of throttle movement
without runtime/HTTP/WebGL errors. The canonical private `hosted-content` package
is refreshed to the validated 106-record manifest with its prior manifests and
older blobs retained; future uploads must preserve this complete dependency set.

Four-client timing uses one Mac, headless Chromium Metal, 1280×896 and synthetic
focus: 71.00–75.86 FPS, 25.9–27.2ms frame p95, 2.61–3.11ms render CPU,
2.1–6.6ms physics round trips and 41–52ms relay RTT. Concurrent validation
overlap was not measured. These are bounded measurements, not phone or general
Internet guarantees. At this earlier checkpoint the reflection/fog work was
isolated WIP; it was later publicly validated in e18a371e below.

## Public fog/reflection release e18a371e (2026-10-12)

Public main `e18a371e81b471137dd5fb1121b05d454584b1d0` passes
[CI 38312395498](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38312395498).
The validated current-main artifact matches 32 authored files and all 405
served objects. The canonical R2 package contains 106 records, 306 referenced
files and 406 approved objects across three changes. HTTPS verification
matches every hash; the gallery has 118 entries and 99 screenshots.

Public ENV01/ENV04/FREE05 fog checks match 3,000 GPU coverage samples with zero
byte differences, and the tested screen-affine interpolation sample passes.
The compressed-prelit packing comparison is source-correlated, not a complete
VU/GS or PS2 hardware replay. Reflections draw ENV01's 88 groups/275 instances
with 141,770 contributing pixels and FREE05's three groups/78 instances with
236,380 pixels; 2,304 GPU comparisons have maximum UV error 7.15e-7.

All nine maps pass. Material inspection reports 4,618 rain, 73,970 lava, 6,335
waterflow and 115,571 ski-jump contributing pixels, with pause/context retention.
Four wet maps plus 390px projection, dry-map behavior, Vancouver desktop/390px
contact, frontend routes and copied-link FREE05 multiplayer pass. Vancouver
draws 78 placements with 43 colliders, 123 wheel-support frames, 31 body-contact
frames, takeoff at step 19 and later landing. The multiplayer pair renders 13
remote draws with both rigs, audio and movement. No source tree-lighting completion
is claimed in this release. Near-vegetation materials were later verified publicly
across all nine maps in d9596d9, including the reconstructed Sun/ambient inputs.
Those component checks do not establish the complete inherited callback or global
frame path. Native terrain/water GS precision, other lighting/shadows/lens
behavior, LOD and weather remain open.

## Public near-vegetation material and transport gate d9596d9 (2026-10-12)

Public main `d9596d937907395440b61e78be61b2045d987d4d` (PR #12) passed
[CI 38318057407](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38318057407)
and is deployed. The verified deployment identity matches the validated artifact.
Canonical R2 retains 106 records and 306 references. The upload validates 408
approved objects totaling 310,198,438 bytes; HTTPS verification matches all 32
authored files and 407 served objects by hash, size and MIME. The gallery has
120 entries and 101 screenshots.

The public near-vegetation material check passes all nine maps. Six hundred
actual-GPU RGBA cases have zero byte differences. Public transport validation
covers 1,200 steps and exactly matches sequential frame/camera decimal outputs;
no production timing benchmark was run. These results do not establish full
scene rendering, distant vegetation transitions, LOD, PS2 GS/hardware or
end-to-end performance equivalence.

The public all-map route passes on ENV01–ENV08 and FREE05. Vancouver desktop and
390px checks draw all 78 placements and retain 43 colliders and 37,649 plants;
the ride records 123 wheel-support frames, 31 body-contact frames, takeoff at
step 19, later landing and ATM frame 1, with the map unlocked by default. Public
touch input matches batched and sequential simulation for 124 steps, including
shifted release and cancellation. The public copied-link FREE05 pair passes with
13 remote draws per client, male/female rigs, one voice per client, host cleanup
and a measured remote distance of 4.9307901763 feet. These checks do not include
a production FPS/timing claim; the transport microbenchmark is host-only.

The inspected ENV07 hero is a capture from this public deployment, but its PNG
and gallery reference remain in private storage for a later validated current-main
release. Weather callbacks, distant vegetation/LOD, point lights, shadows and
native GS/full-frame behavior remain open. This release does not claim complete
source tree lighting or whole-game/PS2 parity.

## Public Caribou race preview — 2026-10-13

Main `af3e6b5a7ac3fc4095477326597a27cfe84b97ce` passed CI and automatic
production deployment in run [38333554805](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38333554805).
The validated current-main artifact was fetched and its public deployment
identity confirmed before R2 publication. The separate package validated 417
approved objects and uploaded five changes: the National archive, expanded
runtime audio, the inspected host portrait race image, manifest and gallery.
It contains 108 runtime records; no source content or PNG entered GitHub/CI.

Public HTTPS verification matches 33 authored files and 416 served objects by
hash/size/MIME, with 129 log entries and 108 screenshots at this verification
point. The public race test passes desktop/390px portrait/844px landscape,
countdown input lock, pause, five named CPUs, player drive, touch steering and
cancellation, restart, no disc selector and no JS/GL errors. The results card
uses a separate render-only fixture; it is not proof of a complete original
course replay. CPU route motion is authored; original AI physics, passing and
rider collisions remain unfinished. The published picture retains its honest
host-capture caption. See [race evidence](RACING.md).

## Public late-join and multiplayer visibility release (2026-10-13)

Main `551d7fd890946f18cc8ef8430c12f843138fe758` passed CI
[38446510103](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38446510103).
The validated artifact and `/deployment.json` identify the same commit on both
`https://furyremastered.com/` and
`https://atv-offroad-fury-2.champtoday.workers.dev/`. Full HTTPS verification
matched 39 authored/static files and all 446 served objects by hash, size and
MIME. The separately validated R2 package contains 447 approved objects totaling
338,648,873 bytes; three changed objects were uploaded: the larger-label host
capture, Players capture and gallery. The verified gallery snapshot contains
160 entries and 131 screenshots before this text-only record.

The public two-client feature run preserved custom lobby code `FV70MYWD`, joined
a guest after play started, and rendered 13 remote draws on both clients. The
paused Players spawn target measured 342×64 pixels at (24,96), placing the guest
about 10 feet from the host. Owner-local contact counts reached 5 and 3. A moving
flatbed run sampled 20 positions with a maximum train-relative error of 1.29715
feet; host departure cleaned up guests. Fourteen public relay cases pass,
including custom-code and duplicate-code handling. The deployed relay WASM
SHA-256 is
`8776744065b1b42d6ac9faf13034c0c39d7246202ea3476646400b622b634753`.

The public four-browser route selected quads 1, 14, 26 and 2. Each rendered
39 remote draws and three engine voices; every rider's movement reached all
observers, and host end cleared guest rooms, models and audio without HTTP or page
errors. These automated tests ran in the tested browser/host environment; they do
not establish physical-phone behavior or geographically separate network latency.
The shared train clock freezes when its host pauses, and contacts remain
owner-local without server physics reconciliation. Racing, competitive authority,
host migration and full original-game parity remain unfinished. See
[implementation limits](MULTIPLAYER.md).

## Public rendering optimization release — 2026-10-13

Main `124b67bdac878dd1e1a481703c29fe2a5ecf00f3` passed CI and automatic
production deployment in [run 38475701679](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38475701679).
PR #42's final CI run [38475141913](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38475141913)
passed before merge. The validated current-main artifact was fetched before the
R2 update. Both `https://furyremastered.com/` and
`https://atv-offroad-fury-2.champtoday.workers.dev/` report the same deployment
commit. Full Cloudflare verification passed on both origins, matching 43
authored files and 453 served objects by hash, size and MIME. The validated R2
package contains 454 approved objects totaling 343,700,282 bytes; two changed
objects were uploaded: the inspected gameplay screenshot and gallery. No runtime
content changed. The public gallery snapshot at verification has 179 entries and
138 screenshots. The published screenshot keeps its host-only Chromium Metal,
390×844/DPR3 gameplay caption; it is not a public-browser or phone capture.

Public Chromium checks passed no-disc startup without file selection, original
title/controller/pause routes, 390px touch driving and reload, and all nine map
loading/driving routes with terrain and vegetation. No JavaScript or WebGL errors
were recorded. In the public two-second lifecycle probe, active play rendered 120
frames at about 60 FPS, advanced 119 physics steps, submitted 14,520 draws and
cleared the HUD zero times. Paused and synthetic blur-visible probes each had
zero RAF callbacks, draws, HUD clears and physics steps; focus, driving and
context restoration woke the game successfully. Public held-touch Nac Nac testing
passed wreck separation, ground contact, automatic recovery and resumed driving.

The public multiplayer feature retry passed late join with 13 remote draws per
client, host recovery, nearby spawn at about 10 feet, the 342×64 mobile Players
hit target, owner contacts (10 and 10), 20 train samples with at most 0.908207
feet relative error, both head-on Front wrecks with peer replication, and
departure cleanup. An initial run timed out because its fixture clicked Leave
and immediately closed the host page before the explicit-close handshake
finished. The test now waits for owner state to clear and its WebSocket to close,
and waits for the guest room code to clear before destroying the host page. This
resolved a harness ordering assumption against the existing 30-second recovery
grace; no production transport code changed.

Additional public protocol-v3 checks recorded 170 driving messages / 22,914
bytes over 10.01 seconds using 135- and 117-byte packets. Suspended riders each
sent two poses over about 11 seconds and remained fresh; late join restored the
host at zero positional error, resume delivered 41 messages in 2.515 seconds,
and a 5.518-second solo interval sent no snapshots. This confirms the deployed
transport behavior and is not a new rendering-performance claim. The four-client
route passed with quads 1/14/26/2: each client rendered three peers in 39 draws
with three engine voices; all drivers' movement reached every observer, and
explicit owner leave cleared guest codes, remotes, audio, impacts and models with
zero page or HTTP errors. The abrupt-owner-close recovery-grace regression also
passed: a copied-invite two-rig session cleared the guest after abrupt host
closure within the existing 45-second test window, covering the 30-second
recovery grace. It rendered 13 remote draws and one engine voice per client, with
9.953 feet of remote movement. The four-browser screenshot remains private and
was not published. The paired host
`gl.finish` render-wall results remain separate from public gameplay checks and
do not prove main-thread CPU, normal FPS, physical-phone, battery or thermal
gains. See [performance evidence](PERFORMANCE.md).

## Public pause submenu verification — 2026-10-13

Pause submenu polish PR #49 merged as `c1a92fa8fdb8e7ae364a220d5d642614e85c9f0a`;
main CI and automatic deployment passed in run `38484978361`, and the public
deployment identity matched. Public actual-source browser checks passed all six
secondary screens at five viewport sizes, controller capture/remapping and
defaults, Escape cancellation, music and slider behavior, single-player
freeze/resume, and a two-client Lobby session with neutral local input, live
physics, remote movement, continuing audio, and leave cleanup. These checks ran
without page errors. At the time of this browser checkpoint, the separate
R2/gallery hash audit and inspected pause-menu screenshot upload had not yet run.
