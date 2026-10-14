# Crisp HD polish checkpoints

The requested presentation retains ATV2's red, silver and gold identity and
original hierarchy while replacing raster UI text with crisp device-resolution
type. It does not seek a pixelated PS2 output. Keep backgrounds and borders
restrained, particularly on gameplay controls. Existing decoded artwork is a
reference; enlarging the original textures cannot create high-resolution detail.

Delivery sequence: tight crash camera and shake removal; start/menu, five-section
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
