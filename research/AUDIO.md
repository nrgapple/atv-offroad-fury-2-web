# Audio reconstruction

The port decodes original VAG samples in Rust and schedules bounded PCM buffers through Web Audio. Browser resampling/mixing is not a bit-exact SPU2 reconstruction. The original game-wide music policy, mixer, spatial effects and progression integration are incomplete.

## Soundtrack selection and streaming

The exact-build `music.pb` catalog contains 21 songs. Every record has `Locked=0`; all are eligible for the recovered non-repeating shuffle. The runtime R2 manifest already includes all 21 original music files. Their header-derived lengths range from approximately 154 to 271 seconds. They remain outside GitHub and CI.

Before the selector work, changing maps constructed a new audio player and reloaded the catalog, resetting the original shuffle to seed 1. Repeated short sessions therefore began with the same track even though every song was available. The updated catalog operation preserves shuffle state across map loads when the catalog is unchanged. An explicit selection marks the song as current/played in that same shuffle. The original RTC seed/global random call order is still unknown. A fresh-page browser adaptation avoids repeating the locally remembered last song at startup; it does not claim original save-file behavior.

The browser Music interface lists title and artist from the original catalog, supports direct selection, previous/next history, on/off and volume, and remembers settings/history locally when browser storage is available. Previous returns to the preceding selection; next returns through forward history before requesting a new original shuffle selection. The browser settings and Music menu are adaptations, not a recovered complete original soundtrack-options screen.

Playback fetches only manifest-approved, hash-validated content. The existing content reader retains at most two lazy soundtrack blobs alongside startup archives; decoding reads at most 64 KiB of encoded audio per operation and schedules roughly 3–4 seconds of PCM. Tracks are queued on the same sample clock when one ends. Selecting another track cancels queued voices and invalidates pending reads before reinitializing ADPCM predictor history, so slow earlier requests cannot play over the new selection. A failed stream can be retried with another selection. Music off pauses queued buffers at their sample offsets and stops additional streaming until enabled again.

The pause Music screen can preview selected songs while vehicle sounds remain silent. This preview has its own bounded streaming tick because paused physics no longer drives audio updates. The frontend must clear preview on menu exit or focus/visibility loss. Browser gesture restrictions still apply; the port does not bypass autoplay policy.

Validation: focused tests cover all 21 catalog selections, bounded PCM scheduling, contiguous end-of-song transitions, in-flight selection cancellation, pause/resume including an in-flight chunk, persisted settings and selection history, failed-request recovery and paused preview without engine playback. Host Chromium validation on port 4176 also selected all 21 original tracks through the actual Rust/WASM decoder and hosted runtime manifest. Each produced bounded scheduled PCM; the run recorded 63 chunks, a 65,536-byte maximum decode input, zero underruns and zero browser/music errors. Music off froze the playhead with no live voices, and Music on resumed its clock. This exercises the start of every song; end-of-song transitions use focused synthetic tests. Public menu/release verification remains separate. These checks establish player behavior and playback-clock progress, not a listening comparison or full original audio parity.

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
Additional rider, impact, surface and world sound scheduling remains open.
