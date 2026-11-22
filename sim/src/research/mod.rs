//! Publicly funded research. Money buys scholars' time; what their time
//! yields is uncertain.
//!
//! OWNER: demography agent. Contract in docs/DESIGN.md §Research.
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResearchState {}

/// Morning: staff academies to match funding, pay for the day's work, roll for progress.
pub fn start_of_day(_w: &mut World) {}
