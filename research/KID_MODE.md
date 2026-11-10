# Kid difficulty (port enhancement)

Kid is a difficulty that only the port has. A small child presses GO and the
game rides the race for them. The original USA v2.00 build has no such mode,
and none of the behavior below comes from the original. Original difficulties,
event types and modes behave as before. Issue #526.

## Where it is offered

- **Single Player → Race → Difficulty** cycles Normal → Expert → Kid (Left
  reverses). The setup saves Kid as `difficulty: 3` in the existing
  `atv-player-selection` race value. Old saves (1 or 2) read as before. A
  missing or unknown value reads Normal.
- Kid is offered only for the Single Race event in a one-player race. With
  Practice, Lap Attack or Duel selected, or in split screen, the row skips Kid,
  and a saved Kid reads and starts as Normal. Championships and the Hall of
  Fame difficulty rows have no Kid.
- **Race links** (Copy ride link, `?difficulty=`) degrade Kid to Normal: the
  link only writes `difficulty=expert`. Kid is a local choice.
- **Online rooms** (Host Game → CPU Riders) read a Kid setup as Normal CPUs
  and ordinary controls.

## CPU riders

Kid CPUs use the Normal options. The exact build has no easier CPU setting.
`atv_core::race_ai::Options::difficulty` treats 0 and 1 as Normal, and only
Expert leaves out AI flag 0x400 (`002f64a0`, `001f9690`, `002536b0`). The race
loads with difficulty 1 and a separate `Settings::kid` flag
(`atv-game` `ride::race`).

## Controls while racing

- **GO held:** the race path driver (`atv_core::race_ai::Driver`, slot 4, the
  same driver that rides the player after the finish and under `?autopilot=1`)
  rides the player for the whole race. Its throttle, brake, steering and lean
  pass through unchanged. The authored CPU stuck and off-track recovery
  applies, as it does after the finish.
- **Tricks:** while GO is held the player's rider also takes the CPUs' stunt
  choice (`Driver::stunt_choice`, then `request_cpu_stunt`, as
  `CpuRacer::step` does). So the driver jumps and tricks on flagged jumps the
  same way CPUs do. Nothing else uses this player path: after the finish and
  under `?autopilot=1` the driver still does no stunts.
- **GO released:** the driver is not stepped. The quad gets a full brake, with
  no throttle, steering or lean. The player's ordinary Off Track rule
  (5 seconds, then back to the course) still applies. The stuck counter does
  not run while GO is released.
- **What counts as GO:** any touch on the screen except the toolbar, Space,
  ArrowUp, W, Enter, Numpad Enter, the remappable Gas binding, and a
  controller's A/Cross (standard button 0) or right trigger (button 7).
  Pause (Escape, Start, the toolbar button) and Reset still work.
- During the countdown and after the finish, the race's ordinary imposed
  controls apply: full brake on the grid, then the post-finish driver.

## Touch

In a Kid race the normal touch layout is not used: no compass, D-pad, lean
pad, preload or trick wheel. Nothing is drawn while no finger is down. Each
finger down shows one gold "GO" disc under it, with no directions.
`atv_ui::touch::TouchControls::set_kid` tracks up to five fingers in a fixed
array, so the frame step allocates nothing. Toolbar presses are checked first,
so the GO touch never takes the Pause button.

## Results and records

The path driver rode the race, so a Kid race keeps no Hall of Fame or personal
records (`attach_records`) and awards no Profile Points (the results show "-"
in the points column, as for online races). The race finish, results panel
and the Continue, Restart and Quit choices are the standard ones.

## Limits

- Freestyle and other rides without a race course have no Kid. The difficulty
  only exists on the race setup.
- Motorcycles: the driver rides a motorcycle as it does after the finish, but
  stunt choices apply only to ATVs.
- v1 is a single race. The request's "like a 7 race" may mean a series of
  races. No series is built.

## Verification

- Unit tests: `atv-game` `kid_mode_gates_the_driver_on_go`, `atv-ui`
  `kid_mode_touches_are_go_without_directions` and
  `kid_is_a_single_race_difficulty`, `atv-core` race page label, `atv-app`
  `kid_go_reads_touch_keys_and_pad` and
  `kid_races_normal_cpus_with_the_kid_controls`.
- Browser test: `tools/test_engine_race_kid.mjs` (phone viewport, touch).
