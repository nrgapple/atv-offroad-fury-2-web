# Original menu artwork

The menu can use isolated original art with Rust-owned layout and hit regions. These assets are not screenshots of a completed menu page; the current responsive panels, text and controls remain independently rendered. Upscaling a source texture does not recover extra detail.

## Available logo and ATV thumbnail

| Asset | Original archive | Decoded size and transparency | Runtime status |
| --- | --- | --- | --- |
| `ui/frontend/textures/mmlogo01.pst` | `/UI/RESOURCE/PAGEA/PAGE01.RES` | 256×128, alpha 0–255 | Already in hosted runtime archive |
| `ui/frontend/textures/mmlogo02.pst` | `/UI/RESOURCE/PAGEA/PAGE01.RES` | 128×128, alpha 0–255 | Already in hosted runtime archive |
| `ui/frontend/textures/multi_atv01.pst` | `/UI/RESOURCE/QUADINFO.RES` | 256×128, alpha 0–255; 24,949 of 32,768 pixels fully transparent | Added to packaging allowlist; transfer requires committed-package regeneration and validation |

The two logo parts are adjacent in the original Main widgets: x 262 and x 518, both y 55. Their combined native dimensions are 384×128 (3:1). The ATV thumbnail is a transparent 2:1 image of the orange Bombardier quad, with no labels or background. Decoded images were visually inspected privately; no screenshots or texture bytes enter GitHub. `QUADINFO.RES` is 336,011 bytes. The thumbnail's native size should inform layout so it is not presented as a newly high-resolution render.

`STARTAPP.RES`'s `atvoverlay.pst` is only a rounded stats panel, not an ATV picture. `PAGE01.RES`'s `bk_mainmenu.pst` is an opaque 640×448 red-tinted scenic backdrop without menu text/buttons. Neither needs to be mistaken for an isolated vehicle image.

## Higher-resolution original menu model

The exact build also has `/UI/RESOURCE/QUAD/ATV01.RES`, containing `ui/frontend/geometry/atv01.psg` (287,448 decoded bytes; 129,848-byte archive). The existing Rust decoder accepts this mesh: one node, one LOD, 10,245 triangles and zero unparsed trailer bytes. Its 11 material names include `atv01_ui_hires02`, `atv01_engine_ui_hires`, and other UI-specific materials. Their textures are present in `/STRTUI1A.RES` and `/STRTUI2.RES`.

Those UI model/texture archives are not currently in the R2 runtime manifest. A future rotating model can render at the menu's actual display resolution, but still needs validated material/render-state integration and a targeted runtime package. Successful mesh decoding does not establish original menu camera, lighting, rotation timing or appearance. This discovery does not claim that model is already in the browser menu.
