//! The few places enforcement moves coins. Nothing here creates or destroys
//! money: fines and forfeits go from a household to a treasury, bounties and
//! rewards from a treasury to a household, bribes from one household to another.
//!
//! The treasury and income bookkeeping belong to `law`. While that module is a
//! stub its calls do nothing, so under `cfg(test)` each helper finishes the
//! movement itself when it sees that `law` left the balance untouched. Once
//! `law` is real the test-only branches never fire.
use crate::law;
use crate::types::*;

fn sane(x: f32) -> f32 { if x.is_finite() { x.max(0.0) } else { 0.0 } }

/// A jurisdiction id that is safe to index with (the town if `jur` is unknown).
pub(super) fn valid_jur(w: &World, jur: JurId) -> Option<JurId> {
    if jur.ix() < w.polity.jurisdictions.len() { Some(jur) } else if w.polity.town.ix() < w.polity.jurisdictions.len() { Some(w.polity.town) } else { None }
}

/// Take up to `amount` from a household's purse (never below zero). Returns what was taken.
pub(super) fn take(w: &mut World, h: HouseholdId, amount: f32) -> f32 {
    let hh = &mut w.households[h.ix()];
    let got = sane(amount).min(hh.money.max(0.0));
    hh.money -= got;
    got
}

/// Coins already taken from someone go into a treasury.
pub(super) fn collect(w: &mut World, jur: JurId, amount: f32, sid: Option<StatuteId>) {
    let amount = sane(amount);
    let Some(jur) = valid_jur(w, jur) else { return };
    if amount <= 0.0 { return; }
    #[cfg(test)]
    let before = w.polity.jur(jur).treasury;
    law::treasury_collect(w, jur, amount, sid);
    #[cfg(test)]
    if w.polity.jur(jur).treasury == before { w.polity.jur_mut(jur).treasury += amount; }
}

/// Draw from a treasury (never below zero). Returns what it could pay; the caller hands that on.
pub(super) fn pay_out(w: &mut World, jur: JurId, amount: f32, sid: Option<StatuteId>) -> f32 {
    let amount = sane(amount);
    let Some(jur) = valid_jur(w, jur) else { return 0.0 };
    if amount <= 0.0 { return 0.0; }
    #[cfg(test)]
    let before = w.polity.jur(jur).treasury;
    #[allow(unused_mut)]
    let mut paid = law::treasury_pay(w, jur, amount, sid);
    #[cfg(test)]
    if paid == 0.0 && w.polity.jur(jur).treasury == before && before > 0.0 {
        paid = amount.min(before);
        w.polity.jur_mut(jur).treasury -= paid;
    }
    sane(paid)
}

/// Hand a person coins as income (taxed like any other, by `law`).
pub(super) fn credit(w: &mut World, p: PersonId, amount: f32, kind: IncomeKind) {
    let amount = sane(amount);
    if amount <= 0.0 { return; }
    #[cfg(test)]
    let before = w.hh_of(p).money;
    law::pay_income(w, p, amount, kind);
    #[cfg(test)]
    if w.hh_of(p).money == before {
        w.hh_of_mut(p).money += amount;
        w.person_mut(p).income.other += amount;
    }
}
