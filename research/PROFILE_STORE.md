# The Profile Store ("Unlock Items")

The original spends Profile Points in the Profile Store, reached from Main
Menu → Profile Editor → Unlock Items (the manual's "Unlock Items" section;
page `ProfileStore_Menu`, rows ATVs, Rider Equipment, Games, Tracks and
Cheats). This note records the exact-build evidence and what the port
implements. Earning points is in `PROFILE.md`; championships in
`CHAMPIONSHIPS.md`.

## Exact-build evidence

Private exports of the exact USA v2.00 `SCUS_972.11` and its data files,
bounded reading (no runtime traces). The game manual (pages 25–31) is a
secondary source and agrees where it overlaps.

- **Catalogs.** ATVs come from `ui/frontend/uiQuad.txt` (`QUADINFO.RES`,
  25 `[QuadN]` entries: `Name`, `Access`, `IngameID`, `Points`); rider gear
  from `uiRider.txt` (`RIDEINFO.RES`, `Access_n`/`Points_n` per item, see
  `RIDER_GEAR.md`); courses from `Tracks.dat` (`access=`/`points=` per
  course, one list per event type, `0021cca0`). ATV prices run from 250
  (Patriot) through 2,500 (Nac's Racing) to 10,000 (Ravage Talon Black and
  the B6000); the Honda/Kawasaki/Bombardier stock ATVs are 1,000 but start
  owned. Helmets cost 50, jerseys 75, gloves, boots and goggles 25. Course
  prices are 100–2,500 (Short Tracks 100 each).
- **New profile (`00320db0`).** Each catalog item's owned flag is its
  `Access != 0`, so access 1 and 3 (the hidden Honda 300EX) start owned:
  eight ATVs, as the manual says. Each event type's open flag comes from
  `0021add0`, which sets every type's price to 500 and leaves all but King
  of the Hills (9) and Treasure Hunt (12) open: the manual's "two mini
  games are available, but two are locked". The championship flags
  (+0x5d4 Normal, +0x5d5 Expert) start with Amateur Enduro, Nationals and
  Supercross on Normal and Amateur Freestyle on both.
- **Purchase.** The page handlers (`0029c4d0` ATV, `0029daa0` gear,
  `00296ca0` course, `0029b3b0` game) do nothing for an owned item; else
  `00323480` subtracts the price from the *remaining* points (+0x6bc) only
  when the result stays at or above zero, sets the owned flag
  (`003235e0`, `00323580`, `00323640`, `00323850`, each counting one more
  unlocked item at +0x6c1), redraws the remaining points (`"%d"`) and plays
  sound 8. Short of points it plays sound 6 and changes nothing; no message
  string was found. Earned points (`00323440`) add to both the lifetime
  (+0x6b8) and remaining totals.
- **Not for sale.** `002f73d0` refuses store ATVs 8–11 (the blue, red,
  yellow and green Ravage Talons) and `002f8510` the ATV2 helmets 20–23 and
  ATV2 jerseys/sleeves/pants 19–22 (1-based); `0021f440` grants exactly
  these for completing the Training Course (manual: "You get those free,
  just for completing the Training Course!").
- **Jersey.** A Jersey purchase also unlocks the sleeves and pants of the
  same index (`0029daa0`; executable gear order is Helmets, Shirts, Gloves,
  Boots, Goggles, Pants, Sleeves, `0021db50`).
- **Course unlocks (`00323640`).** Unlocking a course of Amateur/Pro
  Enduro, Amateur Freestyle or Amateur Nationals (except the list's last,
  San Jacinto Isles) also opens the same index in the Waypoint Editor, King
  of the Hills, Tag and Treasure Hunt lists; Pro Freestyle opens index + 7
  of the three game lists; Short Tracks opens game index 3 (first course)
  or 5. Championship rounds call it for the next course (`003222b0`) and
  gold/silver trophies for the course after the Normal series.
- **Cheats (`0033ede0`).** Entered on Unlock Items → Cheats (page
  `Profile_NameEntry`, "Enter Cheat:", "Invalid cheat!"): GIMMEPTS ("Added
  1,000 Profile Points!", `00323440`), IGIVEUP ("Unlocked Everything!":
  the next five), SHOWROOM (every ATV but the last, G Ride), TRLBLAZR (every
  course of types 0–8 but San Jacinto Isles in the five lists that end with
  it), THREADS (all gear but each slot's last, G-Ride), GAMEON (the four
  games and their courses), GOLDCUPS ("Unlocked All Championship Events!":
  `003238a0` Normal and Expert for all 16 types, Short Tracks included),
  GABRIEL ("Unlocked San Jacinto Isles!"), and session switches WIDESCRN,
  FLYPAPER, EATDIRT, BRINGIT, GETONUP, STOPCAM and SYNC30HZ.
- **G-Ride.** The G-Ride ATV and gear were the reward for an ATV Offroad
  Fury save on the memory card (executable message at 0x4318a0); the
  cheats skip them. The purchase handlers do not refuse them.
- **Short Tracks.** Nothing found calls `003238a0` for type 8 except
  GOLDCUPS/IGIVEUP, and the manual lists 14 championships without it.
  Short Tracks *courses* are sold in the store (100 each).

Uncertain: whether the store page lists hide the access-3 Honda 300EX
(the port hides it, as access 3 is documented "Hidden"); the exact rows and
layout of the store pages (the port shows one item at a time, like the
original's "Switch Equipment"/"Switch Event" pages, but its own layout);
whether G-Ride is listed in the store.

## Port

- `atv_core::store`: catalog parsers, ownership queries, purchases,
  Training reward, course unlock cascade and the cheats, with unit tests.
  The profile keeps an additive `store` record (points spent, ATVs, gear,
  courses, games and cheats; serde defaults, so old saves load as a new
  profile's unlocks). Remaining points are lifetime points minus spent.
- Single Player → **Unlock Items** (authored location: the port has no
  Profile Editor page yet; move it there once it exists). Pages: ATVs,
  Rider Equipment (Switch Equipment: Helmet, Jersey, Gloves, Goggles,
  Boots), Games, Tracks (Switch Event through the nine event lists) and
  Cheats (a text field and Enter). Each item page shows the item with
  Locked/Unlocked, Purchase Item while locked, the "Point Cost:" and
  "Current Profile Points:" readout, and works with keyboard (Left/Right
  steps items), controller and touch. The ATV page shows the browsed ATV
  on the stage.
- **Authored messages:** "Unlocked <item>!", "Not enough Profile Points:
  … costs N." (the original only plays an error sound), and "To unlock this
  item... Complete the Training Course." for the Training items.
- **Gating (Single Player only).** The garage skips locked ATVs and gear
  (Original gear stays available) and marks locked vehicles; a locked
  saved ATV gives way to the first owned one in store order (uiQuad's
  default, the Honda 400EX Yellow) when the garage opens or a Single
  Player ride starts, and locked gear to the rider's own. Single Race and
  Freestyle refuse locked courses with "To unlock this course... Spend N
  Profile Points in Unlock Items." Championship rounds race their series'
  courses regardless, and advancing or a gold/silver trophy unlocks courses
  as above. The Short Tracks championship appears and opens with GOLDCUPS
  or IGIVEUP.
- **Not gated:** multiplayer and Arlington Free Ride (the original uses the
  host's unlocks online; the port has no profile exchange), ride links
  (`?venue=`/`?race=` URLs), the motorcycles and ATV06 (never in the
  original store), and the games (the port has no mini-games).
- **Limits.** The port has no Training Course objectives, so the coloured
  Talons and ATV2 gear are reachable only through SHOWROOM/THREADS/IGIVEUP;
  the session cheats WIDESCRN, FLYPAPER, GETONUP, STOPCAM and SYNC30HZ
  report "(not available in this port)"; EATDIRT and BRINGIT arm the next
  Single Race's AI cheats; no G-Ride memory-card reward.

Checks: `atv-core` store tests (catalogs, new-profile ownership, purchase
rule, Training items, Jersey, courses/games and cascade, cheats);
`atv-ui` `store_sells_unlocks_and_gates_single_player`;
`tools/test_engine_profile_store.mjs` enters GIMMEPTS, buys the Patriot,
is refused the Talon Black, reloads and picks the Patriot in the garage
while a locked ATV stays unselectable.
