//! The statute book in motion: checking and describing drafts, enacting and
//! repealing, deciding whether an act is lawful, taxes and transfers, legal
//! statuses, and event rules.
//!
//! OWNER: law agent. Contract in docs/DESIGN.md §Law.
use crate::statute::*;
use crate::types::*;
use serde::{Deserialize, Serialize};

/// Lookup tables over the active statutes, rebuilt by `rebuild`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LawState {}
/// Per-person fiscal state (income they have chosen to hide, what they owe).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FiscalState {}

/// The outcome of someone setting out to do something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No rule forbids it.
    Lawful,
    /// Forbidden, and they chose to obey. The act does not happen.
    Refrained(StatuteId),
    /// Forbidden; they did it anyway and were not stopped. The act happens.
    Violated(StatuteId),
    /// Forbidden; they tried and were stopped in the act. The act does not happen.
    Stopped(StatuteId),
}
impl Verdict {
    /// Does the act go ahead?
    pub fn proceeds(self) -> bool { matches!(self, Verdict::Lawful | Verdict::Violated(_)) }
}

/// A duty that currently binds a person (from a `Mode::Require` conduct clause).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duty { pub statute: StatuteId, pub clause: usize }

/// Legal bounds on a price for one buyer / tenant / worker.
#[derive(Clone, Copy, Debug)]
pub struct Bounds { pub min: f32, pub max: f32, pub statute: StatuteId, pub clause: usize }

// ---- drafting
/// Check a draft. Returns problems in words its author can act on; empty means valid.
pub fn validate(_w: &World, _s: &Statute) -> Vec<String> { Vec::new() }
/// One plain-English line per clause, saying exactly what it does mechanically.
pub fn describe(_w: &World, _s: &Statute) -> Vec<String> { Vec::new() }
/// The reference manual handed to whoever drafts statutes: the format, every clause, every variable.
pub fn manual() -> String { String::new() }

// ---- the statute book
/// Put a statute into force: assign its id, apply its one-off effects
/// (property, offices, units, minting, repeals...), index it, chronicle it.
pub fn enact(_w: &mut World, _s: Statute, _passed_by: &str) -> StatuteId { StatuteId(0) }
pub fn repeal(_w: &mut World, _id: StatuteId, _how: &str) -> bool { false }
/// Reserve the next statute id (bills are given their id when introduced, so opinions can refer to them).
pub fn next_id(w: &mut World) -> StatuteId {
    let max = w.statutes.iter().map(|s| s.id.0).chain(w.polity.bills.iter().map(|b| b.statute.id.0)).max().unwrap_or(0);
    StatuteId(max + 1)
}
/// Rebuild the lookup tables (after enactment, repeal, load).
pub fn rebuild(_w: &mut World) {}
/// Does this statute bind acts done at this place?
pub fn applies_at(_w: &World, _sid: StatuteId, _place: PlaceId) -> bool { true }

// ---- conduct
/// A person is about to do something. Records it as a wish, finds any rule
/// that forbids it here and now, lets the person choose (`decide::would_break`),
/// and hands offences to `enforce::report_offence`.
pub fn attempt(_w: &mut World, _p: PersonId, _act: &Act) -> Verdict { Verdict::Lawful }
/// Like `attempt` but for someone who will not knowingly break the law (state firms, the scrupulous).
pub fn permitted(_w: &World, _p: PersonId, _act: &Act) -> bool { true }
/// The `Require` rule, if any, that binds this person to `action` right now.
pub fn duty(_w: &World, _p: PersonId, _action: Action) -> Option<Duty> { None }
/// The person has decided whether to fulfil a duty. Call once per occasion:
/// records refusals as offences (returns true if they were stopped / punished on the spot).
pub fn duty_outcome(_w: &mut World, _p: PersonId, _duty: Duty, _fulfilled: bool, _cost: f32) -> bool { false }

// ---- money
/// Pay a person. Withholds income taxes (unless they are hiding this income),
/// credits their household, records it. Returns the net amount received.
pub fn pay_income(_w: &mut World, _p: PersonId, _amount: f32, _kind: IncomeKind) -> f32 { 0.0 }
/// The marginal tax rate this person expects on another coin of wages.
pub fn expected_wage_tax(_w: &World, _p: PersonId, _wage: f32) -> f32 { 0.0 }
/// Collect sales tax on a sale by `firm`; returns the tax taken (0 if the seller hides it).
pub fn sale_tax(_w: &mut World, _firm: FirmId, _good: Good, _value: f32) -> f32 { 0.0 }
pub fn sale_tax_rate(_w: &World, _firm: FirmId, _good: Good) -> f32 { 0.0 }
/// Import (true) or export duty rate on a good, as a fraction.
pub fn tariff_rate(_w: &World, _import: bool, _good: Good) -> f32 { 0.0 }
pub fn collect_tariff(_w: &mut World, _import: bool, _good: Good, _value: f32) -> f32 { 0.0 }
pub fn inheritance_tax(_w: &mut World, _deceased: PersonId, _estate: f32) -> f32 { 0.0 }
/// Legal price limits that apply to `who` paying / being paid `price` for `target`.
pub fn price_bounds(_w: &World, _target: Priced, _who: PersonId, _price: f32) -> Option<Bounds> { None }
/// Share of a purchase the treasury would cover for this buyer (0..=1).
pub fn subsidy_rate(_w: &World, _who: PersonId, _target: Priced) -> f32 { 0.0 }
/// Draw a subsidy on a purchase worth `gross`; returns the coins the treasury covered.
pub fn subsidy(_w: &mut World, _who: PersonId, _target: Priced, _gross: f32) -> f32 { 0.0 }
/// Spend from a treasury (never below zero). Returns what was actually paid.
pub fn treasury_pay(_w: &mut World, _jur: JurId, _amount: f32, _statute: Option<StatuteId>) -> f32 { 0.0 }
pub fn treasury_collect(_w: &mut World, _jur: JurId, _amount: f32, _statute: Option<StatuteId>) {}

// ---- time
/// Morning: scheduled event rules, status gains and losses, transfers, the
/// mint; on the first day of the week, audits, evasion choices and weekly levies.
pub fn start_of_day(_w: &mut World) {}
/// Event rules scheduled for this hour.
pub fn on_slot(_w: &mut World) {}
/// Night: fold today's offence / compliance counts into each statute's flout rate.
pub fn end_of_day(_w: &mut World) {}
/// Announce that something happened to someone, so event rules can fire.
pub fn emit(_w: &mut World, _event: EventKind, _subject: PersonId) {}

// ---- what a rule means to a person
/// What holding a status is worth to this person, in coins a day.
pub fn status_value(_w: &World, _p: PersonId, _status: &str) -> f32 { 0.0 }
/// The direct, first-order effect of a statute on this person in coins a day:
/// taxes they would pay, transfers they would get, wishes it would forbid.
/// No second-order effects - those come from forecasts.
pub fn direct_impact(_w: &World, _p: PersonId, _s: &Statute) -> f32 { 0.0 }
