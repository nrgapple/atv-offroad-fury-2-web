# Private source repository and public Sites hosting

Source repository: https://github.com/nrgapple/atv-offroad-fury-2-web (private).
The user selected **public HTTPS Sites hosting**, matching the Game Boy project.
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

Unattended publication can instead run in a linked Sites schedule or a local Codex
heartbeat that has source access and uses native Sites tools to obtain a fresh
credential per run. Only deploy a successful build for the current `main` commit,
and skip when that commit is already public. Source access and actual unattended
publication must be verified independently of creating a schedule. A polling schedule
introduces its polling interval as deployment delay; GitHub CI alone is not proof
that public automatic deployment has been configured.

## Release commands

```sh
make check
make web
python3 tools/package_release.py --commit "$(git rev-parse HEAD)"
python3 -m unittest discover -s tools -p 'test_release*.py'
```

`work/release/static/` is the asset-free static output for hosting, and
`work/release/release.json` contains commit and file hashes. The packager validates
the release before returning. Do not change a release after its Sites source has
been pushed and its archive packaged.

GitHub's [artifact download documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
and [GitHub CLI documentation](https://cli.github.com/manual/gh_run_download)
describe the authenticated CI artifact retrieval used by an updater.
