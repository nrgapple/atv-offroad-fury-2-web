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

The CI-artifact-to-public-Site path was verified for main commit
`76f276315650e29841cb122095654239a03cbbed`; public `/deployment.json` returns
that exact SHA. Native Sites deployment `appgdep_6acbffb0631c81918bc9d2fae48ae6e3`
succeeded at 2026-10-11 21:30:26 UTC. Public v4 was tested from a fresh browser
session without selecting or uploading a disc: the Site manifest loaded 68 terrain
textures, 64 sky tiles, 174 scene instances, the ATV/rider, 21 engine samples and
the 21-song catalog. The regression exercised driving, throttle/brake/reset, camera
cycling, controller/focus, touch/cancel and graphics-context recovery. Music played
with zero underruns; there were no JavaScript/WebGL errors. At 390px the mobile
layout had no horizontal overflow and the disc picker was absent. App content used
the same-origin Site manifest and content blobs. Public v4 still uses the sampled
seated rider pose; the newer local normal-riding blend is not in that release.

The previous public version on commit
`3c9f7bdc72810c23afc2aa461810e911a998f01c` received devlog-only verification; its
autostart and manifest requests were not tested. The preceding version
`5d160a747e9730c9e07f35afda9bac941c15f062` passed a real-disc browser run for
terrain, sky, props, ATV/rider, engine sample scheduling, chassis, throttle/brake/
reset, controller/focus, touch/cancel and graphics-context recovery. That older
check required selecting the ISO and is not the basis for the no-disc result.

The first local autoload checkpoint was verified on the development host before
v4 was published. Desktop and 390px mobile captures showed original terrain, ATV,
rider, engine samples and streaming music loaded through a same-origin manifest,
with no JavaScript/WebGL errors, no music underruns and no mobile overflow. The
public v4 test now independently verifies the same no-disc flow on the deployed
Site.

At public v4 verification, `/devlog.json` served 13 entries with 11 screenshot
images; every screenshot returned HTTP 200 with `image/png`. Later local rider and
public-autoload entries are prepared in the working tree for the next source-backed
Site update.

The heartbeat `publish-atv-web-builds` has run once at 2026-10-11 21:12 UTC and
skipped because main was unchanged. That confirms a scheduled check ran, not an
automated deployment: the v4 publication was completed through the native Sites
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
