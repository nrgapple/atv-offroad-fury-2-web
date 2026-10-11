# Private GitHub and automatic deployment

Repository: https://github.com/nrgapple/atv-offroad-fury-2-web (private).
The existing **private remote** URL is https://nrgapples-mac-mini.tail96c48c.ts.net:8443/.
Authorized Tailscale access and this Mac being awake are required. The service remains
bound to host loopback because the existing Tailscale Serve proxy supplies remote access.
No new public endpoint, tunnel, DNS record, or Pages deployment is created.

## Deployment flow

Every push to `main` runs `.github/workflows/ci.yml` on a GitHub-hosted Ubuntu runner.
Pull requests receive the same checks. The workflow checks tracked-file hygiene,
Rust formatting/tests/Clippy, input tests, Python syntax, and deployment rejection/
rollback tests. It builds the browser WASM and packages an explicit file allowlist.
There is no disc, BIOS, extracted game content, reference checkout, or screenshot in
the repository or artifact. `make verify` remains a host check because its evidence
requires the local private disc and reference checkouts.

A macOS user LaunchAgent checks GitHub every 60 seconds using the already-authenticated
`gh` CLI. Only a successful `ci.yml` run for the **current main commit**, triggered by
push or manual dispatch, qualifies. The supervisor downloads its `web-release`
artifact, checks the file allowlist, SHA-256 checksums, WASM header, and commit metadata,
then starts the new server and requires its health response to report that commit.
If server startup fails, it restores the previous release. Failed/pending CI leaves
the current release running. Pull-request artifacts are never deployed.

The server and supervisor persist across Codex turns. launchd restarts the supervisor
when needed and starts it at user login. The most recent successful release also
starts when GitHub is temporarily unavailable. This is a user-session service; an
unlogged-in Mac or an absent External SSD cannot serve it.

The dynamic development log and its screenshot gallery keep using the original
project's `research/devlog.json` and ignored `private/reports/devlog-images/` directory.
Automatic deployment does not reset ongoing development work or overwrite that data.

## Operations

Install or update the supervisor on this Mac after changing deployment tooling:

```sh
python3 tools/install_deploy_service.py --start
```

Stop any foreground `make serve` before first installation: both use port 4173.
Subsequent `main` pushes deploy automatically without rerunning this command.
There is no self-hosted GitHub runner and no GitHub secret to configure.

Host service: `com.nrgapple.atv-offroad-fury-2.deploy`.
Installed supervisor/config/logs: `~/.local/share/atv-offroad-fury-2/`.
User LaunchAgent: `~/Library/LaunchAgents/com.nrgapple.atv-offroad-fury-2.deploy.plist`.
Artifacts/state: `/Volumes/External SSD/Developer/Builds/atv-offroad-fury-2-web/deploy/`.
`current` is an atomic symlink to the active commit release; `status.json` records its
workflow URL and activation time. `/deployment.json` reports the active commit over
the existing private HTTPS route. Screenshots remain excluded from Git.

Re-run CI from the repository's Actions page if a release artifact has expired before
the host could download it. Artifacts are retained for 14 days; downloaded releases
remain on the SSD. GitHub authentication must remain available to this user's service.

GitHub's [artifact download documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)
and [GitHub CLI command documentation](https://cli.github.com/manual/gh_run_download)
describe the authenticated artifact mechanism used here.
