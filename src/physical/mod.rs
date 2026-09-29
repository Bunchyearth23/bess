//! Physical-engine foundations, separate from the current playable event voice.
//! Component validation does not establish a complete running engine or realism.
pub mod acoustic;
pub mod config;
pub mod controller;
pub mod crank;
pub mod cycle;
pub mod cylinder;
pub mod engine;
pub mod gas;
pub mod induction;
pub mod intake_acoustic;
pub mod manifolds;
mod mechanical;
pub mod thermo;
pub mod tone;

mod radiation;
