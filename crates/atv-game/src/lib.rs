//! Platform-independent game layer shared by the web and desktop builds.
//!
//! The browser previously orchestrated loading and presentation in
//! JavaScript; that logic moves here so every build runs the same code.
pub mod actors;
pub mod ambient;
pub mod archive;
pub mod audio;
pub mod burst;
pub mod crash;
pub mod dirt;
pub mod intro;
pub mod jukebox;
pub mod launch;
pub mod lingering_dust;
pub mod materials;
pub mod multiplayer;
pub mod offload;
pub mod ride;
pub mod scenery;
pub mod sky;
pub mod snow;
pub mod terrain_mesh;
pub mod tire_tracks;
pub mod train;
pub mod vegetation;
pub mod venue;
pub mod water;
pub mod wheel_effects;

pub type Result<T> = std::result::Result<T, String>;

/// Yields once to the host's frame loop. Long loads that must not hold up a
/// frame (other riders' models and engine banks) await this between steps;
/// the app polls such loads once per frame. On the web, once a stretch of
/// main-thread work is long enough the browser gets the thread back here
/// (it paints and handles input), see [`atv_content::yield_frame`].
pub fn yield_frame() -> impl std::future::Future<Output = ()> {
    atv_content::yield_frame()
}
