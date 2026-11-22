//! The clock: what happens in what order.
use crate::types::*;

/// Advance the world by one slot (15 minutes).
pub fn step(_w: &mut World) {}
/// Advance by whole days, as fast as possible.
pub fn run_days(w: &mut World, days: u32) { for _ in 0..days * SLOTS_PER_DAY { step(w); } }
