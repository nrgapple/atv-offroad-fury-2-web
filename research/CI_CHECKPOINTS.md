# Checkpoints without repeated release work

Implementation chats own code, relevant tests and one independent dev-log entry.
One named release owner handles a combined checkpoint's merge, public browser
checks, R2 upload and gallery verification. Contributors provide their tested
commit, required private resource package and inspected images to that owner.
Do not start a second upload or repeat another owner's completed checks.
The GitHub Actions application deployment remains automatic when healthy. The
checked local fallback below is authorized when Actions stalls or fails. The
experimental daemon/release queue and No Mistakes remain disabled.

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

The runtime lane also runs the content-free browser smoke group
(`make browser-smoke`) against the freshly built `web/dist`, served with an empty
content directory, using pinned Playwright 1.58.2 Chromium/WebKit. Content, native
and relay browser groups stay local; see `BROWSER_TESTS.md`.

Updated PRs cancel their superseded runs. Main deployments are never interrupted;
main workflow concurrency remains serialized (GitHub may replace a pending run
with the latest pending main). The trusted main build saves the Rust compilation
and registry cache, keyed by toolchain, lockfiles and commit; PRs only restore it.
The separately validated `worker-build` executable cache remains exact-key only.
No private content, credentials, screenshots or references enter either cache.

## One owner and four milestones

All receipts are ignored under shared `private/reports/release-checkpoints/`.
They are local evidence, not a publication queue or an approval step. Use a stable
combined checkpoint ID and a task/thread name, for example:

```sh
python3 tools/release_checkpoint.py claim --checkpoint menu-polish --owner menu-release
python3 tools/release_checkpoint.py record --checkpoint menu-polish --owner menu-release \
  --stage implementation-ready --commit <tested-sha> --evidence "Relevant checks passed; host evidence paths"
```

Claim before release work. A second owner for the same checkpoint is rejected.
After merge, record GitHub's actual merge time (rather than the later observation):

```sh
python3 tools/release_checkpoint.py record --checkpoint menu-polish --owner menu-release \
  --stage merged --pr <number> --evidence "Passing PR CI"
```

The release owner then:

1. Waits for automatic deployment and fetches the **validated live artifact**:
   `python3 tools/fetch_site_release.py --deployed`. Confirm the exact public
   `/deployment.json` commit. Later docs merges do not invalidate this live
   artifact. If another application is deployed meanwhile, its owner takes over
   publication; do not chase main in every implementation chat.
2. Runs relevant public browser checks once, then records `--stage live`, the
   deployed commit and evidence. Host success alone never earns this stage.
3. If runtime resources or screenshot references changed, inspects new screenshots
   and uploads the pinned runtime/gallery package:
   `python3 tools/upload_cloudflare_content.py --release <artifact> --commit <live-sha> --checkpoint menu-polish --owner menu-release`.
   The uploader enforces ownership, takes one shared nonblocking publication lock,
   and checks successful-main provenance and the deployed identity before pointer
   publication. It never requires unrelated newer docs to rebuild the game.
   For text-only metadata with unchanged runtime manifest and screenshot references,
   skip the R2 upload: the checked text publication retains the approved image
   overlay. Confirm the public text and unchanged image references instead of
   downloading the unchanged runtime package again.
4. After an R2/resource change, runs `python3 tools/verify_cloudflare.py --url https://furyremastered.com --commit <live-sha> --release <artifact>`
   and verifies the supported Workers address when required. Record
   `--stage gallery-complete` with evidence, including when no new PNGs were needed.
5. Reads `python3 tools/release_checkpoint.py status --checkpoint menu-polish` to
   report implementation, merge, live and gallery times separately. Unknown stages
   remain pending. Do not describe gallery waiting as ongoing implementation.

Keep notes about public verification in the existing checkpoint's own entry, or
batch independent verification entries for the lightweight metadata lane. A note
must not trigger another gameplay build or another full gameplay test pass when
runtime bytes have not changed. Verify its metadata publication and new images;
repeat gameplay checks only for changed code, a failure or a new unresolved risk.
A feature may be reported live once its public tests pass while its owner finishes
publication bookkeeping, with the remaining gallery stage stated explicitly.

Primary public HTTPS is https://furyremastered.com/; the Workers address remains
supported. GitHub's production environment and deployment history remain visible.
CI verifies authored application hashes and committed dev-log text. Runtime
resources and inspected PNGs remain in the separate allowlisted private-to-R2
package, never GitHub source or CI. Never publish WIP, ISO/BIOS/raw executable,
unrelated disc content, reverse-engineering output or reference repositories.

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
at the exact merged `origin/main` commit, run:

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
Continue the same public browser, R2, gallery, and checkpoint milestone steps.
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
