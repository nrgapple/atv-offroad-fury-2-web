# Home Screen installation

Fury Remastered's authored manifest uses a stable `/` identity and scope, a
`/play.html` start URL, standalone display, game category, description and PNG
icons at 192 and 512 pixels and an explicit 180px Apple touch icon. All current
Home Screen icons use the full authored Fury Remastered SVG wordmark on a red
gradient, matching the in-app branding. `tools/build_fury_app_icons.mjs` renders
the three sizes from `web/fury-remastered.svg`; `web/fury-app-icons.json` stores
those authored PNG derivatives with source/image hashes. The normal build checks
and decodes them into the release, so icon pixels and page references deploy
atomically through CI without waiting for a separate R2 upload. The new versioned
filenames replace the earlier original-ATV2 URLs on both entry pages, the manifest,
install banner and help dialog. Safari root aliases serve the 180px static icon;
the legacy 512px alias serves the 512px static icon. Historical recovered icons
stay in the separate R2 package for old links and are never copied into CI.

Host verification for this update: WASM build, delivered identity/PNG dimensions,
12 Worker tests, 35 release checks (one skipped), two install-state tests,
setup/source exclusions and syntax checks pass. The 180px and 512px rendered
wordmarks were inspected. Physical-device Home Screen icon caching is unverified;
an existing saved app may need to be removed and added again after deployment.

The title screen advertises adding the app on iPhone/iPad, including iPads that
identify as a desktop Mac. The banner uses the shared metal/gold controls and
does not cover Press START. It hides on other menus, loading and gameplay, and
when launched in standalone/fullscreen display or iOS `navigator.standalone`.
Dismissal lasts seven days; denied storage falls back to the current page session.
The installed app draws menu decoration on a separate canvas across the full
viewport, while the text/buttons retain the safe-area inset layout. This removes
the empty top/bottom menu bands without putting buttons under the status bar or
home indicator. Portrait-to-landscape rotation keeps those content insets. The
manifest and page use the title palette instead of the former navy background.
The system status bar and home indicator remain controlled by iOS.

There is no website API for triggering iOS Home Screen installation. As checked
on 2026-10-13, WebKit's
[BeforeInstallPrompt request](https://bugs.webkit.org/show_bug.cgi?id=193959)
is resolved WONTFIX; installation remains browser-controlled. The banner opens
a native HTML dialog explaining Page Menu / Share → Add to Home Screen → Open
as Web App (if shown) → Add. The compact Safari toolbar in the user's screenshot
puts Share in the page menu; other toolbar layouts expose Share directly.
See [Apple's steps](https://support.apple.com/guide/iphone/open-as-web-app-iphea86e5236/ios)
and [Safari 26's Home Screen behavior](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/).
Non-Safari iOS browsers get a Safari fallback if their Share menu lacks the action.

Other browsers see the promotion only after delivering `beforeinstallprompt`.
The native prompt is invoked once from the user's button click, never on load.
Acceptance or `appinstalled` hides the banner; dismissal follows the same cooldown,
and prompt failure provides browser-menu instructions. Keyboard actions inside
the banner/dialog cannot also activate game menus, and controller input is blocked
behind the modal until a neutral sample after closing.

Installation does not make the game offline-ready. No service worker or extra
resource cache is introduced by this feature: startup still requires Internet
access to the validated runtime package, and multiplayer requires connectivity.
Existing asset-version and release/content consistency rules still apply.

The user's iOS Add to Home Screen capture showed a generated letter icon. The
existing `/play.html` path is valid and already used an absolute icon link.
The previous public 180px and 512px PNGs return 200/image/png with correct
dimensions for Safari and Apple WebKit networking user agents; the OS fallback
cannot be reproduced from those requests. New links use fresh hash URLs without
query strings, and the Worker also serves conventional `/apple-touch-icon.png`
and `/apple-touch-icon-precomposed.png` URLs from the same validated R2 package.
The former `/app-icon-512.png` URL now resolves to the authored Fury Remastered icon.
The former F-only icon is no longer used. Current static compatibility aliases
resolve to the same checked assets as the explicit icon links.
Icons are checked for hash, size, PNG signature/dimensions and non-symlink paths
before upload. Native iOS icon selection and cached install metadata still need
physical-device confirmation after deployment.

Host Chromium and WebKit verification: authored WASM build, 85 JavaScript tests, 34 release tests
(one skip), source/setup verification and repository hygiene pass. The existing
frontend browser gate passes title/menu/keyboard/controller navigation, Tioga
driving, pause/Controls and portrait Caribou reload. The new
`tools/test_install_app.mjs` gate covers iPhone instructions, 44px hit targets,
unobstructed START, keyboard/controller isolation, title-only visibility, dismissal
across reload, both standalone signals, desktop-identifying iPad, 320px layout,
blocked storage, native prompt accept/dismiss/failure/appinstalled lifecycles,
and full-viewport background with synthetic safe areas in both orientations.
Platform and install signals in that browser gate are synthetic; physical iOS,
OS-level installation and automatic prompt eligibility are not established by it.
All three icon PNGs decode at their expected dimensions, and conventional Safari
root icon paths return PNG responses. Screenshots are kept in ignored private storage. Public verification follows a
passing current-main CI deployment and validated screenshot-gallery upload.
