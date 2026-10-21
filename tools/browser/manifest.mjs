// Classification of every tools/test_*.mjs browser check. tools/browser/manifest.test.mjs
// (run by `make check`) fails when a test file is missing here or an entry is stale.
//
// Groups:
//   content-free  Runs against web/dist with NO private runtime content (CI smoke).
//   content       Needs the ignored private hosted-content package served by the runner.
//   relay         Needs content plus a same-origin multiplayer relay (wrangler/workerd or
//                 public HTTPS). The static runner server has no relay; pass --url.
//   manual        One-off/paired gates needing untracked baselines, a second server or
//                 extra tooling. Run by name with the documented environment.
//   broken        Kept for reference; currently fails for the recorded reason.
//
// `native` maps the environment variables naming private original-game comparison
// reports to their usual ignored location. Those tests run their content checks in the
// content group (ATV_NATIVE=skip) and the full comparison only in the explicitly
// requested `native` group, which fills unset variables from these paths when the
// files exist; a missing report fails loudly.
// `defaults` lists user-facing defaults the test relies on (crash mode, camera, shake).

export const GROUPS = ['content-free', 'content', 'relay', 'manual', 'broken'];
export const SELECTABLE = [...GROUPS, 'native'];

export const TESTS = {
  'test_ambient_catalog.mjs': { group: 'content' },
  'test_ambient_effects.mjs': { group: 'content' },
  'test_app_identity.mjs': { group: 'content-free' },
  'test_alpha_hype.mjs': { group: 'content' },
  'test_arcade_flips.mjs': { group: 'content' },
  'test_atv_flips.mjs': { group: 'content' },
  'test_bindings.mjs': { group: 'content' },
  'test_body_impact_audio.mjs': {
    defaults: ['pins crash mode crazy'],
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: expects Pause to suspend the AudioContext; pause now keeps the soundtrack running (PR #40)'
  },
  'test_engine_touch.mjs': { group: 'content' },
  'test_engine_hud.mjs': { group: 'content' },
  'test_engine_pause.mjs': { group: 'content' },
  'test_engine_settings.mjs': { group: 'content' },
  'test_engine_ride_link.mjs': { group: 'content' },
  'test_engine_freeride.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (shared Free Ride rooms, room WebSocket); run with --url'
  },
  'test_engine_front.mjs': { group: 'content' },
  'test_engine_multiplayer_dropin.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_audio.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_train.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (room WebSocket, ENV08 train); run with --url'
  },
  'test_engine_multiplayer.mjs': {
    group: 'relay',
    reason:
      'needs the multiplayer relay at the same origin plus a native fury build (FURY_BIN) for the Rust host; run with --url'
  },
  'test_engine_preview.mjs': { group: 'content-free' },
  'test_engine_race.mjs': { group: 'content' },
  'test_engine_sleep.mjs': { group: 'content' },
  'test_engine_motorcycle.mjs': { group: 'content' },
  'test_engine_interpolation.mjs': { group: 'content' },
  'test_rust_audio.mjs': { group: 'content' },
  'test_rust_audio_activation.mjs': { group: 'content' },
  'test_boundary_launch.mjs': { defaults: ['pins crash mode crazy'], group: 'content' },
  'test_browser.mjs': { group: 'content-free' },
  'test_camera_gorn.mjs': {
    defaults: ['crash camera (tight)', 'crash-mode cycle order', 'no camera shake'],
    group: 'content'
  },
  'test_clean_landing.mjs': { group: 'content' },
  'test_cpu_behavior.mjs': { defaults: ['pins crash mode crazy'], group: 'content' },
  'test_devlog.mjs': { group: 'content-free' },
  'test_diagnostics.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has a 'Stats: Off (F3)' row (settings moved under the Settings hub)"
  },
  'test_enhanced_audio.mjs': { group: 'content' },
  'test_enhanced_mobile.mjs': { group: 'content' },
  'test_fog.mjs': { group: 'content', native: { ATV_FOG_NATIVE: 'private/reports/map-fidelity/fog-native.json' } },
  'test_frontend.mjs': { group: 'content' },
  'test_freeride.mjs': {
    group: 'relay',
    reason:
      'needs the multiplayer relay at the same origin (shared Free Ride rooms, room WebSocket) and the Rust codec example; run with --url'
  },
  'test_funny_crash.mjs': {
    defaults: ['pins crash mode crazy'],
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: the bailout no longer produces intact-ATV aerial ragdoll frames before impact'
  },
  'test_graphics_presets.mjs': {
    group: 'content',
    note: 'Set ATV_GRAPHICS_BASELINE to a server running the previous main build for the Original pixel-identical gate'
  },
  'test_hd_settings.mjs': {
    defaults: ['crash mode (original)', 'crash camera (tight)', 'chase camera (original)'],
    group: 'content'
  },
  'test_hud_efficiency.mjs': {
    group: 'manual',
    reason: 'Needs an untracked baseline hud.js under ATV_PERF_OUTPUT (default work/verification/performance/baseline)'
  },
  'test_install_app.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: standalone safe-area check: #menu-background never satisfies the expected inset/size'
  },
  'test_ios_fullscreen.mjs': { group: 'content' },
  'test_jukebox.mjs': { group: 'content' },
  'test_launch_links.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_lifecycle.mjs': { group: 'content' },
  'test_lobby_previews.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_maps.mjs': { group: 'content' },
  'test_menu_models.mjs': { group: 'content' },
  'test_menu_preferences.mjs': { group: 'content' },
  'test_minimap.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has 'HUD settings' (now Settings > Display > Minimap configuration)"
  },
  'test_minimap_instrument.mjs': { group: 'content' },
  'test_minimap_orientation.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has 'HUD settings' (now Settings > Display > Minimap configuration)"
  },
  'test_minimap_projection.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has 'HUD settings' (now Settings > Display > Minimap configuration)"
  },
  'test_minimap_release.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: the cached f2b59ec hud.js fixture lacks HeadUpDisplay.snapshot(), which current game.js calls, so the old-cache page throws'
  },
  'test_mobile_ui.mjs': { group: 'content' },
  'test_mobile_utilities.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: touch toolbar now also shows 'Keyboard controls (H)' (#79); decide whether that button belongs on touch layouts before updating the expected [Pause, Camera]"
  },
  'test_motorcycle_controls.mjs': { defaults: ['pins crash mode crazy'], group: 'content' },
  'test_motorcycle_crash.mjs': { defaults: ['pins crash mode crazy unless ATV_CRASH_MODE'], group: 'content' },
  'test_motorcycle_menu.mjs': { group: 'content' },
  'test_motorcycle_train.mjs': { group: 'content' },
  'test_motorcycle_vegetation.mjs': { group: 'content' },
  'test_motorcycles.mjs': { defaults: ['pins crash mode crazy'], group: 'content' },
  'test_multiplayer.mjs': {
    defaults: ['pins crash mode crazy'],
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_multiplayer_dropin.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_multiplayer_efficiency.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_multiplayer_features.mjs': {
    defaults: ['pins crash mode crazy'],
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_multiplayer_four.mjs': {
    group: 'relay',
    reason: 'needs the multiplayer relay at the same origin (Host/Join Game, room WebSocket); run with --url'
  },
  'test_multiplayer_relay.mjs': {
    group: 'manual',
    reason:
      'Not a browser test: drives a running Rust relay (ATV_RELAY_URL) through WS_MODULE and the atv-relay codec example build'
  },
  'test_music.mjs': { group: 'content' },
  'test_offline.mjs': { group: 'content' },
  'test_one_handed_touch.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has the 'One-handed mobile' row at the root (Settings hub)"
  },
  'test_pause_menu.mjs': { group: 'content' },
  'test_player_models.mjs': {
    group: 'content',
    note: 'Compares against private/reports/engine-audio-configs and writes private reports'
  },
  'test_portrait_camera.mjs': { defaults: ['follow camera', 'chase camera (original)'], group: 'content' },
  'test_race.mjs': { group: 'content' },
  'test_race_ai.mjs': { group: 'content' },
  'test_pwa_ios.mjs': { group: 'content' },
  'test_quad_audio.mjs': { group: 'content' },
  'test_race_courses.mjs': { group: 'content' },
  'test_race_music.mjs': { group: 'content' },
  'test_rails.mjs': { group: 'content' },
  'test_reflections.mjs': {
    group: 'content',
    native: { ATV_REFLECTION_NATIVE: 'private/reports/map-fidelity/reflection-native.json' }
  },
  'test_render_efficiency.mjs': {
    group: 'manual',
    reason: 'Paired timing against a second baseline server (ATV_PERF_BASELINE)'
  },
  'test_render_lifecycle.mjs': { group: 'content' },
  'test_renderer.mjs': {
    defaults: ['follow camera preset/fov'],
    group: 'content',
    env: { ATV_TEST_BUNDLED: '1' },
    note: 'Hosted content; set ATV_TEST_ISO instead for the private disc path'
  },
  'test_rider_gear.mjs': { group: 'content' },
  'test_scenery_materials.mjs': { group: 'content' },
  'test_settings.mjs': { group: 'content' },
  'test_simulation_transport.mjs': { group: 'content' },
  'test_slope_turn.mjs': { group: 'content' },
  'test_snow.mjs': { group: 'content' },
  'test_snow_collisions.mjs': { group: 'content' },
  'test_speed_rush.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: speedRush.draws never reaches 1 while driving with the default Normal setting'
  },
  'test_stadium_collisions.mjs': { group: 'content' },
  'test_stadium_screens.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: 'glow source groups visible' assertion fails after the stadium screen/lamp restoration (#68)"
  },
  'test_steering_sensitivity.mjs': {
    group: 'broken',
    reason:
      "Fails on unmodified origin/main too: pause menu no longer has 'Controls' at the root (Settings hub), and its steer signs predate the unmirrored steering sign (3574454)"
  },
  'test_tight_crash_camera.mjs': {
    defaults: ['pins crash mode crazy', 'crash camera (tight)', 'camera shake ignored'],
    group: 'broken',
    reason: 'Fails on unmodified origin/main too: the scripted route no longer exercises rider-priority separation'
  },
  'test_touch.mjs': { group: 'content' },
  'test_touch_batches.mjs': { group: 'content' },
  'test_touch_pose.mjs': { group: 'content' },
  'test_touch_ramp.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: right-hand touch hold never reaches airborne preload_active on the Caribou ramp under current touch controls'
  },
  'test_touch_stunts.mjs': {
    defaults: ['pins crash mode crazy'],
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: short-hop touch stunt never reaches the expected stunt state under current touch controls'
  },
  'test_train.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: train pixel capture hook (trainPixelEvidence) is never set by the current renderer'
  },
  'test_train_audio.mjs': {
    group: 'broken',
    reason:
      'Fails on unmodified origin/main too: expects Pause to suspend the AudioContext; pause now keeps the soundtrack running (PR #40)'
  },
  'test_train_collisions.mjs': { group: 'content' },
  'test_train_riding.mjs': { group: 'content' },
  'test_train_smoke.mjs': { group: 'content' },
  'test_training_start.mjs': { group: 'content' },
  'test_trick_wheel.mjs': { group: 'content-free' },
  'test_ui_polish.mjs': { group: 'content' },
  'test_uphill_camera.mjs': { defaults: ['follow camera preset 0', 'pins chase camera assisted'], group: 'content' },
  'test_vancouver.mjs': { group: 'content' },
  'test_vegetation_billboards.mjs': {
    group: 'content',
    native: {
      ATV_BILLBOARD_NATIVE: 'private/reports/map-fidelity/billboard-light-native.json',
      ATV_BILLBOARD_GEOMETRY: 'private/reports/map-fidelity/billboard-geometry-native.json'
    }
  },
  'test_vegetation_contact.mjs': { group: 'content' },
  'test_vegetation_materials.mjs': {
    group: 'content',
    native: { ATV_ECO_NATIVE: 'private/reports/map-fidelity/vegetation-sun-native.json' }
  },
  'test_water_physics.mjs': { defaults: ['pins crash mode crazy'], group: 'content' },
  'test_water_render.mjs': { group: 'content' },
  'test_weather.mjs': { group: 'content' },
  'test_weather_lifecycle.mjs': { group: 'content' },
  'test_wheel_effects.mjs': { group: 'content' },
  'test_wheel_wet.mjs': { group: 'content' },
  'test_world_orientation.mjs': { defaults: ['follow camera'], group: 'content' },
  'test_wreck.mjs': {
    defaults: ['crash-mode cycle order', 'pins legacy atv-funny-crashes', 'crash camera (tight)'],
    group: 'content'
  }
};
