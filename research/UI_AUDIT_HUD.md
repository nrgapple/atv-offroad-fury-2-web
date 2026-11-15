# In-ride multiplayer overlay audit (issue #679, part 2)

Owner feedback (2026-11-14): the new multiplayer UI should be built "a
little off the classic hud and the menu screens", with no "AI background
black ui". This note lists every in-ride overlay added by epic #631, what
deviated from the classic look, and the classic element that replaces it.
The menu pages are audited separately in research/UI_AUDIT_MULTIPLAYER.md.

## The classic vocabulary (`crates/atv-ui/src/hud/classic.rs`)

These are existing port components, reused rather than restyled:

- **Race HUD lettering** (hud::race, hud::meter): the condensed italic
  display face at weight 800 with the overlays' dark `#080808bb` outline, in
  the overlay silver (190,190,190) and gold (218,192,31). The race HUD draws
  no boxes behind its text.
- **Race Results panel** (hud::results): the original race-over panel. It
  has a title band in condensed italics that fades to the right, the
  standings table (grey column labels, the player's row in `#f1d84e`) and
  the race-over buttons (the gold pill for the main choice, the quiet button
  beside it, and the gold focus ring).
- **Select Track entries** (front `course_list`): a row with the menu's gold
  bullet, the name with its drop shadow and the kind in small gold capitals.
  The chosen entry sits on the glossy gold selection pill with dark
  lettering. Keyboard focus shows the white inset ring.
- **Peer name tags** (hud::peers): white italic lettering with a black
  outline and no box.

`classic.rs` builds the Race Results pieces with the same constants as
`results.rs`. The only change is that its buttons are 48 px tall instead of
46, to keep the touch-target rule.

## Audit

| Overlay | What deviated (before) | Classic replacement (after) |
| --- | --- | --- |
| Free Ride hub: status chip (event, duel, waiting) | A rounded `#0c0c0cc8` chip of its own | Race HUD info line: gold title, silver detail and the clock, with no box |
| Free Ride hub: Challenge button | A flat `#ffd166` rounded card with two-line dark text | Race-over gold pill button, "Challenge <name>" |
| Free Ride hub: offer prompt with ring timer | A blue-black `#101820e0` card, a ring of 20 dots, a green `#7cff6b` ACCEPT and a grey NO | Race Results panel: "Race?" title band with the seconds in HUD gold, the rider's name, then Accept (gold pill) and Decline (quiet button) |
| Free Ride hub: rotation vote ("Next Map") | A dark card with grey and green rounded tiles | Race Results panel with the title band and clock; each venue is a Select Track entry (bullet, name, votes in HUD gold), and the rider's choice sits on the gold pill |
| Free Ride hub: result toasts | A dark card with a gold glow on a win, and green `+points` | Race Results panel: the title band (gold on a win) over a RIDER / POINTS standings table. A one-word answer (DECLINED, BUSY) is a race HUD sign with no box |
| Free Ride hub: GOAL / JOIN / HILL world tags | Already drawn as peer name tags (hud::peers, the event marker slot) | Unchanged |
| Free Ride hub: join circle | Gold ground dots in the world, not a panel | Unchanged |
| Quick chat: bubbles over riders | A dark `#101010d9` pill with a coloured border | Name-tag lettering above the rider's tag, with no box; emotes drawn bare |
| Quick chat: feed | Already outlined italic lettering, but the player's own name was an invented orange-gold, and the feed was printed over the Quick Play / private-room panel's title (the room's lobby shows while riders chat) | The player's name uses the HUD player gold `#f1d84e`; the feed starts under the panel when one shows |
| Quick chat: wheel | A `#080808b0` disc behind dark `#1a1a1ae6` pills with white borders, lit in flat yellow | The pause menu's darkening behind Show Results tiles (`#0c0c0cdb`, radius 5); the lit target sits on the gold menu pill with dark lettering |
| Quick chat: CHAT toolbar button | Already the classic toolbar control (hud::toolbar) | Unchanged |
| Ghost challenges: medal reveal on the results screen | Inside Race Results; the NEW tag was a flat `#f6e061` chip | The NEW tag sits on the menu's gold pill |
| Follow camera (#671, watching a round) | The watched rider's own tag floated mid-screen under the Watch button and ran into the other riders' tags (in one capture another rider's tag lay on the followed quad). Tags were ranked by distance from the parked spectator, not the camera. The touch controls and "HOLD ANYWHERE TO RIDE" coaching showed although a spectator cannot ride | The Watch button names the followed rider, whose tag is hidden (it keeps its map marker). Tags are ranked from the camera, and a farther tag that would cover a nearer one stacks above it (this also fixes bunched start grids). The touch controls are hidden while watching |
| Quick Play and private-room panel | See the section below | See the section below |

## Quick Play and private-room panel

PR #672 (podium rewards) also changes `hud/quickplay.rs`, so this panel is
restyled after that PR lands. Its audit:

| Part | What deviated (before) | Classic replacement |
| --- | --- | --- |
| The panel | A rounded `#101010c7` card | The Race Results panel and its title band, with the phase's seconds in HUD gold |
| Lobby seats | `#0000008c` rounded chips, a blue `#5b8def` CPU pill, a gold HOST pill and coloured ping bars | Race Results standings rows (RIDER / PING / READY): HOST and CPU as small column-label capitals, the ping in milliseconds, a gold tick for ready, the player's row in gold and open seats dimmed |
| Vote cards | `#000000a6` cards with invented per-mode colours | Select Track entries: the track name with the mode in small gold capitals, the votes in HUD gold and the rider's choice on the gold pill |
| Countdown card | A big version of the vote card | The picked Select Track entry on the gold pill under the title band and timer |
| Podium and rewards | Gold, silver and bronze blocks with rounded corners, plus green points chips | The Race Results standings: place and rider, then time or points |
| Rematch / Leave, Start / Mode / CPUs, Ready, Skip | `#ffffff1f` buttons with radius 8 | Race-over buttons |
| Spectator "Next round" chip | A `#000000a6` pill | A race HUD sign in gold, with no box; Watch is a race-over button |
| Placement on a phone | The panel at the top covered the minimap at 360 px portrait | Below the minimap when the screen is tall enough; the vote leaves out the seat table on a phone (the cards' counts show the votes) |
