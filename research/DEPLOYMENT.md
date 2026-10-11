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
reference checkout, generated game pseudocode, or screenshots. CI's development log
contains authored text; screenshot references are omitted because screenshots remain
outside GitHub. An authorized Sites gallery can be retained in the Site's separate
hosting checkout. `make verify` remains a host check because it requires the local
private disc and reference checkouts. Artifacts are retained for 14 days.

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

The local Codex heartbeat `publish-atv-web-builds` is configured **ACTIVE every 15
minutes** in this thread. It checks successful current-main CI through
`fetch_site_release.py`, stages the verified artifact through `stage_site.py`, and
uses native Sites tools with a fresh credential to publish. It skips unchanged
commits and leaves the prior public build active while CI is pending or failing.
There is up to one polling interval of deployment delay. The automation needs the
Mac/local Codex executor available; public Site visitors do not need this Mac online.

The CI-artifact-to-public-Site path has now been verified end to end for main
commit `5d160a747e9730c9e07f35afda9bac941c15f062`: the successful GitHub artifact
was staged, saved and deployed through native Sites, and the public
`/deployment.json` identifies that exact commit. The deployed development log
contains 10 entries and serves all 8 approved screenshots. A real-disc Playwright
run against the public HTTPS site passed terrain, sky, props, ATV/rider, engine
sample scheduling, chassis/throttle/brake/reset, controller/focus, touch/cancel,
graphics-context recovery, and no-game-upload checks. The selected ISO remained
in the local test browser.

The first scheduled heartbeat execution has **not yet been observed**; creation
proves the schedule is saved, not that an unattended deployment has run. The
publisher still needs a separate unattended-run check before automatic publication
can be claimed as verified.

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

`work/release/static/` is the asset-free static output for hosting, and
`work/release/release.json` contains commit and file hashes. The packager validates
the release before returning. Do not change a release after its Sites source has
been pushed and its archive packaged.

GitHub's [artifact download documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
and [GitHub CLI documentation](https://cli.github.com/manual/gh_run_download)
describe the authenticated CI artifact retrieval used by an updater.
