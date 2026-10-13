# Fury Remastered identity

`web/fury-remastered.svg` is the shared transparent 960×320 wordmark for the title,
boot/recovery loading screen and dev-log header. All lettering is authored path
geometry; there are no font dependencies, linked resources or embedded bitmaps.
The red/orange face, metallic edge, forward slant and gold signature recall the
original ATV Offroad Fury 2 identity without using its recovered raster logo.
This is new branding, not an exact vector trace of the original mark.

Home Screen icons now use the full SVG wordmark on a red gradient, in authored
180/192/512px PNG derivatives. `tools/build_fury_app_icons.mjs` regenerates
`web/fury-app-icons.json` using Sharp; `SHARP_MODULE` can select an existing
installation. The normal build verifies source/image hashes and produces the
three static icons without any font or original-resource dependency. These ship
atomically with the Apple, manifest, browser-icon and installation-banner links.
The square F-only icon and the later recovered-ATV2 Home Screen choice are
superseded by the user's request to use the new full Fury Remastered logo.

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
# Home Screen caching

Fresh icon filenames replace the former R2 URLs. Safari's conventional root
aliases return the new 180px PNG, and the legacy large-icon alias returns 512px.
Existing saved Home Screen apps may retain cached artwork until removed and
added again. Physical iOS installation remains unverified. See [PWA.md](PWA.md).
