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
  to the marked circle and the original banners. The recovered arrow
  positions (25 ft over the goal, and Steering's first circle until passed)
  show as screen tags with an edge arrow and as minimap markers, through the
  multiplayer tag renderer.
- Pitch is the body's forward-axis climb (nose down negative); in the
  browser a plain ramp jump flies at about +0.43 rad and fails Jump and Land,
  while leaning forward reaches −0.2 to −0.3 rad and passes it.

## Adaptations and open items

- **Speech with captions (2026-11-11).** The 46 lines are mono 44.1 kHz
  VAGs on the disc, `AUDIO/TRAINING/LINES1/LINE001-019.VAG` and
  `LINES20/LINE020-046.VAG`; the executable holds the strings `lines1`,
  `lines20` and `%s%s\line%03d.vag` beside its dialogue tables. The R2
  `AUDIO.RES` carries them unchanged (`training/lines*/line%03d.vag`), read
  only when a Training ride's sound loads, and `atv_game::audio::speech`
  plays the current line on the effects bus (SFX volume, paused with the
  game), stopping it when the course hushes or skips. Each line now holds
  the course for its sample's length, as the original waits for the line to
  end; the port's captions stay on screen as subtitles (the original has
  none). The introduction waits up to 8 s for the sound to load (an
  authored allowance); without the lines (older package, no sound) the
  captions keep their length-based timing. Assumptions: the original's
  dialogue volume and bus are not recovered (the port uses full volume on
  the SFX bus), and the 15 `[Markers]` cues (jumbotron and preload-sound
  timing) are not played, as there is no jumbotron replay.
- **No demonstration replay.** The jumbotron VCR replays are not drawn.
- **Arrows (2026-10-27).** `00224a80` builds two pointer objects
  (`002ea3d0`, vtable 0x45ea90) from `arrow.slt` through `002ea490` with
  scale 4 (`T00_TR.RES` `geometry/arrow.psg`: a flat 1 × 2.25 ft arrow
  along +Z, so 4 × 9 ft). `00224860` places them with `002ea880`
  (position and a direction; the side vector is direction × a stored
  reference chosen by |y| > 0.99, the up vector side × direction): the
  goal's 25 ft over its circle pointing (0, −1, 0), Steering's 25 ft over
  circle 0 pointing (1, 0, 0) (read from the stores in the assembly).
  The update `002ea500` moves the arrow 16 ft/s along its direction for
  0.45 s and back for 0.45 s, resets it at 0.9 s, looks along the
  direction (`00164dd0`) and turns its up vector by π · phase / 0.9.
  The port poses both models from the course's arrows each frame and hides
  them when not placed (`training::arrow_matrix`). Assumptions: the two
  stored reference vectors (+Z for vertical, +Y otherwise) and which of
  `001aafe0`/`001ab060` is the cosine were not read; the arrow spins, so
  only its starting phase depends on them. The port's screen tag and
  minimap marker stay as authored aids.
  The dialogue markers' jumbotron/preload cues are not reproduced.
- **Field mappings (assumptions).** The drive command (+0x9e4) is read as gas,
  or brake once the ATV no longer rolls forward; +0x198 +0x34 and +0x904 as
  any wheel touching the ground; +0x198 +0x38, which arms the jump watch, as
  the first ground contact after placement; the rider-crash and wreck fields
  as the port's wreck phase; stunting as an unfinished stunt chain.
- **Pad mapping.** Accept is keyboard Enter, the pad's South button or a tap
  (`00121cb0` resolves abstract action ids; the original's in-ride binding
  of action 4 was not traced, it may be the gas button); action 7's replay is
  implemented in the core but not bound.
- **Order of the introduction.** The original also waits for +0x20c (set
  elsewhere, likely the opening camera) before lesson 1; the port starts once
  the lines end.

Checks: `cargo test -p atv-core training`, `atv-ui`
`training_course_row_rides_the_training_venue`, `ui_menu` pause labels, and
`tools/test_engine_training.mjs` (content group): lesson 1 failed by
reversing and then ridden to a stop, lessons 2–6 passed by the
`trainingPass=1` hook, the Talons owned and shown Unlocked, a second visit
at the original pace without the prize line (Accept skip, Restart Training,
Quit to Main Menu), and every other lesson ridden from its start
(`trainingStart=N`) with keyboard input: Steering, Reverse, Jump and Land
(lean forward), Stunt (a stunt button) and Preload pass, plain jumps fail the
last three with lines 20, 26 and 34. These are host Chromium checks; the
public deployment and physical devices are separate gates.

## Voice trigger audit — 2026-11-14 (#545)

Strategy: retain the shared Rust course and disc VAG player, rather than add a
second instruction system. The disc has no subtitle text. The HUD now omits the
port-authored instruction summaries (retained only in diagnostics); no invented
spoken text or replacement recordings are shipped. Restart Training preserves
monotonically advancing speech request serials so restarting during Intro actually
interrupts and restarts that sample instead of retaining the previous voice.

Facts from bounded exact-build reading: `001508e0` maps one-based dialogue numbers
1–19 to the first folder and 20–46 to the second. `002275a0` reads the disc dialogue
configuration. Intro is 1, optional prize 2; lessons' Jumbo/Chase are 3/4, 7/8,
11/12, 16/17, 22/23, and 28–29/30–31. Success randomly selects 5/9/13/18/24,
last success is 32; idle reminders select 6/10/14/19/25/33; wrecks select 38–44.
Special failures are 46, 45, 15, 20/21, 26/27, 34/35; endings are 36/37.
These values agree with the disc dialogue data and the existing Rust tables.
`00150a20` queues a line when requested while busy, `00150e20` can stop and clear
the old queue before a failure. `00225980` queues introduction, demonstration,
Chase and reminder speech and interrupts for failures. `00150bd0` clears both the
playing handle and queued lines. The course already implements those transitions;
the mixer follows its current line, replaces the previous voice and hushes on skip.
The new audio tests also assert that a naturally ended sample never restarts merely
because its request is still current.

Volume evidence: `00224a80` supplies the dialogue group to `00150660`;
`00150a20` passes that group's handle to `002f2720`. `002852b0` applies the same
profile SFX slider value to dialogue and effects groups, separately from music.
This supports the existing SFX routing. The original also reduces other effects
and the event music to 30 during Training (`00224a80`); that relative mix is not
yet reproduced by the port's shared SFX/music controls. SPU2 group gain and timing
have not been compared in native execution. Marker-driven demo/preload effects
and the jumbotron replay remain open; the race-load cue audit is in AUDIO.md.
