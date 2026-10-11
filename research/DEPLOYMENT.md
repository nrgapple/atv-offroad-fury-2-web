# Private source repository and public Sites hosting

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

The local Codex heartbeat `publish-atv-web-builds` is configured **ACTIVE every 15
minutes** in this thread. It checks successful current-main CI through
`fetch_site_release.py`, stages the verified artifact through `stage_site.py`, and
uses native Sites tools with a fresh credential to publish. It skips unchanged
commits and leaves the prior public build active while CI is pending or failing.
There is up to one polling interval of deployment delay. The automation needs the
Mac/local Codex executor available; public Site visitors do not need this Mac online.

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
because main was unchanged. V4 and v5 were published through the native Sites
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
