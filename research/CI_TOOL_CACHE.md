# Pinned Worker build tool cache

The PWA release's PR CI spent 259 seconds installing `worker-build 0.8.7`
([38438794714](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38438794714));
main spent 260 seconds on the same compilation
([38439883981](https://github.com/nrgapple/atv-offroad-fury-2-web/actions/runs/38439883981)).
Main's implementation checks took 113 seconds, application build 82 seconds,
Cloudflare deployment 19 seconds and public CI verification 25 seconds. This
checkpoint addresses the repeated tool installation; those other gates remain.

The web job derives a key from the pinned tool version, runner OS/architecture,
the complete resolved `rustc -vV` output and Cargo version. The schema prefix
allows deliberate invalidation when this cache policy changes. Project commits
and game data are not key inputs because this executable is independent of them.

Only two files are restored/saved: the dedicated temporary tool directory's
`bin/worker-build` and `identity.json`. No Cargo home/registry, credentials,
Wrangler configuration, project target/build outputs or private game assets are
cached. The installer compiles with `--version 0.8.7 --locked --force` into a
separate temporary Cargo target. The validated executable is then installed at
the path the existing `make relay` command already uses.

`tools/worker_build_cache.py` accepts a cache only on an exact action hit. Before
executing it, it checks the compatibility record and binary checksum; it then
checks the executable's own `--version` output. Missing, partial, corrupt,
incompatible, non-executable or wrong-version file entries trigger a locked cold
installation. Symlinked cache directories fail closed; a symlinked executable is
removed without touching its target. A failed install cannot leave a valid record.
Cache service errors are optional; tool validation/installation, tests, builds,
artifact validation, deployment and public hash verification are required.

Restore and save use separate, commit-pinned `actions/cache` v6 actions. PRs only
restore; the save action runs after a successful main web build/artifact upload,
only for main `push` or `workflow_dispatch`. No broader restore keys or cross-OS
archive reuse are enabled. GitHub prevents main from restoring PR merge-ref
caches; this adds a main-only producer policy as well. A checksum record is an
integrity/compatibility check, not a cryptographic signature: the producer scope
is the trust boundary. An invalid immutable cache entry falls back to cold builds
until that entry is removed or the schema/key is changed.

Primary sources reviewed on 2026-10-13:

- [GitHub cache scopes and restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub restore/save actions and inputs](https://github.com/actions/cache)
- [Pinned worker-build source](https://github.com/cloudflare/workers-rs/blob/454fbd3c6ba3109be6158b19a04102abfcabfef1/worker-build/src/main.rs), which implements `--version` as an immediate version response

Host validation covers eight failure/reuse/policy tests, the existing 34 release
tests (one skip), syntax, setup and source hygiene. A seeded actual macOS/ARM64
0.8.7 executable passes the helper's warm validation in 0.206 seconds without
installation. This is not a GitHub cache timing measurement.

Remote validation requires a cold PR build, a successful main build that populates
the trusted cache, and a rerun of the PR that restores that main cache. The rerun
skips deployment through the existing PR condition, while retaining tests and
release packaging. Preserve exact run IDs, action hit/miss, helper source/duration,
tool identity and step wall times in the private verification receipt. Do not
claim a total-CI speedup until those runs establish it. The separate R2 package
and single-publisher current-main flow remain unchanged.
