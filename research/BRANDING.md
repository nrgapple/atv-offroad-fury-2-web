# Fury Remastered identity

`web/fury-remastered.svg` is the shared transparent 960×320 wordmark for the title,
boot/recovery loading screen and dev-log header. All lettering is authored path
geometry; there are no font dependencies, linked resources or embedded bitmaps.
The red/orange face, metallic edge, forward slant and gold signature recall the
original ATV Offroad Fury 2 identity without using its recovered raster logo.
This is new branding, not an exact vector trace of the original mark.

`web/app-icon.svg` uses the same F and palette at 512×512. The authored PNG sizes
in `web/app-icons.json` are derived from it for Apple and manifest compatibility.
Page titles, Apple app name and manifest use Fury Remastered; Apple icon query
version 2 requests the replacement artwork. Existing installed shortcuts may
need to be added again depending on device caching.

The menu decodes the SVG once and draws it through the Rust artwork command.
Failed image decoding keeps the FURY / REMASTERED text fallback. The title logo
ends above the Start button at both portrait and landscape aspect ratios.
The SVG is part of the authored build/release/server allowlists, independently of
private runtime content. Historical dev-log entries retain original names.

Host checks: release WASM build, six Rust menu tests, formatting, nine release
tests, setup/source exclusions, JS/Python syntax, and served icon/manifest checks.
Visual checks cover desktop 1280×720 and portrait 390×844, Start navigation and
the dev-log header. These checks do not establish physical iOS behavior or any
new gameplay fidelity.
