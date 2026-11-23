//! Founding the town: the map and its wards, the people and their
//! households, the firms and who owns them, and the founding constitution.
//!
//! OWNER: worldgen agent. Contract in docs/DESIGN.md §Worldgen.
//!
//! * `charter` - the polity (town, four wards, the Sovereign) and the founding statutes;
//! * `map`     - tiles, lots, places, wards by position, walking times;
//! * `people`  - households, people, ownership, tenancies, jobs, friendships;
//! * `names`   - name lists.
use crate::types::*;

pub mod charter;
pub mod map;
pub mod names;
pub mod people;
#[cfg(test)]
mod tests;

pub use charter::founding_statutes;
pub use map::{MAP_H, MAP_W};

/// Build a complete, ready-to-run world from a seed.
///
/// The clock stands at 06:00 on day 0; everyone is at home. All chance is
/// drawn from `World::rng`, so the same seed always founds the same town.
pub fn generate(seed: u64) -> World {
    let mut w = World::empty(seed);
    w.town_name = names::TOWN_NAMES[w.rng.index(names::TOWN_NAMES.len())].to_string();
    charter::found_polity(&mut w);
    map::lay_out(&mut w);
    people::settle(&mut w);
    map::rebuild_travel(&mut w);
    first_reckoning(&mut w);
    let (souls, homes) = (w.persons.len(), w.households.len());
    let town = w.town_name.clone();
    w.chronicle("people", format!("{town} is founded: {souls} people in {homes} households, under a Sovereign."));
    charter::enact_founding(&mut w);
    crate::stats::refresh(&mut w);
    w
}

/// Add a house to a living town on a free lot fronting a road: stamps the map
/// and creates the place (unowned, in the ward it falls in). None if there is no room.
///
/// The lot is the free one nearest the centre of town, found from the tiles
/// and the footprints of the places already standing. The new house also
/// gets real walking times to everywhere else in `Map::travel`.
pub fn add_home(w: &mut World, name: &str) -> Option<PlaceId> {
    let (cx, cy) = map::centre(w);
    let (lw, lh) = map::size_of(PlaceKind::Home);
    let lot = map::find_lot(w, lw, lh, |x, y| -((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt())?;
    map::stamp_building(&mut w.map, lot.rect);
    let ward = map::ward_at(w, lot.rect.x as f32 + lot.rect.w as f32 / 2.0, lot.rect.y as f32 + lot.rect.h as f32 / 2.0);
    let id = map::push_place(w, PlaceKind::Home, if name.is_empty() { "House" } else { name }, lot.rect, lot.door, ward);
    map::extend_travel(w);
    Some(id)
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() { return 0.0; }
    v.sort_by(f32::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }
}

/// Household wealth and ranks, and the town figures other modules read, as
/// they stand at the founding - so that the founding statutes are enacted
/// into a town that already knows its own numbers. `stats::refresh` keeps
/// them from then on.
fn first_reckoning(w: &mut World) {
    let day = w.day();
    let mut wealth: Vec<f32> = w.households.iter().map(|h| h.money).collect();
    for pl in &w.places {
        if let Owner::Household(h) = pl.owner { if let Some(x) = wealth.get_mut(h.ix()) { *x += pl.value; } }
    }
    let mut order: Vec<usize> = (0..wealth.len()).collect();
    order.sort_by(|a, b| wealth[*b].total_cmp(&wealth[*a]).then(a.cmp(b)));
    let n = order.len();
    for (rank, &h) in order.iter().enumerate() {
        let hh = &mut w.households[h];
        hh.wealth = wealth[h];
        hh.wealth_rank = rank as u32 + 1;
        hh.wealth_pct = if n > 1 { 100.0 * (n - 1 - rank) as f32 / (n - 1) as f32 } else { 50.0 };
    }

    let adults: Vec<&Person> = w.persons.iter().filter(|p| p.present && p.age_on(day) >= ADULT_AGE).collect();
    let workforce: Vec<&&Person> = adults.iter().filter(|p| p.age_on(day) < ELDER_AGE).collect();
    let idle = workforce.iter().filter(|p| p.job.is_none()).count();
    let mut pref_mean = [0.0; N_PREFS];
    for p in &adults { for i in 0..N_PREFS { pref_mean[i] += p.prefs.0[i] / adults.len().max(1) as f32; } }
    let treasury = w.polity.jurisdictions.get(w.polity.town.ix()).map(|j| j.treasury).unwrap_or(0.0);
    let s = Stats {
        day,
        population: w.persons.iter().filter(|p| p.present).count() as u32,
        adults: adults.len() as u32,
        median_wage: median(adults.iter().filter_map(|p| p.job.as_ref()).filter(|j| !j.owner).map(|j| j.wage).collect()),
        median_income: median(adults.iter().map(|p| p.income.avg).collect()),
        median_wealth: median(wealth),
        median_rent: median(w.places.iter().filter(|p| p.tenant.is_some()).map(|p| p.rent).collect()),
        cost_of_living: 5.0,
        unemployment: if workforce.is_empty() { 0.0 } else { idle as f32 / workforce.len() as f32 },
        price: [Good::Food.base_price(), Good::Wood.base_price(), Good::Wares.base_price(), Good::Ale.base_price()],
        drink_price: people::founding_price(FirmKind::Tavern),
        clinic_price: people::founding_price(FirmKind::Clinic),
        school_price: people::founding_price(FirmKind::School),
        treasury,
        trust: if adults.is_empty() { 0.0 } else { adults.iter().map(|p| p.trust).sum::<f32>() / adults.len() as f32 },
        homeless: w.households.iter().filter(|h| h.home.is_none()).map(|h| h.members.len() as u32).sum(),
        money_supply: w.households.iter().map(|h| h.money).sum::<f32>() + w.firms.iter().map(|f| f.cash).sum::<f32>() + w.polity.jurisdictions.iter().map(|j| j.treasury).sum::<f32>(),
        pref_mean,
        ..Stats::default()
    };
    w.stats = s;
}
