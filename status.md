# Status and coordination

This file holds only current coordination rules. It is not a progress log and
records no commit IDs, test counts or chat thread IDs; those go stale. Earlier
versions are in `git log -p -- status.md`.

## Where current state lives

- **Deployed build:** `https://furyremastered.com/deployment.json` names the live
  commit. GitHub Actions history on `main` shows each deployment.
- **Component status and known limits:** `HANDOFF.md`, then the matching
  `research/*.md` note.
- **Checkpoint evidence:** dev-log entries in `research/devlog/entries/` and the
  public `/devlog.html`.
- **Release milestones:** `python3 tools/release_checkpoint.py status
  --checkpoint <id>` (receipts in ignored shared
  `private/reports/release-checkpoints/`).

## Claiming work

- Each chat works in its own worktree and task branch from current
  `origin/main` (`AGENTS.md`). `git worktree list` shows active branches; do not
  reuse or edit another chat's worktree.
- Before release work, claim the checkpoint with
  `tools/release_checkpoint.py claim --checkpoint <id> --owner <name>`. A second
  owner for the same checkpoint is rejected.
- Shared files (`crates/atv-web/src/lib.rs`, `web/game.js`, `web/frontend.js`,
  build/release tools, `Makefile`) change often; keep edits narrow and rebase
  on `origin/main` before opening a PR.

## Priorities

The baseline goal is an almost exact port of the original, followed by optional
enhancements behind explicit settings. Open areas are listed under "Open issues
and known limits" in `HANDOFF.md`.

Product decisions (user, 2026-10-27):

- **Local split-screen is wanted** so people can play on a TV with several
  controllers. Every mode that works in split-screen must also work in online
  multiplayer rooms, and online rooms should offer all of these modes (races,
  championships where sensible, Freestyle, mini-games), up to the room's
  configured player cap.
- **CPU riders must not get stuck.** Where the original's recovered rules leave a
  CPU rider looping or stranded, the port may add smarter authored recovery
  (for example resetting further along the route). Keep it separate from
  recovered logic and label it as a port adaptation in the research note.
