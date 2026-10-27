# The Training Course

The original Main Menu's third row, Training Course, rides six lessons on the
Training venue (TR/SO archives, see [FREESTYLE_MAPS.md](FREESTYLE_MAPS.md)).
Completing them is the only way, cheats aside, to own the four coloured Ravage
Talons and the ATV2 helmet, jersey, sleeves and pants
([PROFILE_STORE.md](PROFILE_STORE.md)). This note records the exact-build
evidence, the port and what remains open.

## Evidence

Private Ghidra exports of the exact USA v2.00 `SCUS_972.11` (bounded reading
of `QuadzillaEventTraining.cpp`, vtable `00457e30`, no runtime traces) and the
disc's `AUDIO/TRAINING/DIALOGUE.AUD`. Observed facts:

- **Event object.** Constructor `00224450`, entry `00224a80`, per-frame update
  `00225980`, input `00225200`, Restart Training `00226d60`, lesson resolve
  `00226f30`, result `002273a0`, per-lesson reset `00224860`, dialogue loader
  `002275a0`. Fields: +0x1d8 phase (0 introduction, 1 lessons, 2 ending,
  3 done), +0x1d4 lesson, +0x1d0 lesson state (0 demonstration, 1 active,
  2 success, 3 failure), +0x1f8 speech (queue, playing, done).
- **Lessons.** `004085f0` names six: Gas and Brake, Reverse, Steering, Jump and
  Land, Stunt, Preload. Starts at `004084f0`, facings at `00408540`.
- **Circles.** `00408590` holds four (centre, radius 18). `00227140` counts a
  circle only while the ATV has ground contact. `00227250` gives each lesson's
  start circle (3, 0, 3, 1, 1, 2) and the jump table at `0042d3f0` (via
  `00227540`) its goal (0, 3, 1, 2, 2, 3). `00224860` hangs an `arrow.slt`
  marker 25 units above the goal; Steering also marks circle 0.
- **Rules** (`00225980`, active state):
  - Gas and Brake: the drive command at or below −0.5 for a cumulative 2 s
    fails it (Special 46); stopping (speed under 1) in the goal after
    leaving the start circle passes it.
  - Reverse: the drive command at or above 0.5 for 2 s fails it (45); the goal
    passes it.
  - Steering: the second circle before the first fails it (15); circle 0 then
    circle 1 passes it.
  - Jump and Land: a rider crash fails it (21). From each take-off (contact
    lost) the update watches the flight: a nose-down pitch of −5° or more
    (`-0.08726645` rad) marks it pitched; a landing 40 or more units from the
    take-off without that fails it (20); a shorter hop rearms. The goal after
    a pitched jump passes it.
  - Stunt: in the goal while not stunting, a landed stunt passes it, otherwise
    it fails (26). Vtable +0x184 (`00227520`) records a stunt start (+0x230)
    and a landed stunt (+0x22c).
  - Preload: the same take-off/landing watch, with preload held instead of
    pitch; a 40-unit jump without it fails (34), the goal fails without it
    and passes with it.
  - Any wreck afterwards fails the lesson with a random Wreck line, or the
    Stunt (27) / Preload (35) crash line when that lesson's move was made.
- **Flow.** A demonstration plays `tracks\lesson%d.vcr` on the jumbotron (render
  target `00225720`/`002255c0`, images `textures\jumbotron%1d`) with the
  lesson's Jumbo lines; the lesson name shows for 2 s (`002e2cf0`). Active
  play first holds the ATV for the Chase lines (+0x948/+0x94c), then a random
  TooLong line every 24 s (no failure). `002273a0` shows "Lesson Success" or
  "Lesson Failed" for 2 s; after the line and one more second `00226f30`
  advances to the next demonstration, or retries a failure at the lesson start
  without one. In Training the wreck recovery calls `00226f30` (`001e8d10`).
  Accept (pad button 4) skips a demonstration or Chase line after 0.5 s;
  button 7 replays the lesson from its demonstration (`00225200`).
- **Completion.** IntroPrize (line 2) and FirstEnding (36) play only while the
  profile does not own store ATV 8, the blue Talon (profile +0x5c8 + 8);
  otherwise RestEnding (37). The ending always calls `0021f440`, which grants
  ATVs 8–11, helmets 20–23 and ATV2 shirts/sleeves/pants 19–22. No separate
  completion flag or per-lesson progress is saved. Restart Training
  (`00226d60`) returns to the introduction and lesson 1.
- **Dialogue.** `DIALOGUE.AUD` lists 46 VAG lines (`LINES1`, `LINES20`):
  Intro/IntroPrize, per lesson Jumbo, Chase and Special lines, Success 1–5,
  FinalSuccess, TooLong 1–6, Wreck 1–7, two endings, and 15 timed markers
  that drive jumbotron and preload-sound cues. There are no subtitles; the
  instructions exist only as speech.

## Port

- `atv_core::training` is the course state machine with the recovered tables,
  rules, timers and order, and unit tests for every lesson, the failures,
  retry, skip, replay, reminders and both endings.
- Main Menu → **Training Course** (before Profile Editor, shown when the
  Training archives are available) rides the venue with the garage choice
  under the Single Player unlocks. `atv-app/src/training.rs` samples the ride
  (position, velocity, drive, ground contact, pitch, preload, wreck, stunt
  chains), places the ATV at each lesson start, holds it on full brake while
  a line plays, and grants the reward to the loaded profile. Its end returns
  to the main menu with "Training Course complete! Unlocked the Ravage Talons
  and ATV2 gear." on a first completion. Pause shows Restart Training;
  leaving mid-course keeps nothing, as the original.
- The HUD panel shows "Lesson n of 6: name", the line's caption, the distance
  to the marked circle and the original banners.

## Adaptations and open items

- **Captions instead of speech.** The 46 VAG lines are not packaged or played;
  each line is a port-authored caption (not a transcription) shown for a
  length-based time. Hosting the lines through the R2 package and playing
  them would restore the original timing.
- **No demonstration replay or arrow model.** The jumbotron VCR replays and the
  `arrow.slt` marker are not drawn; the HUD distance stands in for the arrow.
  The dialogue markers' jumbotron/preload cues are not reproduced.
- **Field mappings (assumptions).** The drive command (+0x9e4) is read as gas,
  or brake once the ATV no longer rolls forward; +0x198 +0x34 and +0x904 as
  any wheel touching the ground; +0x198 +0x38, which arms the jump watch, as
  the first ground contact after placement; the rider-crash and wreck fields
  as the port's wreck phase; stunting as an unfinished stunt chain.
- **Pad mapping.** Accept is keyboard Enter/Space, the pad's South button or
  a tap; button 7's replay is implemented in the core but not bound.
- **Order of the introduction.** The original also waits for +0x20c (set
  elsewhere, likely the opening camera) before lesson 1; the port starts once
  the lines end.

Checks: `cargo test -p atv-core training`, `atv-ui`
`training_course_row_rides_the_training_venue`, `ui_menu` pause labels, and
`tools/test_engine_training.mjs` (content group): lesson 1 failed by
reversing and then ridden to a stop, lessons 2–6 passed by the
`trainingPass=1` hook, the Talons owned and shown Unlocked, a second visit
without the prize line, Restart Training and Quit to Main Menu.
