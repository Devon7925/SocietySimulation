//! Birth, death, marriage, inheritance of preferences, upbringing, migration.
//!
//! OWNER: demography agent. Contract in docs/DESIGN.md §Demography.
//!
//! Nothing here decides what a policy "results in". People die by a hazard
//! that depends on their age and health; couples have a child when one more
//! is worth it to them; friends marry when each would be better off; the
//! miserable leave and outsiders come when the town looks better than the
//! road. Whatever a law does to the population it does through those.
//!
//! Layout: `people` (creating people, inheritance, upbringing), `death`
//! (mortality, estates, departures), `family` (births, marriage, coming of
//! age, leaving home), `migration`, `households` (shared bookkeeping).
use crate::types::*;
use serde::{Deserialize, Serialize};

mod death;
mod family;
mod households;
mod migration;
pub mod names;
mod people;
#[cfg(test)]
mod tests;

pub use death::death_hazard;
pub use family::{birth, child_worth, marriage_gain};
pub use households::{close_kin, vacant_homes};
pub use migration::{town_appeal, OUTSIDE_WAGE};
pub use people::{gene_means, pref_means, pref_means_of};

/// A household that has decided to leave and is on its way to a gate.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Leaving {
    pub household: HouseholdId,
    /// The day they set out.
    pub since: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DemoState {
    /// Running totals since the founding.
    pub births: u32,
    pub deaths: u32,
    pub arrivals: u32,
    pub departures: u32,
    /// The same, since this morning.
    pub births_today: u32,
    pub deaths_today: u32,
    pub arrivals_today: u32,
    pub departures_today: u32,
    /// Coin carried into town by arrivals, and out of it by those who left, since the founding.
    pub coin_in: f32,
    pub coin_out: f32,
    /// For each household (by id): days in a row its adults have been faring very badly.
    pub low_days: Vec<u16>,
    /// Households walking to a gate to leave for good.
    pub leaving: Vec<Leaving>,
}

impl DemoState {
    pub fn is_leaving(&self, h: HouseholdId) -> bool { self.leaving.iter().any(|l| l.household == h) }
}

/// Morning: ageing, illness and death, births, courtship and marriage,
/// coming of age, households forming and dissolving, arrivals and departures;
/// weekly, the slow shaping of the young by family, school and friends.
pub fn start_of_day(w: &mut World) {
    w.demo.births_today = 0;
    w.demo.deaths_today = 0;
    w.demo.arrivals_today = 0;
    w.demo.departures_today = 0;
    death::mortality(w);
    family::coming_of_age(w);
    family::births(w);
    family::courtship(w);
    family::leaving_home(w);
    migration::emigration(w);
    migration::immigration(w);
    if w.weekday() == 0 { people::upbringing(w); }
}

/// A person dies. Handles the estate, the household, their job and offices, grief.
pub fn die(w: &mut World, p: PersonId, cause: &str) { death::die(w, p, cause) }
/// A person leaves town for good (emigration, exile). Handles the same loose ends.
pub fn depart(w: &mut World, p: PersonId, why: &str) { death::depart(w, p, why) }
/// Create a person and add them to a household. Preferences are random draws
/// (use `inherit` for a child of known parents).
pub fn new_person(w: &mut World, household: HouseholdId, sex: Option<Sex>, age_years: f32, surname: &str) -> PersonId { people::new_person(w, household, sex, age_years, surname) }
/// Genes and expressed preferences for a child of these parents under `Settings::heritability`.
pub fn inherit(w: &mut World, mother: PersonId, father: Option<PersonId>) -> (Prefs, Prefs) { people::inherit(w, mother, father) }
pub fn new_household(w: &mut World, name: &str) -> HouseholdId { people::new_household(w, name) }
