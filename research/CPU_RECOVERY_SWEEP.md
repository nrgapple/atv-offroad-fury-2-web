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
