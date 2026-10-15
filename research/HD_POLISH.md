# Crisp HD polish checkpoints

The requested presentation retains ATV2's red, silver and gold identity and
original hierarchy while replacing raster UI text with crisp device-resolution
type. It does not seek a pixelated PS2 output. Keep backgrounds and borders
restrained, particularly on gameplay controls. Existing decoded artwork is a
reference; enlarging the original textures cannot create high-resolution detail.

Delivery sequence: tight crash camera and shake removal; start/menu, four-section
persistent settings and HUD polish; optional speed-driven peripheral blur.
Each gate receives host checks, its own dev-log entry and one owner's public
release receipts. Physical phones and exact PS2 appearance remain unverified.

## Crash camera

The former camera fitted all posed mesh bounds, including detached mechanical
parts and limbs. Terrain clearance pulled the eye backwards along the fitted
ray without a cap. Both could make a wreck tiny on screen.

The authored replacement measures only torso/pelvis and central chassis nodes.
It shares attention while centers are close, smoothly reduces the bike's weight
between 8 and 22 feet, and follows only the rider beyond 22 feet. The dolly is
bounded to 16–32 feet, including portrait. Terrain clearance raises the eye and
re-aims at the target instead of retreating further. Extreme terrain can still
raise the eye significantly; prop occlusion is not solved. Native simulation,
crash damage, recovery and camera frames remain unchanged.

Camera shake and its setting are removed. Old stored shake preferences have no
effect. Contact event detection remains only for the existing impact sound.

## Speed-effect research and implementation direction

[SuperTuxKart's shader](https://github.com/supertuxkart/stk-code/blob/master/data/shaders/motion_blur.frag)
uses a masked area around the kart and screen-space samples for its boost effect.
Its current shader reconstructs previous-frame screen displacement; this request
instead calls for speed-driven radial streaking without camera-turn blur. The
source carries GPL-3.0-or-later; no code is copied or incorporated.
[NVIDIA GPU Gems, GPU Image Processing in Apple's Motion](https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-25-gpu-image-processing-apples-motion)
describes rendering an image into a texture for GPU processing. These are research
references, not proof of original ATV2 behavior.

Plan an independently authored WebGL2 single-frame radial sample pass, gradual
onset around 40 mph, protected central rider region, a bounded peripheral warp,
and no HUD processing. Include Off/intensity choices, reduced-motion defaults,
resize/context recovery and no processing cost when disabled or below threshold.

### Speed rush v2

The first pass protected the rider with a fixed screen ellipse and sampled through
the ATV, so it smeared or blurred the vehicle on portrait phones and at other
camera distances. Version 2 (`web/speed-rush.js`):

- Writes stencil ref 1 while drawing the player's ATV/rider; the pass skips those
  pixels exactly and gives zero weight to samples inside the vehicle's projected
  screen bounds, so it never streaks onto nearby ground.
- Radiates from the projected travel direction (linear velocity), smoothed and
  clamped on screen, rather than a fixed screen point.
- Jitters tap offsets with interleaved gradient noise (Jimenez, Call of Duty:
  Advanced Warfare, 2014) so 5 taps on coarse-pointer devices and 8 elsewhere
  look smooth; it drops 2 taps while frames run under 45 fps. The centre region
  exits after one fetch.
- Draws procedural wind lines in the same pass: polar cells around the travel
  focus, moving outward in log-radius with a bright head and fading tail. The
  research included CC0/Unlicense polar speed-line shaders
  ([Godot Shaders](https://godotshaders.com/shader/speed-lines-shader-for-godot-4/),
  [anime-speed-lines](https://github.com/mirzabeig/anime-speed-lines)); no code
  is copied.
- Widens FOV by up to 9% focal length; dynamic FOV is the most commonly
  reported racing sense-of-speed cue ([SuperTuxKart forum](https://forum.supertuxkart.net/thread-200.html)).
  Adds a light peripheral vignette.
- Normalises radius by the shorter screen side so portrait phones streak at the
  sides.

Off (and reduced-motion default Off) restores the original camera and costs
nothing. Physical-phone frame timing remains unverified; host checks used
Chrome with a 390x844 @3x mobile emulation (DPR capped at 1.5 by the renderer).

## Menu and HUD checkpoint

Main Menu and Pause now open the same Settings hub with four sections: Controls,
Display, Audio and Extras. Back returns to the entry menu. Existing bindings,
steering, touch, map and soundtrack preferences retain their storage keys; Extras
adds a saved Tight/Original crash-camera option, with Tight as the default. The
original choice uses the recovered camera without the authored crash framing.

Canvas menu text and original-position HUD text now use vector type rasterized at
device resolution. Original metrics retain HUD placement. DOM setting rows share
the condensed typography, red/charcoal palette and gold selection accent; repeated
pills, button backgrounds and boxed gameplay controls are removed. Main/single
menus use the existing real-time vehicle preview in place of the small bitmap ATV
thumbnail. Course photos and model textures remain their original source detail.

Host checks cover four viewports, 2x DPR, keyboard entry, 44px settings targets,
persistence across reload, actual driving, standalone HUD and main-preview
rotation. Physical phones/controllers and exact original appearance are not proven.

## Speed rush checkpoint

The authored seven-sample radial pass starts at 40 mph and eases toward its cap at
75 mph. It protects the projected rider and central view, with a small bounded
peripheral warp. It uses only the current image and speed, never camera movement
or previous frames. HUD and menus are drawn separately and remain crisp.
Display offers saved Off, Low, Normal and High choices; reduced-motion users
default to Off unless they explicitly saved a choice. The pass skips GPU work
below the threshold, during crashes/menus, and when disabled. Its image storage is
three bytes per output pixel, allocated only when needed, resized with the canvas
and rebuilt after context recovery. This is an optional authored enhancement,
not a claim of recovered original-game behavior.
