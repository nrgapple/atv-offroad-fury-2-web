//! Ride audio shared by every build: original samples, the recovered sound
//! decisions in atv-core and a software mixer with the browser game's Web
//! Audio semantics. Platform output lives in atv-app.
pub mod bank;
pub mod mixer;
pub mod music;
pub mod ride;
