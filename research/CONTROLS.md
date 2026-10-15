# Original controls and browser bindings

The baseline is the USA manual's printed pages 7–8, checked visually against the recovered input and stunt routing. The [original manual](https://www.videogamemanual.com/PS2/ATV%20Offroad%20Fury%202%20%28USA%29.pdf) describes a stick preload: pull down at the base of a jump, then quickly push up before its peak. It does not describe holding a separate jump button. The touch brace/release and keyboard Space mapping are explicit browser adaptations; the Rust ramp gates and preload force remain the recovered baseline. See [touch evidence](TOUCH.md) and [crash limits](WRECK.md).

## Driving audit

| Action | Original default | Browser controller default | Keyboard default |
| --- | --- | --- | --- |
| Accelerate | Cross | Bottom face / right trigger (0 / 7) | W or Up arrow |
| Brake | Square | Left face / left trigger (2 / 6) | S or Down arrow; brakes while rolling forward |
| Reverse | Cross + Square | Bottom + left face chord (0 + 2) | Keep holding S / Down once stopped; optional extra key unbound |
| Steer | Left stick left/right | Left stick horizontal or D-pad | A/D or left/right arrows |
| Lean | Left stick forward/back | Left stick vertical or D-pad | Q forward; E or Left Shift back |
| Preload | Quick down → up stick motion while jumping | Same native lean motion; separate hold/release initially unbound | Native lean motion, or Space hold/release |
| Wheelie | Down + Cross | Lean back + accelerate | Lean back + accelerate |
| Stunt modifiers | Circle, Triangle, L1 | Right face, top face, left shoulder (1, 3, 4) | J, K, L; then tap a direction (WASD, arrows, Q/E) |
| Reset | L1 + R1, Practice only | Left + right shoulder (4 + 5) | R or Backspace |
| Pause | Start | Start (9) | Escape or P |
| Change camera | Not established by these manual pages | Right stick click (11), port adaptation | C |
| Camera orbit | Not established by these manual pages | Right stick horizontal, port adaptation | Z / X |

Controller numbers are the browser standard gamepad layout, not native PS2 device codes.

**Keyboard drive adaptation.** Keyboard gas and brake form one authored drive axis resolved in Rust at each fixed step (`browser_input::BrowserDrive`, sharing the touch drive rules): S brakes while rolling forward, selects reverse below 1 ft/s and keeps reverse gear while coasting; W brakes a backward roll before driving forward. Airborne input cannot change gear. S wins when both are held. Controllers keep the native Cross + Square reverse chord. Keyboard tricks use the most recently pressed held direction while a trick key is held; gas/brake count as Up/Down there. A direction already held before the trick key waits 150 ms for a fresh direction, so holding W does not force every trick Up. Saved version-1 keyboard rows that still equal the old defaults adopt the new ones; customized rows are kept. A controls card (`web/keyboard-help.js`) opens on the first keyboard ride, closes after 15 s and toggles with H or the Keys toolbar button. While riding, a click that leaves focus on the page or toolbar no longer drops driving keys. Trigger alternatives, reset in the current Freestyle reconstruction, camera shortcuts and keyboard bindings are port conveniences. Bluetooth pairing and physical iOS/controller hardware are not validated by synthetic browser tests.

Reverse consumes the brake binding used in its active chord, matching Cross + Square without simultaneously applying Square braking. An independent brake alternative, such as the left trigger or keyboard S, still applies. The four driving axes preserve analog amounts and signed-axis remapping. Browser right-positive steering and stunt X preserve the native sign: +X is screen-right in the corrected source camera convention.

The mapping profile exposes sixteen actions: throttle, brake, two steering directions, two lean directions, reverse, three stunt modifiers, preload hold/release, camera change, two camera orbit directions, reset and pause. Pause → Controls edits keyboard/controller mappings separately, including single keys, buttons, signed axes and up to four-input chords. Clear removes the selected binding; restore defaults affects the selected device. The authored profile is stored locally in the browser, with session-only behavior if storage is unavailable. Menu navigation retains its fixed accessible controls independently of gameplay remapping.

## All 24 stunt selections

The manual's printed page 8 and the Rust stunt table agree on every modifier/direction pair below. Directions refer to the left stick; simultaneous modifiers form the six groups. L1 alone is not a listed stunt group.

| Modifier group | Up | Down | Left | Right |
| --- | --- | --- | --- | --- |
| Circle | Nac Nac | No Footed Can Can | Suicide No Hander | Bar Hop |
| Triangle | Heel Clicker | Cliff Hanger | Nothing | Point Back |
| Circle + Triangle | Cat Nac | Cordova | Scorpion | Double Heart Attack |
| Circle + L1 | Indian Air | Disco Can | Tsunami | Shaolin |
| Triangle + L1 | One Handed Indian Air | Tweak Air | Lazy Boy | Boogie Nights |
| Circle + Triangle + L1 | Heart Attack Indian Air | One Footed Cordova | Holy Man | Sidewinder |

The same remappable lean/steer directions feed selection, so changing a gameplay axis does not leave a hidden default stunt axis. Touch currently exposes the four Circle-family gestures through an extra right-side finger; the primary right thumb retains throttle/bracing. This is incomplete touch coverage of the original 24-stunt menu. Selection and clip playback do not establish original scoring/chaining, landing timing, rider IK, full flips or crash parity.

## Cancellation and verification

Keyboard camera/reset/pause taps are queued from events so a complete short press between physics batches is not lost. Continuous driving and preload are sampled once per submitted batch. Held actions fire once; menu/blur/remap/controller loss clears queued actions. Controller edge controls wait for neutral before rearming. A fresh nonrepeat keyboard press can rearm immediately, including a quick Escape press directly after Resume before any neutral physics sample. OS repeat events cannot reintroduce keys cleared by cancellation. Touch cancellation remains distinct from intentional finger release.

A monotonic browser input generation reaches Rust with each gameplay packet. A changed generation clears the pending native lean-gesture timestamp and preload arming; it preserves any jump force already underway. Without this hook, clearing a held browser brace while a controller remained leaned forward could accidentally look like the original down → up flick on resumption. The original force gates and tuning are unchanged.

`web/bindings.test.mjs` covers all 24 stunt inputs, analog signed-axis remapping, reverse/brake chords, every action's persistence/clear/default restoration, controller loss, subscription cancellation, and brief keyboard action taps. `web/touch.test.mjs` retains the independent two-thumb mapping. Two native Vehicle regressions exercise cancellation: a rising grounded fixture triggers preload when left intact but produces no boost after cancellation; an already active preload retains its force timer/strength. `tools/test_bindings.mjs` passed against the isolated combined development build: actual WASM keyboard/controller remapping, analog axes, reverse/brake chords, brief and held discrete actions, immediate Resume → fresh Escape in one JavaScript task, paused simulation, disconnect/blur/OS-repeat handling and no browser errors. It replayed the original Caribou route into two fresh WASM instances, reached arming after 33 driving steps, and showed that resumed forward lean boosts only the uncancelled instance; the cancelled instance has no active/armed preload and zero strength. No vehicle poses or forces were modified. The existing touch transport regression also passed 124 original-map steps, including takeoff inside a four-step batch, while both portrait thumb drags kept camera orbit unchanged. The real touch ramp test passed arming, boost, airborne contacts, landing and resumed driving (9.37 ft/s upward at arming, 22.25 ft/s peak during active preload). These are host browser checks of a candidate build; they do not establish public deployment or physical iOS/controller equivalence.
