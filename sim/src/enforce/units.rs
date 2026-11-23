//! The watch itself: founding and disbanding units, clerks, the pay scheme,
//! shifts and postings, who is on duty where, and how closely they are watched
//! from above.
use super::{instruments, money, Directive};
use crate::polity::{Actor, Unit};
use crate::statute::*;
use crate::types::*;
use crate::{decide, economy, scope};

// ------------------------------------------------------------- membership

pub(super) fn unit_ix_of(w: &World, p: PersonId) -> Option<usize> {
    let firm = w.persons.get(p.ix())?.job.as_ref()?.firm;
    w.polity.units.iter().position(|u| !u.disbanded && u.firm == firm)
}

/// The enforcement unit a person serves in, if any.
pub fn unit_of(w: &World, p: PersonId) -> Option<UnitId> { unit_ix_of(w, p).map(|i| w.polity.units[i].id) }

/// May this person be taken on by `firm`? True for anything but a unit with an
/// eligibility rule they fail. (For the labour market to ask before hiring.)
pub fn eligible_for(w: &World, firm: FirmId, p: PersonId) -> bool {
    if p.ix() >= w.persons.len() { return false; }
    match w.polity.units.iter().find(|u| !u.disbanded && u.firm == firm) {
        Some(u) => scope::person_bool(w, &u.def.eligibility, p, true),
        None => true,
    }
}

/// Does the unit's writ run at this place?
pub(super) fn writ_at(w: &World, u: &Unit, place: PlaceId) -> bool {
    u.jur.ix() < w.polity.jurisdictions.len() && w.polity.within(w.place(place).ward, u.jur)
}

/// Is this statute the unit's business? It must belong to the unit's
/// jurisdiction or one beneath it, and be named in the mandate if there is one.
pub(super) fn covers(w: &World, u: &Unit, sid: StatuteId) -> bool {
    let Some(st) = w.statute(sid) else { return false };
    if st.jur.ix() >= w.polity.jurisdictions.len() || u.jur.ix() >= w.polity.jurisdictions.len() { return false; }
    if !w.polity.within(st.jur, u.jur) { return false; }
    u.def.mandate.is_empty() || u.def.mandate.iter().any(|t| t.trim().eq_ignore_ascii_case(st.title.trim()))
}

pub(super) fn enforces(w: &World, e: PersonId, sid: StatuteId) -> bool {
    unit_ix_of(w, e).map(|i| covers(w, &w.polity.units[i], sid)).unwrap_or(false)
}

/// Is there any enforcer on the payroll who could be brought to act on this statute here?
pub(super) fn anyone_enforces(w: &World, sid: StatuteId, place: PlaceId) -> bool {
    w.polity.units.iter().any(|u| !u.disbanded && covers(w, u, sid) && writ_at(w, u, place) && staff(w, u).next().is_some())
}

/// Members of a unit who are in town and at liberty.
pub(super) fn staff<'a>(w: &'a World, u: &'a Unit) -> impl Iterator<Item = PersonId> + 'a {
    let firm = u.firm;
    w.firms.get(firm.ix()).into_iter().flat_map(|f| f.workers.iter().copied()).filter(move |e| {
        w.persons.get(e.ix()).map(|p| p.present && p.jailed_until.is_none() && p.job.as_ref().map(|j| j.firm == firm).unwrap_or(false)).unwrap_or(false)
    })
}

/// Out on patrol, or walking to their post.
pub(super) fn on_duty(p: &Person) -> bool {
    p.present && p.jailed_until.is_none() && (p.activity == Activity::Patrol || (p.travel.is_some() && p.next.as_ref().map(|n| n.activity == Activity::Patrol).unwrap_or(false)))
}

pub fn on_duty_at(w: &World, place: PlaceId) -> Vec<PersonId> {
    let mut out = Vec::new();
    if place.ix() >= w.places.len() { return out; }
    for u in &w.polity.units {
        if u.disbanded || !writ_at(w, u, place) { continue; }
        for e in staff(w, u) {
            let p = w.person(e);
            if p.loc == place && on_duty(p) && !out.contains(&e) { out.push(e); }
        }
    }
    out.sort();
    out
}

pub fn enforcers_of(w: &World, jur: JurId) -> Vec<PersonId> {
    let mut out = Vec::new();
    if jur.ix() >= w.polity.jurisdictions.len() { return out; }
    for u in &w.polity.units {
        if u.disbanded || u.jur.ix() >= w.polity.jurisdictions.len() || !w.polity.within(u.jur, jur) { continue; }
        for e in staff(w, u) { if !out.contains(&e) { out.push(e); } }
    }
    out.sort();
    out
}

pub fn force_of(w: &World, enforcer: PersonId) -> f32 {
    let Some(p) = w.persons.get(enforcer.ix()) else { return 0.0 };
    if !p.present || p.jailed_until.is_some() { return 0.0; }
    let body = (p.needs.health / 100.0).clamp(0.2, 1.0) * (0.8 + 0.4 * p.skill.clamp(0.0, 1.0));
    match unit_ix_of(w, enforcer) {
        Some(i) if w.polity.units[i].def.armed => 2.5 * w.tech[Field::Arms].max(0.1) * body,
        Some(_) => 1.25 * body,
        None => body,
    }
}

// ------------------------------------------------- incentives and oversight

/// Coins this enforcer personally gains by bringing a conviction carrying `fine`:
/// the bounty, their share of the fine, and one step out of a quota shortfall.
pub(super) fn incentive(w: &World, u: &Unit, e: PersonId, fine: f32) -> f32 {
    let d = &u.def;
    let relief = if d.quota_per_week > 0.0 && (w.person(e).legal.cases_week as f32) < d.quota_per_week { d.quota_penalty.max(0.0) } else { 0.0 };
    d.bounty.max(0.0) + d.fine_share.clamp(0.0, 1.0) * fine.max(0.0) + relief
}

/// The chance that an enforcer of this unit who looks away from an offence
/// against `sid` is noticed. A commander notices now and then, if the office
/// is filled and its holder cares about the statute; inspectors notice more.
pub(super) fn supervision(w: &World, u: &Unit, sid: StatuteId) -> f32 {
    let mut p_cmd = 0.0;
    if !u.def.commander.trim().is_empty() {
        if let Some(o) = w.polity.office_by_name(u.jur, &u.def.commander) {
            match w.polity.office(o).holder {
                Some(Actor::Player) => p_cmd = 0.15,
                Some(Actor::Person(c)) if c.ix() < w.persons.len() && w.person(c).present => {
                    p_cmd = 0.2 * (0.5 + 0.5 * decide::opinion(w, c, sid)).clamp(0.0, 1.0);
                }
                _ => {}
            }
        }
    }
    1.0 - (1.0 - p_cmd) * (1.0 - instruments::p_inspect(w, u.jur))
}

/// What being caught looking away costs: docked pay now, and a step toward dismissal.
pub(super) fn dereliction_cost(u: &Unit) -> f32 { 4.0 * u.def.wage.max(1.0) }

/// A superior noticed the enforcer let an offence pass: two days' pay is
/// docked to the treasury, and the third such reprimand costs them their place.
pub(super) fn reprimand(w: &mut World, e: PersonId, ui: usize) {
    let (jur, wage, source, name) = { let u = &w.polity.units[ui]; (u.jur, u.def.wage.max(0.0), u.created_by, u.def.name.clone()) };
    let hid = w.person(e).household;
    let took = money::take(w, hid, 2.0 * wage);
    money::collect(w, jur, took, source);
    let p = w.person_mut(e);
    p.felt.punished += took;
    p.legal.reprimands += 1;
    let strikes = p.legal.reprimands;
    w.note(e, "work", format!("Reprimanded for looking the other way, and docked {took:.1} coins."));
    if strikes >= 3 {
        w.person_mut(e).legal.reprimands = 0;
        economy::leave_job(w, e, &format!("Dismissed from the {name} for neglect of duty."));
    }
}

// --------------------------------------------------- founding and disbanding

fn clean(x: f32, hi: f32) -> f32 { if x.is_finite() { x.clamp(0.0, hi) } else { 0.0 } }

/// Get a public body's firm from the economy. If the economy hands back
/// something that is not such a body (it is not implemented yet, or declined),
/// set up a bare one here: a name, a payroll and a wage offer, nothing more.
pub(super) fn public_firm(w: &mut World, kind: FirmKind, jur: JurId, name: &str, capacity: u32, wage: f32) -> FirmId {
    let n0 = w.firms.len();
    let f = economy::found_public_firm(w, kind, jur, name, capacity, wage);
    if let Some(firm) = w.firms.get(f.ix()) {
        if firm.kind == kind && (f.ix() >= n0 || firm.name == name) { return f; }
    }
    let home = if kind == FirmKind::Watch { PlaceKind::Watchhouse } else { PlaceKind::Townhall };
    let place = w.place_of_kind(home).or_else(|| w.place_of_kind(PlaceKind::Townhall)).unwrap_or(w.map.plaza);
    let id = FirmId(w.firms.len() as u32);
    w.firms.push(Firm {
        id, kind, name: name.to_string(), place, owner: Owner::State(jur), jur, manager: None, workers: Vec::new(), open: true, suspended: false, illicit: None,
        cash: 0.0, stock: 0.0, input: 0.0, price: 0.0, sell_price: 0.0, wage_offer: wage, vacancies: capacity, capacity,
        today: Ledger::default(), yesterday: Ledger::default(), avg_sales: 0.0, avg_profit: 0.0, effort_now: 0.0, econ: Default::default(),
    });
    id
}

/// Bring a public body's strength and pay scale in line: those serving are
/// paid on the new scale, and any beyond the new strength are let go, newest first.
pub(super) fn staff_to(w: &mut World, fid: FirmId, count: u32, wage: f32, why: &str) {
    let Some(f) = w.firms.get_mut(fid.ix()) else { return };
    f.capacity = count;
    f.wage_offer = wage;
    f.open = true;
    let workers = f.workers.clone();
    for &e in &workers {
        if let Some(j) = w.persons.get_mut(e.ix()).and_then(|p| p.job.as_mut()) { if j.firm == fid { j.wage = wage; } }
    }
    for &e in workers.iter().skip(count as usize).rev() {
        if e.ix() < w.persons.len() { economy::leave_job(w, e, why); }
    }
}

/// Create or replace (by name) a unit. `firm` lets a caller supply the Watch
/// firm that backs it; otherwise the economy founds one.
pub fn apply_unit_with_firm(w: &mut World, jur: JurId, def: &UnitDef, source: Option<StatuteId>, firm: Option<FirmId>) -> UnitId {
    let jur = money::valid_jur(w, jur).unwrap_or(jur);
    let mut def = def.clone();
    def.name = def.name.trim().to_string();
    def.staff = def.staff.min(200);
    def.wage = clean(def.wage, 1.0e6);
    def.fine_share = clean(def.fine_share, 1.0);
    def.bounty = clean(def.bounty, 1.0e6);
    def.quota_per_week = clean(def.quota_per_week, 1.0e4);
    def.quota_penalty = clean(def.quota_penalty, 1.0e6);
    let firm = firm.filter(|f| f.ix() < w.firms.len());
    let existing = w.polity.units.iter().position(|u| u.def.name.eq_ignore_ascii_case(&def.name));
    let ix = match existing {
        Some(ix) => {
            let old = w.polity.units[ix].firm;
            let old_ok = w.firms.get(old.ix()).map(|f| f.kind == FirmKind::Watch).unwrap_or(false);
            let fid = match firm { Some(f) => f, None if old_ok => old, None => public_firm(w, FirmKind::Watch, jur, &def.name, def.staff, def.wage) };
            w.polity.units[ix].firm = fid;
            ix
        }
        None => {
            let fid = match firm { Some(f) => f, None => public_firm(w, FirmKind::Watch, jur, &def.name, def.staff, def.wage) };
            let ix = w.polity.units.len();
            w.polity.units.push(Unit { id: UnitId(ix as u32), jur, def: def.clone(), firm: fid, disbanded: false, created_by: source });
            ix
        }
    };
    let fid = w.polity.units[ix].firm;
    {
        let u = &mut w.polity.units[ix];
        u.jur = jur;
        u.disbanded = false;
        u.created_by = source;
        u.def = def.clone();
    }
    if let Some(f) = w.firms.get_mut(fid.ix()) {
        f.name = def.name.clone();
        f.jur = jur;
        f.owner = Owner::State(jur);
    }
    staff_to(w, fid, def.staff, def.wage, &format!("Let go as the {} was cut down.", def.name));
    w.enforce.cache.ok = false;
    UnitId(ix as u32)
}

pub fn disband(w: &mut World, name: &str) -> bool {
    let Some(ix) = w.polity.units.iter().position(|u| !u.disbanded && u.def.name.eq_ignore_ascii_case(name.trim())) else { return false };
    let (fid, title) = (w.polity.units[ix].firm, w.polity.units[ix].def.name.clone());
    w.polity.units[ix].disbanded = true;
    let workers = w.firms.get(fid.ix()).map(|f| f.workers.clone()).unwrap_or_default();
    for e in workers {
        if e.ix() >= w.persons.len() { continue; }
        let p = w.person_mut(e);
        p.legal.post = None;
        p.legal.bonus_due = 0.0;
        economy::leave_job(w, e, &format!("The {title} was disbanded."));
    }
    if let Some(f) = w.firms.get_mut(fid.ix()) {
        f.capacity = 0;
        f.vacancies = 0;
        f.open = false;
    }
    w.enforce.directed.retain(|d| d.unit.ix() != ix);
    w.enforce.cache.ok = false;
    w.chronicle("law", format!("The {title} was disbanded."));
    true
}

pub fn set_clerks(w: &mut World, jur: JurId, count: u32, wage: f32) {
    let Some(jur) = money::valid_jur(w, jur) else { return };
    let (count, wage) = (count.min(50), clean(wage, 1.0e6));
    let known = w.enforce.clerks.iter().find(|(j, _)| *j == jur).map(|(_, f)| *f).filter(|f| f.ix() < w.firms.len());
    let fid = match known {
        Some(f) => f,
        None => {
            // An office the jurisdiction already keeps is taken over rather than doubled.
            let found = w.firms.iter().find(|f| f.kind == FirmKind::Office && f.jur == jur && f.open && !w.enforce.inspectors.iter().any(|(_, i)| *i == f.id)).map(|f| f.id);
            let f = match found {
                Some(f) => f,
                None if count == 0 => return,
                None => { let name = format!("Clerks of {}", w.polity.jur(jur).name); public_firm(w, FirmKind::Office, jur, &name, count, wage) }
            };
            w.enforce.clerks.retain(|(j, _)| *j != jur);
            w.enforce.clerks.push((jur, f));
            f
        }
    };
    staff_to(w, fid, count, wage, "Let go as the clerks' office was cut down.");
}

/// A commander orders a unit to keep a post for some days. True if the unit and place exist.
pub fn direct(w: &mut World, unit: &str, place: PlaceId, days: f32) -> bool {
    if place.ix() >= w.places.len() { return false; }
    let Some(ix) = w.polity.units.iter().position(|u| !u.disbanded && u.def.name.eq_ignore_ascii_case(unit.trim())) else { return false };
    let until = w.slot + (days.clamp(0.0, 365.0) * SLOTS_PER_DAY as f32) as u32;
    w.enforce.directed.retain(|d| !(d.unit.ix() == ix && d.place == place));
    w.enforce.directed.push(Directive { unit: UnitId(ix as u32), place, until });
    for e in w.firms.get(w.polity.units[ix].firm.ix()).map(|f| f.workers.clone()).unwrap_or_default() {
        if let Some(p) = w.persons.get_mut(e.ix()) { p.legal.post_until = 0; }
    }
    true
}

// ------------------------------------------------------------ the morning

/// Keep each unit's firm in line with its statute and let go anyone the
/// statute no longer allows to serve.
pub(super) fn sync(w: &mut World) {
    for ix in 0..w.polity.units.len() {
        let (fid, staff_n, wage, disbanded) = { let u = &w.polity.units[ix]; (u.firm, u.def.staff, u.def.wage, u.disbanded) };
        if disbanded { continue; }
        let Some(f) = w.firms.get_mut(fid.ix()) else { continue };
        f.capacity = staff_n;
        f.wage_offer = wage;
        let workers = f.workers.clone();
        for e in workers {
            if e.ix() >= w.persons.len() || !w.person(e).present { continue; }
            if !scope::person_bool(w, &w.polity.units[ix].def.eligibility, e, true) {
                let name = w.polity.units[ix].def.name.clone();
                economy::leave_job(w, e, &format!("No longer allowed to serve in the {name}."));
            }
        }
    }
}

/// Settle the parts of the pay scheme that depend on results. Each morning:
/// bounties and fine shares earned since the last settlement, paid from the
/// unit's treasury as far as it can. On the first day of the week: pay docked
/// for each conviction short of the quota, and the tally starts again.
pub(super) fn settle_pay(w: &mut World) {
    let day = w.day();
    let weekly = w.weekday() == 0 && day > 0;
    for ix in 0..w.polity.units.len() {
        let (jur, fid, source, quota, penalty, wage, disbanded) = { let u = &w.polity.units[ix]; (u.jur, u.firm, u.created_by, u.def.quota_per_week, u.def.quota_penalty, u.def.wage, u.disbanded) };
        if disbanded { continue; }
        let workers = w.firms.get(fid.ix()).map(|f| f.workers.clone()).unwrap_or_default();
        for e in workers {
            if e.ix() >= w.persons.len() { continue; }
            let due = w.person(e).legal.bonus_due;
            if due > 0.01 {
                let got = money::pay_out(w, jur, due, source);
                w.person_mut(e).legal.bonus_due = (due - got).max(0.0);
                if got > 0.0 {
                    money::credit(w, e, got, IncomeKind::Wage);
                    w.note(e, "money", format!("Paid {got:.1} coins for the cases I brought."));
                }
                if got + 0.01 < due {
                    let p = w.person_mut(e);
                    p.trust = (p.trust - 0.02).max(0.0);
                    w.note(e, "money", "The treasury could not pay all I was owed for my cases.");
                }
            }
            if !weekly { continue; }
            let (cases, served, hid) = { let p = w.person(e); (p.legal.cases_week, p.job.as_ref().map(|j| j.since_day <= day - DAYS_PER_WEEK as i32).unwrap_or(false), p.household) };
            let short = (quota - cases as f32).max(0.0);
            if served && penalty > 0.0 && short > 0.0 {
                let took = money::take(w, hid, (short * penalty).min(5.0 * wage.max(1.0)));
                money::collect(w, jur, took, source);
                w.person_mut(e).felt.punished += took;
                w.note(e, "money", format!("Docked {took:.1} coins for bringing {cases} cases against a quota of {quota:.0}."));
            }
            w.person_mut(e).legal.cases_week = 0;
        }
    }
}

// ------------------------------------------------------- shifts and postings

fn shift_of(def: &UnitDef, index: usize) -> Shift {
    match def.shift { Shift::Split => if index.is_multiple_of(2) { Shift::Day } else { Shift::Evening }, s => s }
}

/// If the `index`-th member of a unit is on shift now, the absolute slot at which the shift ends.
pub(super) fn shift_end(w: &World, def: &UnitDef, index: usize) -> Option<u32> {
    // Each member rests one day in seven, staggered so the unit is never all at home.
    if w.weekday() == (REST_DAY + index as u32) % DAYS_PER_WEEK { return None; }
    let h = w.hour();
    let day0 = w.day() as u32 * SLOTS_PER_DAY;
    let at = |hour: u32| day0 + hour * SLOTS_PER_HOUR;
    match shift_of(def, index) {
        Shift::Day => (7.0..15.0).contains(&h).then(|| at(15)),
        Shift::Evening => (15.0..23.0).contains(&h).then(|| at(23)),
        Shift::Night => if h >= 22.0 { Some(at(30)) } else if h < 6.0 { Some(at(6)) } else { None },
        Shift::Split => None,
    }
}

pub fn post_for(w: &mut World, enforcer: PersonId) -> Option<PlaceId> {
    let ui = unit_ix_of(w, enforcer)?;
    let p = w.person(enforcer);
    if !p.present || p.jailed_until.is_some() { return None; }
    let fid = w.polity.units[ui].firm;
    let index = w.firms.get(fid.ix())?.workers.iter().position(|x| *x == enforcer)?;
    let end = shift_end(w, &w.polity.units[ui].def, index)?;
    if let Some(post) = p.legal.post {
        if w.slot < p.legal.post_until && post.ix() < w.places.len() { return Some(post); }
    }
    if !w.enforce.cache.ok || w.enforce.cache.n_statutes != w.statutes.len() { instruments::rebuild(w); }

    // Where the people are, and where the rest of the unit already stands.
    let mut crowd = vec![0u16; w.places.len()];
    for q in &w.persons {
        if q.present && q.travel.is_none() { if let Some(c) = crowd.get_mut(q.loc.ix()) { *c = c.saturating_add(1); } }
    }
    let mut fellows = vec![0u8; w.places.len()];
    for &m in &w.firms[fid.ix()].workers {
        if m == enforcer { continue; }
        if let Some(q) = w.persons.get(m.ix()) {
            if let Some(post) = q.legal.post { if q.legal.post_until > w.slot && post.ix() < fellows.len() { fellows[post.ix()] = fellows[post.ix()].saturating_add(1); } }
        }
    }
    let since = w.slot.saturating_sub(2 * SLOTS_PER_DAY);
    let candidates = w.enforce.cache.posts.get(ui).cloned().unwrap_or_default();
    let mut weights = Vec::with_capacity(candidates.len());
    let mut total = 0.0;
    for &pl in &candidates {
        let reports = w.enforce.hot.iter().filter(|(h, s)| *h == pl && *s >= since).count() as f32;
        let ordered = w.enforce.directed.iter().any(|d| d.unit.ix() == ui && d.place == pl && d.until > w.slot);
        let checkpoint = w.enforce.cache.marks.get(pl.ix()).map(|m| m & instruments::CHECKPOINT != 0).unwrap_or(false);
        let pull = 1.0 + 0.5 * crowd[pl.ix()] as f32 + 3.0 * reports + if ordered { 40.0 } else { 0.0 } + if checkpoint { 6.0 } else { 0.0 };
        let wt = pull / (1.0 + 2.0 * fellows[pl.ix()] as f32);
        weights.push(wt);
        total += wt;
    }
    let post = if candidates.is_empty() || total <= 0.0 {
        w.firms[fid.ix()].place
    } else {
        let mut r = w.rng.f32() * total;
        let mut chosen = candidates[candidates.len() - 1];
        for (i, wt) in weights.iter().enumerate() {
            if r < *wt { chosen = candidates[i]; break; }
            r -= wt;
        }
        chosen
    };
    let until = end.min(w.slot + 2 * SLOTS_PER_HOUR);
    let p = w.person_mut(enforcer);
    p.legal.post = Some(post);
    p.legal.post_until = until;
    Some(post)
}

/// Once an hour: where the watch stands, people see it. That steadies them
/// (less disorder felt); in a private home it is an intrusion.
pub(super) fn presence(w: &mut World) {
    let mut spots: Vec<PlaceId> = Vec::new();
    for u in &w.polity.units {
        if u.disbanded { continue; }
        for e in staff(w, u) {
            let p = w.person(e);
            if on_duty(p) && p.travel.is_none() && !spots.contains(&p.loc) { spots.push(p.loc); }
        }
    }
    if spots.is_empty() { return; }
    let places = &w.places;
    for p in w.persons.iter_mut() {
        if !p.present || p.travel.is_some() || !spots.contains(&p.loc) { continue; }
        if p.activity == Activity::Patrol { continue; }
        p.felt.disorder -= 0.04;
        if places[p.loc.ix()].kind == PlaceKind::Home { p.felt.watched += 0.25; }
    }
}
