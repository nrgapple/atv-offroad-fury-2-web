# CPU repeat-wreck sweep — issue #545, 2026-11-14

Strategy: preserve the recovered path, driver, vehicle and wreck routines;
improve only the explicitly authorized authored repeat/stall recovery layer.
PR #403 already supplied that layer. All four requested courses finish on the
current seed-7 baseline, but riders still spend many attempts on the same
physical failures. This checkpoint reduces those repeated attempts; it does
not establish original-game driving parity or eliminate natural wrecks.

## Where the failures recur

The complete baseline incident logs put sx15 wrecks around (320, 280),
(560, 520) and (600, 440), sx03 around (80, 80) and (360, 600), and sx05
around (560, 440) and (440, 240), rounded to 40-foot bins. These are fresh
host observations, not PS2 traces.

The sx15 disc TDF's main route has a flag-0x70 jump at node 32, approximately
(608, 488), with a 220 ft/s speed, followed by the flag-0x30 landing nodes
at approximately (608, 438) and (608, 413). Its late-course stretch also
passes the (320, 280) cluster, near nodes 161–164; nodes 162–163 carry the
no-reset bit. The historical exact-build reset/approach investigation in
[RACING.md](RACING.md) identifies the accepted point before the jump and
sx03/sx05's corner-before-jump recovery (sx05 node 44, jump node 47).
The reset clearance test checks a placement, not whether a quad starting
there can negotiate the following corner and jump. An accepted point can
therefore reproduce the same failure; an 80-foot authored skip can still
leave the rider inside that approach.

Important distinction: the historical sx15 75 ft/s leader-cap finding was
for Normal. `Options::difficulty(2)` makes this sweep Expert, which already
bypasses that cap. This checkpoint does not claim the cap explains these
Expert failures. Exact original takeoff/landing behavior remains unverified.
The fresh results support improving recovery placement and repetition cost,
not changing recovered speed or physics rules.

The historical env05endl pin was a rider held against an obstacle inside
the wide Enduro polygon: no off-track avoidance/reset, speed bouncing above
the slow-riding threshold, and a reset to its own last valid position. The
existing route-progress stall detector addresses that case. It does not
reproduce as a stall here: all four riders finish with no authored recovery
in both seed-7 runs. Its physical wreck count is unchanged.

## Authored change

Assist on the second reset within 80 feet of the previous accepted reset,
then skip on the third, rather than assist on the third and skip on the
fourth. One assisted retry remains before relocating. A skip tries validated
placements 320, 360, 400, 440, 480 and 500 feet ahead instead of beginning at
80 feet. The maximum displacement, clearance validation, fallback placement,
15-second stall rule and 400-foot clearing rule are unchanged. Finish and
Enduro candidates still stop 20 feet before their gate; missed Enduro gates
retain the existing return-before-the-gate behavior. Finished riders never
receive a skip. There are no new per-frame allocations.

## Full-length measurements

USA v2.00 hosted content; four original CPU roster seats; Expert (2); seed 7;
player idle; weather off; each course's default race length. Before is main
58da9aee (the intervening main changes affect menus, not race simulation).
After is this checkpoint. Times are simulation seconds at the last CPU's
actual finish; all riders finish without estimates.

Wrecks count transitions out of Riding only through that rider's finish.
The baseline's complete bounded incident log was filtered by each rider's
exact finishing tick: invert the race's f32 60 Hz clock and add its observed
241-step countdown offset. Every finish/elapsed clock matches that timeline
exactly, and no incident log is truncated. The updated example excludes
post-finish driving directly. This avoids counting crashes by already
finished CPUs while waiting for the last rider.

| Course | Laps | Wrecks before | After | Last CPU finish before (s) | After (s) |
| --- | ---: | ---: | ---: | ---: | ---: |
| sx15 | 5 | 155 | 126 | 591.50 | 488.39 |
| sx03 | 5 | 50 | 48 | 405.65 | 425.52 |
| sx05 | 5 | 58 | 49 | 438.67 | 398.06 |
| env05endl | 3 | 13 | 13 | 431.22 | 431.22 |
| nat01 | 5 | 0 | 0 | 235.39 | 235.39 |
| sx12 | 5 | 0 | 0 | 230.32 | 230.32 |
| env06st | 5 | 0 | 0 | 87.39 | 87.39 |

Sx03's last rider takes 19.88 seconds longer despite two fewer wrecks; this
is not an across-the-board race-time improvement. Sx15 and sx05 finish
103.11 and 40.61 seconds sooner. The three control courses have identical
recorded riders, lap marks, times, incident logs, roster, recovery counters,
step counts and elapsed clocks. This is a three-course control comparison,
not a claim that all other courses or seeds were swept.

Reproduce with the ignored content package, through the host lease:

```sh
RUSTC_WRAPPER="$PWD/tools/sccache-rustc" python3 tools/host_build.py -- \
  cargo run --release -p atv-game --example cpu_times -- \
  private/reports/hosted-content out.jsonl \
  --courses sx15,sx03,sx05,env05endl,nat01,sx12,env06st \
  --difficulty 2 --seed 7 --require-finish
```

Focused validation: the CPU recovery observer tests; recovery-station tests
that exercise progress credit and finish/Enduro gate bounds; `make precheck`;
`make web`; and `tools/test_engine_cpu_recovery.mjs`, registered in the content
group. The browser test runs one SX15 lap with four Expert CPUs and an idle
player on `/play.html`, requiring actual CPU finishes. These are host-native
and host-Chromium checks, not physical-phone measurements. All focused gates
pass on the final policy, including the rebuilt Wasm and SX15 browser test.

Limitations: authored thresholds and placement; substantial remaining wrecks;
no fresh PS2 execution comparison; one full-length seed; original corner,
preload and landing fidelity remain open. A recovery can credit up to 500
feet of route that the rider did not drive, while still requiring the gate.

# Supercross wreck causes — issue #643, 2026-11-14

Strategy: classify where and how the CPUs wreck, compare the driver with
the exact build, and correct the driver only where it differs. The
recovery layer above is unchanged.

## Where the wrecks were

`cpu_times` now records, for every wreck, its kind, the race station's
node, the airborne run (takeoff node and speed, air time, time since
landing), contact counts and impact point, the offset from the racing
line and the nearest rider. Expert, seed 7, four CPUs, idle player, five
laps, before this change (host observations, not PS2 traces):

- **Airborne back wrecks on the big jumps** (pitch above the 85° air
  limit before touchdown): sx03 node 122 (18 of 48, takeoff at node 118 at
  about 75 ft/s, 1.6 s in the air), sx05 nodes 184–185 (16 of 49, takeoff
  at node 181 at 78–82 ft/s), sx15 nodes 159–162 and 32–33 (about 20 of
  126).
- **Grounded front impacts against the infield hay bales of hairpins**:
  sx15 nodes 26–27, 43–45 and 135–137 (`hay_tit`, `hay_hbm`, `hay_sho`),
  sx05 nodes 42–43 (`hay_rkc`), sx03 nodes 77–85 (`hay_one`). The rider
  is 12–33 ft inside the line, still within the track polygon (half
  widths 33–44 ft; the two legs of each hairpin are closer together than
  their widths), at 40–65 ft/s, with 1–11 static obstacle contacts. Every
  hit lands on the same bale each lap.
- **Rider contacts** at the same corners (no obstacle contact, another
  rider within 8 ft).

## Exact-build reading

Private Ghidra exports of SCUS_972.11, bounded reading; nothing traced or
executed.

Facts:

- **Speed and throttle (`0024f9d0`).** The target speed is the aim node's
  (+0x278) TDF tag 18 (−1 means 150 ft/s), blended with the next node's
  (+0x54) by the aim fraction +0x27c under AI flag 0x40000, which race
  creation `001f9690` sets for every CPU. Throttle and brake are 1 or 0
  around 1 ft/s deadbands (+0x2b4/+0x2b8 = 1, `001f9690`). The speed
  compared is the body velocity magnitude (`001eb380`: AI +0xa0 =
  |body +0x270|). No other Supercross or jump speed rule exists in the
  driver; the flag-0x40 preload and the Normal leader limit are as already
  ported. The port matches. On sx15 the jump nodes ask 58.7–88 ft/s and
  most others 150 or 220 ft/s, so the CPUs run flat out between jumps.
- **Heading gain.** The constructor `0024ef20` stores 2.5 at +0x238 and race
  creation overwrites it with 5, the port's value. `001ab8f0` is atan2,
  built on the acos table at `001ab720`.
- **Air lean.** Above 20 ft (+0x3d8) the lean is
  sin(clamp(+0x24c × (+0x90 − +0x200), ±π/2)) with +0x24c = 1.5 and
  +0x200 = −+0x23c × atan2(d·up, d·forward), +0x23c = 0.5, where d is the
  corrected direction minus the nose. `001eb380` copies body +0x2a0..+0x2a8
  into AI +0x90..+0x98, and `001469e0` integrates body +0x2a0 into the
  orientation quaternion (q̇ = ½ q ω) after clamping it as a rate. +0x90 is
  therefore the body-local pitch rate, not an angle.
- **Wreck limits.** `001e8690` scales the vertical impact limit by 1.25 only
  for track kinds 0–2 (Enduro, Freestyle); race courses use 1.0, as the
  port does. The port never applies the 1.25 (#660).
- The hay bales are ordinary static collision models (material 0).

Not established: whether the original CPUs strike the same bales; the
aim/route locator `0017b9d0` called by `0017cf10` is not in the exports;
the original lookahead measures the distance to RaceStatus's station point
(+0xb8), where the port re-projects the rider within its station window.

## Change

The port compared an Euler pitch, atan2(2(wx + yz), 1 − 2(x² + y²)), with
the wanted rate +0x200. That angle also depends on the heading (the same
30° pitch reads 30°, 90° and −150° at headings 0°, 90° and 180°), so a CPU
in the air received a lean unrelated to its rotation and could rotate
backwards past the limit. `race_ai::Driver` now
uses the body's local angular velocity x (`DriverPose::local_angular_velocity`),
as `0024f9d0` does. It also drives the path driver on the player's quad
(autopilot, after the finish). Nothing else changes.

## Measurements

Before is this branch's base (origin/main 88d4f02b) with the new
telemetry; after adds the lean change. Same setup as above: four original
CPU seats, idle player, weather off, five laps, hosted content, host-native
`cpu_times`. Wrecks are summed over the four CPUs up to each one's finish;
times are the last CPU's finish in simulation seconds; every race finishes.

| Course | Difficulty | Seed | Wrecks before | After | Last finish before | After |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| sx15 | Expert | 7 | 126 | 56 | 488.4 | 393.7 |
| sx15 | Expert | 11 | 122 | 60 | 468.5 | 429.1 |
| sx15 | Expert | 23 | 135 | 61 | 478.6 | 446.2 |
| sx03 | Expert | 7 | 48 | 43 | 425.5 | 408.9 |
| sx03 | Expert | 11 | 50 | 38 | 413.8 | 404.0 |
| sx03 | Expert | 23 | 59 | 41 | 436.6 | 380.5 |
| sx05 | Expert | 7 | 49 | 39 | 398.1 | 380.9 |
| sx05 | Expert | 11 | 52 | 35 | 405.9 | 370.4 |
| sx05 | Expert | 23 | 53 | 37 | 398.3 | 352.6 |
| sx15 | Normal | 7 | 169 | 119 | 557.1 | 528.3 |
| sx15 | Normal | 11 | 168 | 137 | 562.0 | 524.8 |
| sx15 | Normal | 23 | 168 | 115 | 573.9 | 527.2 |
| sx03 | Normal | 7 | 36 | 42 | 447.6 | 432.6 |
| sx03 | Normal | 11 | 36 | 28 | 468.7 | 433.7 |
| sx03 | Normal | 23 | 34 | 42 | 452.4 | 437.1 |
| sx05 | Normal | 7 | 50 | 46 | 422.9 | 421.4 |
| sx05 | Normal | 11 | 50 | 45 | 430.9 | 491.8 |
| sx05 | Normal | 23 | 40 | 49 | 422.5 | 429.8 |

All 51 race courses, Expert, seed 7: 1,369 CPU wrecks before, 887 after;
the 16 Supercross courses 615 → 330. Large drops: sx14 65 → 16, sx02
82 → 48, sx16 66 → 29, sx04 21 → 2, sx09 22 → 1, env07 92 → 33,
env04ends 89 → 43, nat07 62 → 34. Rises: env04 13 → 23, env01endl
30 → 37, nat06 34 → 40, env03 9 → 11. nat01, sx08, sx12, env05, env06st
stay at 0; env05endl and the other unchanged courses ride identically.
Best CPU laps fall on several courses (sx15 80.9 → 64.2 s, sx02 82.0 →
62.9, sx14 77.7 → 64.6, sx04 59.3 → 49.8, env07 107.4 → 89.7), below the
world-board house-rider anchors measured with the old driver (#665).

What is left (Expert, seed 7, after): on sx15 25 of 56 wrecks are the
hairpin hay bales, 18 airborne or landing, 7 rider contacts; on Normal with
the idle player 77 of 119 are bale and rhythm-section obstacle hits, made
worse by the leader limit's slow approaches. sx03's landing at node 123
after the node-118 jump is now a front impact on touchdown (12 of 43). The
counts are far above nat01/sx12's zero, so #643 stays open for the
hairpin bales, which need exact-build evidence (does the original CPU
cut the hairpin the same way, and does `0017b9d0` change the aim) before
any driving change.

Validation: `race_ai` unit tests (including the new pitch-rate case),
`make precheck`, `make web`, and `tools/test_engine_cpu_recovery.mjs`
(one SX15 lap, four Expert CPUs, idle player on `/play.html`). Host-native
and host-Chromium results only; no PS2 comparison.
