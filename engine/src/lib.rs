//! Claude Talk audio engine.
//!
//! * [`graph`] – routing rules (no echo, no feedback).
//! * [`dsp`] – gain, meters, true-peak limiter.
//! * [`drift`] / [`resample`] – clock-drift compensation and rate conversion.
//! * [`mixer`] – the real-time mixing core used by every output.

pub mod analysis;
pub mod drift;
pub mod dsp;
pub mod generator;
pub mod graph;
pub mod mixer;
pub mod resample;

/// Internal sample rate of the engine.
pub const ENGINE_RATE: u32 = 48_000;
/// Internal channel count of the engine (stereo).
pub const ENGINE_CHANNELS: usize = 2;

#[cfg(windows)]
pub mod win;
