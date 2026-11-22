//! Daily life: needs, the choice of what to do next, and wellbeing - the one
//! number each person is trying to raise, built from their base preferences.
//!
//! OWNER: life agent. Contract in docs/DESIGN.md §Life.
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LifeState {}
/// Per-person planner state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Mind {}

/// Morning: per-person resets and the day's standing decisions.
pub fn start_of_day(_w: &mut World) {}
/// One slot of one person's life: needs tick; arrive if a walk is done; when
/// the current activity ends, choose the next.
pub fn update_person(_w: &mut World, _p: PersonId) {}
/// Night: fold the day into each person's wellbeing and trust; refresh opinions of statutes.
pub fn end_of_day(_w: &mut World) {}

/// Send a person walking to a place to begin `plan` on arrival (immediately if already there).
pub fn go(_w: &mut World, _p: PersonId, _to: PlaceId, _plan: Plan) {}
/// Wellbeing as it stands this instant, from current state (no smoothing).
pub fn wellbeing_now(_w: &World, _p: PersonId) -> Wellbeing { Wellbeing::default() }
/// Recompute everyone's opinion of one statute or bill now (used when a bill is introduced).
pub fn form_opinions(_w: &mut World, _sid: StatuteId) {}
