# Replays (#542)

## Full-race replay — 2026-11-14

A port enhancement. The results' **Replay** plays the whole race from its
start (the grid during the intro and countdown) with the player and every CPU
rider, through the finish, using the instant replay's controls (play/pause,
½×/¼×, ±2 s skips, scrub bar, Home/End), its Chase, Trackside and Orbit
cameras and photo mode from a paused replay. Pause → Replay still opens 15 s
before the latest moment, but in a race it can scrub back to the start.

Evidence (facts):
- The executable's string inventory (`private/reports/ghidra/inventory.json`)
  contains only "Stop Action Replay Cam Enabled!/Disabled!" (the STOPCAM
  cheat) for "replay"; no replay menu or race-replay text was found.
- The original Training Course plays jumbotron replays (`TRAINING.md`); those
  are not this feature.

Assumption: the original has no full-race replay, so this is an approved
enhancement with no original behavior to match.

Implementation (`crates/atv-app/src/instant_replay.rs`, `replay_view.rs`):
- The instant replay recorder gained a whole-race mode, chosen while the ride
  is a race. It samples the drawn poses and ride camera at 15 Hz (every fourth
  step) from the first drawn frame and stops 3 s after race over (every local
  human finished). A restart (the step going back) starts a new recording.
- Storage is bounded by a 24 MiB matrix budget plus at most 9,000 samples of
  bookkeeping (about 1.2 MiB). When the budget is full, every other sample is
  dropped in place and the sampling halves. A long race therefore fits the
  same memory with coarser samples. Rows are allocated as the race goes, so a
  short race uses less.
- Each rider's two world matrices are stored exactly. Node matrices are
  quantised to 16 bits: axes to 1/8192, and translations to 1/128 of a unit
  from the first node of their part (±256 units). This uses 24 bytes per node
  instead of 48. The free-ride 30 s ring uses the same encoding.
- Playback, blending and the cameras are unchanged. Nothing feeds back into
  the simulation. A ride that is not recording or replaying pays nothing new.

Measured (host Chromium, `tools/test_engine_race_replay.mjs`, sx01, one lap,
three CPUs, 1:16 recorded): 1,056 samples at 15 Hz, 200 cells (4.7 KiB) per
sample (about 45 nodes per rider), 9.6 MiB allocated. Before compaction the
same race preallocated 78.6 MiB. At that row width the 24 MiB budget holds
5,242 samples (5.8 min at 15 Hz). Eight riders hold about 2.9 min before the
first thinning. The worst case is about 25 MiB (the budget plus about 1 MiB of
sample bookkeeping), whatever the course length. Courses' laps run 1,748–9,974 ft (`RACING.md`). A race longer than
the 15 Hz capacity is thinned to 7.5 Hz, then 3.75 Hz.

Limitations: no replay sound or particles (the dust, spray and crash effects
are not recorded). Online races do not record. Weather advances with
playback, but other world animation is not rewound.
