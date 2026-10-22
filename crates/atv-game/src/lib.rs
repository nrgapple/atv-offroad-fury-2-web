//! Platform-independent game layer shared by the web and desktop builds.
//!
//! The browser previously orchestrated loading and presentation in
//! JavaScript; that logic moves here so every build runs the same code.
pub mod ambient;
pub mod archive;
pub mod audio;
pub mod crash;
pub mod intro;
pub mod jukebox;
pub mod launch;
pub mod lingering_dust;
pub mod materials;
pub mod multiplayer;
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
/// the app polls such loads once per frame.
pub fn yield_frame() -> impl std::future::Future<Output = ()> {
    struct Yield(bool);
    impl std::future::Future for Yield {
        type Output = ();
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            if self.0 {
                std::task::Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }
    }
    Yield(false)
}
