//! Official assessments: the one way the state can learn anything about what
//! people are like inside, and never more than roughly.
//!
//! A statute may establish a test of one disposition. Clerks examine those it
//! covers; each examination costs the treasury clerk-time, intrudes on the
//! person examined, and records a score that is the truth blurred by error.
//! Later statutes can read the score. Nothing else in the law can see
//! preferences at all.
use crate::enforce::{self, Offence, Trail};
use crate::statute::*;
use crate::types::*;
use crate::{decide, law, scope};

/// Examinations one clerk can conduct in a day.
const TESTS_PER_CLERK: u32 = 6;

/// How far off a test is likely to be (standard deviation of its error), given
/// what is spent on it and how good the town's record-keeping is.
pub fn error_sd(w: &World, cost: f32) -> f32 {
    (0.34 - 0.03 * cost.max(0.0)).clamp(0.07, 0.34) / w.tech[Field::Administration].max(0.5)
}

fn name_ix(w: &mut World, name: &str) -> usize {
    if let Some(i) = w.assessment_names.iter().position(|n| n.eq_ignore_ascii_case(name)) { return i; }
    w.assessment_names.push(name.to_string());
    w.assessment_names.len() - 1
}

/// Morning: clerks work through whoever is due an examination.
pub fn start_of_day(w: &mut World) {
    let due: Vec<(StatuteId, usize, JurId, AssessmentDef)> = w.active_clauses().filter_map(|(s, i, c)| match c { Clause::Assessment(a) => Some((s.id, i, s.jur, a.clone())), _ => None }).collect();
    if due.is_empty() { return; }
    let day = w.day();
    for (sid, clause, jur, def) in due {
        let ix = name_ix(w, &def.name);
        // Clerks employed by this jurisdiction or one above it do the examining.
        let chain = w.polity.chain(jur);
        let clerks: u32 = w.firms.iter().filter(|f| f.kind == FirmKind::Office && f.open && chain.contains(&f.jur)).map(|f| f.workers.len() as u32).sum();
        let mut capacity = clerks * TESTS_PER_CLERK;
        if capacity == 0 { continue; }
        let interval = match def.period { Period::Once => i32::MAX, Period::Day => 1, Period::Week => DAYS_PER_WEEK as i32, Period::Year => DAYS_PER_YEAR as i32 };
        let people: Vec<PersonId> = w.persons.iter().filter(|p| p.present && p.jailed_until.is_none()).map(|p| p.id).collect();
        for pid in people {
            if capacity == 0 { break; }
            let last = w.person(pid).scores.iter().find(|(i, _, _)| *i as usize == ix).map(|(_, _, d)| *d);
            if let Some(d) = last { if interval == i32::MAX || day - d < interval { continue; } }
            // The statute binds only those who live within its jurisdiction.
            let home_ward = w.home_of(pid).map(|h| w.place(h).ward);
            if let Some(ward) = home_ward { if !w.polity.within(ward, jur) { continue; } }
            if !scope::person_bool(w, &def.who, pid, false) { continue; }
            // Being examined is an intrusion; those who mind enough, and dare, refuse.
            let daily = decide::daily_scale(w, pid);
            let mind = w.person(pid).prefs[Pref::Autonomy] * daily * 0.6;
            let fine = scope::person_num(w, &def.sanction.fine, pid, 0.0).max(0.0);
            let cost = decide::sanction_cost(w, pid, &def.sanction, fine);
            let place = w.person(pid).loc;
            let p_caught = 0.9; // a missed appointment is on record by its nature
            if decide::would_break(w, pid, sid, mind, p_caught, cost) {
                if let Some(s) = w.statute_mut(sid) { s.stats.offences += 1; s.stats.today_offences += 1; }
                w.stats.offences += 1;
                enforce::report_offence(w, Offence { offender: pid, statute: sid, clause, place, trail: Trail::Paper, gain: mind, stake: 0.0, what: format!("refusing the {} examination", def.name.replace('_', " ")), sanction: def.sanction.clone(), fine, victim: None });
                continue;
            }
            if law::treasury_pay(w, jur, def.cost, Some(sid)) + 1e-3 < def.cost { capacity = 0; break; } // no coin, no clerks' time
            capacity -= 1;
            let sd = error_sd(w, def.cost);
            let truth = w.person(pid).prefs[def.measures];
            let score = (truth + w.rng.gauss(0.0, sd)).clamp(0.0, 1.0);
            let p = w.person_mut(pid);
            p.scores.retain(|(i, _, _)| *i as usize != ix);
            p.scores.push((ix as u16, score, day));
            p.felt.watched += 1.5;
            if let Some(s) = w.statute_mut(sid) { s.stats.today_refrained += 1; }
            w.note(pid, "law", format!("Examined by the clerks for the {} register.", def.name.replace('_', " ")));
        }
    }
}

/// One plain sentence saying what an assessment clause does.
pub fn describe(w: &World, def: &AssessmentDef) -> String {
    let often = match def.period { Period::Once => "once", Period::Day => "every day", Period::Week => "every week", Period::Year => "every year" };
    format!("Clerks examine everyone for whom `{}` {often}, at {:.1} coins of clerk-time each, and record a score from 0 to 1 for {} as score(\"{}\"); the score is typically off by about {:.2}.", def.who, def.cost, def.measures.name(), def.name, error_sd(w, def.cost))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::*;
    #[test]
    fn sharper_tests_cost_more() {
        let (w, _) = tiny_world();
        assert!(error_sd(&w, 0.5) > error_sd(&w, 5.0));
        assert!(error_sd(&w, 100.0) >= 0.07);
    }
    #[test]
    fn untested_people_have_no_score() {
        let (mut w, t) = tiny_world();
        w.assessment_names.push("temper".into());
        assert!(!scope::person_bool(&w, "score(\"temper\") < 0.5", t.ada, true));
        assert!(!scope::person_bool(&w, "tested(\"temper\")", t.ada, true));
        w.person_mut(t.ada).scores.push((0, 0.2, 0));
        assert!(scope::person_bool(&w, "tested(\"temper\") && score(\"temper\") < 0.5", t.ada, false));
    }
}
