# Multiplayer and online UI audit (#679)

The owner's direction (2026-11-14): the new online and multiplayer screens
must be built from the classic menu and race HUD pieces, not from dark
translucent cards. This note records what each new screen did and which
classic component replaces it. The in-ride overlays (`atv-ui/src/hud`) are
audited separately.

## Menu pages

Classic vocabulary (`crates/atv-ui/src/front/mod.rs`): the silver stage,
`heading` title strip, `menu_row` with the gold bullet and gold row pill,
`stepper_row`'s gold value face, `gold_button`, `tabs`, `course_list`, the
`details` label/value table, `plate`, `hero` and the garage `stats_panel`
meter.

Screenshots (ignored, `private/reports/devlog-images/ui-audit-menus/`,
`before/` and `after/`, 1280×720 and 360×780) were taken with
`work/ui-audit/capture_menus.mjs`, which mocks the game server and boards.
The before set is the deployed main build 6ea3dfd5.

| Page or element | Before | Classic replacement |
| --- | --- | --- |
| Main Menu → Quick Play row | Already a plain `menu_row` | Unchanged |
| Quick Play: Race / Stunts | Two invented cards, 200×150, rounded 12px, black `#000000a6` fill, gold when focused | Two `menu_row`s with the gold bullet and pill; the playlist ("Races · Hills", "Freestyle · Treasure") as a gold value on the right; the stage keeps the logo and ride |
| Multiplayer → Free Ride badges | Red glowing pill with the map name, green glowing dot and "n riding" | The map and riders as one gold value on the right ("Caribou Peak · 3 riding"), dimmed grey when nobody rides; no glow |
| Friends list | Glyph prefixes in the label (`●`, `○`, `+`, `…`, `⊘`) and "· Join" | `Name: Value` rows: the name, then Join / Online / Offline / Request / Sent / Blocked in the gold value face (Offline dimmed) |
| Friends: Recent row and Recent Riders | "Recent · 2", "✓ Ann · 2h" | "Recent: 2"; "Ann: Friend", "Vega: 1h" |
| Friends table, rider page | `● Online`, `○ Offline`, `+ Wants to be friends`, `… Sent`, `◆ Gold` | Plain labels in the `details` table: Online / Offline, Friend Request, Request Sent, Gold |
| Profile → Online card | Level and Next Tier as `▰▰▱▱` text, League `◆ Silver · 420` | The garage stat meter (ten gold steps) before the numbers; League `Silver · 420` |
| Online → Friends row | "Friends · ● 2 · +1" | "Friends" with "+1 New" (requests) or "2 Online" as its value |
| Weekly League | `▶` markers on the rider's rows | Plain ranked rows; "Your Rank: #4 · Top 2%" |
| Weekly Challenge page | Already the `details` table and a `menu_row` | Unchanged |
| Medal coins (course list, Profile Overview strip, results) | Flat linear-gradient coins and flat badge pills | The menus' glossy bullet shading (radial highlight, dark rim, short hard shadow) in the medal's colour |
| Single Race → Ghost row | Already a `stepper_row` | Unchanged |
| Host/Join Game name dice | White die on a dark `#0000008c` square | Dark die on a small gold pill (as Ride and Done) |

## Open items

- The Host/Join text fields keep the port's dark input boxes, which predate
  the online work and are shared with Profile Name entry and Enter Cheats.
- The Author medal stays green (`atv_core::medals::Medal::color`); the colour
  identifies the medal on results and lists.
- Host Chromium only; physical phones were not checked.
