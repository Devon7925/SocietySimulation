//! The chain from an offence to its consequence. Every link is a person:
//! a witness who tells or keeps quiet, an enforcer who acts or looks away,
//! who takes a bribe or refuses it, and (in `court`) whoever sits in judgment.
//! Nothing here says what a law results in; it only asks each person in turn.
use super::{court, detect, instruments, money, punish, units, Offence, Trail};
use crate::polity::Actor;
use crate::statute::*;
use crate::types::*;
use crate::{decide, law, scope};
use serde::{Deserialize, Serialize};

/// An offence on its way through the chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub offence: Offence,
    /// The townsperson who told the watch, if one did (they are owed any informer's reward).
    #[serde(default)]
    pub informer: Option<PersonId>,
    /// When it happened.
    pub slot: u32,
    /// Brought by an enforcer against someone who had done nothing.
    #[serde(default)]
    pub baseless: bool,
    /// Enforcers who have looked at it and let it pass.
    #[serde(default)]
    pub declined: Vec<PersonId>,
}
impl Case {
    pub fn new(offence: Offence, slot: u32) -> Case { Case { offence, informer: None, slot, baseless: false, declined: Vec::new() } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome { Ignored, Bribed, Acquitted, Convicted }

/// What the offender stands to lose if convicted, in coins.
fn severity_of(w: &World, o: &Offence) -> f32 {
    decide::sanction_cost(w, o.offender, &o.sanction, o.fine.max(0.0)) + if o.sanction.confiscate { o.stake.max(0.0) } else { 0.0 }
}

fn heat(w: &mut World, place: PlaceId) {
    let now = w.slot;
    let hot = &mut w.enforce.hot;
    hot.push((place, now));
    if hot.len() > 64 { hot.remove(0); }
}

fn action_of(w: &World, sid: StatuteId, clause: usize) -> Option<Action> {
    match w.statute(sid)?.clauses.get(clause)? { Clause::Conduct(c) => Some(c.action), _ => None }
}

/// Nobody acted. Keep what could still come to light: things done behind
/// closed doors (a search may find them) and goods carried off today (a
/// checkpoint may).
fn remember(w: &mut World, case: Case) {
    let keep = match case.offence.trail {
        Trail::Private => true,
        Trail::Seen => action_of(w, case.offence.statute, case.offence.clause).map(detect::carried).unwrap_or(false),
        Trail::Paper => false,
    };
    if !keep { return; }
    let hid = w.person(case.offence.offender).household;
    let same = |w: &World, c: &Case| w.persons.get(c.offence.offender.ix()).map(|p| p.household == hid).unwrap_or(false);
    if w.enforce.latent.iter().filter(|c| same(w, c)).count() >= 6 {
        if let Some(ix) = w.enforce.latent.iter().position(|c| same(w, c)) { w.enforce.latent.remove(ix); }
    }
    w.enforce.latent.push(case);
    if w.enforce.latent.len() > 300 { w.enforce.latent.remove(0); }
}

/// Morning: let go of what can no longer be found.
pub(super) fn forget(w: &mut World) {
    let (day, now) = (w.day(), w.slot);
    w.enforce.latent.retain(|c| {
        let d = (c.slot / SLOTS_PER_DAY) as i32;
        match c.offence.trail { Trail::Private => day - d <= instruments::MEMORY_DAYS, _ => d == day }
    });
    w.enforce.hot.retain(|(_, s)| now.saturating_sub(*s) <= 3 * SLOTS_PER_DAY);
    w.enforce.directed.retain(|d| d.until > now);
}

pub fn report_offence(w: &mut World, off: Offence) -> bool {
    if off.offender.ix() >= w.persons.len() || off.place.ix() >= w.places.len() || w.statute(off.statute).is_none() { return false; }
    w.enforce.today.offences += 1;
    let (offender, sid, place, trail) = (off.offender, off.statute, off.place, off.trail);
    let mut case = Case::new(off, w.slot);

    // Whoever sees a rule broken in the open sees a little disorder - more if
    // someone was wronged by it, none if they think the rule itself is the wrong.
    let seen_by = detect::witnesses(w, offender, place, trail);
    if trail == Trail::Seen {
        let weight = if case.offence.victim.is_some() { 1.0 } else { 0.5 };
        for &q in &seen_by {
            let d = weight * (0.5 + 0.5 * decide::opinion(w, q, sid)).max(0.0);
            w.person_mut(q).felt.disorder += d;
        }
    }
    // With no one on any payroll whose business this is, there is nobody to stop it and nobody to tell.
    if !units::anyone_enforces(w, sid, place) {
        remember(w, case);
        return false;
    }

    // Enforcers standing there see it for themselves.
    if trail != Trail::Paper {
        let spot: Vec<PersonId> = units::on_duty_at(w, place).into_iter().filter(|e| *e != offender && units::enforces(w, *e, sid)).take(3).collect();
        for e in spot {
            match act_on(w, e, &case) {
                Outcome::Ignored => case.declined.push(e),
                Outcome::Bribed => return false,
                Outcome::Acquitted | Outcome::Convicted => { heat(w, place); return true; }
            }
        }
    }

    // Otherwise it rests on someone telling the watch.
    match find_reporter(w, &case, &seen_by) {
        Some(informer) => {
            if let Some(st) = w.statute_mut(sid) { st.stats.reported += 1; }
            w.enforce.today.reports += 1;
            heat(w, place);
            case.informer = informer;
            w.enforce.pending.push(case);
        }
        None => remember(w, case),
    }
    false
}

/// What staying silent could cost a witness whom the law requires to inform.
fn silence_cost(w: &World, q: PersonId, place: PlaceId) -> (Option<law::Duty>, f32) {
    let Some(d) = law::duty(w, q, Action::Inform) else { return (None, 0.0) };
    let Some(Clause::Conduct(c)) = w.statute(d.statute).and_then(|s| s.clauses.get(d.clause)) else { return (Some(d), 0.0) };
    let fine = scope::person_num(w, &c.sanction.fine, q, 0.0).max(0.0);
    (Some(d), decide::sanction_cost(w, q, &c.sanction, fine) * detect::detection_chance(w, q, d.statute, place, Trail::Private))
}

/// Does anyone tell the watch? `Some(Some(p))` if townsperson `p` does,
/// `Some(None)` if it comes to light through a clerk or a watchpost.
fn find_reporter(w: &mut World, case: &Case, seen_by: &[PersonId]) -> Option<Option<PersonId>> {
    let o = &case.offence;
    let (offender, sid) = (o.offender, o.statute);
    let severity = severity_of(w, o);

    if o.trail == Trail::Paper {
        // Nobody sees it done. A clerk may find the hole in the books, and then
        // has an offence in front of them like any other official.
        let jur = w.statute(sid)?.jur;
        let clerks: Vec<PersonId> = detect::clerks_of(w, jur).into_iter().filter(|c| *c != offender).collect();
        if clerks.is_empty() { return None; }
        let p = detect::paper_chance(w, jur, detect::register_of(w, sid, o.clause));
        if !w.rng.chance(p) { return None; }
        let clerk = clerks[w.rng.index(clerks.len())];
        let wage = w.person(clerk).job.as_ref().map(|j| j.wage).unwrap_or(0.0);
        let p_sup = detect::clerk_supervision(w, jur);
        if decide::would_enforce(w, clerk, offender, sid, 0.0, p_sup, 4.0 * wage.max(1.0), severity) {
            let name = w.person(offender).full_name();
            w.note(clerk, "work", format!("Found in the books that {name} had been {}, and reported it.", o.what));
            return Some(None);
        }
        if let Some(st) = w.statute_mut(sid) { st.stats.ignored += 1; }
        w.enforce.today.ignored += 1;
        return None;
    }

    let reward = instruments::informer_reward(w, o.place, o.fine);
    // The one who was wronged has their own reason to speak: what they lost.
    if let Some(v) = o.victim.filter(|v| *v != offender && seen_by.contains(v)) {
        if decide::would_report(w, v, offender, sid, reward + o.stake.max(o.gain).max(0.0), severity) { return Some(Some(v)); }
    }
    let n = seen_by.len();
    if n > 0 {
        // Start anywhere in the crowd, so it is not always the same people who are asked first.
        let start = w.rng.index(n);
        for k in 0..n.min(12) {
            let q = seen_by[(start + k) % n];
            if Some(q) == o.victim { continue; }
            let (duty, at_risk) = if w.enforce.nested == 0 { silence_cost(w, q, o.place) } else { (None, 0.0) };
            let tells = decide::would_report(w, q, offender, sid, reward + at_risk, severity);
            if let Some(d) = duty {
                // A duty to inform was either kept or broken here.
                let cost = decide::regard(w, q, offender) * severity;
                w.enforce.nested += 1;
                law::duty_outcome(w, q, d, tells, cost);
                w.enforce.nested -= 1;
            }
            if tells { return Some(Some(q)); }
        }
    }
    if o.trail == Trail::Seen && instruments::marked(w, o.place, instruments::WATCHPOST) {
        let p = instruments::post_notices(w);
        if w.rng.chance(p) { return Some(None); }
    }
    None
}

/// An enforcer has an offence in front of them - seen, reported, or turned up
/// by a search or a check. They decide whether to act; the accused may try to
/// buy them off; if not, the court hears it and the sentence is carried out.
pub(super) fn act_on(w: &mut World, enforcer: PersonId, case: &Case) -> Outcome {
    let o = &case.offence;
    let (offender, sid) = (o.offender, o.statute);
    let Some(ui) = units::unit_ix_of(w, enforcer) else { return Outcome::Ignored };
    if offender.ix() >= w.persons.len() || !w.person(offender).present || w.statute(sid).is_none() { return Outcome::Ignored; }
    let severity = severity_of(w, o);
    let name = w.person(offender).full_name();
    if !case.baseless {
        let (incentive, p_sup, derel) = { let u = &w.polity.units[ui]; (units::incentive(w, u, enforcer, o.fine), units::supervision(w, u, sid), units::dereliction_cost(u)) };
        if !decide::would_enforce(w, enforcer, offender, sid, incentive, p_sup, derel, severity) {
            if let Some(st) = w.statute_mut(sid) { st.stats.ignored += 1; }
            w.enforce.today.ignored += 1;
            w.note(enforcer, "work", format!("Knew of {name} {} and let it pass.", o.what));
            // Looking away is sometimes noticed from above.
            if w.rng.chance(p_sup) { units::reprimand(w, enforcer, ui); }
            return Outcome::Ignored;
        }
    }
    let day = w.day();
    w.person_mut(offender).legal.last_charge = Some((sid, day));
    if try_bribe(w, enforcer, ui, case, severity) { return Outcome::Bribed; }
    if !court::hear(w, case, enforcer, severity) {
        w.enforce.today.acquitted += 1;
        let by = w.person(enforcer).full_name();
        w.note(offender, "law", format!("Charged with {} by {by} of the watch. The court let me go.", o.what));
        w.note(enforcer, "work", format!("Charged {name} with {}; the court would not convict.", o.what));
        if case.baseless {
            let p = w.person_mut(offender);
            p.trust = (p.trust - 0.03).max(0.0);
        }
        return Outcome::Acquitted;
    }
    let paid = punish::carry_out(w, offender, sid, &o.sanction, o.fine, &o.what, Some(enforcer), o.stake, case.baseless);
    if let Some(i) = case.informer { pay_informer(w, i, case, paid); }
    Outcome::Convicted
}

/// The informer's reward: a share of the fine actually paid plus any flat sum, from the treasury that took the fine.
fn pay_informer(w: &mut World, informer: PersonId, case: &Case, paid: f32) {
    let o = &case.offence;
    // To be informed on is to have been watched by one's neighbours.
    w.person_mut(o.offender).felt.watched += 1.0;
    if informer.ix() >= w.persons.len() || !w.person(informer).present { return; }
    let reward = instruments::informer_reward(w, o.place, paid);
    if reward <= 0.0 { return; }
    let Some(jur) = w.statute(o.statute).map(|s| s.jur) else { return };
    let got = money::pay_out(w, jur, reward, Some(o.statute));
    let name = w.person(o.offender).full_name();
    if got > 0.0 {
        money::credit(w, informer, got, IncomeKind::Other);
        w.note(informer, "money", format!("Paid {got:.1} coins for telling the watch about {name}."));
    } else {
        let p = w.person_mut(informer);
        p.trust = (p.trust - 0.02).max(0.0);
        w.note(informer, "money", format!("Told the watch about {name}; the reward was promised and not paid."));
    }
}

/// The accused offers what they would give to make the charge go away; the
/// enforcer takes it or does not. Offering is itself an act the law may
/// forbid, so it goes through `law::attempt` like any other. True if a bribe
/// changed hands and the matter ended there.
fn try_bribe(w: &mut World, enforcer: PersonId, ui: usize, case: &Case, severity: f32) -> bool {
    if w.enforce.bribing { return false; }
    let o = &case.offence;
    let off = o.offender;
    if !w.is_adult(off) || w.person(off).household == w.person(enforcer).household { return false; }
    let offer = decide::bribe_offer(w, off, severity);
    if offer < 1.0 { return false; }
    let mut act = Act::new(Action::Bribe, w.person(off).loc, severity - offer);
    act.other = Some(enforcer);
    act.price = offer;
    w.enforce.bribing = true;
    let verdict = law::attempt(w, off, &act);
    w.enforce.bribing = false;
    if !verdict.proceeds() || !w.person(off).present || units::unit_ix_of(w, enforcer) != Some(ui) { return false; }
    let offer = offer.min(w.hh_of(off).money.max(0.0));
    if offer < 1.0 { return false; }
    let (jur, wage) = { let u = &w.polity.units[ui]; (u.jur, u.def.wage.max(1.0)) };
    let p_caught = instruments::p_inspect(w, jur);
    let (name, by) = (w.person(off).full_name(), w.person(enforcer).full_name());
    if !decide::would_take_bribe(w, enforcer, offer, p_caught, 12.0 * wage + offer) {
        w.note(enforcer, "work", format!("{name} offered me {offer:.1} coins to forget {}. I would not take it.", o.what));
        return false;
    }
    // Coins pass from one household to the other, off the books.
    let (from, to) = (w.person(off).household, w.person(enforcer).household);
    let got = money::take(w, from, offer);
    w.household_mut(to).money += got;
    {
        let e = w.person_mut(enforcer);
        e.income.other += got;
        e.legal.bribes_taken += 1;
    }
    {
        // They have learned that the law is for sale.
        let p = w.person_mut(off);
        p.trust = (p.trust - 0.03).max(0.0);
    }
    if let Some(st) = w.statute_mut(o.statute) { st.stats.bribes += 1; }
    w.enforce.today.bribes += 1;
    w.note(off, "crime", format!("Caught {} - but {by} of the watch took {got:.1} coins to forget it.", o.what));
    w.note(enforcer, "crime", format!("Took {got:.1} coins from {name} to overlook {}.", o.what));
    instruments::inspect(w, enforcer, ui, o.statute, got, "taking a bribe");
    true
}

/// Reports waiting on an enforcer. Each goes to the enforcer on duty who can
/// reach the accused soonest; if none is on duty it waits, for up to a day.
pub(super) fn resolve_pending(w: &mut World) {
    let now = w.slot;
    let cases = std::mem::take(&mut w.enforce.pending);
    let mut keep = Vec::new();
    for mut case in cases {
        if case.slot >= now { keep.push(case); continue; }
        let off = case.offence.offender;
        if now - case.slot > SLOTS_PER_DAY || !w.persons.get(off.ix()).map(|p| p.present).unwrap_or(false) { continue; }
        let at = w.person(off).loc;
        let mut best: Option<(u32, PersonId)> = None;
        for u in &w.polity.units {
            if u.disbanded || !units::covers(w, u, case.offence.statute) || !units::writ_at(w, u, case.offence.place) { continue; }
            for e in units::staff(w, u) {
                if e == off || case.declined.contains(&e) || !units::on_duty(w.person(e)) { continue; }
                let d = w.travel_slots(w.person(e).loc, at);
                if best.map(|(bd, be)| d < bd || (d == bd && e < be)).unwrap_or(true) { best = Some((d, e)); }
            }
        }
        match best {
            None => keep.push(case),
            Some((_, e)) => {
                if act_on(w, e, &case) == Outcome::Ignored {
                    case.declined.push(e);
                    if case.declined.len() < 2 { keep.push(case); }
                }
            }
        }
    }
    keep.append(&mut w.enforce.pending);
    w.enforce.pending = keep;
}

// ---------------------------------------------------------- baseless charges
//
// A unit that pays by the conviction, or docks pay for missing a quota, makes
// a charge worth something to the enforcer whether or not anything was done.
// Nothing below says that such schemes produce false charges. It asks, of an
// enforcer standing among people who have done nothing, the same question
// `decide::would_enforce` asks of one who has seen an offence - is bringing
// this charge worth it to me? - with the difference that conscience and the
// risk of exposure now weigh against acting instead of for it.

/// How easily this person could make an enforcer regret wronging them:
/// office, a seat, a place in the state's service, money, friends.
fn reprisal(w: &World, t: PersonId) -> f32 {
    let p = w.person(t);
    let actor = Actor::Person(t);
    let mut r = 0.6 * (w.household(p.household).wealth_pct / 100.0).clamp(0.0, 1.0) + 0.04 * p.friends.len().min(10) as f32;
    if w.polity.offices.iter().any(|o| !o.abolished && o.holder == Some(actor)) { r += 1.0; }
    if w.polity.bodies.iter().any(|b| !b.abolished && b.members.contains(&actor)) { r += 0.6; }
    if matches!(p.job.as_ref().map(|j| j.role), Some(Role::Enforcer | Role::Clerk)) { r += 0.6; }
    r
}

/// What bringing a charge with nothing behind it is worth to the enforcer
/// (local psychology in the style of `decide::would_enforce`; it belongs in
/// `decide.rs`). For: what the charge pays. Against: harm to someone they
/// care about; their sense of fairness and of duty to the rule they would be
/// abusing, heavier the heavier the sentence; the chance inspectors find out;
/// and what the accused could do to them in return.
fn frame_score(w: &World, e: PersonId, t: PersonId, sid: StatuteId, incentive: f32, p_exposed: f32, exposure_cost: f32, severity: f32) -> f32 {
    let p = w.person(e);
    let daily = decide::daily_scale(w, e);
    let weight = (severity / daily).min(6.0);
    let harm = decide::regard(w, e, t) * weight;
    let conscience = (0.9 * p.prefs[Pref::Fairness] + 0.9 * decide::duty(w, e, sid).min(1.0)) * (0.6 + 0.4 * weight);
    (incentive / daily) * (0.5 + p.prefs[Pref::Wealth])
        - 1.2 * harm
        - conscience
        - p_exposed * (exposure_cost / daily) * (1.7 - 1.3 * p.prefs[Pref::Risk])
        - 0.5 * reprisal(w, t)
}

fn charge_name(a: Action) -> &'static str {
    match a {
        Action::Buy => "buying what is forbidden", Action::Sell => "selling what is forbidden", Action::Produce => "making what is forbidden",
        Action::Consume => "taking what is forbidden", Action::Work => "working when forbidden", Action::Employ => "unlawful hiring",
        Action::BeOutside => "being out of doors", Action::Visit => "being where it is forbidden", Action::Protest => "protesting",
        Action::OwnProperty => "unlawful ownership", Action::Emigrate => "trying to leave", Action::Immigrate => "unlawful entry",
        Action::HaveChild => "having a child", Action::Marry => "an unlawful marriage", Action::Steal => "theft", Action::Forage => "foraging",
        Action::Bribe => "offering a bribe", Action::StandForOffice => "standing for office", Action::Petition => "petitioning",
        _ => "breaking the law",
    }
}

/// Every couple of hours, in units that pay by results, each enforcer on
/// patrol looks at the people around them with their pay in mind.
pub(super) fn consider_charges(w: &mut World) {
    for ui in 0..w.polity.units.len() {
        let pays = { let u = &w.polity.units[ui]; !u.disbanded && (u.def.bounty > 0.0 || u.def.fine_share > 0.0 || (u.def.quota_per_week > 0.0 && u.def.quota_penalty > 0.0)) };
        if !pays { continue; }
        let out: Vec<PersonId> = units::staff(w, &w.polity.units[ui]).filter(|e| { let p = w.person(*e); units::on_duty(p) && p.travel.is_none() }).collect();
        for e in out { consider_charge(w, ui, e); }
    }
}

fn consider_charge(w: &mut World, ui: usize, e: PersonId) {
    if units::unit_ix_of(w, e) != Some(ui) { return; }
    let place = w.person(e).loc;
    let day = w.day();
    // Whom would it cost me least to charge? Someone I do not care for, who cannot easily make me regret it.
    let mut target: Option<(f32, PersonId)> = None;
    for q in &w.persons {
        if !q.present || q.id == e || q.loc != place || q.travel.is_some() || q.jailed_until.is_some() || q.age_on(day) < ADULT_AGE { continue; }
        if q.legal.last_charge.map(|(_, d)| d == day).unwrap_or(false) { continue; }
        if units::on_duty(q) && units::unit_ix_of(w, q.id).is_some() { continue; }
        let cost = decide::regard(w, e, q.id) + 0.5 * reprisal(w, q.id);
        if target.map(|(c, _)| cost < c).unwrap_or(true) { target = Some((cost, q.id)); }
    }
    let Some((_, t)) = target else { return };
    // Under which rule would the charge pay best?
    let (jur, exposure) = { let u = &w.polity.units[ui]; (u.jur, 12.0 * u.def.wage.max(1.0)) };
    let p_exposed = instruments::p_inspect(w, jur);
    let rules: Vec<(StatuteId, usize)> = w.enforce.cache.chargeable.iter().copied().take(16).collect();
    let mut best: Option<(f32, Offence)> = None;
    for (sid, ci) in rules {
        let u = &w.polity.units[ui];
        let Some(st) = w.statute(sid).filter(|s| s.is_active()) else { continue };
        let Some(Clause::Conduct(c)) = st.clauses.get(ci) else { continue };
        if !units::covers(w, u, sid) || !law::applies_at(w, sid, place) || !scope::person_bool(w, &c.who, t, true) { continue; }
        let act = Act::new(c.action, place, 0.0);
        let fine = scope::act_num(w, &c.sanction.fine, t, &act, 0.0).max(0.0);
        let severity = decide::sanction_cost(w, t, &c.sanction, fine);
        // The charge pays only if it sticks; a court is less sure than the enforcer's own word.
        let sticks = match w.polity.jur(st.jur).court { CourtDef::Summary => 1.0, _ => 0.5 * court::estimate(w, st.jur, t, sid, severity) };
        let incentive = units::incentive(w, u, e, fine) * sticks;
        let score = frame_score(w, e, t, sid, incentive, p_exposed, exposure, severity);
        if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            let what = charge_name(c.action).to_string();
            best = Some((score, Offence { offender: t, statute: sid, clause: ci, place, trail: Trail::Seen, gain: 0.0, stake: 0.0, what, sanction: c.sanction.clone(), fine, victim: None }));
        }
    }
    let Some((score, off)) = best else { return };
    if score <= 0.2 + w.rng.range(-0.12, 0.12) { return; }

    let sid = off.statute;
    let name = w.person(t).full_name();
    w.enforce.today.false_charges += 1;
    w.person_mut(e).legal.false_charges += 1;
    w.note(e, "work", format!("Charged {name} with {}. There was nothing in it, but it counts toward my number.", off.what));
    let mut case = Case::new(off, w.slot);
    case.baseless = true;
    let outcome = act_on(w, e, &case);
    // Inspectors may learn of it; if they act, the charge is undone as far as it can be.
    if units::unit_ix_of(w, e) == Some(ui) && instruments::inspect(w, e, ui, sid, 0.0, "bringing a false charge") && outcome == Outcome::Convicted {
        punish::release(w, t, "the charge was found to be false");
    }
}
