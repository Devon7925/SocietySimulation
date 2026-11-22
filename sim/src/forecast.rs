//! Foresight. People judge things they have not lived through by a forecast:
//! the world is forked, the change applied, both branches run forward, and
//! each person's wellbeing compared. How (and whether) this is done is a
//! world setting, so different notions of foresight can be tried.
use crate::polity::{Actor, ForecastSummary};
use crate::rng::Rng;
use crate::statute::Statute;
use crate::types::*;
use crate::{decide, engine, gov, law, life};
use serde::{Deserialize, Serialize};

/// Something whose consequences can be forecast.
#[derive(Clone, Debug)]
pub enum Change {
    Enact(Statute),
    Repeal(StatuteId),
    /// A different person holding an office.
    Seat(OfficeId, Option<Actor>),
    /// Several changes together (the programme of an uprising, a candidate's platform).
    Many(Vec<Change>),
}

/// Predicted effect of a change on each person.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Forecast {
    /// Change in total wellbeing for each person, indexed by `PersonId` (0 for the absent).
    pub delta: Vec<f32>,
    pub summary: ForecastSummary,
}
impl Forecast {
    pub fn of(&self, p: PersonId) -> f32 { self.delta.get(p.ix()).copied().unwrap_or(0.0) }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ForecastState {
    /// Number of forks run so far (for profiling and the UI).
    pub forks_run: u32,
    /// Forks run today, against the daily budget.
    pub forks_today: u32,
    /// Forecasts kept for bills and statutes people still have to form a view on.
    pub stored: Vec<(StatuteId, Forecast)>,
}

/// Forks the town may run in one day on behalf of its people. The player's
/// own proposals are always forecast (see `forecast_now`).
pub const DAILY_FORK_BUDGET: u32 = 8;

fn apply(w: &mut World, change: &Change) {
    match change {
        Change::Enact(s) => { law::enact(w, s.clone(), "forecast"); }
        Change::Repeal(id) => { law::repeal(w, *id, "forecast"); }
        Change::Seat(office, who) => gov::seat(w, *office, *who, "forecast"),
        Change::Many(list) => for c in list { apply(w, c) },
    }
}

/// The direct, first-order reading of a change: what it would take from or
/// give to each person on their present record, with no knock-on effects.
fn first_order(w: &World, change: &Change, out: &mut [f32]) {
    match change {
        Change::Enact(s) => for p in w.persons.iter().filter(|p| p.present) {
            out[p.id.ix()] += (law::direct_impact(w, p.id, s) / decide::daily_scale(w, p.id) * 0.5).tanh() * 0.3;
        },
        Change::Repeal(id) => if let Some(s) = w.statute(*id) {
            for p in w.persons.iter().filter(|p| p.present) {
                out[p.id.ix()] -= (law::direct_impact(w, p.id, s) / decide::daily_scale(w, p.id) * 0.5).tanh() * 0.3;
            }
        },
        Change::Seat(..) => {}
        Change::Many(list) => for c in list { first_order(w, c, out) },
    }
}

/// A copy of the world fit to run ahead in: no diaries, no chronicle, no history.
fn fork_of(w: &World, seed: u64) -> World {
    let mut f = w.clone();
    f.depth = w.depth + 1;
    f.rng = Rng::new(seed);
    f.chronicle.clear();
    f.history.clear();
    f.forecast.stored.clear();
    for p in f.persons.iter_mut() { p.diary.clear(); }
    f
}

struct Outcome {
    /// Mean total wellbeing of each person over the later part of the run.
    well: Vec<f32>,
    price: [f32; N_GOODS],
    treasury: f32,
    unemployment: f32,
    population: u32,
    wage: f32,
    offences: f32,
}

fn run(mut f: World, days: u32) -> Outcome {
    let n = f.persons.len();
    let mut well = vec![0.0f32; n];
    let mut samples = 0u32;
    let mut offences = 0.0;
    let warmup = days / 3;
    for d in 0..days {
        for _ in 0..SLOTS_PER_DAY { engine::step(&mut f); }
        offences += f.statutes.iter().map(|s| s.stats.today_offences as f32).sum::<f32>();
        if d >= warmup {
            samples += 1;
            for i in 0..n.min(f.persons.len()) {
                if f.persons[i].present { well[i] += life::wellbeing_now(&f, PersonId(i as u32)).total; }
                else { well[i] += -1.0; } // dead or gone: as bad as it gets
            }
        }
    }
    for x in well.iter_mut() { *x /= samples.max(1) as f32; }
    Outcome { well, price: f.stats.price, treasury: f.polity.jurisdictions.iter().map(|j| j.treasury).sum(), unemployment: f.stats.unemployment, population: f.stats.population, wage: f.stats.median_wage, offences: offences / days.max(1) as f32 }
}

fn pct(a: f32, b: f32) -> f32 { if a.abs() < 1e-6 { 0.0 } else { (b - a) / a * 100.0 } }

fn by_fork(w: &World, change: &Change, horizon: u32, samples: u32) -> Forecast {
    let n = w.persons.len();
    let mut delta = vec![0.0f32; n];
    let mut notes = Vec::new();
    let samples = samples.max(1);
    for s in 0..samples {
        let seed = w.seed ^ (w.slot as u64).wrapping_mul(0x9E37_79B9) ^ (s as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407);
        let base = fork_of(w, seed);
        let mut alt = base.clone();
        apply(&mut alt, change);
        let a = run(base, horizon);
        let b = run(alt, horizon);
        for i in 0..n { delta[i] += (b.well[i] - a.well[i]) / samples as f32; }
        if s == 0 {
            for g in ALL_GOODS {
                let c = pct(a.price[g as usize], b.price[g as usize]);
                if c.abs() >= 4.0 { notes.push(format!("{} price {}{:.0}%", g.name(), if c > 0.0 { "+" } else { "" }, c)); }
            }
            let dw = pct(a.wage, b.wage);
            if dw.abs() >= 3.0 { notes.push(format!("median wage {}{:.0}%", if dw > 0.0 { "+" } else { "" }, dw)); }
            let du = (b.unemployment - a.unemployment) * 100.0;
            if du.abs() >= 2.0 { notes.push(format!("unemployment {}{:.0} points", if du > 0.0 { "+" } else { "" }, du)); }
            let dt = (b.treasury - a.treasury) / horizon.max(1) as f32;
            if dt.abs() >= 1.0 { notes.push(format!("treasury {}{:.1} coins a day", if dt > 0.0 { "+" } else { "" }, dt)); }
            let dp = b.population as i32 - a.population as i32;
            if dp != 0 { notes.push(format!("population {}{}", if dp > 0 { "+" } else { "" }, dp)); }
            let dof = b.offences - a.offences;
            if dof.abs() >= 1.0 { notes.push(format!("offences {}{:.0} a day", if dof > 0.0 { "+" } else { "" }, dof)); }
        }
    }
    finish(w, delta, horizon, notes)
}

fn finish(w: &World, delta: Vec<f32>, horizon: u32, notes: Vec<String>) -> Forecast {
    let adults = w.present_adults();
    let n = adults.len().max(1) as f32;
    let mean = adults.iter().map(|p| delta[p.ix()]).sum::<f32>() / n;
    let better = adults.iter().filter(|p| delta[p.ix()] > 0.01).count() as f32 / n;
    let worse = adults.iter().filter(|p| delta[p.ix()] < -0.01).count() as f32 / n;
    Forecast { delta, summary: ForecastSummary { horizon_days: horizon, mean_delta: mean, share_better: better, share_worse: worse, notes } }
}

/// Forecast a change under the world's `Settings::forecast` mode. Inside a
/// fork this never forks again: it falls back to first-order effects.
pub fn forecast(w: &World, change: &Change) -> Forecast {
    let n = w.persons.len();
    let direct = || { let mut d = vec![0.0f32; n]; first_order(w, change, &mut d); d };
    if w.is_fork() { return finish(w, direct(), 0, Vec::new()); }
    match w.settings.forecast {
        ForecastMode::Experience => finish(w, direct(), 0, vec!["judged by direct effects only".into()]),
        ForecastMode::Fork { horizon_days, samples } => by_fork(w, change, horizon_days.clamp(1, 120), samples.clamp(1, 8)),
        ForecastMode::Blend { horizon_days, samples } => {
            // Each person follows the forecast as far as their schooling lets them and leans on the obvious for the rest.
            let mut f = by_fork(w, change, horizon_days.clamp(1, 120), samples.clamp(1, 8));
            let d = direct();
            for p in w.persons.iter().filter(|p| p.present) {
                let follow = (0.35 + 0.65 * p.education).clamp(0.0, 1.0);
                f.delta[p.id.ix()] = follow * f.delta[p.id.ix()] + (1.0 - follow) * d[p.id.ix()];
            }
            let (horizon, notes) = (f.summary.horizon_days, f.summary.notes);
            finish(w, f.delta, horizon, notes)
        }
    }
}

fn cost(w: &World) -> u32 {
    match w.settings.forecast { ForecastMode::Experience => 0, ForecastMode::Fork { samples, .. } | ForecastMode::Blend { samples, .. } => 2 * samples.clamp(1, 8) }
}

/// Forecast regardless of the daily budget (for the player's own proposals).
pub fn forecast_now(w: &mut World, change: &Change) -> Forecast {
    let f = forecast(w, change);
    if !w.is_fork() { let c = cost(w); w.forecast.forks_run += c; w.forecast.forks_today += c; }
    f
}

/// Like `forecast`, but returns None when today's budget of forks is spent
/// (callers should then fall back to `law::direct_impact` or wait a day).
pub fn try_forecast(w: &mut World, change: &Change) -> Option<Forecast> {
    if w.is_fork() { return Some(forecast(w, change)); }
    let c = cost(w);
    if c > 0 && w.forecast.forks_today + c > DAILY_FORK_BUDGET { return None; }
    Some(forecast_now(w, change))
}

/// Keep a forecast so that opinions about `sid` can be formed from it.
pub fn store(w: &mut World, sid: StatuteId, f: Forecast) {
    w.forecast.stored.retain(|(id, _)| *id != sid);
    w.forecast.stored.push((sid, f));
    if w.forecast.stored.len() > 40 { w.forecast.stored.remove(0); }
}
pub fn stored(w: &World, sid: StatuteId) -> Option<&Forecast> { w.forecast.stored.iter().find(|(id, _)| *id == sid).map(|(_, f)| f) }
