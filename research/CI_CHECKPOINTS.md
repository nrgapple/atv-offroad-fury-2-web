# Checkpoints without repeated release work

Implementation chats own code, relevant tests and one independent dev-log entry.
R2 only serves content, so any chat may upload to it when its work needs it,
without a release owner: runtime resources with
`tools/upload_cloudflare_content.py --content-only` (additive and serialized by
the shared publication lock), screenshots/gallery with the full upload pinned to
the deployed commit. A named release owner (`tools/release_checkpoint.py`) is
optional coordination for a combined checkpoint; when one exists, do not repeat
its completed checks.
The GitHub Actions application deployment remains automatic when healthy. The
local fallback below requires an explicit user request in the current conversation. No
Mistakes remains disabled. The experimental local release queue (`atv-release`)
was removed; see `git log -- research/RELEASE_QUEUE.md` for its history.

## Three CI lanes

- **Documentation:** README, AGENTS, HANDOFF and research Markdown use repository
  hygiene, credential scans and metadata tests. They do not install Rust, compile,
  create an application artifact or deploy the Worker.
- **Dev-log metadata:** the same validation applies to committed checkpoint JSON.
  PRs stop after that fast validation. On main, CI downloads the successful
  artifact for the currently deployed commit, proves that every intervening file
  change is documentation/metadata, and replaces only dev-log text and deployment
  identity. Game JS, CSS, WASM and relay bytes, including their browser cache keys,
  stay identical. The artifact records `application_commit` and `reused_from`.
  GitHub deploys and verifies this lightweight publication normally, without Rust
  compilation. An unavailable/expired artifact, changed runtime, invalid ancestry
  or unavailable production identity falls back to full checks/builds.
- **Runtime or unknown files:** code, frontend, tooling, dependencies, workflow and
  mixed changes retain full tests, lint and release builds. Manual dispatch also
  forces this path. The explicit allowlist is in `tools/ci_changes.py`.

On pull requests the runtime lane also runs the content-free browser smoke group
(`make browser-smoke`) against the freshly built `web/dist`; the main build after
merge relies on that PR result. After deploying, CI verifies the commit, hashes and
dev log on `https://furyremastered.com/` (the same Worker serves the workers.dev
address). The smoke group serves an empty content directory and uses pinned Playwright 1.58.2 Chromium/WebKit, installed by
`tools/playwright_ci.py` with bounded retries and trusted caches (`CI_TOOL_CACHE.md`). Content, native
and relay browser groups stay local; see `BROWSER_TESTS.md`.

A newer push cancels the superseded `web` build on a PR branch. On `main` the running
build finishes (GitHub keeps only the newest pending one) and deploys if it advances
production, even when newer merges are still building; requiring each build to be
main's tip starved production while merges landed every few minutes (2026-10-21:
production stayed on #259 through six later merges). The deploy job has its own
`deploy-production` concurrency group: deploys run one at a time, are never
interrupted, and an older build reaching it after a newer deploy is rejected.
`main` requires the `web` check, so PRs use `gh pr merge --auto` and merge when it
passes; branches need not be up to date with `main` first. The trusted main build saves the Rust compilation
and registry cache, keyed by toolchain, lockfiles and commit; PRs only restore it.
The separately validated `worker-build` executable cache remains exact-key only.
No private content, credentials, screenshots or references enter either cache.

The production deployment applies pending D1 cloud-save migrations before the
Worker update, using the same pinned Wrangler and deploy token (which requires
D1 Edit/Write). It runs after the stale-build gate under `deploy-production`
serialization; migration failure stops the Worker update. The migration command
uses root `wrangler.jsonc`, whose `migrations_dir` points at the checked-out SQL,
while the Worker deploy uses the staged config. Applied files are skipped on
retries. Migrations must remain compatible with the running Worker until the new
version is deployed; see [cloud-save operations](CLOUD_SAVE.md#operations).

## Routine delivery

Use one task branch, focused checks, and a PR with auto-merge. For runtime changes
run `make precheck` (hygiene, secrets and formatting; clippy runs in CI's required
check and locally only through `make lint`); for documentation/metadata-only changes run CI's
"Validate documentation, metadata and repository hygiene" commands, using
`python3 tools/check_secrets.py` without `--history` locally. CI owns the history
scan; a notes change does not need to rescan every other local branch. The allowlist
in `tools/ci_changes.py` determines the lane; unknown or mixed paths use runtime
checks. Do not compile Rust for a notes-only change.

Keep a local build/test task active until its process result is consumed. A
background shell job does not by itself schedule an agent wake. For delegated
work follow the returned continuation instruction, and for pending PR checks use
the supported PR watch. Keep each slice owned through delivery.
When parallel work is authorized, use independent tasks to size the team; there
is no fixed two-agent cap. Research, editing, review and documentation can proceed
concurrently. Stagger heavy checks through the bounded host budget (one compiler
and one browser suite in different worktrees, subject to memory headroom) and
reuse valid results. If most agents are waiting or repeatedly conflicting,
finish the pending slices before adding scope. The budget protects host memory;
measure headroom before increasing heavy-work concurrency. Agent count and
compiler concurrency are separate limits.

GitHub validates and deploys code automatically. Confirm the public
`https://furyremastered.com/deployment.json` once at handoff. Report code, public
verification and pending gallery work separately. Unavailable device checks and
unrelated failures reproduced on the baseline are recorded limitations, not new
release gates. Failures introduced by the change and required CI remain gates.

## Content and screenshot publication

Any chat may publish required R2 resources; no release claim, owner, or receipt
is required. `tools/release_checkpoint.py` is optional for a deliberately combined
checkpoint, not part of routine delivery.

- Runtime resources can precede the app deploy. Run `tools/package_content.py`,
  then `tools/package_motorcycles.py`, then
  `tools/upload_cloudflare_content.py --content-only`. Uploads are additive and
  serialized. If the local manifest drops live paths, rebuild it with the current
  resources; do not override the guard.
- For new screenshots, fetch `python3 tools/fetch_site_release.py --deployed`,
  inspect every new image the package will publish, then run
  `python3 tools/upload_cloudflare_content.py --release <artifact> --commit <live-sha>`.
  Pin the validated deployed artifact; do not rebuild just to chase newer docs.
  If older pending images need inspection, inspect them in this publication task
  or report the specific remaining images. Do not defer the same inspection
  indefinitely or hold up unrelated code PRs.
- After an R2 change run
  `python3 tools/verify_cloudflare.py --url https://furyremastered.com --commit <live-sha> --release <artifact>`.
  Text-only metadata with unchanged runtime and screenshot references needs no
  R2 upload or repeat gameplay test; CI reuses checked application bytes.

Primary public HTTPS is https://furyremastered.com/; the Workers address remains
supported. Runtime resources and inspected PNGs stay out of Git and CI. Desktop
archives containing runtime resources are built/uploaded locally by the existing
owner's-Mac release service (see `DESKTOP_RELEASES.md`). Never publish WIP,
ISO/BIOS/raw executable, unrelated disc content or reverse-engineering output.

## Historical checkpoint

On 2026-10-12 02:55 UTC, public and committed dev-log metadata matched main
`1de08b210680c184217bda6b82d5e912a19a6dce`: 50 entries and 45 screenshots.
The implementation checkout had three newer host-only entries for HUD, failed
Nac Nac recovery and prop-ramp wheel support, all explicitly pending publication.
Direct messaging to **Port core gameplay systems** failed with
`already has an active writer`; its latest thread activity showed ongoing tests.
The audit did not stop that writer or commit its dirty game files. These observations
are historical evidence, not a claim that the newer implementation is deployed.

## Retrying, and the user-requested local deployment

GitHub Actions is the deployment path. Every check and deploy step has a time
limit, and JavaScript tests have a per-test timeout, so a hang fails with the
step or test named instead of stalling until someone cancels it. Retry a failed
or timed-out main run with `gh run rerun <run-id> --failed`, or fix forward.

Only when the user explicitly asks for a host deployment, from a clean worktree
at the exact merged `origin/main` commit, claim a checkpoint with
`tools/release_checkpoint.py claim --checkpoint <id> --owner <owner>` and run
the command below. This exceptional host fallback requires that claim; the
routine Actions and R2 paths above do not.

```sh
python3 tools/deploy_local.py --user-requested --commit <full-main-sha> \
  --checkpoint <claimed-id> --owner <owner> --reason "User requested local deploy"
```

`--prefer-actions` first waits for Actions (ten minutes without step progress,
twenty minutes total); without `--user-requested` the tool then stops and points
back to rerunning Actions. When it does publish, it acquires the shared publication
lock, temporarily sets the repository variable
`FURY_DEPLOYMENT_MODE=local`, cancels competing main runs and requires them to be
terminal, then runs local implementation/security/release checks, source-resource
verification, browser WASM and Rust relay builds. It refuses dirty/unmerged/stale
checkouts, uses the authored file allowlist, and deploys with pinned Wrangler using
the existing host login. It preserves existing domain bindings and verifies all
authored bytes and relay identity on both public HTTPS addresses.

Successful checked artifacts and receipts persist under ignored shared
`private/reports/local-releases/<sha>/`. `fetch_site_release.py --deployed`, the
R2 publication guard, and host metadata reuse accept that verified artifact.
Continue the same relevant public verification and R2/gallery steps.
The user-facing game and dev log must still be verified independently of host
checks. Actions runs use a freshness gate so a delayed older build cannot replace
newer main/local production. The temporary variable is restored after takeover;
if cancellation cannot become terminal within two minutes, publication stops and
the local guard remains in place for a retry. Inspect runs before restoring it.
This is a foreground fallback command, not a new daemon or recurring job.

Local compilation is namespaced by canonical checkout path, even if a build
cache is shared or a target directory is symlinked. Do not link one task's Cargo
target directly to another's. Every browser build executes the current menu ABI
against its compiled WASM; local publication repeats the smoke test against the
packaged bytes before touching production. A checksum alone proves transport
integrity, not correspondence between JavaScript and the compiled Rust schema.
