// Classification of every tools/test_*.mjs browser check. tools/browser/manifest.test.mjs
// (run by `make check`) fails when a test file is missing here or an entry is stale.
// Tests of the retired JavaScript game live in the pinned pre-switch build
// (python3 tools/js_reference.py test).
//
// Groups:
//   content-free  Runs against web/dist with NO private runtime content (CI smoke).
//   content       Needs the ignored private hosted-content package served by the runner.
//   multiplayer   Needs content plus a local game server at the same origin (make
//                 server-run behind tools/serve_multiplayer.mjs). The static runner
//                 server has none; pass --url.
//   manual        One-off/paired gates needing untracked baselines, a second server or
//                 extra tooling. Run by name with the documented environment.
//   broken        Kept for reference; currently fails for the recorded reason.
//
// `native` maps the environment variables naming private original-game comparison
// reports to their usual ignored location. Those tests run their content checks in the
// content group (ATV_NATIVE=skip) and the full comparison only in the explicitly
// requested `native` group, which fills unset variables from these paths when the
// files exist; a missing report fails loudly.
// `exclusive` keeps timing-sensitive checks isolated from host compilation.
// Multiplayer, manual, broken and native runs are always exclusive.
// `defaults` lists user-facing defaults the test relies on (crash mode, camera, shake).

export const GROUPS = ['content-free', 'content', 'multiplayer', 'manual', 'broken'];
export const SELECTABLE = [...GROUPS, 'native'];

export const TESTS = {
  'test_app_identity.mjs': { group: 'content-free' },
  'test_engine_boot_stall.mjs': { group: 'content-free' },
  'test_engine_touch.mjs': { group: 'content' },
  'test_engine_hud.mjs': { group: 'content' },
  'test_engine_pause.mjs': { group: 'content' },
  'test_engine_photo.mjs': { group: 'content' },
  'test_engine_replay.mjs': { group: 'content' },
  'test_engine_race_replay.mjs': { group: 'content' },
  'test_engine_alpha.mjs': { group: 'content' },
  'test_engine_alpha_muffle.mjs': { group: 'content' },
  'test_engine_alpha_landing.mjs': { group: 'content' },
  'test_engine_air_blur.mjs': { group: 'content' },
  'test_engine_shell.mjs': { group: 'content' },
  'test_engine_sw_update.mjs': { group: 'content' },
  'test_engine_remastered.mjs': { group: 'content' },
  'test_engine_remastered_rain.mjs': { group: 'content' },
  'test_engine_fold_layout.mjs': { group: 'content' },
  'test_engine_crisp_hud.mjs': { group: 'content' },
  'test_engine_shadows_prefetch.mjs': { group: 'content' },
  'test_engine_visibility.mjs': { group: 'content' },
  'test_engine_arcade_touch.mjs': {
    group: 'manual',
    reason:
      'Compares with the JavaScript game: serve the pinned reference (python3 tools/js_reference.py serve) and set ATV_JS_URL'
  },
  'test_engine_settings.mjs': { group: 'content' },
  'test_engine_game_settings.mjs': { group: 'content' },
  'test_engine_accessibility.mjs': { group: 'content' },
  'test_engine_options_layout.mjs': { group: 'content' },
  'test_engine_fps.mjs': { group: 'content' },
  'test_engine_session_settings.mjs': { group: 'content' },
  'test_engine_links.mjs': { group: 'content' },
  'test_engine_wake_lock.mjs': { group: 'content' },
  'test_engine_diagnostics.mjs': { group: 'content' },
  'test_engine_background_audio.mjs': { group: 'content' },
  'test_engine_jukebox.mjs': { group: 'content' },
  'test_engine_menu_jukebox.mjs': { group: 'content' },
  'test_engine_ride_songs.mjs': { group: 'content' },
  'test_engine_audio_page.mjs': { group: 'content' },
  'test_engine_boot.mjs': { group: 'content' },
  'test_engine_lifecycle.mjs': { group: 'content' },
  'test_engine_soundtrack.mjs': { group: 'content' },
  'test_engine_weather.mjs': { group: 'content' },
  'test_engine_ride_link.mjs': { group: 'content' },
  'test_engine_menu_preload.mjs': { exclusive: true, group: 'content' },
  'test_engine_menu_preload_ride.mjs': { exclusive: true, group: 'content' },
  'test_engine_content_retry.mjs': { group: 'content' },
  'test_engine_decoded_cache.mjs': { group: 'content' },
  'test_engine_freeride.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (shared Free Ride rooms, room WebSocket); run with --url'
  },
  'test_engine_front.mjs': { group: 'content' },
  'test_engine_race_gear.mjs': { group: 'content' },
  'test_engine_race_practice.mjs': { group: 'content', timeout: 600 },
  'test_engine_race_kid.mjs': { group: 'content', timeout: 600 },
  'test_engine_race_duel.mjs': { group: 'content', timeout: 900 },
  'test_engine_race_lap_attack.mjs': { group: 'content', timeout: 900 },
  'test_engine_menu_parity.mjs': { group: 'content' },
  'test_engine_menu_flows.mjs': { group: 'content' },
  'test_engine_menu_navigation.mjs': { group: 'content' },
  'test_engine_credits.mjs': { group: 'content' },
  'test_engine_menu_divider.mjs': { group: 'content' },
  'test_engine_menu_long_frames.mjs': { exclusive: true, group: 'content' },
  'test_engine_menu_scroll.mjs': { exclusive: true, group: 'content' },
  'test_engine_status_bar.mjs': { group: 'content' },
  'test_engine_menu_transitions.mjs': { exclusive: true, group: 'content' },
  'test_engine_menu_motion.mjs': { group: 'content' },
  'test_engine_menu_phone_layout.mjs': { group: 'content' },
  'test_engine_selector_latency.mjs': { exclusive: true, group: 'content' },
  'test_engine_garage_load.mjs': { exclusive: true, group: 'content' },
  'test_engine_context_loss.mjs': { group: 'content' },
  'test_engine_menu_music.mjs': { group: 'content' },
  'test_engine_freeride_full.mjs': {
    group: 'manual',
    reason: 'load measurement: a full Free Ride room of synthetic riders on a local game server (never production)'
  },
  'test_engine_freeride_relay.mjs': {
    group: 'multiplayer',
    timeout: 900,
    reason: 'needs a local game server at the same origin (Rust riders without links in a private room, then a shared Free Ride room, then with a rider on a build without pose batches; counts the frames each sends the server); run with --url'
  },
  'test_engine_multiplayer_links.mjs': {
    group: 'broken',
    reason:
      'direct WebRTC links need a server that offers them; only the retired Cloudflare relay did, and fury-server answers links: false (research/GAME_SERVER.md)'
  },
  'test_engine_multiplayer_tag.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest Tag room); run with --url'
  },
  'test_engine_multiplayer_koth.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest King of the Hills room); run with --url'
  },
  'test_engine_multiplayer_treasure.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest Treasure Hunt room); run with --url'
  },
  'test_engine_multiplayer_freestyle.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game → Room: Freestyle; Rust guest and late spectator, host handoff); run with --url'
  },
  'test_engine_multiplayer_race.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest race a Supercross room); run with --url'
  },
  'test_engine_multiplayer_race_cpus.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest race a Supercross room filled with host-simulated CPUs); run with --url'
  },
  'test_engine_multiplayer_race_cpu_sounds.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (a Rust guest hears the host CPUs\' drive-on sounds in an ENV01 room race); run with --url'
  },
  'test_engine_multiplayer_race_cpus_handoff.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (the host of a CPU-filled Supercross room leaves mid-race; two Rust guests retire the CPUs); run with --url'
  },
  'test_engine_games_tag.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Games → Tag hosts a room; a Rust guest joins); run with --url'
  },
  'test_engine_multiplayer_hockey.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Rust host and guest Hockey room); run with --url'
  },
  'test_engine_multiplayer_dropin.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_context.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_audio.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_end.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (room creation, slot-key reclaim, Free Ride); run with --url'
  },
  'test_engine_multiplayer_menu.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (room creation, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_pause.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_presence.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_room_front.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (room creation, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_tags.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (Host Game, room WebSocket); run with --url'
  },
  'test_engine_multiplayer_train.mjs': {
    group: 'multiplayer',
    reason: 'needs a local game server at the same origin (room WebSocket, ENV08 train); run with --url'
  },
  'test_engine_multiplayer_idle_host.mjs': {
    group: 'multiplayer',
    reason:
      'needs a local game server at the same origin plus a native fury build (FURY_BIN) for the stopped host; run with --url'
  },
  'test_engine_multiplayer.mjs': {
    group: 'multiplayer',
    reason:
      'needs a local game server at the same origin plus a native fury build (FURY_BIN) for the Rust host; run with --url'
  },
  'test_engine_preview.mjs': { group: 'content-free' },
  'test_engine_race.mjs': { group: 'content' },
  'test_engine_cpu_audio.mjs': { group: 'content' },
  'test_engine_split_screen.mjs': { exclusive: true, group: 'content', timeout: 900 },
  'test_engine_split_facing.mjs': { group: 'content', timeout: 600 },
  'test_engine_split_games.mjs': { group: 'content', timeout: 900 },
  'test_engine_split_tag.mjs': { group: 'content', timeout: 600 },
  'test_engine_split_4p.mjs': { group: 'content', timeout: 1500 },
  'test_engine_split_seats.mjs': { group: 'content', timeout: 900 },
  'test_engine_split_objectives.mjs': { group: 'content', timeout: 900 },
  'test_engine_race_grid.mjs': { group: 'content' },
  'test_engine_race_drop_gate.mjs': { group: 'content' },
  'test_engine_race_finish_gate.mjs': { group: 'content' },
  'test_engine_race_alternates.mjs': { group: 'content', timeout: 900 },
  'test_engine_enduro_race.mjs': { group: 'content', timeout: 900 },
  'test_engine_user_course.mjs': { group: 'content', timeout: 900 },
  'test_engine_race_profile_points.mjs': { group: 'content' },
  'test_engine_profile_menu.mjs': { group: 'content' },
  'test_engine_profile_store.mjs': { group: 'content', timeout: 600 },
  'test_engine_profile_editor.mjs': { group: 'content', timeout: 900 },
  'test_engine_passkey.mjs': { group: 'content', timeout: 600 },
  'test_engine_unlock_everything.mjs': { group: 'content', timeout: 600 },
  'test_engine_profile_stunts.mjs': { group: 'content', timeout: 900 },
  'test_engine_hall_of_fame.mjs': { group: 'content', timeout: 900 },
  'test_engine_world_boards.mjs': { group: 'content', timeout: 900 },
  'test_engine_world_ghosts.mjs': { group: 'content', timeout: 900 },
  'test_engine_world_submit.mjs': { group: 'content', timeout: 900 },
  'test_engine_text_fields_touch.mjs': { group: 'content' },
  'test_engine_championship.mjs': { group: 'content', timeout: 1500 },
  'test_engine_championship_loading.mjs': { group: 'content' },
  'test_engine_championship_enduro.mjs': { group: 'content', timeout: 900 },
  'test_engine_freestyle_objectives.mjs': { group: 'content', timeout: 900 },
  'test_engine_freestyle_targets.mjs': { group: 'content', timeout: 900 },
  'test_engine_toy_actors.mjs': { group: 'content', timeout: 600 },
  'test_engine_actor_collision.mjs': { group: 'content', timeout: 600 },
  'test_engine_training.mjs': { group: 'content', timeout: 900 },
  'test_engine_games.mjs': { group: 'content', timeout: 900 },
  'test_engine_game_cpus.mjs': { group: 'content', timeout: 900 },
  'test_engine_toy_explosions.mjs': { group: 'content', timeout: 900 },
  'test_engine_hockey_tag.mjs': { group: 'content', timeout: 600 },
  'test_engine_sleep.mjs': { group: 'content' },
  'test_engine_motorcycle.mjs': { group: 'content' },
  'test_engine_interpolation.mjs': {
    group: 'manual',
    reason: 'Compares with the JavaScript presenters of the pinned pre-switch build: run python3 tools/js_reference.py prepare first'
  },
  'test_engine_wheel_reprojection.mjs': { group: 'content' },
  'test_engine_cameras.mjs': { group: 'content' },
  'test_engine_rumble.mjs': { group: 'content' },
  'test_rust_audio.mjs': { group: 'content' },
  'test_rust_audio_activation.mjs': { group: 'content' },
  'test_devlog.mjs': { group: 'content-free' },
  'test_stats.mjs': { group: 'content-free' }
};

// Unknown/heavier runs retain conservative admission. Mixed selections use the
// strongest requirement so a performance report never includes compiler contention.
export function hostWorkload(runs) {
  return runs.length && runs.every(({ name, group }) => TESTS[name]
    && ['content-free', 'content'].includes(group) && !TESTS[name].exclusive)
    ? 'browser' : 'exclusive';
}
