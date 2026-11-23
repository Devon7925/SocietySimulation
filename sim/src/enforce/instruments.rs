//! Tools of detection: informer rewards, checkpoints, registers, home
//! searches, watchposts and inspectors. Each raises the chance that an offence
//! comes to light; none of them punishes anyone by itself - what they turn up
//! still has to be acted on by an enforcer and heard by the court. Those who
//! are checked, watched and searched feel it (`Felt::watched`).
use super::chain::{self, Case};
use super::{money, units, Cache, Offence, Trail};
use crate::polity::Instrument;
use crate::statute::*;
use crate::types::*;
use crate::{decide, economy, law, scope};

pub(super) const CHECKPOINT: u8 = 1;
pub(super) const WATCHPOST: u8 = 2;
/// Days for which what a household did behind closed doors can still be found by a search.
pub(super) const MEMORY_DAYS: i32 = 5;

fn surveillance(w: &World) -> f32 { w.tech[Field::Surveillance].max(0.0) }
/// Chance a search of the home turns up a given hidden offence.
pub(super) fn search_finds(w: &World) -> f32 { (0.75 * surveillance(w)).min(0.95) }
/// Chance a check of the person turns up goods they should not be carrying.
fn check_finds(w: &World) -> f32 { (0.7 * surveillance(w)).min(0.95) }
/// Chance a watchpost notices an offence done in the open in front of it.
pub(super) fn post_notices(w: &World) -> f32 { (0.5 * surveillance(w)).min(0.95) }

/// An instrument works while it is switched on and the statute that set it up stands.
pub(super) fn in_force(w: &World, ins: &Instrument) -> bool {
    ins.def.on && ins.jur.ix() < w.polity.jurisdictions.len()
        && match ins.created_by.and_then(|s| w.statute(s)) { Some(st) => !matches!(st.status, StatuteStatus::Repealed | StatuteStatus::Rejected), None => true }
}

/// Does the instrument's jurisdiction take in this place?
pub(super) fn reaches(w: &World, ins: &Instrument, place: PlaceId) -> bool { w.polity.within(w.place(place).ward, ins.jur) }

pub(super) fn marked(w: &World, place: PlaceId, bit: u8) -> bool { w.enforce.cache.marks.get(place.ix()).map(|m| m & bit != 0).unwrap_or(false) }

/// What informing on an offence here pays if it ends in a conviction with this fine.
pub(super) fn informer_reward(w: &World, place: PlaceId, fine: f32) -> f32 {
    let mut r = 0.0;
    for ins in &w.polity.instruments {
        if let InstrumentKind::InformerReward { share, flat } = &ins.def.kind {
            if in_force(w, ins) && reaches(w, ins, place) { r += share.clamp(0.0, 1.0) * fine.max(0.0) + flat.max(0.0); }
        }
    }
    if r.is_finite() { r } else { 0.0 }
}

/// Inspectors whose remit takes in units of `jur`: those of that jurisdiction and of any above it.
pub fn inspectors_of(w: &World, jur: JurId) -> Vec<PersonId> {
    let mut out = Vec::new();
    if jur.ix() >= w.polity.jurisdictions.len() { return out; }
    for ins in &w.polity.instruments {
        if !matches!(ins.def.kind, InstrumentKind::Inspectors { .. }) || !in_force(w, ins) || !w.polity.within(jur, ins.jur) { continue; }
        let Some((_, fid)) = w.enforce.inspectors.iter().find(|(j, _)| *j == ins.jur) else { continue };
        let Some(f) = w.firms.get(fid.ix()) else { continue };
        for &i in &f.workers {
            let ok = w.persons.get(i.ix()).map(|p| p.present && p.jailed_until.is_none() && p.job.as_ref().map(|j| j.firm == *fid).unwrap_or(false)).unwrap_or(false);
            if ok && !out.contains(&i) { out.push(i); }
        }
    }
    out
}

/// Chance that inspectors come to know of something an enforcer of `jur` did or failed to do.
pub(super) fn p_inspect(w: &World, jur: JurId) -> f32 {
    if w.enforce.inspectors.is_empty() { return 0.0; }
    (0.2 * inspectors_of(w, jur).len() as f32 * surveillance(w)).min(0.8)
}

fn same_tool(a: &InstrumentDef, b: &InstrumentDef) -> bool {
    use InstrumentKind::*;
    let same_kind = std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind);
    if same_kind && !a.label.trim().is_empty() && a.label.trim().eq_ignore_ascii_case(b.label.trim()) { return true; }
    match (&a.kind, &b.kind) {
        (InformerReward { .. }, InformerReward { .. }) | (Inspectors { .. }, Inspectors { .. }) => true,
        (Checkpoint { places: x }, Checkpoint { places: y }) | (Watchpost { places: x }, Watchpost { places: y }) => x.trim() == y.trim(),
        (Registry { of: x }, Registry { of: y }) => x == y,
        (Search { who: x, .. }, Search { who: y, .. }) => x.trim() == y.trim(),
        _ => false,
    }
}

/// Add an instrument, or replace the matching one in the same jurisdiction
/// (so a later statute can change its terms or switch it off).
pub(super) fn apply(w: &mut World, jur: JurId, def: &InstrumentDef, source: Option<StatuteId>) {
    let Some(jur) = money::valid_jur(w, jur) else { return };
    match w.polity.instruments.iter().position(|i| i.jur == jur && same_tool(&i.def, def)) {
        Some(ix) => { let i = &mut w.polity.instruments[ix]; i.def = def.clone(); i.created_by = source; }
        None => w.polity.instruments.push(Instrument { jur, def: def.clone(), created_by: source }),
    }
    if let InstrumentKind::Inspectors { count, wage } = &def.kind {
        let n = if def.on { (*count).min(50) } else { 0 };
        let wage = if wage.is_finite() { wage.max(0.0) } else { 0.0 };
        let known = w.enforce.inspectors.iter().find(|(j, _)| *j == jur).map(|(_, f)| *f).filter(|f| f.ix() < w.firms.len());
        let fid = match known {
            Some(f) => Some(f),
            None if n == 0 => None,
            None => {
                let name = format!("Inspectors of {}", w.polity.jur(jur).name);
                let f = units::public_firm(w, FirmKind::Office, jur, &name, n, wage);
                w.enforce.inspectors.retain(|(j, _)| *j != jur);
                w.enforce.inspectors.push((jur, f));
                Some(f)
            }
        };
        if let Some(f) = fid { units::staff_to(w, f, n, wage, "Let go as the inspectors were stood down."); }
    }
    w.enforce.cache.ok = false;
}

/// Rebuild the lookup tables: where each unit may be posted, which places are
/// checkpoints or watchposts, which rules a check can test.
pub(super) fn rebuild(w: &mut World) {
    let mut c = Cache { ok: true, n_statutes: w.statutes.len(), marks: vec![0; w.places.len()], ..Default::default() };
    for ins in &w.polity.instruments {
        if !in_force(w, ins) { continue; }
        let (expr, bit) = match &ins.def.kind {
            InstrumentKind::Checkpoint { places } => (places, CHECKPOINT),
            InstrumentKind::Watchpost { places } => (places, WATCHPOST),
            _ => continue,
        };
        for pl in &w.places {
            if w.polity.within(pl.ward, ins.jur) && scope::place_bool(w, expr, pl.id, false) { c.marks[pl.id.ix()] |= bit; c.any_marks = true; }
        }
    }
    for u in &w.polity.units {
        let mut posts = Vec::new();
        if !u.disbanded && u.jur.ix() < w.polity.jurisdictions.len() {
            // "Anywhere" means anywhere the public goes; a statute that wants
            // the watch inside homes or workshops has to say so.
            let anywhere = u.def.posts.trim().is_empty() || u.def.posts.trim() == "true";
            for pl in &w.places {
                if !w.polity.within(pl.ward, u.jur) { continue; }
                if if anywhere { pl.kind.public() } else { scope::place_bool(w, &u.def.posts, pl.id, false) } { posts.push(pl.id); }
            }
            if u.def.bounty > 0.0 || u.def.fine_share > 0.0 || (u.def.quota_per_week > 0.0 && u.def.quota_penalty > 0.0) { c.by_results = true; }
        }
        c.posts.push(posts);
    }
    for (st, ci, cl) in w.active_clauses() {
        if let Clause::Conduct(cd) = cl {
            if cd.mode != Mode::Forbid { continue; }
            c.chargeable.push((st.id, ci));
            if matches!(cd.action, Action::BeOutside | Action::Visit) { c.standing.push((st.id, ci)); }
        }
    }
    w.enforce.cache = c;
    // Inspectors whose instrument has lapsed are stood down.
    for k in 0..w.enforce.inspectors.len() {
        let (jur, fid) = w.enforce.inspectors[k];
        let wanted = w.polity.instruments.iter().any(|i| i.jur == jur && matches!(i.def.kind, InstrumentKind::Inspectors { .. }) && in_force(w, i));
        let (staffed, wage) = w.firms.get(fid.ix()).map(|f| (f.capacity > 0, f.wage_offer)).unwrap_or((false, 0.0));
        if !wanted && staffed { units::staff_to(w, fid, 0, wage, "Let go as the inspectors were stood down."); }
    }
}

// ------------------------------------------------- checkpoints and watchposts

/// Every slot, if there are checkpoints or watchposts: note who has just come
/// to one. A watchpost is simply felt. At a checkpoint, an enforcer standing
/// there checks each newcomer once.
pub(super) fn scan_marked(w: &mut World) {
    let mut posted: Vec<(PlaceId, PersonId)> = Vec::new();
    for u in &w.polity.units {
        if u.disbanded { continue; }
        for e in units::staff(w, u) {
            let p = w.person(e);
            if units::on_duty(p) && marked(w, p.loc, CHECKPOINT) && units::writ_at(w, u, p.loc) { posted.push((p.loc, e)); }
        }
    }
    let felt = 0.3 * surveillance(w).min(3.0);
    for i in 0..w.persons.len() {
        let (present, loc, jailed, at_mark) = { let p = &w.persons[i]; (p.present, p.loc, p.jailed_until.is_some(), p.legal.at_mark) };
        if !present { continue; }
        let mark = w.enforce.cache.marks.get(loc.ix()).copied().unwrap_or(0);
        if mark == 0 {
            if at_mark.is_some() { w.persons[i].legal.at_mark = None; }
            continue;
        }
        if jailed { continue; }
        let pid = PersonId(i as u32);
        if at_mark != Some(loc) {
            let p = &mut w.persons[i];
            p.legal.at_mark = Some(loc);
            p.legal.checked = false;
            if mark & WATCHPOST != 0 { p.felt.watched += felt; }
        }
        if mark & CHECKPOINT == 0 || w.persons[i].legal.checked { continue; }
        if units::on_duty(&w.persons[i]) && units::unit_ix_of(w, pid).is_some() {
            w.persons[i].legal.checked = true;
            continue;
        }
        if let Some(&(_, e)) = posted.iter().find(|(pl, e)| *pl == loc && *e != pid) {
            w.persons[i].legal.checked = true;
            check_person(w, e, pid, loc);
        }
    }
}

/// The rule in clause `ci` of `sid`, if `p` is breaking it simply by being at `place` now.
fn standing_offence(w: &World, p: PersonId, sid: StatuteId, ci: usize, place: PlaceId) -> Option<Offence> {
    let st = w.statute(sid)?;
    if !st.is_active() { return None; }
    let Clause::Conduct(c) = st.clauses.get(ci)? else { return None };
    if c.mode != Mode::Forbid { return None; }
    let kind = w.place(place).kind;
    let what = match c.action {
        Action::BeOutside if kind.outdoor() => "being out of doors".to_string(),
        Action::Visit => format!("being at the {}", kind.name()),
        _ => return None,
    };
    if !law::applies_at(w, sid, place) { return None; }
    let act = Act::new(c.action, place, 0.0);
    if !scope::person_bool(w, &c.who, p, true) || !scope::act_bool(w, &c.when, p, &act, true) { return None; }
    let fine = scope::act_num(w, &c.sanction.fine, p, &act, 0.0).max(0.0);
    Some(Offence { offender: p, statute: sid, clause: ci, place, trail: Trail::Seen, gain: 0.0, stake: 0.0, what, sanction: c.sanction.clone(), fine, victim: None })
}

/// An enforcer at a checkpoint stops someone: they are questioned, what they
/// carry is looked through, and their right to be there is checked.
fn check_person(w: &mut World, e: PersonId, p: PersonId, place: PlaceId) {
    let day = w.day();
    w.enforce.today.checks += 1;
    let first_today = {
        let q = w.person_mut(p);
        q.felt.watched += 1.0;
        let first = q.legal.stopped_day != Some(day);
        q.legal.stopped_day = Some(day);
        first
    };
    if first_today {
        let at = w.place(place).name.clone();
        w.note(p, "law", format!("Stopped and questioned by the watch at {at}."));
    }
    // Goods got unlawfully today are still on them.
    let find = check_finds(w);
    let mut found = Vec::new();
    let mut i = 0;
    while i < w.enforce.latent.len() {
        let c = &w.enforce.latent[i];
        let carried = c.offence.offender == p && c.offence.trail == Trail::Seen && c.slot / SLOTS_PER_DAY == day as u32;
        if carried && units::enforces(w, e, c.offence.statute) && w.rng.chance(find) {
            found.push(w.enforce.latent.remove(i));
            continue;
        }
        i += 1;
    }
    for c in found { chain::act_on(w, e, &c); }
    // Rules they break by being here at all: a curfew, a place barred to them, a pass they lack.
    let standing = w.enforce.cache.standing.clone();
    for (sid, ci) in standing {
        let q = w.person(p);
        if !q.present || q.jailed_until.is_some() { break; }
        if q.legal.last_charge == Some((sid, day)) || !units::enforces(w, e, sid) { continue; }
        let Some(off) = standing_offence(w, p, sid, ci, place) else { continue };
        let case = Case::new(off, w.slot);
        chain::act_on(w, e, &case);
    }
}

// ------------------------------------------------------------ home searches

/// Each morning, homes of households that a search order singles out are
/// searched with the stated frequency - if there is an enforcer to do it.
pub(super) fn run_searches(w: &mut World) {
    let orders: Vec<(JurId, String, f32)> = w.polity.instruments.iter().filter(|i| in_force(w, i)).filter_map(|i| match &i.def.kind {
        InstrumentKind::Search { who, per_week } => Some((i.jur, who.clone(), *per_week)),
        _ => None,
    }).collect();
    if orders.is_empty() { return; }
    // One enforcer can go through only so many houses before the day's work.
    let mut load: Vec<(PersonId, u8)> = Vec::new();
    for (jur, who, per_week) in orders {
        let p_day = (per_week / DAYS_PER_WEEK as f32).clamp(0.0, 1.0);
        if p_day.is_nan() || p_day <= 0.0 { continue; }
        for h in 0..w.households.len() {
            let hid = HouseholdId(h as u32);
            let Some(home) = w.households[h].home else { continue };
            if home.ix() >= w.places.len() || !w.polity.within(w.place(home).ward, jur) { continue; }
            let members = w.members(hid);
            if members.is_empty() || !members.iter().any(|m| scope::person_bool(w, &who, *m, true)) { continue; }
            if !w.rng.chance(p_day) { continue; }
            let mut hands: Vec<PersonId> = Vec::new();
            for u in &w.polity.units {
                if u.disbanded || !units::writ_at(w, u, home) { continue; }
                for e in units::staff(w, u) {
                    let busy = load.iter().find(|(x, _)| *x == e).map(|(_, n)| *n).unwrap_or(0);
                    if w.person(e).household != hid && busy < 4 && !hands.contains(&e) { hands.push(e); }
                }
            }
            if hands.is_empty() { continue; }
            let e = hands[w.rng.index(hands.len())];
            match load.iter_mut().find(|(x, _)| *x == e) { Some(l) => l.1 += 1, None => load.push((e, 1)) }
            search_home(w, e, hid);
        }
    }
}

fn search_home(w: &mut World, e: PersonId, hid: HouseholdId) {
    w.enforce.today.searches += 1;
    for m in w.members(hid) {
        w.person_mut(m).felt.watched += 2.0;
        if w.is_adult(m) { w.note(m, "law", "The watch came before dawn and went through the house."); }
    }
    let house = w.household(hid).name.clone();
    w.note(e, "work", format!("Searched the {house} house."));
    let find = search_finds(w);
    let mut found = Vec::new();
    let mut i = 0;
    while i < w.enforce.latent.len() {
        let c = &w.enforce.latent[i];
        let theirs = c.offence.trail == Trail::Private && w.persons.get(c.offence.offender.ix()).map(|p| p.present && p.household == hid).unwrap_or(false);
        if theirs && units::enforces(w, e, c.offence.statute) && w.rng.chance(find) {
            found.push(w.enforce.latent.remove(i));
            continue;
        }
        i += 1;
    }
    for c in found { chain::act_on(w, e, &c); }
}

// --------------------------------------------------------------- inspectors

/// An enforcer has just taken a bribe or brought a charge against someone
/// who had done nothing. If there are inspectors, one may come to know of it;
/// whether they act is theirs to decide, as it is for any official with an
/// offence in front of them. If they do, the enforcer forfeits the gain and
/// is dismissed. Returns true if the enforcer was caught.
pub(super) fn inspect(w: &mut World, e: PersonId, ui: usize, sid: StatuteId, forfeit: f32, wrong: &str) -> bool {
    let (jur, wage, source, unit) = { let u = &w.polity.units[ui]; (u.jur, u.def.wage.max(1.0), u.created_by, u.def.name.clone()) };
    let p = p_inspect(w, jur);
    if p <= 0.0 { return false; }
    let inspectors: Vec<PersonId> = inspectors_of(w, jur).into_iter().filter(|i| *i != e).collect();
    if inspectors.is_empty() || !w.rng.chance(p) { return false; }
    let inspector = inspectors[w.rng.index(inspectors.len())];
    let name = w.person(e).full_name();
    if !decide::would_enforce(w, inspector, e, sid, 0.0, 0.0, 0.0, 12.0 * wage + forfeit) {
        w.note(inspector, "work", format!("Learned that {name} had been {wrong}, and let it lie."));
        return false;
    }
    let hid = w.person(e).household;
    let took = money::take(w, hid, forfeit);
    money::collect(w, jur, took, source);
    {
        // Dismissed, they lose what they had gained and whatever the unit still owed them for their cases.
        let q = w.person_mut(e);
        q.felt.punished += took + 5.0 * wage;
        q.legal.bonus_due = 0.0;
    }
    w.note(inspector, "work", format!("Reported {name} of the {unit} for {wrong}."));
    w.note(e, "crime", format!("The inspectors found me {wrong}. I am dismissed from the {unit}."));
    w.chronicle_about("crime", format!("{name} of the {unit} was dismissed for {wrong}."), vec![e], Some(sid));
    economy::leave_job(w, e, "");
    true
}
