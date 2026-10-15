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
// `native` lists private original-game comparison reports. Those tests run their
// content checks in the content group (ATV_NATIVE=skip) and the full comparison only
// in the explicitly requested `native` group, where missing data fails loudly.
// `defaults` lists user-facing defaults the test relies on (crash mode, camera, shake).

export const GROUPS = ['content-free', 'content', 'relay', 'manual', 'broken'];
export const SELECTABLE = [...GROUPS, 'native'];

export const TESTS = {
  'test_ambient_catalog.mjs': { group: 'content' },
  'test_ambient_effects.mjs': { group: 'content' },
  'test_app_identity.mjs': { group: 'content-free' },
  'test_bindings.mjs': { group: 'content' },
  'test_body_impact_audio.mjs': { group: 'content' },
  'test_boundary_launch.mjs': { group: 'content' },
  'test_browser.mjs': { group: 'content-free' },
  'test_camera_gorn.mjs': { group: 'content' },
  'test_clean_landing.mjs': { group: 'content' },
  'test_cpu_behavior.mjs': { group: 'content' },
  'test_devlog.mjs': { group: 'content-free' },
  'test_diagnostics.mjs': { group: 'content' },
  'test_enhanced_mobile.mjs': { group: 'content' },
  'test_fog.mjs': { group: 'content', native: ['ATV_FOG_NATIVE'] },
  'test_frontend.mjs': { group: 'content' },
  'test_funny_crash.mjs': { group: 'content' },
  'test_hd_settings.mjs': { group: 'content' },
  'test_hud_efficiency.mjs': { group: 'content' },
  'test_install_app.mjs': { group: 'content' },
  'test_ios_fullscreen.mjs': { group: 'content' },
  'test_launch_links.mjs': { group: 'content' },
  'test_lifecycle.mjs': { group: 'content' },
  'test_lobby_previews.mjs': { group: 'content' },
  'test_maps.mjs': { group: 'content' },
  'test_menu_models.mjs': { group: 'content' },
  'test_menu_preferences.mjs': { group: 'content' },
  'test_minimap.mjs': { group: 'content' },
  'test_minimap_instrument.mjs': { group: 'content' },
  'test_minimap_orientation.mjs': { group: 'content' },
  'test_minimap_projection.mjs': { group: 'content' },
  'test_minimap_release.mjs': { group: 'content' },
  'test_mobile_ui.mjs': { group: 'content' },
  'test_mobile_utilities.mjs': { group: 'content' },
  'test_motorcycle_controls.mjs': { group: 'content' },
  'test_motorcycle_crash.mjs': { group: 'content' },
  'test_motorcycle_menu.mjs': { group: 'content' },
  'test_motorcycles.mjs': { group: 'content' },
  'test_multiplayer.mjs': { group: 'content' },
  'test_multiplayer_dropin.mjs': { group: 'content' },
  'test_multiplayer_efficiency.mjs': { group: 'content' },
  'test_multiplayer_features.mjs': { group: 'content' },
  'test_multiplayer_four.mjs': { group: 'content' },
  'test_multiplayer_relay.mjs': { group: 'content' },
  'test_music.mjs': { group: 'content' },
  'test_one_handed_touch.mjs': { group: 'content' },
  'test_pause_menu.mjs': { group: 'content' },
  'test_player_models.mjs': { group: 'content' },
  'test_portrait_camera.mjs': { group: 'content' },
  'test_race.mjs': { group: 'content' },
  'test_race_ai.mjs': { group: 'content' },
  'test_race_courses.mjs': { group: 'content' },
  'test_rails.mjs': { group: 'content' },
  'test_reflections.mjs': { group: 'content', native: ['ATV_REFLECTION_NATIVE'] },
  'test_render_efficiency.mjs': { group: 'content' },
  'test_render_lifecycle.mjs': { group: 'content' },
  'test_renderer.mjs': { group: 'content' },
  'test_scenery_materials.mjs': { group: 'content' },
  'test_settings.mjs': { group: 'content' },
  'test_simulation_transport.mjs': { group: 'content' },
  'test_slope_turn.mjs': { group: 'content' },
  'test_snow.mjs': { group: 'content' },
  'test_snow_collisions.mjs': { group: 'content' },
  'test_speed_rush.mjs': { group: 'content' },
  'test_stadium_collisions.mjs': { group: 'content' },
  'test_stadium_screens.mjs': { group: 'content' },
  'test_steering_sensitivity.mjs': { group: 'content' },
  'test_tight_crash_camera.mjs': { group: 'content' },
  'test_touch.mjs': { group: 'content' },
  'test_touch_batches.mjs': { group: 'content' },
  'test_touch_pose.mjs': { group: 'content' },
  'test_touch_ramp.mjs': { group: 'content' },
  'test_touch_stunts.mjs': { group: 'content' },
  'test_train.mjs': { group: 'content' },
  'test_train_audio.mjs': { group: 'content' },
  'test_train_collisions.mjs': { group: 'content' },
  'test_training_start.mjs': { group: 'content' },
  'test_trick_wheel.mjs': { group: 'content-free' },
  'test_ui_polish.mjs': { group: 'content' },
  'test_uphill_camera.mjs': { group: 'content' },
  'test_vancouver.mjs': { group: 'content' },
  'test_vegetation_billboards.mjs': { group: 'content', native: ['ATV_BILLBOARD_NATIVE'] },
  'test_vegetation_contact.mjs': { group: 'content' },
  'test_vegetation_materials.mjs': { group: 'content', native: ['ATV_ECO_NATIVE'] },
  'test_water_physics.mjs': { group: 'content' },
  'test_water_render.mjs': { group: 'content' },
  'test_weather.mjs': { group: 'content' },
  'test_weather_lifecycle.mjs': { group: 'content' },
  'test_wheel_effects.mjs': { group: 'content' },
  'test_wheel_wet.mjs': { group: 'content' },
  'test_world_orientation.mjs': { group: 'content' },
  'test_wreck.mjs': { group: 'content' },
};
