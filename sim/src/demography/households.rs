//! Bookkeeping shared by deaths, marriages and migrations: who lives in which
//! household, whose purse is whose, who holds what, who is kin to whom.
use crate::types::*;
use crate::{economy, law};

/// Take a person off their household's roll (their `household` id is kept, as a record).
pub(super) fn remove_member(w: &mut World, p: PersonId) {
    let h = w.persons[p.ix()].household;
    w.households[h.ix()].members.retain(|m| *m != p);
}

/// Move one person to another household. If they were indoors at the old home they wake in the new one.
pub(super) fn move_person(w: &mut World, p: PersonId, to: HouseholdId) {
    let from = w.persons[p.ix()].household;
    if from == to { return; }
    let (old_home, new_home) = (w.households[from.ix()].home, w.households[to.ix()].home);
    w.households[from.ix()].members.retain(|m| *m != p);
    if !w.households[to.ix()].members.contains(&p) { w.households[to.ix()].members.push(p); }
    let person = &mut w.persons[p.ix()];
    person.household = to;
    if let (Some(old), Some(new)) = (old_home, new_home) {
        if person.loc == old && person.travel.is_none() { person.loc = new; }
    }
}

/// Give up a household's home: it stops living there, and any tenancy ends.
pub(super) fn vacate(w: &mut World, h: HouseholdId) {
    if let Some(home) = w.households[h.ix()].home.take() {
        if let Some(pl) = w.places.get_mut(home.ix()) {
            if pl.tenant == Some(h) { pl.tenant = None; }
        }
    }
    w.households[h.ix()].arrears = 0.0;
}

/// Change who owns a place. `economy::set_owner` does the work (the firm in
/// it, its manager, any tenancy); this makes sure the title itself has moved,
/// so nothing is ever left owned by a household that no longer exists.
pub(super) fn transfer_property(w: &mut World, place: PlaceId, to: Owner) {
    let old = w.places[place.ix()].owner;
    if old == to { return; }
    economy::set_owner(w, place, to);
    if w.places[place.ix()].owner != to {
        w.places[place.ix()].owner = to;
        if let Some(f) = w.places[place.ix()].firm {
            if let Some(firm) = w.firms.get_mut(f.ix()) {
                if firm.owner == old {
                    firm.owner = to;
                    if let Owner::State(j) = to { firm.jur = j; }
                }
            }
        }
    }
    // Nobody is their own tenant.
    if let Owner::Household(h) = to {
        if w.places[place.ix()].tenant == Some(h) { w.places[place.ix()].tenant = None; }
    }
}

/// Fold one household into another: its people, its purse and stores, and
/// whatever it owns. The old home is given up. No coin is made or lost.
pub(super) fn merge_households(w: &mut World, from: HouseholdId, into: HouseholdId) {
    if from == into { return; }
    for m in w.households[from.ix()].members.clone() { move_person(w, m, into); }
    let (money, stock) = {
        let h = &mut w.households[from.ix()];
        (std::mem::take(&mut h.money), std::mem::take(&mut h.stock))
    };
    let h = &mut w.households[into.ix()];
    h.money += money;
    for g in 0..N_GOODS { h.stock[g] += stock[g]; }
    for place in w.properties_of(from) { transfer_property(w, place, Owner::Household(into)); }
    vacate(w, from);
}

/// Pay coins into a treasury by way of `law::treasury_collect`, making sure they arrive.
pub(super) fn to_treasury(w: &mut World, jur: JurId, amount: f32) {
    if amount <= 0.0 || jur.ix() >= w.polity.jurisdictions.len() { return; }
    let before = w.polity.jur(jur).treasury;
    law::treasury_collect(w, jur, amount, None);
    let got = w.polity.jur(jur).treasury - before;
    if got < amount - 1e-3 { w.polity.jur_mut(jur).treasury += amount - got.max(0.0); }
}

/// Homes nobody lives in.
pub fn vacant_homes(w: &World) -> Vec<PlaceId> {
    let mut taken = vec![false; w.places.len()];
    for h in &w.households {
        if h.members.is_empty() { continue; }
        if let Some(home) = h.home { if home.ix() < taken.len() { taken[home.ix()] = true; } }
    }
    w.places.iter().filter(|p| p.kind == PlaceKind::Home && !taken[p.id.ix()] && p.tenant.is_none()).map(|p| p.id).collect()
}

/// The cheapest empty home whose rent a household with this income a day could bear.
pub(super) fn affordable_home(w: &World, income: f32) -> Option<PlaceId> {
    let mut best: Option<(f32, PlaceId)> = None;
    for id in vacant_homes(w) {
        let rent = w.place(id).rent.max(0.0);
        if rent > 0.35 * income { continue; }
        if best.is_none_or(|b| rent < b.0) { best = Some((rent, id)); }
    }
    best.map(|b| b.1)
}

/// How well housed a household is: 0 with no home, 1 renting, 2 in a home of its own; then by its purse.
pub(super) fn housing_rank(w: &World, h: HouseholdId) -> (u8, f32) {
    let hh = w.household(h);
    let tier = match hh.home.and_then(|p| w.places.get(p.ix())) {
        None => 0,
        Some(pl) if pl.owner == Owner::Household(h) => 2,
        Some(_) => 1,
    };
    (tier, hh.money)
}

/// How many a home sleeps, from its footprint.
pub(super) fn home_capacity(w: &World, home: PlaceId) -> usize {
    let r = w.place(home).rect;
    ((r.w * r.h) / 2).clamp(4, 9) as usize
}

fn parents(w: &World, p: PersonId) -> [Option<PersonId>; 2] {
    let q = w.person(p);
    [q.mother, q.father]
}

pub(super) fn shares_parent(w: &World, a: PersonId, b: PersonId) -> bool {
    let (pa, pb) = (parents(w, a), parents(w, b));
    pa.iter().flatten().any(|x| pb.iter().flatten().any(|y| x == y))
}

/// Parent and child, siblings (whole or half), grandparent and grandchild, aunt or uncle and niece or nephew.
pub fn close_kin(w: &World, a: PersonId, b: PersonId) -> bool {
    if a == b { return true; }
    let (pa, pb) = (parents(w, a), parents(w, b));
    if pa.contains(&Some(b)) || pb.contains(&Some(a)) { return true; }
    if shares_parent(w, a, b) { return true; }
    // A parent of one is the other's grandparent, or the other's sibling.
    for x in pa.iter().flatten() {
        if parents(w, *x).contains(&Some(b)) || shares_parent(w, *x, b) { return true; }
    }
    for y in pb.iter().flatten() {
        if parents(w, *y).contains(&Some(a)) || shares_parent(w, *y, a) { return true; }
    }
    false
}

/// Parents, children and siblings of `p` who live in town under another roof.
pub(super) fn kin_elsewhere(w: &World, p: PersonId) -> u32 {
    let me = w.person(p);
    let elsewhere = |q: PersonId| w.persons.get(q.ix()).is_some_and(|x| x.present && x.household != me.household);
    let mut kin: Vec<PersonId> = Vec::new();
    for q in [me.mother, me.father].into_iter().flatten() {
        kin.push(q);
        kin.extend(w.person(q).children.iter().copied().filter(|s| *s != p));
    }
    kin.extend(me.children.iter().copied());
    kin.sort();
    kin.dedup();
    kin.into_iter().filter(|q| elsewhere(*q)).count() as u32
}
