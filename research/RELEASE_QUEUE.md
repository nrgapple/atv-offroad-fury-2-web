# Local multi-worktree release queue

The Mac owns production release execution. GitHub stores authored source/history
and runs lightweight source checks; it does not build WASM or deploy the game.
The Rust `atv-release` CLI and one launchd worker coordinate every release.

## Submit a completed checkpoint

Commit the intended changes in a clean worktree, then run one command:

```sh
make release
```

A chat adding recovered runtime dependencies or new inspected screenshots uses:

```sh
make release RELEASE_ARGS='--content /absolute/private/hosted-content --images /absolute/private/devlog-images --reviewed-images'
```

The content directory must be a complete package accepted by
`tools/package_content.py`, not a disc/extraction directory. Omit `--content` to
reuse the last publicly verified dependency package. An explicit content package uses the allowlist validator from the submitted Git commit, so a checkpoint can introduce newly authorized runtime resource paths; the shared validator is refreshed after successful publication. Omit `--images` to use this
worktree's ignored `private/reports/devlog-images/`; previously approved images
are available from shared queue storage. `--reviewed-images` records the
submitting agent's assertion that it inspected all new image pixels. It does not
perform image inspection. Never supply it for uninspected screenshots.

Submission snapshots only referenced, validated runtime blobs and PNGs, pins HEAD
with a local Git ref, and atomically returns a durable job ID. Later edits, branch
movement and worktree removal cannot alter the submitted revision/dependency bytes.
Repeated submission of the same commit returns its existing job. Source changes
and generated files are never auto-committed. Resource files never enter GitHub.

```sh
make release-status
~/.local/bin/atv-release wait JOB_ID
~/.local/bin/atv-release retry JOB_ID
```

`RELEASE_ARGS='--wait'` also waits for completion. Jobs include source SHA,
integrated release SHA, stage, completion/failure status and error. Logs live in
`<queue>/jobs/<id>/worker.log`, with upload/public-verification receipts under the
job's isolated checkout `private/reports/`. Status is durable local data; there
are no unsolicited messages to other chats.

## Worker behavior

1. Serialize releases with an OS file lock; recover interrupted jobs after restart.
2. Fetch `origin/main` and merge the pinned checkpoint in an isolated worktree.
   Independent dev-log entries merge by ID. Code conflicts and incompatible edits
   to the same entry fail for the submitting chat to resolve.
3. Combine dependency manifests against the submission's baseline. Independent
   resource changes survive; conflicting changes to the same resource fail.
   Validate frozen bytes again and assemble only referenced approved screenshots.
4. Run repository hygiene, Rust formatting/tests/Clippy, JS tests, Python/release
   rejection tests and Worker tests. Build the authored WASM/app locally once.
5. Package the exact integrated commit and pin the browser's manifest URL to it.
   Push only source via `origin`, with a normal fast-forward update to main.
6. Refresh existing Wrangler auth, incrementally upload allowlisted R2 objects,
   write release-specific manifest/gallery keys and verify staged R2 bytes.
7. Recheck upstream identity and deploy the checked Worker/static assets selecting
   that release's manifests. No mutable live R2 pointer is changed by staging.
8. Verify public commit, authored hashes, runtime/gallery hashes/sizes/MIME and
   manifest pin. Run public no-disc desktop/390px frontend and dev-log checks.
   Promote the shared dependency baseline only after these checks pass.

`complete` means public verification passed. A failed pre-push job leaves main
and production untouched and other independent jobs can proceed. A failed job
that might have pushed blocks later publication until an explicit `retry`.
Retries retain source/resources and can repeat idempotent stages. They do not
silently retry tests forever, rewrite source, resolve code conflicts, force-push,
or mark an unverified deployment complete. If main changed after a push, the
worker stops for reconciliation rather than releasing an older revision.

R2 and Workers are separate services, not one transaction. Immutable per-release
manifests plus upload-before-deploy protect the previous release during staging;
older browser tabs retain their own manifest URL. Previous R2 objects/manifests
are retained. If public checks fail after deployment, the queue halts and retains
all evidence; automatic Worker rollback is not implemented. Repair/retry or
explicitly redeploy the previous verified artifact. There is no automatic disk
retention/pruning yet; job snapshots and build artifacts must be monitored.

All production writers must use this queue. Its local lock cannot serialize an
independent direct GitHub push/Cloudflare deployment by another machine. Actions'
previous auto-deploy job is removed so it cannot race the worker. Lightweight
GitHub checks are independent evidence, not another app build or release gate.

## One-time host setup

Rust 1.89+ is required for OS file locks. Build and install the controller from
its tested checkout; credentials remain in existing GitHub/Wrangler stores.

```sh
cargo build --locked --release -p atv-release
./target/release/atv-release install \
  --storage '/absolute/private/queue-storage' \
  --target '/absolute/local/build-cache' \
  --content '/absolute/private/verified-runtime-package' \
  --playwright '/absolute/node_modules/playwright'
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.champtoday.atv-release.plist
```

The CLI installs `~/.local/bin/atv-release`, a copied dependency helper, a shared
configuration in the repository's common `.git/atv-release.json`, and the launchd
plist. Every linked worktree discovers the same queue via the common Git directory.
The service starts at login and restarts after crashes. Queue storage must be
private local storage outside tracked source; an SSD is suitable. Build cache can
be on a separate local disk. The initialization gallery must match prior approved
upload hashes. Use `--content` for the previously verified runtime package, especially when the shared default folder contains another chat's WIP. Initialization requires every blob/manifest and historical screenshot to match the prior upload receipt. Do not initialize from unverified/WIP dependency content.

The Mac must be awake, the storage mounted, and GitHub/Cloudflare authentication
valid to complete jobs. Submissions do not schedule Codex tasks or expose a new
network service. `worker --once` drains currently eligible jobs for foreground
operation; the installed worker polls without building when idle.

## Verification

`make check` covers the Rust queue lock/receipt/recovery-selection tests. The
release rejection suite checks frozen snapshots, allowlists, screenshot approval,
resource conflicts, real Git dev-log integration, immutable manifest routing,
and staged R2 corruption. For the host-only compiled CLI concurrency test:

```sh
cargo build --locked -p atv-release
ATV_RELEASE_TEST_BIN=/absolute/target/debug/atv-release \
  python3 -m unittest discover -s tools -p 'test_release*.py'
```

That test uses disposable repositories and a deliberately failing build gate:
two worktrees submit concurrently, duplicates reuse receipts, later edits stay
private, and worker failures produce no push/deployment. It never contacts R2.
The actual production checkpoint provides separate end-to-end release evidence.
