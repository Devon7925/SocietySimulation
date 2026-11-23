//! Births, courtship and marriage, coming of age, leaving home.
use super::households::*;
use super::{names, people};
use crate::statute::{Act, Action, EventKind};
use crate::types::*;
use crate::{decide, economy, law};

/// A woman can bear children from this age until `MOTHER_MAX_AGE`.
pub const MOTHER_MIN_AGE: f32 = 17.0;
pub const MOTHER_MAX_AGE: f32 = 43.0;
/// Friends court only once each holds the other at least this dear.
pub const COURT_AFFINITY: f32 = 0.7;
/// Chance a day that a pair who would both gain by marrying get round to it.
pub const PROPOSAL_CHANCE: f32 = 0.08;

fn can_act(w: &World, p: &Person) -> bool {
    p.present && p.jailed_until.is_none() && !w.demo.is_leaving(p.household)
}

// ------------------------------------------------------------------ births

/// What one more child is worth to this couple, in days of their income, or
/// None if it is not worth having: they do not care enough for family given
/// the children they already have, there is no room, or they could not feed it.
///
/// The wish falls with each child already born, so those who value family
/// little stop at one or none, most at two or three, and those who value it
/// greatly go on to four or five. The keep of a child (about half an adult's) must come out of what
/// the household earns beyond its present needs, or out of savings that would
/// cover the shortfall for two years.
pub fn child_worth(w: &World, mother: PersonId, father: PersonId) -> Option<f32> {
    let day = w.day();
    let (m, f) = (w.person(mother), w.person(father));
    let living: Vec<&Person> = m.children.iter().filter_map(|c| w.persons.get(c.ix())).filter(|c| c.alive).collect();
    if living.iter().any(|c| c.age_on(day) < 1.0) { return None; }
    let n = living.len().max(f.children.iter().filter(|c| w.persons.get(c.ix()).is_some_and(|c| c.alive)).count()) as f32;
    let family = 0.5 * (m.prefs[Pref::Family] + f.prefs[Pref::Family]);
    let wish = family * 1.7 / (1.0 + 0.75 * n) - 0.3;
    if wish <= 0.0 { return None; }

    let hh = w.household(m.household);
    let home = hh.home.filter(|h| h.ix() < w.places.len())?;
    let (mut adults, mut young, mut income) = (0.0f32, 0.0f32, 0.0f32);
    for id in &hh.members {
        let q = &w.persons[id.ix()];
        if !q.present { continue; }
        if q.age_on(day) >= ADULT_AGE { adults += 1.0 } else { young += 1.0 }
        income += q.income.avg;
    }
    if (adults + young) as usize >= home_capacity(w, home) { return None; }

    let cost = w.stats.cost_of_living.max(1.0);
    let rent = if w.place(home).tenant == Some(hh.id) { w.place(home).rent } else { 0.0 };
    let spare = income - cost * (adults + 0.5 * young) - rent;
    let keep = 0.5 * cost;
    let shortfall = (keep - spare).max(0.0);
    if shortfall * 2.0 * DAYS_PER_YEAR as f32 > hh.money.max(0.0) { return None; }
    Some(30.0 * wish * (1.0 - 0.5 * shortfall / keep))
}

/// Each morning, each couple for whom another child is worth it has a small
/// chance of one - if the law allows, or they are willing to defy it.
pub(super) fn births(w: &mut World) {
    let day = w.day();
    let mut couples: Vec<(PersonId, PersonId)> = Vec::new();
    for m in &w.persons {
        if m.sex != Sex::Female || m.sterile || !can_act(w, m) { continue; }
        let age = m.age_on(day);
        if !(MOTHER_MIN_AGE..MOTHER_MAX_AGE).contains(&age) { continue; }
        let Some(f) = m.spouse.and_then(|s| w.persons.get(s.ix())) else { continue };
        if f.sex != Sex::Male || f.sterile || !can_act(w, f) || f.household != m.household || f.age_on(day) < ADULT_AGE { continue; }
        couples.push((m.id, f.id));
    }
    for (m, f) in couples {
        let Some(worth) = child_worth(w, m, f) else { continue };
        if !w.rng.chance((0.006 + 0.0016 * worth).min(0.05)) { continue; }
        let value = worth * 0.5 * (decide::daily_scale(w, m) + decide::daily_scale(w, f));
        let place = w.home_of(m).unwrap_or(w.person(m).loc);
        let mut act = Act::new(Action::HaveChild, place, value);
        act.other = Some(f);
        if !law::attempt(w, m, &act).proceeds() { continue; }
        birth(w, m, Some(f));
    }
}

/// A child is born to `mother` (and `father`, if known), into her household.
pub fn birth(w: &mut World, mother: PersonId, father: Option<PersonId>) -> PersonId {
    let (genes, prefs) = people::inherit(w, mother, father);
    let h = w.person(mother).household;
    let surname = if w.household(h).name.is_empty() { w.person(mother).surname.clone() } else { w.household(h).name.clone() };
    let sex = if w.rng.chance(0.5) { Sex::Female } else { Sex::Male };
    let name = names::given(w, sex, &surname);
    let loc = w.household(h).home.unwrap_or(w.person(mother).loc);
    let id = PersonId(w.persons.len() as u32);
    let mut c = Person::new(id, &name, &surname, sex, w.day(), h, loc);
    c.genes = genes;
    c.prefs = prefs;
    c.needs = Needs { hunger: 20.0, energy: 90.0, social: 80.0, fun: 70.0, health: w.rng.gauss(88.0, 5.0).clamp(60.0, 100.0) };
    c.mother = Some(mother);
    c.father = father;
    c.until = w.slot;
    w.persons.push(c);
    w.households[h.ix()].members.push(id);
    w.persons[mother.ix()].children.push(id);
    if let Some(f) = father { w.persons[f.ix()].children.push(id); }
    w.demo.births += 1;
    w.demo.births_today += 1;

    let child = if sex == Sex::Female { "daughter" } else { "son" };
    let mother_name = w.person(mother).full_name();
    match father {
        Some(f) => {
            w.note(mother, "family", format!("Our {child} {name} was born today."));
            w.note(f, "family", format!("Our {child} {name} was born today."));
            let father_name = w.person(f).full_name();
            w.chronicle_about("people", format!("A {child}, {name}, was born to {mother_name} and {father_name}."), vec![id, mother, f], None);
        }
        None => {
            w.note(mother, "family", format!("My {child} {name} was born today."));
            w.chronicle_about("people", format!("A {child}, {name}, was born to {mother_name}."), vec![id, mother], None);
        }
    }
    law::emit(w, EventKind::Birth, id);
    id
}

// ---------------------------------------------------------------- marriage

/// What marrying `other` is worth to `me`, in coins: the company of someone
/// dear to them, against the freedom given up, a gap in years, what pooling
/// purses does to their share, and the keep of any children the other brings.
pub fn marriage_gain(w: &World, me: PersonId, other: PersonId) -> f32 {
    let day = w.day();
    let (p, o) = (w.person(me), w.person(other));
    let daily = decide::daily_scale(w, me);
    let company = 20.0 * p.affinity(other) * (0.3 + 0.7 * p.prefs[Pref::Family]);
    let freedom = 6.0 * p.prefs[Pref::Autonomy];
    let mismatch = 0.5 * ((p.age_on(day) - o.age_on(day)).abs() - 8.0).max(0.0);
    let per_adult = |h: HouseholdId| w.household(h).money / w.adults_of(h).len().max(1) as f32;
    let (mine, theirs) = (per_adult(p.household), per_adult(o.household));
    let purse = if p.household == o.household { 0.0 } else { 0.5 * (theirs - mine) * 0.5 * (0.5 + p.prefs[Pref::Wealth]) };
    let brought = o.children.iter().filter(|c| w.persons.get(c.ix()).is_some_and(|c| c.present && c.household == o.household && c.age_on(day) < ADULT_AGE && !p.children.contains(&c.id))).count() as f32;
    let keep = brought * 0.5 * w.stats.cost_of_living.max(1.0) * DAYS_PER_YEAR as f32 * (1.0 - p.prefs[Pref::Family]);
    daily * (company - freedom - mismatch) + purse - keep
}

fn free_to_marry(w: &World, p: &Person, day: i32) -> bool {
    can_act(w, p) && p.spouse.is_none() && p.age_on(day) >= ADULT_AGE
}

/// Each morning: unmarried adults who are dear friends, not close kin, and
/// would each be better off married may marry, law permitting.
pub(super) fn courtship(w: &mut World) {
    let day = w.day();
    let mut pairs: Vec<(PersonId, PersonId)> = Vec::new();
    let mut spoken_for: Vec<PersonId> = Vec::new();
    for a in &w.persons {
        if !free_to_marry(w, a, day) || spoken_for.contains(&a.id) { continue; }
        // The dearest friend who feels the same.
        let mut best: Option<(f32, PersonId)> = None;
        for f in &a.friends {
            if f.id <= a.id || f.affinity < COURT_AFFINITY || spoken_for.contains(&f.id) { continue; }
            let Some(b) = w.persons.get(f.id.ix()) else { continue };
            if !free_to_marry(w, b, day) || b.sex == a.sex || close_kin(w, a.id, b.id) { continue; }
            let mutual = f.affinity.min(b.affinity(a.id));
            if mutual < COURT_AFFINITY { continue; }
            if best.is_none_or(|x| mutual > x.0) { best = Some((mutual, b.id)); }
        }
        if let Some((_, b)) = best {
            pairs.push((a.id, b));
            spoken_for.push(a.id);
            spoken_for.push(b);
        }
    }
    for (a, b) in pairs {
        let (gain_a, gain_b) = (marriage_gain(w, a, b), marriage_gain(w, b, a));
        if gain_a <= 0.0 || gain_b <= 0.0 { continue; }
        if !w.rng.chance(PROPOSAL_CHANCE) { continue; }
        let mut act = Act::new(Action::Marry, w.person(a).loc, gain_a);
        act.other = Some(b);
        if !law::attempt(w, a, &act).proceeds() { continue; }
        let mut act = Act::new(Action::Marry, w.person(b).loc, gain_b);
        act.other = Some(a);
        if !law::attempt(w, b, &act).proceeds() { continue; }
        marry(w, a, b);
    }
}

/// `mover` goes to live in household `into`. If they leave other adults
/// behind they take their share of the purse and their own children; if they
/// were the only adult, their whole household comes with them.
fn join_household(w: &mut World, mover: PersonId, into: HouseholdId) {
    let day = w.day();
    let from = w.person(mover).household;
    if from == into { return; }
    let adults = w.adults_of(from);
    if adults.iter().all(|a| *a == mover) {
        merge_households(w, from, into);
        return;
    }
    let share = w.households[from.ix()].money.max(0.0) / adults.len().max(1) as f32;
    w.households[from.ix()].money -= share;
    w.households[into.ix()].money += share;
    let kids: Vec<PersonId> = w.person(mover).children.iter().copied().filter(|c| {
        let c = w.person(*c);
        let other_parent_stays = [c.mother, c.father].into_iter().flatten().any(|q| q != mover && w.person(q).present && w.person(q).household == from);
        c.present && c.household == from && c.age_on(day) < ADULT_AGE && !other_parent_stays
    }).collect();
    move_person(w, mover, into);
    for k in kids { move_person(w, k, into); }
}

/// Two people marry. The worse-housed of them moves in with the other, and their purses become one.
pub(super) fn marry(w: &mut World, a: PersonId, b: PersonId) {
    w.persons[a.ix()].spouse = Some(b);
    w.persons[b.ix()].spouse = Some(a);
    let (ha, hb) = (w.person(a).household, w.person(b).household);
    if ha != hb {
        let (stays, mover) = if housing_rank(w, hb) > housing_rank(w, ha) { (b, a) } else { (a, b) };
        let into = w.person(stays).household;
        join_household(w, mover, into);
    }
    let (name_a, name_b) = (w.person(a).full_name(), w.person(b).full_name());
    w.note(a, "family", format!("{name_b} and I were married today."));
    w.note(b, "family", format!("{name_a} and I were married today."));
    w.chronicle_about("people", format!("{name_a} and {name_b} were married."), vec![a, b], None);
    law::emit(w, EventKind::Marriage, a);
    law::emit(w, EventKind::Marriage, b);
}

// ------------------------------------------------------------ growing up

/// Sixteenth birthdays.
pub(super) fn coming_of_age(w: &mut World) {
    let day = w.day();
    let sixteen = ADULT_AGE as i32 * DAYS_PER_YEAR as i32;
    let grown: Vec<PersonId> = w.persons.iter().filter(|p| p.present && day - p.birth_day == sixteen).map(|p| p.id).collect();
    for p in grown {
        w.note(p, "family", "I came of age today.");
        law::emit(w, EventKind::ComingOfAge, p);
    }
}

/// Grown children still under a parent's roof move out once they earn enough
/// to keep themselves (and a spouse and children, if they have them) and
/// there is an empty home they can afford. How soon depends on how much they
/// prize their independence over their family.
pub(super) fn leaving_home(w: &mut World) {
    let day = w.day();
    let cost = w.stats.cost_of_living.max(1.0);
    let mut movers: Vec<(Vec<PersonId>, f32, f32)> = Vec::new();
    let mut counted: Vec<PersonId> = Vec::new();
    for p in &w.persons {
        if !can_act(w, p) || p.age_on(day) < ADULT_AGE || counted.contains(&p.id) { continue; }
        let with_parent = [p.mother, p.father].into_iter().flatten().any(|q| w.persons.get(q.ix()).is_some_and(|q| q.present && q.household == p.household));
        if !with_parent || p.job.is_none() { continue; }
        // Those who would go together: a spouse under the same roof, and their young children.
        let mut unit = vec![p.id];
        if let Some(s) = p.spouse { if w.person(s).present && w.person(s).household == p.household { unit.push(s); } }
        let grown = unit.len();
        for i in 0..grown {
            for c in &w.person(unit[i]).children {
                let child = w.person(*c);
                if child.present && child.household == p.household && child.age_on(day) < ADULT_AGE && !unit.contains(c) { unit.push(*c); }
            }
        }
        let income: f32 = unit.iter().map(|q| w.person(*q).income.avg).sum();
        let needs = cost * (grown as f32 + 0.5 * (unit.len() - grown) as f32);
        if income < 1.1 * needs { continue; }
        let urge = 0.02 + 0.06 * p.prefs[Pref::Autonomy] * (1.2 - p.prefs[Pref::Family]);
        counted.extend(unit.iter().copied());
        movers.push((unit, income, urge));
    }
    for (unit, income, urge) in movers {
        if !w.rng.chance(urge) { continue; }
        let Some(home) = affordable_home(w, income) else { continue };
        let first = unit[0];
        let old = w.person(first).household;
        let grown = unit.iter().filter(|q| w.person(**q).age_on(day) >= ADULT_AGE).count() as f32;
        let adults_before = w.adults_of(old).len().max(1) as f32;
        let all_before = w.members(old).len().max(1) as f32;
        let surname = w.person(first).surname.clone();
        let new = people::new_household(w, &surname);
        // They take half an equal share of the purse for each adult going, and some of the larder.
        let share = 0.5 * w.households[old.ix()].money.max(0.0) * grown / adults_before;
        let food = 0.5 * w.households[old.ix()].stock[Good::Food as usize] * unit.len() as f32 / all_before;
        w.households[old.ix()].money -= share;
        w.households[old.ix()].stock[Good::Food as usize] -= food;
        w.households[new.ix()].money += share;
        w.households[new.ix()].stock[Good::Food as usize] += food;
        for q in &unit { move_person(w, *q, new); }
        economy::move_in(w, new, home);
        let place = w.place(home).name.clone();
        w.note(first, "home", format!("Moved out of my parents' house, into {place}."));
        for parent in w.adults_of(old) {
            let name = w.person(first).name.clone();
            w.note(parent, "family", format!("{name} has moved out to a home of their own."));
        }
    }
}
