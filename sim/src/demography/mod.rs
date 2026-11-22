//! Birth, death, marriage, inheritance of preferences, upbringing, migration.
//!
//! OWNER: demography agent. Contract in docs/DESIGN.md §Demography.
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DemoState {}

/// Morning: ageing, illness and death, births, courtship and marriage,
/// coming of age, households forming and dissolving, arrivals and departures;
/// weekly, the slow shaping of the young by family, school and friends.
pub fn start_of_day(_w: &mut World) {}
/// A person dies. Handles the estate, the household, their job and offices, grief.
pub fn die(_w: &mut World, _p: PersonId, _cause: &str) {}
/// A person leaves town for good (emigration, exile). Handles the same loose ends.
pub fn depart(_w: &mut World, _p: PersonId, _why: &str) {}
/// Create a person and add them to a household. Preferences are random draws
/// (use `inherit` for a child of known parents).
pub fn new_person(_w: &mut World, _household: HouseholdId, _sex: Option<Sex>, _age_years: f32, _surname: &str) -> PersonId { PersonId(0) }
/// Genes and expressed preferences for a child of these parents under `Settings::heritability`.
pub fn inherit(_w: &mut World, _mother: PersonId, _father: Option<PersonId>) -> (Prefs, Prefs) { (Prefs::default(), Prefs::default()) }
pub fn new_household(_w: &mut World, _name: &str) -> HouseholdId { HouseholdId(0) }
