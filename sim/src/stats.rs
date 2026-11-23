//! Town-wide aggregates, refreshed nightly.
use crate::types::*;

fn median(v: &mut Vec<f32>) -> f32 {
    if v.is_empty() { return 0.0; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }
}

fn gini(v: &mut Vec<f32>) -> f32 {
    for x in v.iter_mut() { *x = x.max(0.0); }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len() as f32;
    let total: f32 = v.iter().sum();
    if v.is_empty() || total <= 0.0 { return 0.0; }
    let mut acc = 0.0;
    for (i, x) in v.iter().enumerate() { acc += (2.0 * (i as f32 + 1.0) - n - 1.0) * x; }
    acc / (n * total)
}

/// Recompute the parts of `w.stats` other modules read during the day
/// (medians, ranks, head-counts) without recording history.
pub fn refresh(w: &mut World) {
    let day = w.day();
    // Household wealth and ranks.
    let mut value = vec![0.0f32; w.households.len()];
    for pl in &w.places { if let Owner::Household(h) = pl.owner { if h.ix() < value.len() { value[h.ix()] += pl.value; } } }
    let mut live: Vec<(usize, f32)> = Vec::new();
    for h in 0..w.households.len() {
        let wealth = w.households[h].money + value[h];
        w.households[h].wealth = wealth;
        if w.households[h].members.iter().any(|m| w.persons[m.ix()].present) { live.push((h, wealth)); }
    }
    live.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let n = live.len().max(1) as f32;
    for (rank, (h, _)) in live.iter().enumerate() {
        w.households[*h].wealth_rank = rank as u32 + 1;
        w.households[*h].wealth_pct = if live.len() > 1 { 100.0 * (1.0 - rank as f32 / (n - 1.0)) } else { 50.0 };
    }

    let mut wages = Vec::new();
    let mut incomes = Vec::new();
    let mut wealth = Vec::new();
    let (mut pop, mut adults, mut jobless, mut workforce, mut enforcers, mut homeless) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut well, mut trust) = (0.0f32, 0.0f32);
    let mut pref_sum = [0.0f32; N_PREFS];
    for p in w.persons.iter().filter(|p| p.present) {
        pop += 1;
        let age = p.age_on(day);
        if w.households[p.household.ix()].home.is_none() { homeless += 1; }
        if age < ADULT_AGE { continue; }
        adults += 1;
        well += p.wellbeing.total;
        trust += p.trust;
        for k in 0..N_PREFS { pref_sum[k] += p.prefs.0[k]; }
        incomes.push(p.income.avg);
        match &p.job {
            Some(j) => {
                workforce += 1;
                if !j.owner { wages.push(j.wage); }
                if j.role == Role::Enforcer { enforcers += 1; }
            }
            None => if age < 62.0 { workforce += 1; jobless += 1; },
        }
    }
    for (_, v) in &live { let h_adults = 1.0; wealth.push(*v / h_adults); }
    let rents: Vec<f32> = w.places.iter().filter(|p| p.kind == PlaceKind::Home && p.tenant.is_some()).map(|p| p.rent).collect();
    let a = adults.max(1) as f32;
    let s = &mut w.stats;
    s.day = day;
    s.population = pop;
    s.adults = adults;
    s.homeless = homeless;
    s.enforcers = enforcers;
    s.mean_wellbeing = well / a;
    s.trust = trust / a;
    for k in 0..N_PREFS { s.pref_mean[k] = pref_sum[k] / a; }
    if !wages.is_empty() { s.median_wage = median(&mut wages); }
    if !incomes.is_empty() { s.median_income = median(&mut incomes); }
    s.gini = gini(&mut wealth.clone());
    if !wealth.is_empty() { s.median_wealth = median(&mut wealth); }
    if !rents.is_empty() { s.median_rent = median(&mut rents.clone()); }
    s.unemployment = if workforce > 0 { jobless as f32 / workforce as f32 } else { 0.0 };
    if s.price.iter().all(|p| *p == 0.0) { for g in ALL_GOODS { s.price[g as usize] = g.base_price(); } }
    s.cost_of_living = s.price[Good::Food as usize] * 1.4 + s.median_rent / 2.0 + s.price[Good::Wares as usize] * 0.12 + s.price[Good::Wood as usize] * 0.05;
    let town = w.polity.town;
    if let Some(j) = w.polity.jurisdictions.get(town.ix()) { s.treasury = j.treasury; }
    s.money_supply = w.households.iter().map(|h| h.money).sum::<f32>() + w.firms.iter().map(|f| f.cash).sum::<f32>() + w.polity.jurisdictions.iter().map(|j| j.treasury).sum::<f32>();
}

/// Recompute `w.stats` and append to `w.history`. Per-day counters that other
/// modules add to (`offences`, `births`...) are recorded and then reset.
pub fn end_of_day(w: &mut World) {
    refresh(w);
    w.stats.revenue = w.polity.jurisdictions.iter().map(|j| j.revenue_today).sum();
    w.stats.spending = w.polity.jurisdictions.iter().map(|j| j.spending_today).sum();
    w.stats.births = w.demo.births_today;
    w.stats.deaths = w.demo.deaths_today;
    w.stats.arrivals = w.demo.arrivals_today;
    w.stats.departures = w.demo.departures_today;
    w.stats.protesters = w.persons.iter().filter(|p| p.present && p.civic.protesting).count() as u32;
    if !w.is_fork() {
        let snap = w.stats.clone();
        w.history.push(snap);
        if w.history.len() > 1500 { w.history.remove(0); }
    }
    let s = &mut w.stats;
    s.offences = 0; s.punished = 0; s.bribes = 0; s.thefts = 0; s.births = 0; s.deaths = 0; s.arrivals = 0; s.departures = 0;
    for j in w.polity.jurisdictions.iter_mut() { j.revenue_today = 0.0; j.spending_today = 0.0; j.shortfall_today = 0.0; }
}
