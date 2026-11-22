//! Founding the town: the map and its wards, the people and their
//! households, the firms and who owns them, and the founding constitution.
//!
//! OWNER: worldgen agent. Contract in docs/DESIGN.md §Worldgen.
use crate::types::*;

/// Build a complete, ready-to-run world from a seed.
pub fn generate(seed: u64) -> World { World::empty(seed) }
/// Add a house to a living town on a free lot fronting a road: stamps the map
/// and creates the place (unowned, in the ward it falls in). None if there is no room.
pub fn add_home(_w: &mut World, _name: &str) -> Option<PlaceId> { None }
