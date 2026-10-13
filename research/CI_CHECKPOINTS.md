# Visible checkpoints and production releases

Each coherent checkpoint includes relevant passing checks, evidence/limitations,
and a dev-log entry through `tools/update_devlog.py`. Commit completed work before
submission. During longer work, report the last commit, current gate, remaining
checks and blockers at least every 30 minutes. Host checks are not a public release.

The local Rust release queue now owns build, integration, source push, R2 upload,
Worker deployment and public verification. From any clean project worktree:

```sh
make release
make release-status
```

For new dependencies/screenshots, use the submission options in
[RELEASE_QUEUE.md](RELEASE_QUEUE.md). One worker serializes releases and snapshots
all job inputs. It builds the integrated commit locally, keeps original resources
and screenshots outside GitHub, uploads and verifies release-specific R2 content
before deployment, then checks public identity, file/object hashes and real
no-disc browser startup. A job is complete only after public checks pass.

GitHub [Actions](https://github.com/nrgapple/atv-offroad-fury-2-web/actions) now runs
lightweight source/release/JS checks. It neither builds WASM nor deploys production.
The old successful-current-main CI artifact fetch/deploy procedure is historical.
There is no publication heartbeat, Sites publication or No Mistakes gate.

Production is **public HTTPS**:
https://atv-offroad-fury-2.champtoday.workers.dev/ and `/devlog.html`.
Job receipts/logs, R2 upload receipts and public verification reports are retained
in ignored local queue storage. `/deployment.json` identifies the released
integrated source SHA, which can differ from the submitting worktree's SHA.

Do not push/deploy independently while using the queue. Never submit WIP,
uninspected screenshots, ISO/BIOS/raw executable, unrelated disc content,
reverse-engineering output or reference repositories. See
[RELEASE_QUEUE.md](RELEASE_QUEUE.md) for conflict/failure/recovery behavior and
[DEPLOYMENT.md](DEPLOYMENT.md) for public checkpoint evidence.
