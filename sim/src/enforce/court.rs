//! How guilt is decided: by the arresting enforcer, by a judge, or by a jury
//! drawn by lot. Judges and jurors are townspeople, and decide as such.
use super::chain::Case;
use crate::polity::Actor;
use crate::statute::*;
use crate::types::*;
use crate::{decide, scope};

/// A judge lets the accused go only if they reject the statute strongly or care for the accused.
const JUDGE_BAR: f32 = 0.5;
/// A juror needs less: any real dislike of the statute, or fellow-feeling for the accused.
const JUROR_BAR: f32 = 0.25;
const WOBBLE: f32 = 0.08;

/// How far someone sitting in judgment leans toward letting the accused go.
/// (Local psychology in the style of `decide.rs`; it belongs there.) They
/// lean that way when they think the statute wrong, when the punishment would
/// hurt someone they care about, and - if there is nothing behind the charge -
/// by as much as fairness matters to them.
fn lenience(w: &World, judge: PersonId, accused: PersonId, sid: StatuteId, severity: f32, baseless: bool) -> f32 {
    let p = w.person(judge);
    let daily = decide::daily_scale(w, judge);
    let op = decide::opinion(w, judge, sid);
    let harm = decide::regard(w, judge, accused) * (severity / daily).min(6.0);
    let doubt = if baseless { 0.25 + 0.75 * p.prefs[Pref::Fairness] } else { 0.0 };
    (-op).max(0.0) - 0.3 * op.max(0.0) + harm + doubt
}

fn would_convict(w: &mut World, judge: PersonId, accused: PersonId, sid: StatuteId, severity: f32, baseless: bool, bar: f32) -> bool {
    lenience(w, judge, accused, sid, severity, baseless) < bar + w.rng.range(-WOBBLE, WOBBLE)
}

fn convict_propensity(w: &World, judge: PersonId, accused: PersonId, sid: StatuteId, severity: f32, bar: f32) -> f32 {
    ((bar + WOBBLE - lenience(w, judge, accused, sid, severity, false)) / (2.0 * WOBBLE)).clamp(0.0, 1.0)
}

fn seated(w: &World, jur: JurId, office: &str) -> Option<Actor> {
    w.polity.office_by_name(jur, office).and_then(|o| w.polity.office(o).holder)
}

/// The chance, as an offender would reckon it, that the court of `jur` convicts on a true charge.
pub(super) fn estimate(w: &World, jur: JurId, accused: PersonId, sid: StatuteId, severity: f32) -> f32 {
    match &w.polity.jur(jur).court {
        CourtDef::Summary => 1.0,
        CourtDef::Judge { office } => match seated(w, jur, office) {
            Some(Actor::Player) => 1.0,
            Some(Actor::Person(j)) if j.ix() < w.persons.len() && w.person(j).present && j != accused => convict_propensity(w, j, accused, sid, severity, JUDGE_BAR),
            _ => 0.0,
        },
        // Who will be drawn is unknown; the town's general view of the statute is not.
        CourtDef::Jury { .. } => (0.75 + w.statute(sid).map(|s| s.stats.approval).unwrap_or(0.0)).clamp(0.03, 0.97),
    }
}

/// Hear a case. True if the accused is found guilty.
pub(super) fn hear(w: &mut World, case: &Case, enforcer: PersonId, severity: f32) -> bool {
    let o = &case.offence;
    let accused = o.offender;
    let Some(jur) = w.statute(o.statute).map(|s| s.jur).filter(|j| j.ix() < w.polity.jurisdictions.len()) else { return false };
    let court = w.polity.jur(jur).court.clone();
    let name = w.person(accused).full_name();
    match court {
        CourtDef::Summary => true,
        CourtDef::Judge { office } => match seated(w, jur, &office) {
            // The player's statute, the player's bench: the charge stands.
            Some(Actor::Player) => true,
            Some(Actor::Person(j)) if j.ix() < w.persons.len() && w.person(j).present && j != accused => {
                let guilty = would_convict(w, j, accused, o.statute, severity, case.baseless, JUDGE_BAR);
                w.note(j, "law", format!("Heard the case of {name}, charged with {}. I found the charge {}.", o.what, if guilty { "proved" } else { "not proved" }));
                guilty
            }
            // An empty bench hears nothing, and nobody is judge in their own cause.
            _ => false,
        },
        CourtDef::Jury { size, pool } => {
            let mut pool: Vec<PersonId> = scope::select_people(w, &pool).into_iter().filter(|q| *q != accused && *q != enforcer && w.person(*q).jailed_until.is_none()).collect();
            let n = (size.max(1) as usize).min(pool.len()).min(25);
            if n == 0 { return false; }
            for i in 0..n {
                let j = i + w.rng.index(pool.len() - i);
                pool.swap(i, j);
            }
            let mut guilty = 0;
            for &j in &pool[..n] {
                if would_convict(w, j, accused, o.statute, severity, case.baseless, JUROR_BAR) { guilty += 1; }
            }
            let convicted = guilty * 2 > n;
            for &j in &pool[..n] {
                w.note(j, "law", format!("Sat on the jury for {name}, charged with {}. We found the charge {}.", o.what, if convicted { "proved" } else { "not proved" }));
            }
            convicted
        }
    }
}
