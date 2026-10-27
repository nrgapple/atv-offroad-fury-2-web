# Profile Points and the player profile

The original keeps single-player progress in profiles: Profile Points earned
in events buy items in the Profile Store, and each profile keeps a race
record. The port has the original Profile Editor and View Profile pages over
a local profile book; the Profile Store and memory-card UI are not ported.
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

- **Course record (`001f1e30`, `00321ff0`, `00322080`, `00324b40`).** The
  finishing human's per-course record (world +0x8c per player, 0x94 bytes,
  found by `00321ff0` from the event type, course and, on Expert, an offset
  past the Normal list) counts races and victories and a victory for the
  ATV ridden; at the default lap count only (3 Enduro, 5 otherwise)
  `00324b40` inserts the race time into the course's top three
  ("New Personal %sFastest 5 Lap Race Time!") and `003325d0` the Hall of
  Fame's. `00322080` then, unless the mode is 1 or 0x15 or more than one
  player rides, adds a race (+0x14) and, for a win, a victory (+0x18) and a
  victory for one of 26 per-ATV counters (+0x2c) to the profile, keeps the
  longest jump (+0x1c), most combos (+0x24) and most points in one jump
  (+0x28) as maxima, and copies the course record back. Inferred, not
  traced: which per-ATV counter the loop picks (it takes the first non-zero
  entry of the race record's ATV columns).
- **Profile pages (UI page archives, labels).** `Profile_Main` (PAGE19):
  Create New Profile, Load Profile, Save Current Profile, View Current
  Profile, Unlock Items, Copy Profile, Delete Profile, and a Warning
  Yes/No. `Profile_NameEntry` (PAGE21): "Enter Player Profile Save File
  Name:" over eight cells ("XXXXXXXX", "________", font `namenumber_entry`)
  and the four "Player Profile save file 1–4" slots of each of "MEMORY CARD
  slot 1-A…1-D". Load (PAGE23), Copy (PAGE22) and Delete (PAGE53) share
  that slot layout. `ProfileView_Menu` (PAGE24): Overview, Trophies,
  Records, ATVs, Rider Equipment, Games. `ProfileView_Main` (PAGE25):
  Percent Complete, Profile Points Earned, Profile Points Remaining, Total
  Races, Total Victories, Win Percentage, Most Winning ATV, Longest Jump,
  Most Stunt Points In An Objective, Most Combos, Most Points In One Jump.
  `ProfileView_Champ` (PAGE26): Highest Trophy Awarded per championship on
  Normal and Expert (None/Gold/Silver/Bronze). Records pages PAGE27–35
  (Am/Pro Nationals, Enduro, Supercross, Freestyle, Short Tracks; classes
  `QuadzillaUIPageProfileView_*`): Total Races, Total Victories, Best Lap
  Time, Best 5-Lap (Enduro 3-Lap) Time, Longest Jump, Most Winning ATV per
  course; Freestyle pages Most Combos, Highest Stunt Score, Highest
  Objective Score, Longest Jump, Most Winning ATV.
- **Strings.** "PLAYER1" beside the Profile Editor page names (`0x4347b8`,
  read as the name entry's default); "Delete this save file?" / "Player
  Profile save %d on MEMORY CARD slot %s." (`0x4314c0`); "Are you sure you
  wish to load profile %s?" (`0x434c70`); "Load/Copy/Delete Successful.";
  "Are you sure you want to create a new profile? Any unsaved data will be
  lost." (`0x430e20`); "No ATV Offroad Fury 2 profiles present on…"
  (`0x431e00`); "Remember, your progress is not being saved! To begin saving
  your progress, create or load a profile in the Profile Editor."
  (`0x431440`); the No Profile Alert "You must have an ATV Offroad Fury 2
  Profile loaded … To create or load a Profile, use the Profile Editor."
  (`0x432490`, used for waypoint races; `0x4323e0` for online); the
  autosave alerts ("Autosaving data to memory card…", "Autosave is currently
  disabled. Would you like to use your current settings to create a profile
  so that your progress will be saved?", selected by alert code in
  `0025e120`); and the overview's "%3.0f%%".
- **Manual (secondary).** The main menu recommends creating a profile before
  playing; Create Profile picks ATV and gear and a name and number, then a
  card (up to four profiles per card) and a save name; the game autosaves;
  Profile Points are earned only in single-player Race, Freestyle and
  Championship; online play requires a profile.

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
- **Profile book (authored storage).** The memory card maps to local
  storage: `fury-profiles` (browser `localStorage`, desktop settings file)
  holds up to four named profiles (one card's four save files) and the
  loaded one; `fury-profile` mirrors the loaded profile, which races credit
  once when the results open (the original's autosave). A save from before
  the book migrates: its implicit profile becomes the loaded PLAYER1, so
  play keeps its progress. Card slots, formatting, space and removal alerts
  have no equivalent and are not shown; "Save Current Profile" is left out
  because every change is saved.
- **Profile Editor.** Main Menu → Profile Editor sits after the ride modes
  and before Settings (the original's fourth row, before Hall of Fame and
  Options). Rows: Create New Profile, Load Profile, View Current Profile,
  Unlock Items (the Profile Store, `PROFILE_STORE.md`),
  Copy Profile, Delete Profile; the line under them reads "Current Profile:
  NAME · Current Profile Points: N". Create and Copy open Profile Name
  Entry: up to eight characters, upper case letters, digits, space and
  `- _ . !` (authored: the `namenumber_entry` glyph set is not recovered),
  starting on PLAYER1 or the next free PLAYERn; names are unique
  (authored: the card's save files are numbered, the original's duplicate
  handling is not recovered). It takes a keyboard or a phone's soft keyboard
  (the page's text input) and a controller's Add Letter row (Left/Right
  picks a character, accept adds it) and Delete Letter. Create saves and
  loads the new profile; Copy saves a renamed copy and keeps the loaded
  one. Load asks "Are you sure you wish to load profile NAME?"; Delete asks
  "Delete this save file?" (starting on No); deleting the loaded profile
  leaves none loaded.
- **View Profile.** Overview (PAGE25 rows), Trophies (each offered series'
  best trophy on Normal / Expert; Freestyle and Short Tracks read "Not
  offered") and Records (event and course rows step through the race
  courses the content holds, grouped as the PAGE27–35 event pages: Total
  Races, Total Victories, Best Lap Time, Best 5-Lap or 3-Lap Time, Most
  Winning ATV). The port measures no jump length, combo count or Freestyle
  objective, so Longest Jump, Most Combos, Most Points In One Jump and Most
  Stunt Points In An Objective read "-"; Overview adds the port's Highest
  Race Stunt Score. Percent Complete is +0x6c1 over the
  original total 201 (`00320db0` sets +0x6c0 to 0xc9): each item unlocked
  one by one (`00323a00`) plus the championship trophy score (gold 3,
  silver 2, bronze 1); items opened by an unlock cheat, which the port keeps
  as flags, are not counted. Profile Points Earned is a lifetime total
  (`earned`) beside the spendable `points` (Remaining), which the store
  spends. ATVs, Rider Equipment and Games show the store's pages without
  Purchase Item (`PROFILE_STORE.md`).
- **Course record.** Each race adds to the loaded profile's record for its
  course id: races, victories, victories per vehicle, best lap and, only at
  the default laps (3 Enduro, 5 otherwise), best total time (one best, not
  the original's top three); per-vehicle victories also count profile-wide
  for Most Winning ATV.
- **No profile loaded.** As `001f1e30` gives a rider without a profile
  nothing, a race then earns no points, is not recorded and its results add
  "Remember, your progress is not being saved!"; Single Player reads "No
  profile loaded · …", and Championship (whose progress lives in the
  profile) and View Profile show the No Profile Alert text (authored
  placement: the original shows that alert for online play and waypoint
  races and offers to create a profile instead).
- Single Player shows "NAME · Profile Points N · R races · W wins" under its
  rows. Championship progress is kept in the same record
  (`CHAMPIONSHIPS.md`).

Checks: `atv-core` profile tests (book create/load/copy/delete, names,
migration, course records, earned points) and race tests (table parsing,
award rule, stunt window); `atv-ui` settings tests (migration, mirror, no
profile) and Profile Editor flow tests; `tools/test_engine_profile_editor.mjs`
migrates an old save, creates, copies, deletes and loads profiles through
the pages, checks Overview/Trophies/Records, a reload, and a race crediting
the loaded profile's sx01 record; `tools/test_engine_race_profile_points.mjs` drives the player
with the race path driver through a one-lap SX01 race and checks the trace
standings, the award and the saved profile; `tools/test_engine_profile_menu.mjs`
checks the Single Player readout for a saved and an unreadable record.
