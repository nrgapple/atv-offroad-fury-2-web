# Replays (#542)

## Broadcast camera — 2026-11-14

A fourth replay camera, **Broadcast**, after Chase, Trackside and Orbit. A
full race opened from the results starts on it; Pause → Replay still starts
on Chase.

Evidence (facts, from `RACING.md` "After-finish camera director"): the
original has a TV director only after the finish. Three seconds after race
over it follows the player between the course's authored trackside cameras
(`<venue>nat.tv`, loader `0031b550`), switching by `0031bb50` (2 s minimum,
8 s maximum wait, the closest listed next camera), rolling each cut's
weights for a follow preset or the TV view (`001f3bb0`), with the distance
FOV (`001ac580`) and the fly-by shake (`0031c2d0`). The port already has it
as `atv_core::race_director::Director`.

Implementation (`crates/atv-app/src/broadcast.rs`):
- In a race the replay runs that same director over replay time, on the
  replayed player's origin, from the live race's seed. A scrub, jump or
  camera change starts it afresh from Camera1, as the original's director
  does when it takes over. Its follow-preset cuts show the recorded ride
  camera (the replay has no live follow camera to set a preset on). The TV
  pose goes through the same cinematic view as the live TV camera. The
  fly-by shake uses the player's recorded speed only (CPU speeds are not
  recorded).
- A ride without TV cameras (a free ride's instant replay) gets built-in
  cuts: a wide crane, trackside, low, head-on, an aerial that flies alongside,
  and the ride camera, placed from the recorded line ahead. Each holds until
  the rider has passed and pulls away, its hold (3–7 s) runs out, or the
  terrain hides the rider (the other side, then a high vantage, is tried).
  These are a port invention, not original behavior.
- Nothing is allocated per frame. The course cameras are cloned once as a
  replay opens. The live director's per-step clone of the current camera
  (`tv_pose`) was removed at the same time; it now borrows it.

Memory for a five-lap race with six riders (unit test
`a_five_lap_race_of_six_riders_fits_at_seven_and_a_half_hertz`: about
six minutes, 50 cells a rider as measured on sx01): the rows are 300 cells
(7.0 KiB). The 24 MiB budget holds 3,495 of them at 15 Hz (3.9 min), so the
race thins once and plays at 7.5 Hz from the grid to the finish, in about
25 MiB including sample bookkeeping (the fixed worst case; see below).

Limitations: replay sound is not added. The ride's audio is a stateful model
fed one simulation step at a time (`AudioStep`: engine voices, placement
resets, train and actor emitters). Driving it from reconstructed replay
steps, at slow motion and across scrubs, would need a second audio model and
recorded engine inputs. That is not a clean fit with the current recorder,
so the replay stays silent as before. No particles either.

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
