//! Opinions: what each adult makes of each statute in force and each bill
//! pending, -1..=1. A bill is judged by foresight - the forecast as that
//! person reads it. A statute in force is judged, more and more, by what it
//! has actually been like to live under.
use super::Lived;
use crate::polity::BillState;
use crate::statute::Statute;
use crate::types::*;
use crate::{decide, forecast, law};

/// How far a forecast change in wellbeing moves an opinion.
const FORESIGHT: f32 = 8.0;
/// How far a direct cost of one day's income per day moves an opinion.
const DIRECT: f32 = 2.5;
/// Nightly step of an opinion toward lived experience.
const DRIFT: f32 = 0.12;

fn find(w: &World, sid: StatuteId) -> Option<&Statute> {
    w.statute(sid).or_else(|| w.polity.bills.iter().find(|b| b.statute.id == sid).map(|b| &b.statute))
}

/// The first-order view: what the rule plainly costs or pays them.
fn first_order(w: &World, pid: PersonId, sid: StatuteId) -> Option<f32> {
    let s = find(w, sid)?;
    Some((DIRECT * law::direct_impact(w, pid, s) / decide::daily_scale(w, pid)).tanh())
}

/// One person's view of a statute or bill they have not (yet) lived under.
fn foresee(w: &mut World, pid: PersonId, sid: StatuteId) -> Option<f32> {
    let delta = forecast::stored(w, sid).map(|f| f.of(pid));
    match (delta, w.settings.forecast) {
        (None, _) => first_order(w, pid, sid),
        // No foresight: only the plainest effect on themselves.
        (Some(d), ForecastMode::Experience) => first_order(w, pid, sid).or_else(|| Some((FORESIGHT * decide::perceive(w, pid, d)).tanh())),
        (Some(d), mode) => {
            let fore = (FORESIGHT * decide::perceive(w, pid, d)).tanh();
            match mode {
                // They trust the forecast as far as their schooling lets them follow it.
                ForecastMode::Blend { .. } => {
                    let k = 0.3 + 0.7 * w.persons[pid.ix()].education.clamp(0.0, 1.0);
                    Some(match first_order(w, pid, sid) { Some(x) => k * fore + (1.0 - k) * x, None => fore })
                }
                _ => Some(fore),
            }
        }
    }
}

/// Recompute every adult's opinion of one statute or bill.
pub(super) fn form(w: &mut World, sid: StatuteId) {
    let day = w.day();
    let (mut sum, mut cnt) = (0.0f32, 0u32);
    for i in 0..w.persons.len() {
        { let p = &w.persons[i]; if !p.present || p.age_on(day) < ADULT_AGE { continue; } }
        let Some(v) = foresee(w, PersonId(i as u32), sid) else { continue };
        let v = v.clamp(-1.0, 1.0);
        w.persons[i].set_opinion(sid, v);
        sum += v;
        cnt += 1;
    }
    if cnt > 0 { if let Some(s) = w.statute_mut(sid) { s.stats.approval = sum / cnt as f32; } }
}

/// Night: opinions of statutes in force drift toward lived experience; views
/// of things that no longer exist are dropped; each statute's approval is the
/// mean adult opinion.
pub(super) fn nightly(w: &mut World) {
    let day = w.day();
    let fork = w.is_fork();
    let active: Vec<(StatuteId, usize)> = w.statutes.iter().enumerate().filter(|(_, s)| s.is_active()).map(|(k, s)| (s.id, k)).collect();
    let pending: Vec<StatuteId> = w.polity.bills.iter().filter(|b| matches!(b.state, BillState::Queued | BillState::Voting { .. } | BillState::Assent { .. })).map(|b| b.statute.id).collect();
    let mut tally = vec![(0.0f32, 0u32); active.len()];
    for i in 0..w.persons.len() {
        let pid = PersonId(i as u32);
        let (adult, total) = {
            let p = &mut w.persons[i];
            if !p.present { continue; }
            if !p.opinions.is_empty() { p.opinions.retain(|(id, _)| active.iter().any(|a| a.0 == *id) || pending.contains(id)); }
            if !p.mind.lived.is_empty() { p.mind.lived.retain(|l| active.iter().any(|a| a.0 == l.sid)); }
            (p.age_on(day) >= ADULT_AGE, p.wellbeing.total)
        };
        if !adult { continue; }
        let daily = decide::daily_scale(w, pid);
        for (k, &(sid, ix)) in active.iter().enumerate() {
            let li = match w.persons[i].mind.lived.iter().position(|l| l.sid == sid) {
                Some(li) => li,
                None => { let l = &mut w.persons[i].mind.lived; l.push(Lived { sid, since: day, wb_at: total, direct: 0.0, hit: 0.0, known: false }); l.len() - 1 }
            };
            // What it plainly costs or pays them, re-reckoned every few days (the reckoning is not cheap).
            let known = w.persons[i].mind.lived[li].known;
            if !known || (!fork && (day as u32).wrapping_add(pid.0).wrapping_add(sid.0) % 3 == 0) {
                let d = law::direct_impact(w, pid, &w.statutes[ix]) / daily;
                let l = &mut w.persons[i].mind.lived[li];
                l.direct = d;
                l.known = true;
            }
            let l = w.persons[i].mind.lived[li];
            // How their life has gone since it took effect counts for most while the change is fresh.
            let fresh = 1.0 / (1.0 + (day - l.since).max(0) as f32 / 14.0);
            let lived = (DIRECT * l.direct + l.hit + 6.0 * (total - l.wb_at) * fresh).tanh();
            let op = match w.persons[i].opinion(sid) {
                Some(o) => o + DRIFT * (lived - o),
                None => foresee(w, pid, sid).unwrap_or(lived),
            }.clamp(-1.0, 1.0);
            let p = &mut w.persons[i];
            p.set_opinion(sid, op);
            p.mind.lived[li].hit *= 0.9;
            tally[k].0 += op;
            tally[k].1 += 1;
        }
    }
    for (k, &(_, ix)) in active.iter().enumerate() {
        if tally[k].1 > 0 { w.statutes[ix].stats.approval = tally[k].0 / tally[k].1 as f32; }
    }
}
