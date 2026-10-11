# Private source repository and public Cloudflare hosting

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
`tools/stage_cloudflare.py` stages that verified artifact for `wrangler deploy`.
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
path; the migrated heartbeat has been configured but an unattended scheduled
R2 maintenance run remains unverified. Original resources and screenshots never
enter GitHub or CI; CI attaches the existing R2 binding without uploading its data.
`/devlog.json` merges screenshot references from the separately uploaded gallery
only when each checkpoint's authored metadata exactly matches the CI revision.
New or edited checkpoint screenshots require another authorized local R2 upload.
The uploader reads checkpoint metadata from the validated current-main commit,
rejects other revisions and skips byte-identical objects recorded by its previous
local upload report. `--force-upload` repairs remote drift; public hash verification
remains required after each content/gallery update.

Host publication commands after successful current-main CI:

```sh
python3 tools/fetch_site_release.py --site-url https://atv-offroad-fury-2.champtoday.workers.dev
python3 tools/stage_cloudflare.py --release <validated-release> --commit <commit>
python3 tools/upload_cloudflare_content.py --release <validated-release> --commit <commit>
npx --yes wrangler@4.149.0 deploy
python3 tools/verify_cloudflare.py --url https://atv-offroad-fury-2.champtoday.workers.dev --commit <commit>
```

The verification command uses the checksum-validated release cached by
`fetch_site_release.py`, verifies all public authored/static hashes and committed
devlog metadata, then fetches every uploaded runtime object and screenshot over
HTTPS to check size, SHA-256 and MIME type, deployment commit, landing route and
gallery references. It does not replace the browser no-disc startup
and actual gameplay regression. The R2 Worker serves GET/HEAD only; content-addressed
blobs use immutable browser caching and manifests/gallery use revalidation. Do not
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
that version. The Site's source commit is separate from the private GitHub source
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
analysis may be added to GitHub source or CI. Keep using the native Sites workflow
and fresh session credential for the Site-only package. Do not claim autoload is
complete until the published build starts without a file-selection step and public
requests are verified against the approved manifest.

The local Codex heartbeat `publish-atv-web-builds` remains **ACTIVE every 15
minutes** in this thread, migrated to Cloudflare verification and validated R2
content/gallery maintenance. GitHub deploys authored application assets on passing
main builds; the heartbeat no longer publishes Sites versions. It skips commits
waiting for CI and never publishes uncommitted devlog metadata. R2 maintenance
needs the Mac/local Codex executor available; public Cloudflare visitors and GitHub
application deployment do not need this Mac online. Screenshot updates can have
up to one polling interval of delay.

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

The heartbeat `publish-atv-web-builds` ran once at 2026-10-11 21:12 UTC and skipped
because main was unchanged; this predates v8. V4 through v8 were published through the native Sites
workflow. A successful unattended publish is still unverified.

## Release commands

```sh
make check
make web
python3 tools/package_release.py --commit "$(git rev-parse HEAD)"
python3 -m unittest discover -s tools -p 'test_release*.py'
```

An unattended publisher first runs `python3 tools/fetch_site_release.py --site-url
<literal-verified-Site-URL>`. It returns JSON with `ready`, `unchanged`, or a waiting
state. On `ready`, use the returned paths and SHA with `python3 tools/stage_site.py
--release <release> --commit <commit>`, then the Sites skill's fresh-credential
source-helper/save/deploy workflow. The fetcher verifies current-main CI success,
artifact hashes and exclusions, and that main did not change during retrieval.
Staging uses the CI development log revision; it overlays approved local screenshot
references only for entry IDs whose authored metadata exactly matches that revision.
Uncommitted local application changes and log edits cannot enter this CI-based publish.

`work/release/static/` is the asset-free CI application output for hosting, and
`work/release/release.json` contains commit and file hashes. The packager validates
the application release before returning. Site-only runtime content is packaged
separately under ignored private storage and does not alter the CI artifact. Do not
change either package after its matching Sites source has been pushed and its
archive prepared.

GitHub's [artifact download documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
and [GitHub CLI documentation](https://cli.github.com/manual/gh_run_download)
describe the authenticated CI artifact retrieval used by an updater.
