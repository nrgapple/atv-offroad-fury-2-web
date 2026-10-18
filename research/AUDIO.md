# Audio reconstruction

The port decodes original VAG samples in Rust and schedules bounded PCM buffers through Web Audio. Browser resampling/mixing is not a bit-exact SPU2 reconstruction. The original game-wide music policy, mixer, spatial effects and progression integration are incomplete.

## Soundtrack selection and streaming

The exact-build `music.pb` catalog contains 21 songs. Every record has `Locked=0`; all are eligible for the recovered non-repeating shuffle. The runtime R2 manifest already includes all 21 original music files. Their header-derived lengths range from approximately 154 to 271 seconds. They remain outside GitHub and CI.

Before the selector work, changing maps constructed a new audio player and reloaded the catalog, resetting the original shuffle to seed 1. Repeated short sessions therefore began with the same track even though every song was available. The updated catalog operation preserves shuffle state across map loads when the catalog is unchanged. An explicit selection marks the song as current/played in that same shuffle. The original RTC seed/global random call order is still unknown. A fresh-page browser adaptation avoids repeating the locally remembered last song at startup; it does not claim original save-file behavior.

The browser Music interface lists title and artist from the original catalog, supports direct selection, previous/next history, on/off and volume, and remembers settings/history locally when browser storage is available. Previous returns to the preceding selection; next returns through forward history before requesting a new original shuffle selection. The browser settings and Music menu are adaptations, not a recovered complete original soundtrack-options screen.

Playback fetches only manifest-approved, hash-validated content. The existing content reader retains at most two lazy soundtrack blobs alongside startup archives; decoding reads at most 64 KiB of encoded audio per operation and schedules roughly 3–4 seconds of PCM. Tracks are queued on the same sample clock when one ends. Selecting another track cancels queued voices and invalidates pending reads before reinitializing ADPCM predictor history, so slow earlier requests cannot play over the new selection. A failed stream can be retried with another selection. Music off pauses queued buffers at their sample offsets and stops additional streaming until enabled again.

The pause Music screen can preview selected songs while vehicle sounds remain silent. This preview has its own bounded streaming tick because paused physics no longer drives audio updates. The frontend must clear preview on menu exit or focus/visibility loss. Browser gesture restrictions still apply; the port does not bypass autoplay policy.

Validation: focused tests cover all 21 catalog selections, bounded PCM scheduling, contiguous end-of-song transitions, in-flight selection cancellation, pause/resume including an in-flight chunk, persisted settings and selection history, failed-request recovery and paused preview without engine playback. Host Chromium validation on port 4176 also selected all 21 original tracks through the actual Rust/WASM decoder and hosted runtime manifest. Each produced bounded scheduled PCM; the run recorded 63 chunks, a 65,536-byte maximum decode input, zero underruns and zero browser/music errors. Music off froze the playhead with no live voices, and Music on resumed its clock. This exercises the start of every song; end-of-song transitions use focused synthetic tests. Public menu/release verification remains separate. These checks establish player behavior and playback-clock progress, not a listening comparison or full original audio parity.

## Streamed soundtrack and the Jukebox

The content package now carries a browser encoding of each song beside its
original VAG. `tools/package_content.py` decodes every song with the same Rust
`VagStream` decoder the fallback uses (`atv-tools music-wav`), then encodes AAC-LC
160 kbps in faststart MP4 (`ffmpeg`, AudioToolbox when present; byte-identical
across runs). The 21 streams total 87.4 MB against 218.4 MB of VAG (GOLDEN: 3.79 MB
for 9.47 MB). Each song record gains `stream: {url, bytes, sha256, type, seconds}`
with a hash-addressed `/content/<sha256>.m4a` URL. This is an authored lossy
re-encoding of the decoded ADPCM, not an original file; the VAG chunks stay in the
package unchanged.

When every song has a stream, `MusicStream` plays through one `<audio>` element
routed by `MediaElementAudioSourceNode` into the existing music gain, so volume,
race holds/fades and the master bus behave as before. The browser fetches byte
ranges and decodes natively off the main thread; there is no WASM decoding or
per-chunk PCM conversion, and songs are seekable. The Rust catalog and shuffle
still choose every song. The Worker serves `.m4a` objects as `audio/mp4` with
single `Range` requests (206/416) and immutable caching; iOS Safari requires
ranges for media. The element pauses whenever the AudioContext is not running.
A silent blob WAV is played inside the first activating gesture so later song
changes are allowed on mobile. Autoplay refusal waits for the next gesture; any
other play or media error switches the session to the VAG decoder and restarts
the song. A package missing any stream (the published one until this upload)
keeps the decoder for the whole catalog.

Every ride builds a fresh `GameAudio` and closes its context, but a media
element can feed only one Web Audio source node ever and mobile browsers unlock
playback per element. One page-wide `MediaOutput` therefore owns the element,
its own small context, the music gain and a 0.65 master matching `GameAudio`.
Each ride's `MusicStream` borrows it, and the first gesture's unlock lasts for
the session. Gesture listeners use the capture phase because the touch driving
controls stop their events at the target. The ride context still gates playback:
its suspension pauses the element. Returning to the original menus sends the race
`leave` event, ending a hold and the finish sting that Quit to Main Menu
previously left behind. The race phase resets with each load. The Jukebox
Pause is transient (`setPaused`). It no longer writes the saved Music setting,
which also silences the menu tune. Settings saved before that fix restore music
once.

The soundtrack publishes nothing to the system media controls. Media Session
metadata and action handlers are cleared, so iOS shows no Now Playing card and no
lock-screen pause can turn Music off. When the page is hidden the song is unloaded
(`MusicStream.park`). On return the ride reloads the same song at the same
position. Settings version 3 restores, once, a Music off that version 2 could
have saved from the system pause.

The **Jukebox** (Main Menu and Pause) follows the original `Jukebox.dat` page in
`PAGE14.RES`, now packaged as a lazy record. It shows that page's red snow
backdrop (`bk_jukebox`) and its 20-frame spinning CD (`cd64.atm`), the genre,
title and artist from `music.pb`, a seek bar, previous/play-pause/next, music
volume, all 21 songs, and a small spectrum from an `AnalyserNode` on the music
gain. The disc has no album covers; the CD and backdrop are the only original
soundtrack artwork. The layout, transport, seeking and spectrum are authored.
Play/Pause is the existing Music on/off setting. Left/Right scrub ten seconds on
the seek bar, step volume by 5%, or change track elsewhere. The decoder fallback
is not seekable, so the seek bar is disabled there.

Validation: unit tests cover streaming through a fake element (no VAG reads or
decodes), seeking and clamping, ended → shuffle, history, on/off, race holds,
context suspension, autoplay refusal, decoder fallback on failure and mixed
packages. Worker tests cover ranges, suffix/open ranges, 416, HEAD and 304.
`tools/test_jukebox.mjs` (content group, host Chromium) opens Main → Jukebox and
Pause → Jukebox with the actual package. Every stream response was `audio/mp4`
with 206 range responses. It checks the original art, all 21 rows, row
selection, seeking to mid-song, pause freezing the position, next/previous,
keyboard scrubbing, 390×844 touch sizes without horizontal overflow, and the
ride overlay. `tools/test_music.mjs` now forces the decoder path. A host probe
against the published (stream-free) package played through the decoder with the
drawn disc and a disabled seek bar. Physical iOS/Android playback is unverified.

## Moving Arlington train body sounds

The original Arlington Freestyle TOY binds eleven body-attached sound emitters
using three original mono VAGs. The reconstructed player loads those original
samples through the compact, R2-only AUDIO archive, preserves the VAG loop
markers, and keeps one voice per body independent of engine sample transitions.
The sound follows the existing Rust VUE route. The source `StartDisabled=FALSE`
records enable these loops; the smoke procedural slot has no sound attachment.
Only these three newly required samples are added to the compact audio bank;
the engine bank remains 21 samples and the soundtrack remains 21 songs.

Exact-build inspection identifies the renderer camera as the listener:
`00215e30` forwards its position, normalized forward/up and the camera's stored
position delta multiplied by the engine reciprocal timestep. `001742f0` chooses
the nearest listener with a strict distance comparison. The authored single
player uses each original camera snapshot before portrait presentation changes,
and computes body/camera velocities from adjacent fixed-step snapshots in Rust.

`00172e70` supplies the original integer volume: 100 at or inside the TOY minimum
150 feet, zero at or outside 350 feet, and `100-trunc(((d-min)/(max-min))*100)`
between them. Relative heading uses the source acos table interpolation,
binary32 degree conversion and truncation. Its unusual Doppler input normalizes
listener-forward minus relative source position and dots this with source minus
listener horizontal velocity; the port preserves that behavior. `0014ed40`
uses the sound constructor's 0.25 Doppler factor, a 1125 divisor, a strict 0.01
change threshold and quantized 4096 pitch. `002f4500` converts heading into two
piecewise integer stereo gains. Web Audio receives these explicit gains and
pitch; it does not substitute a browser distance falloff or spatial panner.

Bounded original-instruction comparisons cover 1,200 attenuation/heading/Doppler
cases (249,428 instructions), 2,166 ordinary stereo gain cases and 2,401 pitch
cases (589,081 instructions), with zero compared integer differences. The source
30001-entry acos table constructor uses a supplied host double-precision acos
converted to binary32; this is not PS2 libm or SPU2 resampling equivalence. The
probes are a bounded MIPS/VU interpreter with explicit services, not a PS2 run.

Host Chromium validation uses the actual Rust/WASM and recovered samples. It
passes eleven bounded loops, three mono 11025 Hz samples with original loop
markers, nonzero decoded PCM through their train-only output nodes, naturally
changing body/camera distance and heading, near/far stereo attenuation, quantized
pitch, pause/resume and graphics-context restoration, with eleven starts and no
duplicate voices or browser/audio errors. Far commands produce zero output;
returning near resumes audibility without recreating voices. The selected test
camera comes from the original route/MOT and is not a normal map spawn. The
inspected capture shows the ATV beside the moving train; the screenshot does
not itself prove sound. Browser tests instrument node output separately from
engine and music playback. Tests are in `tools/test_train_audio.mjs`.

Web Audio looping uses the browser audio clock; per-body native activation/start
phase and hardware mixing/resampling have not been compared by listening on
PS2. Initial history is seeded with the first browser snapshot rather than an
unrecovered source sound-constructor history. Surround mixing, original mixer
saturation, reverb and physical iOS listening remain unverified. Paused music
preview silences vehicle/train output through the inherited gain lifecycle.
Rider body impacts, rider vocals, chassis/surface sounds and venue world
sounds are described below.

## Original rider body impacts

The original `rider.aud` BodyImpact bank supplies four mono VAGs. Only these
four required original bank members are added to the R2-only compact archive;
the 21 engine samples, three train samples and 21 songs remain separate counts.
No game samples or extracted analysis are committed to source or CI.

Exact-build `002ccf90` binds the impact emitter to the rider actor and its
collision body's sampled contact delta, with the quad as its wreck-phase
backpointer. It does not consume the vehicle chassis impact field. `001ef690`
keeps the signed component with strictly greatest absolute value across contact
substeps, sampling before each integration. Tied magnitudes keep the prior
component. The runtime observer records this vector without changing response.

`002cd2e0` clears the sound latch while the quad is riding. During wreck phases,
the first magnitude at least 0.3 triggers selection; subsequent magnitudes at
least 3 can retrigger only when the current body clip has stopped. `002cd1f0`
uses `trunc(magnitude)/(70/sample_count)-1` plus its original random term and
clamps to the ordered bank. Selection consumes one random word even if the
initial-latch path selects while an earlier clip is still playing. `002cd420`
then refuses to play over that existing clip. Browser playback retains this
one-voice guard, plays original one-shot PCM and uses recovered single-player
30/100-foot attenuation, integer stereo and quantized Doppler commands.

Sound position and velocity come from the actor cache, rather than its physical
body. `001e7600` refreshes actor position from the quad Model during riding or
the attached rider phase; detached phases retain the last attached position.
`002d9c10` computes velocity from adjacent cached positions using the engine
reciprocal timestep, unless the prior position is the zero sentinel. The port
preserves that cache and the source zero-sentinel guard. It therefore does not
move the sound to the airborne rider's current physical pelvis.

Bounded original-instruction checks pass 1,200 native phase/latch/index/random
cases (86,383 instructions), 400 signed contact-substep maxima cases (175,383
instructions), 800 actor velocity-cache cases (39,000 instructions) and 400
position-copy/hold cases (21,176 instructions). The position probe supplies
Model access and update services and uses the recovered zero placement offset;
it does not prove whole-game callback ordering or every rider profile. The
native sound-state services supply current playback and source random words.
Initial RTC seed and whole-game random call order remain unverified.

The frontend transports up to 64 completed fixed-step sound updates together,
preserving each contact vector, latch update and random draw. Within that
synchronous batch, the playing guard becomes true after the first permitted
start. This prevents an audio-worker round-trip backlog from dropping a
one-frame impact in a physics render batch. The pending queue remains bounded;
autoplay-locked events are not accumulated for delayed playback after unlock.
Focused tests cover retained intermediate impacts, the one-voice guard,
original one-shot routing, pause/music-preview/reset and missing-bank fallback.
Real touch wreck PCM verification is in `tools/test_body_impact_audio.mjs`.

Host Chromium verification on port 4192 passed the real touch Caribou ramp
and failed Nac Nac landing: sampled rider contact magnitudes 2.824348 and
5.099363 selected two original clips with isolated output energy peaks 0.03029
and 0.02301. All four original body samples decoded. Detached frames retained
the last attached actor sound location, active clips did not overlap, recovery
returned to riding, one-shot playback completed and pause/resume froze/restored
the audio clock. No browser, audio or GL errors occurred. The inspected
portrait capture shows the detached rider and original game view; sound proof
comes from the separately instrumented audio nodes. This is host verification;
public release verification remains separate.

BodyImpact is one of four rider/quad emitters; the others follow. The wheel
particle placement routine previously investigated does not schedule sound.
Hardware listening, SPU2 mixing/resampling and physical iOS playback remain
unverified.

## Rider vocals, chassis and surface sounds

Exact-build evidence comes from private Ghidra exports of the shared sound
table and four emitter classes; no instruction-comparison harness was run for
this component, so these are decompiled-contract reconstructions with unit
tests, not bounded native comparisons. `00213210` loads `rider.aud`'s twelve
vocal lists and `vehicle.aud`'s QuadToQuadImpact and Wreck lists.

**Rider vocals** (`002cc430`/`002cc640`/`002cc7d0`, request `002cc920`, update
`002ccc60`, start `002ccdc0`): rider +268 selects the Female or Male lists.
Events and triggers:

| Event | List | Source trigger |
| --- | --- | --- |
| 1 Bang | Bang | `001eb6f0` quad-to-quad contact with local impact above 1 |
| 2 Jostle | Jostle | `002daa50`, the hard-landing action of `001e8690` |
| 3 wreck | Small below 20, Med below 40, else HardWreck | `001e8a20` wreck entry when the sampled impact is nonzero; `002ccc60` each wrecked frame when the rider body's sampled contact exceeds 3 |
| 4 Fall | Fall | `001e8a20` when the quad is at least 30ft above the ground below it, or a boundary launch (+808) forced the wreck |

Bang, Jostle and Fall step past an immediate repeat; the wreck lists always
step on from a multi-sample pick. One voice: a request selects (consuming a
random word) but starts only while nothing plays and the listener is within the
100ft cull distance (min 30/max 100). The first wrecked frame with any rider
contact stops a playing Fall yell and plays a wreck vocal. Position and Doppler
use the same actor cache as BodyImpact.

**Chassis** (`002cd4e0`/`002cd6c0`, update `002cdb80`; one voice, min 30/max
70): `impact\landing.vag` once the quad has been more than 2ft above the ground
(+9dc, written by `001e5260`) and then samples a world impact of at least 10
while riding, at volume trunc(min(impact,60)/60×100); during a wreck, a
chassis contact with impact at least 10 plays `Wreck[trunc(impact)/(60/n) − 1 +
trunc(3·rand)]`; QuadToQuadImpact (`002cd990`, no-repeat, volume 60) on the
same contact as Bang; `impact\h2ohit02.vag` (`002cdad0`, volume 100) when the
body origin enters water moving downward (normalized −Y velocity above 0.3,
`001d7280` with the +66c latch) and when a rider starts touching water
(`001f0860`; mapped to the port's rider buoyancy contact flag, an inference).
The voice position is fixed where a clip starts, as the original sets it only
then. The chosen cue volume scales that voice's gain (`0014ed40`); earlier
builds played every chassis clip, including soft landings, at full volume. Recovery stops the drive-on emitter's collision sounds.

**Terrain drive-on** (two `001c3330` material sound emitters at the front-left
and front-right wheels, `001c3a90`/`001c3d80`/`001c3eb0`): `001e5260` stores
each side's front-wheel terrain byte (−1 off the ground, 8 half a foot under
water); sound `TerrainDriveOnSound[byte + 1]` plays when byte+1 is at least 2,
so surface 0 is silent. Equal sides play one emitter at full scale, different
sides both at 50%. Pitch is native + step·(mph − SampleSpeed) − trunc((step+1)·
rand), with step = trunc(FreqSlope·(native/SampleSpeed) + 0.5); volume rises
from MinVolume at MinSpeed to 100 at ResonantFreqSpeed and back to MinVolume at
MaxSpeed, all integer arithmetic, mph = body speed × 0.68182. A record whose
clip ended is only updated until a stop. Native SPU pitch is approximated from
the VAG rate (rate·4096/48000); the bank's stored value is unrecovered.

No skid sound exists in this build: there is no skid string, list or sample.
Scene-object **material** drive-on and collision sounds (`MaterialDriveOnSoundN`,
`MaterialCollisionSoundN`, objects mapped by `001fd550`/`001fd7a0` from the
venue's `CollisionObjectsMaterialN` names) need an object sound-material id in
scene collision and are not implemented; GAME01 and Training use only these.

## Venue world sounds and crowd ambience

Every venue's `<venue>.aud` is now loaded. **AmbientSound** records
(`002053d0`, `00205230`, update `001c2880`) are unregistered 2D emitters at
full volume: CycleTime accumulates, then a RandomTriggerPercent draw starts the
clip when idle; a started loop is left alone. The shared play `001c1a50` draws
the percent once per latch (+1c0 bit 4), which only a stop clears, so a
non-100% ambient can fire at most once per map. **SoultreeSound** records
(`00205920`, `00204b80`, factory `00173b20` with flags 2 | OneShot 1 | Looping 8)
register with the manager `001736f0`: an emitter is in range when the nearest
listener is closer than 1.2 × MaxDistance (`001c12e0`), an in-range idle
emitter is (re)started through `001c1a50`, and a playing one out of range is
stopped. Looping accepts the original numeric/T/F forms (`Looping=1` is common).
All 121 OneShot fields are 0. The browser keeps at most 12 Soultree voices,
nearest first (an authored bound; the original has 48 SPU2 voices).

This covers the crowd ambience sets: FREE01–03 and all Supercross venues use
the amateur `crowdf1` / pro `sx` Soultree loops (previously only CrowdPresent
venues played them), and GAME01 (Coonce Arena) plays the indoor/hockey-arena
`icrowd01`/`icrowd02` ambient loops. `Crowd\hockey.aud` (HCHEER/HOCKYEND/
HOCKYWIN) belongs to the hockey event mode (`00335590`), which the port does
not implement. The amateur/pro crowd reactions are described in `RACING.md`.

## Separate SFX volume

Settings → Audio has **SFX** beside the music volume (0–100% in 25% steps,
saved as `atv-audio-v1` in browser storage). It scales the single bus that
carries engine, train, crowd, rider, chassis, drive-on and world sounds; music,
race stingers and menu music keep the music volume. The original game had
separate Music and SFX volumes; the browser control is an adaptation.

## Content and validation

The R2-only compact `AUDIO.RES` gains 172 members (480 → 652, 14,564,439 →
18,739,084 packed bytes): the 93 loose rider vocal VAGs both genders reference,
12 chassis samples and 71 distinct venue drive-on/ambient/Soultree samples (4
crowd loops were already present). No other runtime file changes, and an older
package lacking them leaves these voices silent: vocal names, chassis clips
and world sources whose sample is absent are dropped. Vocal clips stay encoded
and decode on first use into a 24-clip LRU, rather than holding a whole bank of
PCM.

`tools/test_quad_audio.mjs` (content group) runs the actual Rust/WASM decisions
against the original PCM with per-voice output instrumentation: a 70ft drop
lands with `landing.vag` at volume 88; a 200ft drop wrecks with a HardWreck
vocal (`mhurt17`); an off-map start's boundary launch plays a Fall yell
(`myell04`); ENV01 snow starts `drv_snw2`; GAME01 plays its indoor crowd and SX01
its Soultree crowd loops, all with nonzero output energy and no browser errors.
An older content package keeps ENV07 driving with these voices silent. Phone
emulation (host Chromium, 390×844 DPR3, CPU ×4, ENV07 with 55 Soultree and 4
ambient sources, 20s of driving): the batched worker call ran 54 times per
second with a 0.25ms mean (0.9ms p95) round trip, 1.5KB requests, two Soultree
voices live and 66 buffer sources created; main-thread script time was within
noise of the same run with the request emptied (1.28s vs 1.39s). Remote/CPU
riders' vocals, material sounds, hockey reactions and hardware listening
remain open.

## Optional Enhanced sound

Settings → Audio → **Sound: Enhanced / Original** (default Enhanced, saved in
`atv-audio-v1`). Original is the recovered baseline above, unchanged: engine
voices connect straight to the SFX bus and transitions cut. Both Enhanced
layers are authored, not original:

- **Engine crossfade.** The recovered engine state machine still chooses every
  target sample and transition; only a non-overlapping transition changes. The
  previous voice ramps out and the new one ramps in over 120ms through one gain
  node per engine voice, instead of a hard stop. Overlapping commands and the
  reported playhead for the Rust state machine are unchanged.
- **One shared echo send.** A single bus is fed once from the SFX bus (never
  per voice): a lowpass and two damped feedback delays (left/right), mixed back
  into the master. Stadium: 83/127ms, feedback 0.32, 2.8kHz damping, send 0.16.
  Canyon: 210/290ms, feedback 0.38, 2.2kHz, send 0.14. Open venues create no
  nodes. Stadiums come from original data: CrowdPresent or a venue whose
  `<venue>.aud` uses crowd samples (all Supercross venues, FREE01–03 and the
  GAME01 arena). Canyons use an authored terrain measure at the spawn: at least
  10 of 16 headings rise 35ft or more within 300ft. Across the Freestyle venues
  only NAT03 qualifies; this coarse spawn-based test is not a recovered
  classification. Music, race stingers and menu music bypass the echo. No IR
  file, HRTF or PannerNode is used; the recovered stereo/Doppler commands are
  kept.

Cost: the echo is eight nodes per venue. An offline render of 20s of noise
through the bus in host Chromium took about 0.9ms per audio second for either
preset, versus 0.2–0.9ms for the bare graph (CPU throttling does not reach the
audio thread, so this bounds render work rather than phone timing). The
crossfade adds one gain node per engine transition. `tools/test_enhanced_audio.mjs`
checks the stadium bus with nonzero echo output and engine crossfades on SX01,
the NAT03 canyon, no bus on ENV04, and switching to Original through the menu,
which persists and stops crossfading while engine transitions continue.
