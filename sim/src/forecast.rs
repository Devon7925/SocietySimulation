//! Foresight. People judge things they have not lived through by a forecast:
//! the world is forked, the change applied, both branches run forward, and
//! each person's wellbeing compared. How (and whether) this is done is a
//! world setting, so different notions of foresight can be tried.
use crate::polity::{Actor, ForecastSummary};
use crate::statute::Statute;
use crate::types::*;
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

/// Forecast a change under the world's `Settings::forecast` mode. Inside a
/// fork this never forks again: it falls back to first-order effects.
pub fn forecast(_w: &World, _change: &Change) -> Forecast { Forecast::default() }
/// Like `forecast`, but returns None when today's budget of forks is spent
/// (callers should then fall back to `law::direct_impact` or wait a day).
pub fn try_forecast(w: &mut World, change: &Change) -> Option<Forecast> { Some(forecast(w, change)) }
/// Keep a forecast so that opinions about `sid` can be formed from it.
pub fn store(w: &mut World, sid: StatuteId, f: Forecast) {
    w.forecast.stored.retain(|(id, _)| *id != sid);
    w.forecast.stored.push((sid, f));
    if w.forecast.stored.len() > 40 { w.forecast.stored.remove(0); }
}
pub fn stored(w: &World, sid: StatuteId) -> Option<&Forecast> { w.forecast.stored.iter().find(|(id, _)| *id == sid).map(|(_, f)| f) }
