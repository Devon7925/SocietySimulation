//! Carrying out sanctions: fines, forfeits, jail, exile, execution, marks of
//! status, sterilisation - and what they do to the punished and to those who
//! care for them. Also the keeping and release of prisoners.
use super::{money, units};
use crate::statute::*;
use crate::types::*;
use crate::{decide, demography, economy, law, life};

fn span(days: f32) -> String {
    let whole = days.round();
    if (days - whole).abs() < 0.05 { if whole == 1.0 { "1 day".to_string() } else { format!("{whole:.0} days") } } else { format!("{days:.1} days") }
}

fn list(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// Carry out a sanction on `p`. `stake` is the value of goods involved (taken
/// if the sanction confiscates); `baseless` marks a charge with nothing behind
/// it, which the accused and those close to them know. Returns the fine paid.
///
/// Coins: the fine and any forfeit go from the household purse to the
/// treasury of the statute's jurisdiction. Without the goods themselves to
/// hand, a forfeit is taken as their value in coin.
pub(super) fn carry_out(w: &mut World, p: PersonId, sid: StatuteId, s: &Sanction, fine: f32, reason: &str, by: Option<PersonId>, stake: f32, baseless: bool) -> f32 {
    if p.ix() >= w.persons.len() || !w.person(p).present { return 0.0; }
    let (title, st_jur) = match w.statute(sid) { Some(st) => (st.title.clone(), st.jur), None => (String::new(), w.polity.town) };
    let under = if title.is_empty() { String::new() } else { format!(" under \"{title}\"") };
    let jur = money::valid_jur(w, st_jur);
    let daily = decide::daily_scale(w, p);
    let hid = w.person(p).household;
    let name = w.person(p).full_name();
    let day = w.day();

    // The fine. One person answers with their share of the household purse; a
    // collective fine falls on all of it. What cannot be paid is served in jail.
    let fine = if fine.is_finite() { fine.max(0.0) } else { 0.0 };
    let adults = w.adults_of(hid).len().max(1) as f32;
    let purse = w.household(hid).money.max(0.0);
    let within_reach = if s.collective { purse } else { purse / adults };
    let (mut paid, mut seized) = (0.0, 0.0);
    if let Some(j) = jur {
        paid = money::take(w, hid, fine.min(within_reach));
        if s.confiscate { seized = money::take(w, hid, stake); }
        money::collect(w, j, paid + seized, Some(sid));
    }
    let unpaid = fine - paid;
    let mut jail = if s.jail_days.is_finite() { s.jail_days.max(0.0) } else { 0.0 };
    if unpaid > 0.5 { jail += (unpaid / daily).ceil().min(14.0); }
    let jail = jail.min(60.0);
    let leaves = s.exile || s.execute;

    // What it weighs, in coins: money lost, days lost, and the rest as the person would count it.
    let rest = (decide::sanction_cost(w, p, s, 0.0) - s.jail_days.max(0.0) * daily * 1.5).max(0.0);
    let weight = paid + seized + if leaves { 0.0 } else { jail * daily } + rest;

    if !s.add_status.trim().is_empty() {
        if let Some(ix) = w.status_ix_or_add(s.add_status.trim()) { w.person_mut(p).set_status(ix, true); }
    }
    if !s.remove_status.trim().is_empty() {
        if let Some(ix) = w.status_ix(s.remove_status.trim()) { w.person_mut(p).set_status(ix, false); }
    }
    let wronged = if baseless { 1.0 } else { (-decide::opinion(w, p, sid)).max(0.0) };
    {
        let q = w.person_mut(p);
        if s.sterilize { q.sterile = true; }
        q.convictions += 1;
        q.felt.punished += weight;
        // Punishment under a rule they reject, or for nothing, costs the government their trust.
        q.trust = (q.trust - 0.08 * wronged * (weight / daily / 3.0).clamp(0.3, 2.0)).max(0.0);
        q.legal.last_charge = Some((sid, day));
    }
    if let Some(st) = w.statute_mut(sid) {
        st.stats.punished += 1;
        st.stats.fines += paid + seized;
    }
    w.enforce.today.punished += 1;
    w.enforce.today.fines += paid + seized;

    let mut parts: Vec<String> = Vec::new();
    if paid > 0.0 { parts.push(format!("fined {paid:.1} coins")); }
    if seized > 0.0 { parts.push(format!("made to forfeit {seized:.1} coins")); }
    if jail > 0.0 && !leaves { parts.push(format!("jailed for {}", span(jail))); }
    if s.sterilize { parts.push("barred from having children".to_string()); }
    if !s.add_status.trim().is_empty() { parts.push(format!("marked as {}", s.add_status.trim().replace('_', " "))); }
    if !s.remove_status.trim().is_empty() { parts.push(format!("stripped of the standing of {}", s.remove_status.trim().replace('_', " "))); }
    if s.exile && !s.execute { parts.push("banished".to_string()); }
    if s.execute { parts.push("put to death".to_string()); }
    let sentence = if parts.is_empty() { "let off with a warning".to_string() } else { list(&parts) };
    let grave = jail > 0.0 || leaves || s.sterilize || paid + seized >= 2.0 * daily;

    if baseless {
        w.note(p, "crime", format!("Charged with {reason}{under}, though I had done nothing: {sentence}."));
    } else {
        w.note(p, "crime", format!("Caught {reason}{under}: {sentence}."));
    }

    // The enforcer who brought the case: a line in the diary, a case toward
    // the quota, and whatever the unit pays by results (settled next morning).
    if let Some(e) = by.filter(|e| e.ix() < w.persons.len() && *e != p) {
        if let Some(ui) = units::unit_ix_of(w, e) {
            let d = &w.polity.units[ui].def;
            let bonus = d.bounty.max(0.0) + d.fine_share.clamp(0.0, 1.0) * paid;
            let q = w.person_mut(e);
            q.legal.cases_week += 1;
            q.legal.bonus_due += bonus;
        }
        w.note(e, "work", format!("Brought {name} to book for {reason}: {sentence}."));
    }

    // A collective fine is felt by everyone who shares the purse.
    if s.collective && paid > 0.0 {
        let members = w.members(hid);
        let share = paid / members.len().max(1) as f32;
        for m in members {
            if m == p { continue; }
            w.person_mut(m).felt.punished += share;
            if !grave && w.is_adult(m) { w.note(m, "money", format!("The household was fined {paid:.1} coins{under} for what {name} did.")); }
        }
    }

    // Those who care for the punished person feel it with them, and hold it
    // against the government in proportion to how wrong they think the rule.
    let mut close: Vec<(PersonId, f32, f32)> = Vec::new();
    for q in &w.persons {
        if !q.present || q.id == p { continue; }
        let r = decide::regard(w, q.id, p);
        if r < 0.2 { continue; }
        let wrong = if baseless { 1.0 } else { (-decide::opinion(w, q.id, sid)).max(0.0) };
        close.push((q.id, r, wrong));
    }
    for (q, r, wrong) in close {
        let dq = decide::daily_scale(w, q);
        let scale = (weight / dq / 6.0).clamp(0.1, 3.0);
        let person = w.person_mut(q);
        person.felt.punished += (0.25 * r * weight).min(15.0 * dq);
        person.trust = (person.trust - r * (0.015 + 0.10 * wrong) * scale).max(0.0);
        if grave && r >= 0.3 {
            w.note(q, if leaves { "grief" } else { "family" }, format!("{name} was {sentence}{under} for {reason}."));
        }
    }

    if leaves || s.sterilize || jail >= 3.0 || paid + seized >= 30.0 {
        w.chronicle_about("crime", format!("{name} was {sentence}{under} for {reason}."), vec![p], Some(sid));
    }
    law::emit(w, EventKind::Conviction, p);

    if s.execute {
        demography::die(w, p, "executed");
    } else if s.exile {
        demography::depart(w, p, &format!("banished for {reason}"));
    } else if jail > 0.0 {
        jail_for(w, p, jail);
    }
    paid
}

/// Lock someone up for `days` (added to any time they are already serving).
pub(super) fn jail_for(w: &mut World, p: PersonId, days: f32) {
    let now = w.slot;
    let slots = ((days * SLOTS_PER_DAY as f32).round() as u32).max(1);
    let (until, here) = {
        let q = w.person_mut(p);
        let until = q.jailed_until.unwrap_or(now).max(now) + slots;
        q.jailed_until = Some(until);
        if q.legal.jailed_since.is_none() { q.legal.jailed_since = Some(now); }
        q.legal.post = None;
        (until, q.loc)
    };
    if !w.enforce.prisoners.contains(&p) { w.enforce.prisoners.push(p); }
    let cell = w.place_of_kind(PlaceKind::Watchhouse).or_else(|| w.place_of_kind(PlaceKind::Townhall)).unwrap_or(here);
    life::go(w, p, cell, Plan { activity: Activity::Jailed, slots: until - now, doing: "Held in the watch house".to_string(), arg: 0 });
}

pub fn release(w: &mut World, p: PersonId, why: &str) {
    if p.ix() >= w.persons.len() || w.person(p).jailed_until.is_none() { return; }
    let now = w.slot;
    let (since, has_place) = {
        let q = w.person_mut(p);
        q.jailed_until = None;
        // Cut short whatever was keeping them inside, so they choose afresh.
        if q.activity == Activity::Jailed { q.until = now; }
        if q.next.as_ref().map(|n| n.activity == Activity::Jailed).unwrap_or(false) { q.next = None; }
        (q.legal.jailed_since.take(), q.job.as_ref().map(|j| !j.owner).unwrap_or(false))
    };
    w.enforce.prisoners.retain(|x| *x != p);
    if !w.person(p).present { return; }
    w.note(p, "law", if why.trim().is_empty() { "Released from the watch house.".to_string() } else { format!("Released from the watch house: {}.", why.trim()) });
    // More than three days away and the place has gone to someone else.
    if has_place && since.map(|s| now.saturating_sub(s) > 3 * SLOTS_PER_DAY).unwrap_or(false) {
        economy::leave_job(w, p, "Lost my place while I was held.");
    }
}

/// Prisoners are fed by the watch house: plain fare, enough to keep them. No coins move.
fn feed(w: &mut World, p: PersonId) {
    let n = &mut w.person_mut(p).needs;
    n.hunger = n.hunger.min(30.0);
}

/// Morning: free those whose time is up and feed the rest. Also picks up
/// anyone another module has locked up directly.
pub(super) fn morning(w: &mut World) {
    let held: Vec<PersonId> = w.persons.iter().filter(|p| p.present && p.jailed_until.is_some()).map(|p| p.id).collect();
    w.enforce.prisoners = held.clone();
    let now = w.slot;
    for p in held {
        if w.person(p).jailed_until.map(|u| u <= now).unwrap_or(false) {
            release(w, p, "the sentence is served");
        } else {
            let q = w.person_mut(p);
            if q.legal.jailed_since.is_none() { q.legal.jailed_since = Some(now); }
            feed(w, p);
        }
    }
}

/// Every slot while anyone is held: release on the hour it falls due; a midday meal.
pub(super) fn tick(w: &mut World) {
    let now = w.slot;
    let midday = w.slot_of_day() == SLOTS_PER_DAY / 2;
    let mut i = 0;
    while i < w.enforce.prisoners.len() {
        let p = w.enforce.prisoners[i];
        let state = w.persons.get(p.ix()).filter(|q| q.present).and_then(|q| q.jailed_until);
        match state {
            None => { w.enforce.prisoners.remove(i); }
            Some(u) if u <= now => release(w, p, "the sentence is served"),
            Some(_) => { if midday { feed(w, p); } i += 1; }
        }
    }
}
