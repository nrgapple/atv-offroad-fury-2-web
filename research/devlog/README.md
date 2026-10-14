# Independent checkpoint files

Record each checkpoint with the existing command:

```sh
python3 tools/update_devlog.py --title "Checkpoint title" --summary "What changed" \
  --detail "Verification and evidence" --limitation "What remains unverified"
```

The recorder creates `research/devlog/entries/<unique-id>.json`. Stage that file
on your task branch. Each checkpoint has its own ID, including when two writers
use the same title. It never rewrites a shared index, timestamp or status file.
`--image`, `--caption`, `--timestamp`, `--kind`, `--status` and `--working-on`
remain supported. Screenshots stay in ignored `private/reports/devlog-images/`.

`research/devlog.json` retains historical entries and the feature board. Leave
it unchanged when recording checkpoints. Older branches that still contain
historical-file additions remain readable, so the transition does not discard
their entries. After updating a task branch from main, use the new recorder.
Routine logging and publication results belong in independent entry files,
rather than shared `HANDOFF.md` or `research/DEPLOYMENT.md` appendices.

`tools/devlog.py` combines the historical metadata and independent files for the
local server, web build, release artifact, R2 uploader and private release
snapshots. No generated aggregate belongs in Git. `/devlog.json` and
`/devlog.html` retain their existing public formats and URLs. CI packages text
without screenshot references; the separately approved R2 gallery supplies them.
Uploaders and snapshots read the exact committed Git tree, including entry files,
so uncommitted edits cannot enter a validated release.

New entries are assembled by `recorded_at`, with ID as a deterministic tie
breaker. The page still displays them by checkpoint/capture `timestamp`.
Status and current-work overrides live inside the entry that records them;
the latest recorded override wins. An entry without an override leaves that
field unchanged. An override older than historical `updated_at` cannot replace
the historical status. Timezones are required. Duplicate IDs and filename/ID
mismatches fail the build instead of silently hiding evidence.

To correct an independent checkpoint, edit only its own JSON file. Corrections
to the same checkpoint can still conflict and require deliberate resolution;
new checkpoints on separate branches merge through ordinary Git without a
special merge driver. Intentional feature-board and system-documentation edits
remain coordinated changes.

The regression suite exercises 24 concurrent recorder calls, ordinary Git merges
with independent status updates, dirty-file isolation, screenshot snapshots,
release packaging and HTTP GET/HEAD aggregation:

```sh
python3 -m unittest discover -s tools -p 'test_devlog.py'
```
