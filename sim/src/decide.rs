//! Shared psychology: how base preferences turn into the small decisions that
//! every system needs (obey or break, report or look away, enforce or let
//! pass, take the bribe or not).
//!
//! Every module must use these rather than inventing its own, so that one
//! person behaves as one person everywhere. All "costs" and "gains" are in
//! coin-equivalents for that person; `daily_scale` converts.
use crate::statute::Sanction;
use crate::types::*;

/// The number of coins a day that matters to this person: their usual income,
/// or the going wage if they have none.
pub fn daily_scale(w: &World, pid: PersonId) -> f32 {
    let inc = w.person(pid).income.avg;
    (if inc > 0.5 { inc } else { w.stats.median_wage }).max(4.0)
}

/// A person's standing view of a statute, -1..=1 (0 if they have none).
pub fn opinion(w: &World, pid: PersonId, sid: StatuteId) -> f32 {
    w.person(pid).opinion(sid).unwrap_or(0.0)
}

/// How strongly a person feels bound to keep a rule for its own sake (0..~1.3).
/// Stronger in conformists, under a government they trust, for rules they
/// agree with, for rules their neighbours actually keep, and for rules made
/// by a process they had a part in.
pub fn duty(w: &World, pid: PersonId, sid: StatuteId) -> f32 {
    let p = w.person(pid);
    let (flout, voted) = w.statute(sid).map(|s| (s.stats.flout_rate, s.passed_by != "decree" && !s.passed_by.is_empty())).unwrap_or((0.0, false));
    let d = p.prefs[Pref::Conformity] * (0.3 + 0.7 * p.trust) * (1.0 + 0.6 * opinion(w, pid, sid)) * (1.0 - 0.6 * flout) * if voted { 1.15 } else { 1.0 };
    d.max(0.0)
}

/// What a sanction would cost this person, in coins.
pub fn sanction_cost(w: &World, pid: PersonId, s: &Sanction, fine: f32) -> f32 {
    let p = w.person(pid);
    let daily = daily_scale(w, pid);
    let mut c = fine + s.jail_days * daily * 1.5;
    if s.exile { c += daily * 40.0 * (1.0 + p.prefs[Pref::Family]); }
    if s.execute { c += daily * 300.0; }
    if s.sterilize && !p.sterile { c += daily * (10.0 + 60.0 * p.prefs[Pref::Family]); }
    if !s.add_status.is_empty() || !s.remove_status.is_empty() { c += daily * 3.0 * (0.5 + p.prefs[Pref::Esteem]); }
    c
}

/// Would this person break the rule? They do when what they stand to gain
/// outweighs the punishment they expect plus the weight of their conscience.
pub fn would_break(w: &mut World, pid: PersonId, sid: StatuteId, gain: f32, p_detect: f32, cost_if_caught: f32) -> bool {
    if gain <= 0.0 { return false; }
    let daily = daily_scale(w, pid);
    let moral = duty(w, pid, sid) * (daily * 0.9 + gain * 0.8);
    let fear = p_detect.clamp(0.0, 1.0) * cost_if_caught * (1.7 - 1.3 * w.person(pid).prefs[Pref::Risk]);
    let jitter = w.rng.range(0.75, 1.25);
    gain * jitter > moral + fear
}

/// How much `a` cares what happens to `b` (0..=1).
pub fn regard(w: &World, a: PersonId, b: PersonId) -> f32 {
    if a == b { return 1.0; }
    let (pa, pb) = (w.person(a), w.person(b));
    let fam = pa.prefs[Pref::Family];
    if pa.household == pb.household { return 0.5 + 0.5 * fam; }
    let kin = pa.spouse == Some(b) || pa.mother == Some(b) || pa.father == Some(b) || pa.children.contains(&b) || pb.children.contains(&a);
    if kin { return 0.3 + 0.6 * fam; }
    let com = pa.prefs[Pref::Community];
    let aff = pa.affinity(b);
    (aff * (0.3 + 0.7 * com)).max(0.12 * com)
}

/// Would a witness tell the authorities? `reward` is what informing pays, in
/// coins; `severity` is what the offender stands to lose, in coins.
pub fn would_report(w: &mut World, witness: PersonId, offender: PersonId, sid: StatuteId, reward: f32, severity: f32) -> bool {
    if witness == offender { return false; }
    let p = w.person(witness);
    let daily = daily_scale(w, witness);
    let op = opinion(w, witness, sid);
    let town_view = w.statute(sid).map(|s| s.stats.approval).unwrap_or(0.0);
    let harm = regard(w, witness, offender) * (severity / daily).min(6.0);
    let score = (reward / daily) * (0.5 + p.prefs[Pref::Wealth]) + 0.6 * op.max(0.0) + 0.5 * p.prefs[Pref::Conformity] * p.trust + 0.25 * p.prefs[Pref::Security]
        - 1.2 * harm
        - 0.7 * (-op).max(0.0)
        - 0.35 * p.prefs[Pref::Esteem] * (-town_view).max(0.0)
        - 0.2 * p.prefs[Pref::Autonomy];
    score > 0.4 + w.rng.range(-0.15, 0.15)
}

/// Would an enforcer act on an offence in front of them?
///
/// * `incentive` - coins they personally gain by acting (bounty, fine share, quota relief);
/// * `p_supervised` - chance that looking away is noticed by a commander or inspector;
/// * `dereliction_cost` - what that would cost them, in coins.
pub fn would_enforce(w: &mut World, enforcer: PersonId, offender: PersonId, sid: StatuteId, incentive: f32, p_supervised: f32, dereliction_cost: f32, severity: f32) -> bool {
    let p = w.person(enforcer);
    let daily = daily_scale(w, enforcer);
    let op = opinion(w, enforcer, sid);
    let harm = regard(w, enforcer, offender) * (severity / daily).min(6.0);
    let score = (incentive / daily) * (0.5 + p.prefs[Pref::Wealth]) + 0.7 * op.max(0.0) + 0.7 * p.prefs[Pref::Diligence] * p.prefs[Pref::Conformity] * (0.4 + 0.6 * p.trust)
        + p_supervised * (dereliction_cost / daily) * (1.7 - 1.3 * p.prefs[Pref::Risk])
        - 1.2 * harm
        - 0.8 * (-op).max(0.0)
        - 0.15 * (1.0 - p.prefs[Pref::Diligence]);
    score > 0.2 + w.rng.range(-0.12, 0.12)
}

/// Would an enforcer pocket `bribe` coins to forget what they saw?
pub fn would_take_bribe(w: &mut World, enforcer: PersonId, bribe: f32, p_caught: f32, cost_if_caught: f32) -> bool {
    let p = w.person(enforcer);
    let daily = daily_scale(w, enforcer);
    let pay = p.job.as_ref().map(|j| j.wage).unwrap_or(0.0);
    let underpaid = (1.3 - pay / w.stats.median_wage.max(1.0)).clamp(0.2, 1.3);
    let gain = (bribe / daily) * (0.4 + 0.8 * p.prefs[Pref::Wealth]) * underpaid;
    let scruple = p.prefs[Pref::Conformity] * (0.4 + 0.6 * p.trust) * (0.6 + 0.6 * p.prefs[Pref::Fairness]);
    let fear = p_caught * (cost_if_caught / daily) * (1.7 - 1.3 * p.prefs[Pref::Risk]);
    gain > scruple + fear + w.rng.range(-0.1, 0.1)
}

/// The most an offender would pay to make a charge go away.
pub fn bribe_offer(w: &World, offender: PersonId, cost_if_convicted: f32) -> f32 {
    let money = w.hh_of(offender).money.max(0.0);
    (cost_if_convicted * 0.45).min(money * 0.6)
}

/// How a person reads a forecast change in their wellbeing: with error that
/// shrinks with schooling, and discounted by their patience when the effect
/// lies in the future.
pub fn perceive(w: &mut World, pid: PersonId, delta: f32) -> f32 {
    let p = w.person(pid);
    let noise = w.settings.forecast_noise * (1.2 - p.education);
    let patience = 0.6 + 0.4 * p.prefs[Pref::Patience];
    let n = w.rng.gauss(0.0, noise);
    delta * patience * (1.0 + n)
}
