//! Founding the town: the map and its wards, the people and their
//! households, the firms and who owns them, and the founding constitution.
//!
//! OWNER: worldgen agent. Contract in docs/DESIGN.md §Worldgen.
use crate::types::*;

/// Build a complete, ready-to-run world from a seed.
pub fn generate(seed: u64) -> World { World::empty(seed) }
