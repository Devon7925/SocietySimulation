//! Town-wide aggregates, refreshed nightly.
use crate::types::*;

/// Recompute `w.stats` (and household wealth ranks) and append to `w.history`.
pub fn end_of_day(_w: &mut World) {}
/// Recompute the parts of `w.stats` other modules read during the day (medians, prices, ranks) without recording history.
pub fn refresh(_w: &mut World) {}
