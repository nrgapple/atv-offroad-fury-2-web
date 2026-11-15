# In-ride multiplayer overlay audit (issue #679, part 2)

Owner feedback (2026-11-14): the new multiplayer UI should be built "a
little off the classic hud and the menu screens", with no "AI background
black ui". This note lists every in-ride overlay added by epic #631, what
deviated, and what replaced it. The menu pages are audited separately in
research/UI_AUDIT_MULTIPLAYER.md.

## The reference

The look comes from the original front end, reconstructed from the
exact-build disc (private renders of Lobby, MultiATVRiderSelect, Main,
TrackSelect and Options), and from the port's classic menu pages built from
it (Hall of Fame, Select Track). The port's own Race Results panel is a dark
box, so it is not the model. `crates/atv-ui/src/hud/classic.rs` gathers the
pieces:

- **Silver panel:** the front end's `bkg_01` silver (about #b2b2b2 to
  #9e9e9e, the front's `silver` gradient) with dark lettering. Tables have
  thin row separators, as on the Hall of Fame sheet.
- **Title strip:** the page title in condensed bold italics, white, on the
  faint white strip that fades to the right.
- **Gold pill and bullets:** the glossy gold pill marks the chosen row or
  card and the main button. Places 1-3 use the menu's gold bullet, recoloured
  silver and bronze for 2nd and 3rd. Values use the original gold #e0bc00.
- **Player colours:** MultiATVRiderSelect's four columns, red #e12b00, blue
  #0f91c4, green #87c71a and yellow #fee50e. Seats past four repeat them by
  slot.
- **Track photos:** Select Track's photo in a thin gold frame. These are the
  same photos the front end loads behind the title.
- **Race HUD lettering:** clocks and counts use the condensed italic at 800
  with the overlays' dark outline, in the overlay gold and silver.
- **Secondary buttons:** a light silver plate with dark lettering.

The original's black pill hint bar is reserved for button hints; nothing here
uses it. No overlay in this change draws a black or near-black panel.

## Audit

| Overlay | What deviated (before) | Replacement (after) |
| --- | --- | --- |
| Quick Play / private room: panel | A rounded `#101010c7` card | A white title strip with the phase's clock in HUD gold, over a silver sheet |
| Lobby seats | `#0000008c` chips, a blue `#5b8def` CPU pill, a gold HOST pill and coloured ping bars | Player plates in the four player colours by seat. Each shows the name in white, a gold HOST or silver CPU tag, white ping bars and a ready tick; your plate has a gold edge and open seats are plain silver |
| Vote cards | `#000000a6` cards with invented per-mode colours | Select Track cards: the track photo in the thin gold frame, votes in gold on the photo, the track name and its mode in dark-gold capitals; your choice sits on the gold pill |
| Countdown | A big version of the vote card | The picked track's card under the title strip and clock |
| Podium and rewards (#672) | Gold, silver and bronze blocks, green `+points` pills, a `#ffffff26` bar and green `+XP` | Hall of Fame style table on silver. Columns: gold, silver and bronze place bullets, RIDER, TIME or POINTS, LEAGUE (#672's league points), with your row on the gold pill. The level strip below has "Lv n", the menu slider's gold track, `+XP` and the tier in dark-gold capitals. Confetti is gold and silver |
| Rematch / Leave | Pressing changed nothing visible (the view's `stay` was unused), so the podium's 8 s timer looked like the only effect | Gold Rematch and silver Leave buttons. A press shows at once: "Rematch ✓" with the white ring. The UI state reports `stay`/`leaving`, and both loop tests assert it right after the tap |
| Start / Mode / CPUs, Ready, Skip | `#ffffff1f` buttons with radius 8 | The gold pill for the main choice, silver plates for the rest |
| Spectator "Next round" and Watch | A `#000000a6` pill and a dark button | "NEXT ROUND" in race HUD gold with no box; Watch is a silver plate |
| Phone placement | The panel at the top covered the minimap at 360 px portrait | Placed below the minimap when the screen is tall enough. On a phone the vote shows cards two across and leaves out the seat plates |
| Free Ride hub: status line | A rounded `#0c0c0cc8` chip | Race HUD lettering: gold title, silver detail and the clock, no box |
| Free Ride hub: Challenge | A flat `#ffd166` rounded card | The gold pill button, "Challenge <name>" |
| Free Ride hub: offer prompt | A blue-black `#101820e0` card with a 20-dot ring, a green ACCEPT and a grey NO | "Race?" title strip with the seconds in HUD gold, over a silver sheet with the rider's name, a gold Accept and a silver Decline |
| Free Ride hub: Next Map vote | Dark tiles, green when chosen | "Next Map" title strip with its clock, over three Select Track cards with photos, votes in gold and your choice on the gold pill |
| Free Ride hub: results toasts | A dark card with a gold glow and green `+points` | A title strip (gold on a win) over a silver table with medal places and POINTS. A one-word answer (DECLINED, BUSY) is a race HUD sign |
| Free Ride hub: GOAL / JOIN / HILL tags, join circle | Already peer name tags and gold ground dots | Unchanged |
| Quick chat: wheel | A `#080808b0` disc and dark pills over a `#0006` darkened screen | Silver plates with dark lettering, the gold pill when lit, and only a light `#00000026` dim |
| Quick chat: bubbles | A dark `#101010d9` pill with a coloured border | Name-tag lettering (white, black outline) with no box; emotes drawn bare |
| Quick chat: feed | Outlined lettering already, but printed over the Quick Play panel's title, with the player's name in an invented orange-gold | The feed starts under the panel; your name is in the HUD gold |
| Quick chat: CHAT toolbar button | The classic toolbar control already | Unchanged |
| Follow camera (#671) | The watched rider's tag floated mid-screen under the Watch button and ran into other riders' tags. Tags were ranked from the parked spectator. The touch controls and "HOLD ANYWHERE TO RIDE" showed although a spectator cannot ride | The watched rider's tag is hidden (Watch names that rider) but stays on the map. Tags are ranked from the camera, and a farther tag that would cover a nearer one stacks above it (this also fixes bunched start grids). Touch controls are hidden while watching |
| Ghost challenges: medal reveal | Inside Race Results; the NEW tag was a flat `#f6e061` chip | The NEW tag sits on the gold pill. The coin itself comes from `medal.rs`, which the menu restyle (#682) owns |

## Limits

- The track photos come from the front end's art, which loads behind the
  title. A rider who starts from a link (`?quickplay=`, a room link) skips
  the front end, so their votes show the silver photo well with the name and
  mode only; so does a vote that opens before the art has loaded.
- The medal reveal sits inside the port's Race Results panel, the
  single-player race-over screen. That panel is still the dark box the owner
  objected to, but it predates epic #631 and is left for its own restyle.
- Peer name tags draw under the panels (unchanged from before).
- The Random card has no photo; it shows "Random" in the silver well.
