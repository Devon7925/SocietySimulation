//! What a would-be offender expects. `detection_chance` is assembled from the
//! same parts as the real chain in `chain.rs` - the enforcers standing there,
//! the people who would tell, the instruments in force, the court - so that it
//! is roughly right without being a separate rule about outcomes.
use super::court;
use super::{instruments, units, Trail};
use crate::statute::*;
use crate::types::*;
use crate::{decide, scope};

// The two functions below give the chance that `decide::would_report` and
// `decide::would_enforce` come out true: the same score, with the dice
// replaced by their distribution. They must be kept in step with `decide.rs`
// (a test checks that they agree); they belong there, as a score function
// each decision and its forecast could share.

/// Chance that this witness would tell the authorities.
pub(super) fn report_propensity(w: &World, witness: PersonId, offender: PersonId, sid: StatuteId, reward: f32, severity: f32) -> f32 {
    if witness == offender { return 0.0; }
    let p = w.person(witness);
    let daily = decide::daily_scale(w, witness);
    let op = decide::opinion(w, witness, sid);
    let town_view = w.statute(sid).map(|s| s.stats.approval).unwrap_or(0.0);
    let harm = decide::regard(w, witness, offender) * (severity / daily).min(6.0);
    let score = (reward / daily) * (0.5 + p.prefs[Pref::Wealth]) + 0.6 * op.max(0.0) + 0.5 * p.prefs[Pref::Conformity] * p.trust + 0.25 * p.prefs[Pref::Security]
        - 1.2 * harm
        - 0.7 * (-op).max(0.0)
        - 0.35 * p.prefs[Pref::Esteem] * (-town_view).max(0.0)
        - 0.2 * p.prefs[Pref::Autonomy];
    ((score - 0.25) / 0.30).clamp(0.0, 1.0)
}

/// Chance that this enforcer would act on the offence.
pub(super) fn enforce_propensity(w: &World, enforcer: PersonId, offender: PersonId, sid: StatuteId, incentive: f32, p_supervised: f32, dereliction_cost: f32, severity: f32) -> f32 {
    let p = w.person(enforcer);
    let daily = decide::daily_scale(w, enforcer);
    let op = decide::opinion(w, enforcer, sid);
    let harm = decide::regard(w, enforcer, offender) * (severity / daily).min(6.0);
    let score = (incentive / daily) * (0.5 + p.prefs[Pref::Wealth]) + 0.7 * op.max(0.0) + 0.7 * p.prefs[Pref::Diligence] * p.prefs[Pref::Conformity] * (0.4 + 0.6 * p.trust)
        + p_supervised * (dereliction_cost / daily) * (1.7 - 1.3 * p.prefs[Pref::Risk])
        - 1.2 * harm
        - 0.8 * (-op).max(0.0)
        - 0.15 * (1.0 - p.prefs[Pref::Diligence]);
    ((score - 0.08) / 0.24).clamp(0.0, 1.0)
}

/// Who is in a position to see the act: for something done in the open,
/// everyone at the place who is awake (and, on a street, those walking
/// through); for something done behind closed doors, those inside and the
/// offender's own household; for a hole in the books, nobody. Enforcers on
/// duty are not listed - they are asked in their own right.
pub(super) fn witnesses(w: &World, offender: PersonId, place: PlaceId, trail: Trail) -> Vec<PersonId> {
    let mut out = Vec::new();
    if trail == Trail::Paper { return out; }
    let day = w.day();
    let street = w.place(place).kind == PlaceKind::Street;
    let household = w.person(offender).household;
    for p in &w.persons {
        if !p.present || p.id == offender || p.jailed_until.is_some() || p.age_on(day) < 10.0 { continue; }
        let here = p.loc == place && (p.travel.is_none() || street) && p.activity != Activity::Sleep;
        let knows = trail == Trail::Private && p.household == household;
        if !(here || knows) { continue; }
        if units::on_duty(p) && units::unit_ix_of(w, p.id).is_some() { continue; }
        out.push(p.id);
    }
    out
}

// ------------------------------------------------------------ paper trails

/// Clerks whose books cover `jur`: those of its own office and of every jurisdiction above it.
pub fn clerks_of(w: &World, jur: JurId) -> Vec<PersonId> {
    let mut out = Vec::new();
    if jur.ix() >= w.polity.jurisdictions.len() { return out; }
    for (j, fid) in &w.enforce.clerks {
        if j.ix() >= w.polity.jurisdictions.len() || !w.polity.within(jur, *j) { continue; }
        let Some(f) = w.firms.get(fid.ix()) else { continue };
        for &c in &f.workers {
            let ok = w.persons.get(c.ix()).map(|p| p.present && p.jailed_until.is_none() && p.job.as_ref().map(|j| j.firm == *fid).unwrap_or(false)).unwrap_or(false);
            if ok && !out.contains(&c) { out.push(c); }
        }
    }
    out
}

/// The register in which a breach of this clause would show, if one were kept.
pub fn register_of(w: &World, sid: StatuteId, clause: usize) -> Option<Register> {
    Some(match w.statute(sid)?.clauses.get(clause)? {
        Clause::Conduct(c) => match c.action {
            Action::HaveChild | Action::Marry => Register::Births,
            Action::OwnProperty => Register::Property,
            Action::Work | Action::Employ | Action::HoldJob | Action::PublicLabor => Register::Income,
            Action::Buy | Action::Sell | Action::Produce => Register::Trade,
            Action::Immigrate | Action::Emigrate | Action::AttendSchool | Action::Attend | Action::Vote => Register::Residence,
            _ => return None,
        },
        Clause::Levy(l) => match l.base {
            LevyBase::Income | LevyBase::Wage | LevyBase::Profit | LevyBase::RentIncome => Register::Income,
            LevyBase::Wealth | LevyBase::Property => Register::Property,
            LevyBase::Head => Register::Residence,
            LevyBase::Sale | LevyBase::Import | LevyBase::Export => Register::Trade,
            LevyBase::Inheritance => Register::Births,
        },
        Clause::Price(p) => match p.target { Priced::Wage => Register::Income, Priced::Rent => Register::Property, _ => Register::Trade },
        Clause::Transfer(_) | Clause::Subsidy(_) => Register::Income,
        _ => return None,
    })
}

/// The chance that a breach which leaves only a paper trail is noticed in the
/// books of `jur`: nothing without clerks; more with each clerk; much more if
/// they keep a register of the matching kind; all scaled by `Tech[Administration]`.
/// (Also the natural detection rate for `law`'s audits.)
pub fn paper_chance(w: &World, jur: JurId, register: Option<Register>) -> f32 {
    if jur.ix() >= w.polity.jurisdictions.len() { return 0.0; }
    let clerks = clerks_of(w, jur).len() as f32;
    if clerks <= 0.0 { return 0.0; }
    let admin = w.tech[Field::Administration].max(0.0);
    let base = (0.10 * clerks * admin).min(0.6);
    let kept = register.is_some() && w.polity.instruments.iter().any(|i| {
        matches!(&i.def.kind, InstrumentKind::Registry { of } if Some(*of) == register) && instruments::in_force(w, i) && w.polity.within(jur, i.jur)
    });
    let reg = if kept { ((0.25 + 0.08 * clerks) * admin).min(0.85) } else { 0.0 };
    (1.0 - (1.0 - base) * (1.0 - reg)).min(0.95)
}

/// How closely a clerk's own work is looked over: the books are open to whoever
/// holds office over them, and to inspectors.
pub(super) fn clerk_supervision(w: &World, jur: JurId) -> f32 { 1.0 - 0.85 * (1.0 - instruments::p_inspect(w, jur)) }

// ------------------------------------------------------------ the estimate

/// The heaviest sanction the statute carries, as it would fall on this person: (cost in coins, fine).
fn typical_sanction(w: &World, offender: PersonId, st: &Statute) -> (f32, f32) {
    let mut best = (0.0, 0.0);
    for c in &st.clauses {
        let s = match c { Clause::Conduct(c) => &c.sanction, Clause::Price(p) => &p.sanction, _ => continue };
        let fine = scope::person_num(w, &s.fine, offender, 0.0).max(0.0);
        let cost = decide::sanction_cost(w, offender, s, fine);
        if cost >= best.0 { best = (cost, fine); }
    }
    best
}

/// If the watch is told, how likely is it that one of them acts? The mean
/// willingness of those on the payroll whose business this statute is.
fn reach(w: &World, offender: PersonId, sid: StatuteId, place: PlaceId, fine: f32, severity: f32) -> f32 {
    let (mut sum, mut n) = (0.0, 0u32);
    for u in &w.polity.units {
        if u.disbanded || !units::covers(w, u, sid) || !units::writ_at(w, u, place) { continue; }
        let (p_sup, derel) = (units::supervision(w, u, sid), units::dereliction_cost(u));
        for e in units::staff(w, u).take(8) {
            if e == offender { continue; }
            sum += enforce_propensity(w, e, offender, sid, units::incentive(w, u, e, fine), p_sup, derel, severity);
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum / n as f32 }
}

pub fn detection_chance(w: &World, offender: PersonId, sid: StatuteId, place: PlaceId, trail: Trail) -> f32 {
    if offender.ix() >= w.persons.len() || place.ix() >= w.places.len() { return 0.0; }
    let Some(st) = w.statute(sid) else { return 0.0 };
    if st.jur.ix() >= w.polity.jurisdictions.len() { return 0.0; }
    let (severity, fine) = typical_sanction(w, offender, st);
    // Nobody to act on it: nothing follows, whoever sees.
    let told = reach(w, offender, sid, place, fine, severity);
    if told <= 0.0 { return 0.0; }
    let p_court = court::estimate(w, st.jur, offender, sid, severity);
    if p_court <= 0.0 { return 0.0; }

    let mut miss = 1.0f32;
    if trail == Trail::Paper {
        // Whichever clause it is, a register of any matching kind helps; take the best case for the state.
        let mut best = paper_chance(w, st.jur, None);
        for ci in 0..st.clauses.len() {
            if let Some(r) = register_of(w, sid, ci) { best = best.max(paper_chance(w, st.jur, Some(r))); }
        }
        let clerks = clerks_of(w, st.jur);
        let p_sup = clerk_supervision(w, st.jur);
        let mut willing = 0.0;
        for &c in clerks.iter().take(6) {
            let wage = w.person(c).job.as_ref().map(|j| j.wage).unwrap_or(0.0);
            willing += enforce_propensity(w, c, offender, sid, 0.0, p_sup, 4.0 * wage.max(1.0), severity);
        }
        if !clerks.is_empty() { willing /= clerks.len().min(6) as f32; }
        miss *= 1.0 - best * willing * told;
    } else {
        // Enforcers standing there.
        for u in &w.polity.units {
            if u.disbanded || !units::covers(w, u, sid) || !units::writ_at(w, u, place) { continue; }
            let (p_sup, derel) = (units::supervision(w, u, sid), units::dereliction_cost(u));
            for e in units::staff(w, u) {
                let p = w.person(e);
                if e == offender || p.loc != place || !units::on_duty(p) { continue; }
                miss *= 1.0 - enforce_propensity(w, e, offender, sid, units::incentive(w, u, e, fine), p_sup, derel, severity);
            }
        }
        // People who would tell.
        let reward = instruments::informer_reward(w, place, fine);
        let mut silent = 1.0f32;
        for q in witnesses(w, offender, place, trail) {
            silent *= 1.0 - report_propensity(w, q, offender, sid, reward, severity);
            if silent < 0.001 { break; }
        }
        if trail == Trail::Seen && instruments::marked(w, place, instruments::WATCHPOST) { silent *= 1.0 - instruments::post_notices(w); }
        miss *= 1.0 - (1.0 - silent) * told;
        // Searches of the home, for what is done behind its doors.
        if trail == Trail::Private {
            if let Some(home) = w.home_of(offender).filter(|h| h.ix() < w.places.len()) {
                let hid = w.person(offender).household;
                for ins in &w.polity.instruments {
                    let InstrumentKind::Search { who, per_week } = &ins.def.kind else { continue };
                    if !instruments::in_force(w, ins) || !instruments::reaches(w, ins, home) { continue; }
                    if !w.members(hid).iter().any(|m| scope::person_bool(w, who, *m, true)) { continue; }
                    let nightly = (per_week / DAYS_PER_WEEK as f32).clamp(0.0, 1.0) * instruments::search_finds(w);
                    let found = 1.0 - (1.0 - nightly).powi(instruments::MEMORY_DAYS);
                    miss *= 1.0 - found * told;
                }
            }
        }
        // Checkpoints in this ward, for goods carried away and for being where one may not be.
        if trail == Trail::Seen && w.enforce.cache.any_marks {
            let ward = w.place(place).ward;
            let checked = w.places.iter().any(|pl| pl.ward == ward && instruments::marked(w, pl.id, instruments::CHECKPOINT));
            let exposed = st.clauses.iter().any(|c| matches!(c, Clause::Conduct(c) if carried(c.action) || matches!(c.action, Action::BeOutside | Action::Visit)));
            if checked && exposed { miss *= 1.0 - 0.2 * told; }
        }
    }
    ((1.0 - miss) * p_court).clamp(0.0, 0.98)
}

/// Acts that leave the offender carrying something a check would find.
pub(super) fn carried(a: Action) -> bool { matches!(a, Action::Buy | Action::Produce | Action::Steal | Action::Forage) }
