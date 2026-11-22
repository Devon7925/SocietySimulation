//! Firms, markets, wages, rents and the labour market. Prices and wages are
//! never set by rule: each firm nudges its own price toward what clears its
//! stock and hires while one more pair of hands pays for itself.
//!
//! OWNER: economy agent. Contract in docs/DESIGN.md §Economy.
use crate::statute::Priced;
use crate::types::*;
use serde::{Deserialize, Serialize};

/// Town-level economic state (the travelling trader's terms, etc.).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EconState {}
/// Per-firm state private to this module.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FirmEcon {}
/// Per-household state private to this module.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HouseholdEcon {}
/// Per-person working state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkState {
    /// Slots worked today (reset at payroll).
    pub slots_today: u32,
    /// Effective labour this person supplies per slot while working (effort x skill).
    pub effort: f32,
    pub unpaid_days: u32,
}

/// Morning: each firm reviews yesterday, sets prices, decides hiring and
/// lay-offs, buys inputs, sells gluts to the trader, pays out profit. Workers
/// review whether their job is still worth it. Refreshes `w.stats.price`.
pub fn start_of_day(_w: &mut World) {}
/// Every slot: turn `Firm::effort_now` into output, then reset it.
pub fn on_slot(_w: &mut World) {}
/// Night: payroll, rents, household upkeep; weekly housing market and construction.
pub fn end_of_day(_w: &mut World) {}

/// Cheapest price at which a unit of `good` can be had right now (including the trader's import price).
pub fn market_price(_w: &World, good: Good) -> f32 { good.base_price() }
/// A person buys up to `qty` of `good`, cheapest seller first, paying at most
/// `max_price` a unit. Handles `law::attempt(Buy)`, subsidies, sales tax and
/// the seller's side. Goods land in the buyer's household stock. Returns the quantity obtained.
pub fn buy_good(_w: &mut World, _buyer: PersonId, _good: Good, _qty: f32, _max_price: f32) -> f32 { 0.0 }
/// A person pays `firm` for one unit of a service (a drink, a treatment, a day of school). True if served.
pub fn buy_service(_w: &mut World, _buyer: PersonId, _firm: FirmId, _what: Priced) -> bool { false }
/// The effective labour (effort x skill) this person supplies per slot at their job, given
/// their diligence, stake in the firm, supervision, health and how fairly they feel paid.
/// `life` calls this when someone starts a spell of work and stores it in `WorkState::effort`.
pub fn effort_of(_w: &World, _p: PersonId) -> f32 { 0.6 }
/// Staff of `firm` who are on the premises working right now.
pub fn staff_present(_w: &World, _firm: FirmId) -> u32 { 0 }

/// The lowest take-home wage for which this person will work.
pub fn reservation_wage(_w: &World, _p: PersonId) -> f32 { 4.0 }
/// The best open position for this person paying at least `min_net` after tax: (firm, wage, net).
pub fn best_vacancy(_w: &World, _p: PersonId, _min_net: f32) -> Option<(FirmId, f32, f32)> { None }
/// The person asks for the job in person. True if hired (handles wage floors and `law::attempt(Employ)`).
pub fn apply_for(_w: &mut World, _p: PersonId, _firm: FirmId) -> bool { false }
pub fn hire(_w: &mut World, _firm: FirmId, _p: PersonId, _wage: f32) {}
/// Remove a person from their job (quit, dismissal, death). `why` goes in their diary if non-empty.
pub fn leave_job(_w: &mut World, _p: PersonId, _why: &str) {}

/// Change who owns a place, fixing up the firm in it (ownership, manager, pay) and any tenancy.
pub fn set_owner(_w: &mut World, _place: PlaceId, _owner: Owner) {}
/// A household takes up residence (as owner or tenant, according to who owns the place).
pub fn move_in(_w: &mut World, _h: HouseholdId, _place: PlaceId) {}
/// Take goods from a market seller without paying. Returns the quantity taken and the firm robbed.
pub fn steal_goods(_w: &mut World, _thief: PersonId, _good: Good, _qty: f32) -> (f32, Option<FirmId>) { (0.0, None) }
/// Create a public body (watch unit, clerks' office, works crew, academy) paid by `jur`. Returns its firm.
pub fn found_public_firm(_w: &mut World, _kind: FirmKind, _jur: JurId, _name: &str, _capacity: u32, _wage: f32) -> FirmId { FirmId(0) }
/// Build a new house on a free lot for `owner`. Returns it, or None if there is no room.
pub fn build_home(_w: &mut World, _owner: Owner) -> Option<PlaceId> { None }
