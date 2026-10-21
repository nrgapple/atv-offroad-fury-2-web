//! Platform-independent game layer shared by the web and desktop builds.
//!
//! The browser previously orchestrated loading and presentation in
//! JavaScript; that logic moves here so every build runs the same code.
pub mod launch;
pub mod materials;
pub mod scenery;
pub mod sky;
pub mod terrain_mesh;
pub mod vegetation;
pub mod venue;

pub type Result<T> = std::result::Result<T, String>;
