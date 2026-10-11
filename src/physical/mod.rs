//! Physical-engine foundations, separate from the current playable event voice.
//! Component validation does not establish a complete running engine or realism.
pub mod acoustic;
pub mod combustion_model;
pub mod config;
pub mod controller;
pub mod coupled_1d;
pub mod crank;
pub mod cycle;
pub mod cylinder;
pub mod engine;
mod excitation;
pub mod finite_volume;
pub mod fuel_transient;
pub mod gas;
pub mod induction;
pub mod intake_acoustic;
pub mod manifolds;
mod mechanical;
pub mod structural;
pub mod thermo;
pub mod tone;
pub mod turbo_map;
pub mod wave_junction;

mod radiation;
