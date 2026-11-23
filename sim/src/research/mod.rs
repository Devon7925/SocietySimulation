//! Publicly funded research. Money buys scholars' time; what their time
//! yields is uncertain.
//!
//! OWNER: demography agent. Contract in docs/DESIGN.md §Research.
//!
//! Each jurisdiction with a `Clause::Research` in force keeps an academy (a
//! public firm). Its budget sets how many scholars it may employ; the economy
//! hires and pays them. Each morning the previous day's work by scholars is
//! turned into progress, shared among the funded fields in proportion to the
//! coin actually spent on each. When a field's progress reaches the cost of
//! its next step, its `Tech` multiplier rises by a random amount - usually a
//! few percent, now and then much more, sometimes nothing - and the next
//! step costs more. Nothing here knows whether the world is a fork.
use crate::statute::Clause;
use crate::types::*;
use crate::{economy, law};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// A scholar is paid this multiple of the median wage.
pub const SCHOLAR_PREMIUM: f32 = 1.4;
/// Slots of work that make a full scholar's day.
pub const FULL_DAY_SLOTS: f32 = 32.0;
/// Scholar-days the first step in a field costs, and how much dearer each further step is.
pub const STEP_BASE: f32 = 10.0;
pub const STEP_GROWTH: f32 = 1.3;
/// The most that books, instruments and specimens can add to what scholars get done.
pub const MATERIALS_BOOST: f32 = 0.5;

/// The academy a jurisdiction keeps.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Academy {
    pub jur: JurId,
    pub firm: FirmId,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ResearchState {
    /// Work banked toward each field's next step, in able-scholar-days. Indexed by `Field as usize`.
    pub progress: [f32; N_FIELDS],
    /// Steps completed in each field.
    pub level: [u32; N_FIELDS],
    pub academies: Vec<Academy>,
    /// Coins spent on each field at the last reckoning (scholars' wages and materials).
    pub spent: [f32; N_FIELDS],
    /// Scholars whose work counted at the last reckoning.
    pub scholars: u32,
    /// Slots each scholar worked on `worked_day`, noted by `end_of_day` before payroll clears them.
    pub worked: Vec<(PersonId, u32)>,
    pub worked_day: i32,
}

/// Scholar-days of work the next step costs in a field that has taken `level` steps.
pub fn step_cost(level: u32) -> f32 { STEP_BASE * STEP_GROWTH.powi(level.min(60) as i32) }

/// What a scholar earns a day at present.
pub fn scholar_wage(w: &World) -> f32 { SCHOLAR_PREMIUM * w.stats.median_wage.max(4.0) }

/// What a full day of this scholar's work is worth: schooling first, then practice.
pub fn ability(p: &Person) -> f32 { 0.3 + 0.9 * p.education + 0.4 * p.skill }

/// One research clause in force.
struct Line { jur: JurId, field: Field, per_day: f32, statute: StatuteId }

fn is_academy(w: &World, f: FirmId, jur: JurId) -> bool {
    w.firms.get(f.ix()).is_some_and(|x| x.kind == FirmKind::Academy && x.jur == jur)
}

/// The academy `jur` keeps, founding one if `found` and it has none.
fn academy_of(w: &mut World, jur: JurId, found: bool, wage: f32) -> Option<FirmId> {
    if let Some(i) = w.research.academies.iter().position(|a| a.jur == jur) {
        let f = w.research.academies[i].firm;
        if is_academy(w, f, jur) { return Some(f); }
        w.research.academies.remove(i);
    }
    let firm = match w.firms.iter().find(|f| f.kind == FirmKind::Academy && f.jur == jur).map(|f| f.id) {
        Some(f) => f,
        None => {
            if !found { return None; }
            let name = format!("{} Academy", w.polity.jur(jur).name);
            let f = economy::found_public_firm(w, FirmKind::Academy, jur, &name, 0, wage);
            if !is_academy(w, f, jur) { return None; }
            // Scholars work at the academy building, if the town has one.
            if let Some(place) = w.place_of_kind(PlaceKind::Academy) {
                w.firms[f.ix()].place = place;
                if w.places[place.ix()].firm.is_none() { w.places[place.ix()].firm = Some(f); }
            }
            f
        }
    };
    w.research.academies.push(Academy { jur, firm });
    Some(firm)
}

/// The scholars of an academy and the share of a full day each worked on the
/// day now ended. Slots worked are read from the note `end_of_day` took last
/// night if there is one, else from `WorkState::slots_today` if payroll has
/// not cleared it; failing both, from the share of the academy's wage bill
/// its ledger shows was paid for the day, since pay follows hours.
fn days_worked(w: &World, firm: FirmId) -> Vec<(PersonId, f32)> {
    let f = w.firm(firm);
    let staff: Vec<&Person> = f.workers.iter().filter_map(|id| w.persons.get(id.ix()))
        .filter(|p| p.present && p.job.as_ref().is_some_and(|j| j.firm == firm)).collect();
    let noted = w.research.worked_day == w.day() - 1 && !w.research.worked.is_empty();
    let slots = |p: &Person| -> u32 {
        if noted { w.research.worked.iter().find(|(id, _)| *id == p.id).map_or(0, |(_, s)| *s) } else { p.work.slots_today }
    };
    let mut out: Vec<(PersonId, f32)> = staff.iter().map(|p| (p.id, (slots(p) as f32 / FULL_DAY_SLOTS).min(1.0))).collect();
    if !noted && out.iter().all(|(_, d)| *d <= 0.0) {
        let bill: f32 = staff.iter().filter_map(|p| p.job.as_ref()).map(|j| j.wage).sum();
        let paid = f.yesterday.wages.max(f.today.wages);
        let share = if bill > 0.0 { (paid / bill).clamp(0.0, 1.0) } else { 0.0 };
        for d in out.iter_mut() { d.1 = share; }
    }
    out.retain(|(_, d)| *d > 0.0);
    out
}

/// What a completed step did, in words.
fn tidings(field: Field, gain: f32) -> String {
    let name = field.name();
    if gain <= 0.0 { return format!("The scholars finished a long study in {name}. Nothing of use came of it."); }
    let pct = ((gain * 100.0).round() as i32).max(1);
    let less = ((100.0 * gain / (1.0 + gain)).round() as i32).max(1);
    let what = match field {
        Field::Agronomy => format!("A day's work on the farms now yields about {pct}% more food."),
        Field::Forestry => format!("A day's work at the lumberyard now yields about {pct}% more wood."),
        Field::Craft => format!("Workshops and breweries now turn out about {pct}% more for the same work."),
        Field::Medicine => format!("The sick now mend about {pct}% faster, and fewer die of illness and age."),
        Field::Pedagogy => format!("A day at school now teaches about {pct}% more."),
        Field::Building => format!("Building a house now costs about {less}% less."),
        Field::Administration => format!("Clerks and registers now catch about {pct}% more of what is kept from them."),
        Field::Surveillance => format!("Watchers, posts and checkpoints now notice about {pct}% more."),
        Field::Arms => format!("An armed enforcer now brings about {pct}% more force to bear."),
        Field::Commerce => format!("The travelling trader now gives terms about {pct}% better."),
    };
    if gain >= 0.1 { format!("A breakthrough in {name}. {what}") } else { format!("The scholars' work in {name} has borne fruit. {what}") }
}

/// Bank a day's work in a field and take any steps it now pays for.
fn advance(w: &mut World, field: Field, work: f32) {
    let i = field as usize;
    if work <= 0.0 || !work.is_finite() { return; }
    w.research.progress[i] += work;
    loop {
        let cost = step_cost(w.research.level[i]);
        if w.research.progress[i] < cost { break; }
        w.research.progress[i] -= cost;
        w.research.level[i] += 1;
        // Most studies improve practice by a few percent; one in ten changes it; some lead nowhere.
        let roll = w.rng.f32();
        let gain = if roll < 0.15 { 0.0 } else if roll < 0.90 { w.rng.range(0.02, 0.06) } else { w.rng.range(0.12, 0.30) };
        w.tech[field] *= 1.0 + gain;
        w.chronicle("research", tidings(field, gain));
    }
}

/// Morning: staff academies to match funding, pay for the day's work, roll for progress.
pub fn start_of_day(w: &mut World) {
    let n_jurs = w.polity.jurisdictions.len();
    let mut lines: Vec<Line> = Vec::new();
    for (s, _, c) in w.active_clauses() {
        if let Clause::Research { field, per_day } = c {
            if per_day.is_finite() && *per_day > 0.0 && s.jur.ix() < n_jurs { lines.push(Line { jur: s.jur, field: *field, per_day: *per_day, statute: s.id }); }
        }
    }
    let mut jurs: Vec<JurId> = lines.iter().map(|l| l.jur).chain(w.research.academies.iter().map(|a| a.jur)).collect();
    jurs.sort();
    jurs.dedup();

    let wage = scholar_wage(w);
    let mut work = [0.0f32; N_FIELDS];
    let mut spent = [0.0f32; N_FIELDS];
    let mut scholars = 0u32;
    for jur in jurs {
        let budget: f32 = lines.iter().filter(|l| l.jur == jur).map(|l| l.per_day).sum();
        let Some(firm) = academy_of(w, jur, budget > 0.0, wage) else { continue };
        if budget <= 0.0 {
            // Funding has stopped: no posts, and the economy lets the scholars go.
            let f = w.firm_mut(firm);
            f.capacity = 0;
            f.vacancies = 0;
            continue;
        }
        // As many scholars as the budget pays at the going rate for scholars;
        // a budget too small for one offers all of itself to a single post.
        let posts = ((budget / wage).floor() as u32).max(1);
        {
            let f = w.firm_mut(firm);
            f.capacity = posts;
            f.wage_offer = wage.min(budget / posts as f32);
            f.vacancies = f.vacancies.min(posts.saturating_sub(f.workers.len() as u32));
        }

        // The day now ended: who worked, how ably, and what they were paid for it.
        let (mut done, mut wages) = (0.0f32, 0.0f32);
        for (id, share) in days_worked(w, firm) {
            let p = w.person(id);
            done += share * ability(p);
            if p.work.unpaid_days == 0 { wages += share * p.job.as_ref().map_or(0.0, |j| j.wage); }
            scholars += 1;
        }
        if done <= 0.0 { continue; }
        // What the budget leaves after wages buys books, instruments and
        // specimens from beyond the town: coin that leaves the town for good.
        let spare = (budget - wages).max(0.0);
        let mut paid = [0.0f32; N_FIELDS];
        let mut materials = 0.0f32;
        for l in lines.iter().filter(|l| l.jur == jur) {
            let share = l.per_day / budget;
            let got = law::treasury_pay(w, jur, spare * share, Some(l.statute));
            let got = if got.is_finite() { got.clamp(0.0, spare * share) } else { 0.0 };
            paid[l.field as usize] += wages * share + got;
            materials += got;
        }
        let total: f32 = paid.iter().sum();
        if total <= 0.0 { continue; }
        let boost = 1.0 + MATERIALS_BOOST * materials / (materials + wages).max(1e-6);
        for i in 0..N_FIELDS {
            work[i] += done * boost * paid[i] / total;
            spent[i] += paid[i];
        }
    }
    w.research.spent = spent;
    w.research.scholars = scholars;
    w.research.worked.clear();
    for field in ALL_FIELDS { advance(w, field, work[field as usize]); }
}

/// Night, before payroll: note how long each scholar worked today, so the
/// morning's reckoning has it after `WorkState::slots_today` is cleared.
/// Optional: without it `start_of_day` falls back on what it can still see.
pub fn end_of_day(w: &mut World) {
    let mut worked = Vec::new();
    for a in &w.research.academies {
        let Some(f) = w.firms.get(a.firm.ix()) else { continue };
        for id in &f.workers {
            if let Some(p) = w.persons.get(id.ix()) { if p.present { worked.push((p.id, p.work.slots_today)); } }
        }
    }
    w.research.worked = worked;
    w.research.worked_day = w.day();
}
