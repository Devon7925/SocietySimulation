//! Leaving town for good, and arriving from elsewhere.
use super::households::*;
use super::{death, names, people};
use crate::statute::{Act, Action, EventKind};
use crate::types::*;
use crate::{decide, economy, law, life};

/// What a day's work pays in the world beyond the gates.
pub const OUTSIDE_WAGE: f32 = 8.0;
/// Wellbeing below which a household's adults count as faring very badly.
pub const MISERY: f32 = -0.3;
/// Days of that before they think of leaving.
pub const MISERY_DAYS: u16 = 10;
/// Outsiders look the town over this often.
pub const ARRIVAL_EVERY: i32 = 3;

fn valid_gates(w: &World) -> Vec<PlaceId> {
    w.map.gates.iter().copied().filter(|g| g.ix() < w.places.len()).collect()
}

fn nearest_gate(w: &World, from: PlaceId) -> Option<PlaceId> {
    valid_gates(w).into_iter().min_by_key(|g| w.travel_slots(from, *g))
}

/// What pushes a household out and what holds it, in coins: (push, hold).
/// The push is how far below an ordinary life its adults are faring, valued
/// over a season. What holds them is kin and friends under other roofs, their
/// attachment to things as they are, and property they could only sell at a loss.
fn push_and_hold(w: &World, h: HouseholdId, adults: &[PersonId], mean_wellbeing: f32) -> (f32, f32) {
    let n = adults.len().max(1) as f32;
    let (mut push, mut hold) = (0.0f32, 0.0f32);
    for &a in adults {
        let p = w.person(a);
        let daily = decide::daily_scale(w, a);
        push += (0.0 - mean_wellbeing).max(0.0) * daily * 40.0;
        let friends: f32 = p.friends.iter().filter(|f| w.persons.get(f.id.ix()).is_some_and(|q| q.present && q.household != h)).map(|f| f.affinity).sum();
        hold += daily * (4.0 * kin_elsewhere(w, a) as f32 * p.prefs[Pref::Family]
            + 2.0 * friends * (0.3 + 0.7 * p.prefs[Pref::Sociability])
            + 6.0 * p.prefs[Pref::Continuity]);
    }
    let property: f32 = w.properties_of(h).iter().map(|pl| w.place(*pl).value).sum();
    (push / n, hold / n + property * (1.0 - death::HURRIED_SALE))
}

/// Each morning: households whose adults have been faring very badly for a
/// while, and whom little holds, decide to go - law permitting. Those who go
/// walk to a gate; `life` calls `depart` for each as they pass through it.
pub(super) fn emigration(w: &mut World) {
    let day = w.day();
    // Anyone who set out days ago and is still here has surely reached the road by now.
    let stale: Vec<HouseholdId> = w.demo.leaving.iter().filter(|l| day - l.since >= 2).map(|l| l.household).collect();
    for h in stale {
        for m in w.members(h) {
            if w.person(m).jailed_until.is_none() { death::depart(w, m, "emigrated"); }
        }
    }
    {
        let households = &w.households;
        w.demo.leaving.retain(|l| households.get(l.household.ix()).is_some_and(|h| !h.members.is_empty()));
    }
    if w.demo.low_days.len() < w.households.len() { w.demo.low_days.resize(w.households.len(), 0); }

    for i in 0..w.households.len() {
        let h = HouseholdId(i as u32);
        if w.households[i].members.is_empty() || w.demo.is_leaving(h) { continue; }
        let adults = w.adults_of(h);
        if adults.is_empty() { continue; }
        let mean = adults.iter().map(|a| w.person(*a).wellbeing.total).sum::<f32>() / adults.len() as f32;
        let low = &mut w.demo.low_days[i];
        *low = if mean < MISERY { (*low + 1).min(10_000) } else { low.saturating_sub(2) };
        let low = *low;

        // The player decides for the person they are living as.
        let played = w.polity.player.possessing.filter(|p| adults.contains(p));
        let wish = if played.is_some() { w.polity.player.overrides.emigrate } else { None };
        if wish == Some(false) { continue; }
        let forced = wish == Some(true);
        if !forced && low < MISERY_DAYS { continue; }
        if adults.iter().any(|a| w.person(*a).jailed_until.is_some()) { continue; }
        let (push, hold) = push_and_hold(w, h, &adults, mean);
        if !forced && (push <= hold || !w.rng.chance(0.25)) { continue; }
        let Some(head) = played.or_else(|| w.head_of(h)) else { continue };
        let from = w.households[i].home.unwrap_or(w.person(head).loc);
        let Some(gate) = nearest_gate(w, from) else { continue };
        if forced { w.polity.player.overrides.emigrate = None; }

        let value = (push - hold).max(decide::daily_scale(w, head));
        if !law::attempt(w, head, &Act::new(Action::Emigrate, gate, value)).proceeds() {
            // Kept in: they will think of it again before long.
            w.demo.low_days[i] = MISERY_DAYS / 2;
            continue;
        }
        w.demo.leaving.push(super::Leaving { household: h, since: day });
        for m in w.members(h) {
            if w.person(m).jailed_until.is_some() { continue; }
            if w.is_adult(m) { w.note(m, "hardship", "There is nothing left for us here. We are leaving town."); }
            life::go(w, m, gate, Plan { activity: Activity::Leaving, slots: 1, doing: "Leaving town".to_string(), arg: 0 });
        }
    }
}

/// How the town looks from outside, against the life a newcomer already has:
/// the going wage against the wage elsewhere, whether there is work, whether
/// there is an empty home, and how the people there are faring. Above zero,
/// it looks better than staying away.
pub fn town_appeal(w: &World) -> f32 {
    let day = w.day();
    let wage = (w.stats.median_wage - OUTSIDE_WAGE) / OUTSIDE_WAGE;
    let (mut hands, mut idle) = (0.0f32, 0.0f32);
    for p in &w.persons {
        if !p.present { continue; }
        let age = p.age_on(day);
        if !(ADULT_AGE..ELDER_AGE).contains(&age) { continue; }
        hands += 1.0;
        if p.job.is_none() { idle += 1.0; }
    }
    let idle_share = if hands > 0.0 { idle / hands } else { 0.0 };
    let openings: u32 = w.firms.iter().filter(|f| f.open).map(|f| f.vacancies).sum();
    let work = if openings > 0 { 0.2 } else { -0.2 } - 2.0 * (idle_share - 0.05).max(0.0);
    let homes = if vacant_homes(w).is_empty() { -0.4 } else { 0.2 };
    wage + work + homes + w.stats.mean_wellbeing.clamp(-1.0, 1.0)
}

/// Every few days, if the town looks better than the world outside, a
/// household may turn up at a gate hoping to settle.
pub(super) fn immigration(w: &mut World) {
    if !w.settings.immigration || w.day() % ARRIVAL_EVERY != 0 { return; }
    let gates = valid_gates(w);
    if gates.is_empty() { return; }
    let appeal = town_appeal(w);
    if appeal <= 0.0 { return; }
    if !w.rng.chance((0.05 + 0.4 * appeal).min(0.6)) { return; }
    let gate = gates[w.rng.index(gates.len())];
    arrive(w, gate, appeal);
}

/// A household of strangers reaches `gate`: a single person, a couple, a
/// family or a lone parent, with what coin they carry. Returns the household
/// if they were let in (or got in regardless).
pub(super) fn arrive(w: &mut World, gate: PlaceId, appeal: f32) -> Option<HouseholdId> {
    let surname = names::surname(w);
    let h = people::new_household(w, &surname);
    let kind = w.rng.f32();
    let mut adults: Vec<PersonId> = Vec::new();
    let mut kids = 0;
    if kind < 0.40 {
        let age = w.rng.range(18.0, 40.0);
        adults.push(people::new_person(w, h, None, age, &surname));
    } else {
        let lone = kind >= 0.95;
        let age = w.rng.range(20.0, 42.0);
        let (her, his) = (age + w.rng.range(-3.0, 3.0), age + w.rng.range(-3.0, 4.0));
        if lone {
            adults.push(people::new_person(w, h, None, age.max(19.0), &surname));
        } else {
            let a = people::new_person(w, h, Some(Sex::Female), her.max(17.0), &surname);
            let b = people::new_person(w, h, Some(Sex::Male), his.max(17.0), &surname);
            w.persons[a.ix()].spouse = Some(b);
            w.persons[b.ix()].spouse = Some(a);
            adults.push(a);
            adults.push(b);
        }
        if kind >= 0.65 { kids = if lone { w.rng.int(1, 2) } else { w.rng.int(1, 3) }; }
    }
    let day = w.day();
    let youngest = adults.iter().map(|a| w.person(*a).age_on(day)).fold(f32::MAX, f32::min);
    let mother = adults.iter().copied().find(|a| w.person(*a).sex == Sex::Female);
    let father = adults.iter().copied().find(|a| w.person(*a).sex == Sex::Male);
    let mut members = adults.clone();
    for _ in 0..kids {
        let oldest = (youngest - 18.0).min(15.5);
        if oldest < 0.5 { break; }
        let age = w.rng.range(0.5, oldest);
        let c = people::new_person(w, h, None, age, &surname);
        // A child of this couple, not a stranger to them.
        let (genes, prefs) = match (mother, father) {
            (Some(m), f) => people::inherit(w, m, f),
            (None, Some(f)) => people::inherit(w, f, None),
            (None, None) => (w.person(c).genes, w.person(c).prefs),
        };
        let child = &mut w.persons[c.ix()];
        child.genes = genes;
        child.prefs = prefs;
        child.mother = mother;
        child.father = father;
        for parent in &adults { w.persons[parent.ix()].children.push(c); }
        members.push(c);
    }
    for m in &members {
        let p = &mut w.persons[m.ix()];
        p.immigrant = true;
        p.loc = gate;
    }
    // What they carry is coin coming into the town from outside.
    let purse = w.rng.range(20.0, 70.0) * adults.len() as f32;
    w.households[h.ix()].money = purse;
    w.households[h.ix()].stock[Good::Food as usize] = 2.0 * members.len() as f32;
    w.demo.coin_in += purse;

    let head = adults[0];
    let value = w.stats.median_wage.max(4.0) * 30.0 * appeal.max(0.1);
    if !law::attempt(w, head, &Act::new(Action::Immigrate, gate, value)).proceeds() {
        // Turned back, or unwilling to enter against the law: they go on down
        // the road with whatever they still hold, and were never of the town.
        for m in &members { w.persons[m.ix()].present = false; }
        let hh = &mut w.households[h.ix()];
        hh.members.clear();
        w.demo.coin_out += std::mem::take(&mut hh.money).max(0.0);
        hh.stock = [0.0; N_GOODS];
        return None;
    }
    let expected = w.stats.median_wage.max(4.0) * adults.len() as f32;
    if let Some(home) = affordable_home(w, expected) { economy::move_in(w, h, home); }
    w.demo.arrivals += members.len() as u32;
    w.demo.arrivals_today += members.len() as u32;
    let town = if w.town_name.is_empty() { "town".to_string() } else { w.town_name.clone() };
    let gate_name = w.place(gate).name.clone();
    for a in &adults { w.note(*a, "home", format!("Arrived in {town} hoping to make a life here.")); }
    let text = if members.len() > 1 { format!("The {surname} household arrived at the {gate_name}, hoping to settle.") }
        else { format!("{} arrived at the {gate_name}, hoping to settle.", w.person(head).full_name()) };
    w.chronicle_about("people", text, members.clone(), None);
    for m in &members { law::emit(w, EventKind::Immigration, *m); }
    Some(h)
}
