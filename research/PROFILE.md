# Profile Points and the player profile

The original keeps single-player progress in profiles: Profile Points earned
in events buy items in the Profile Store, and each profile keeps a race
record. The port has no original profile screens or memory-card saves yet.
This note records what is recovered and what the port does with it.

## Exact-build evidence

Private exports of the exact USA v2.00 `SCUS_972.11`, bounded reading (no
instruction comparisons, no hardware runs):

- **Course tables (`0020a8d0`).** Race loading reads the race TOY
  `[ChampionshipInfo]` section: `ChampQuadGroup` (already used by the CPU
  roster, `RACING.md`) and, for places 1–5, `AmNormalPoints%d`,
  `ProNormalPoints%d`, `AmHardPoints%d` and `ProHardPoints%d` into four
  five-entry tables. Missing keys read 0. Shipped TOYs fill places 1–3 (for
  example 20/10/5 Amateur Normal and 60/30/15 Amateur Hard).
- **Race award (`001f1e30`).** When a human racer finishes, and only with one
  human player (race settings +0x10 = 1):
  - outside a championship (+0x36d clear), 1st, 2nd and 3rd earn 10, 5 and
    1 points; 1st also counts a win in the race record;
  - in a championship the place points come from the course table chosen by
    the series (Pro when `0021db00` reports a Pro event) and difficulty (Hard
    when +0x1890 = 2), places 1–3 only;
  - then the rider's race stunt score / 1000 is added, truncated to an
    integer;
  - a rider with no profile, or in place 5, earns 0.
  The total is added to the profile (`00323440`) and the race record is
  updated (`00322080`: races, wins, per-ATV wins and best values) before the
  results open.
- **Results (`Overlays.pb` `[RaceOver Menu]`).** The rows are Total Times,
  Best Laps, Total Stunts (score value callback 0, per place) and Profile
  Points (value callback 59, per place). The executable also holds the
  results message "and %d Profile Points!".

Unresolved: the value callback 59 body (what a CPU row shows; the port shows
"-"), the exact race record layout beyond races/wins, and CPU stunt scores (the port does not score CPU
tricks, so CPU rows read 0).

## Port

- `atv_core::profile`: `CourseAwards` reads the TOY tables; `race_award`
  applies the `001f1e30` rule for single races and championship rounds;
  `Profile` keeps lifetime points, races, wins and the best race stunt score.
- `atv_core::race`: each racer's race stunt score is the player's committed
  stunt points scored between GO and the finish (`record_player_stunt`), shown
  per standing with a `player` flag.
- Race Results show Stunt Score and Profile Points columns and append
  "+N Profile Points (T total)" to the summary.
- **Authored:** the original requires a loaded profile. The port keeps one
  implicit local profile, saved as JSON under `fury-profile` (browser
  `localStorage`, desktop settings file) and credited once when the results
  open. Single Player shows "Profile Points N · R races · W wins" under its
  rows (the original's "Current Profile Points:" readout lives on its
  profile pages). Original profile creation, names, multiple profiles, the Profile
  Store and memory-card saves remain missing.

Checks: `atv-core` profile and race tests (table parsing, award rule,
stunt window); `tools/test_engine_race_profile_points.mjs` drives the player
with the race path driver through a one-lap SX01 race and checks the trace
standings, the award and the saved profile; `tools/test_engine_profile_menu.mjs`
checks the Single Player readout for a saved and an unreadable record.
