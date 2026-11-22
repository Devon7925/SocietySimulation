//! Enforcement: who notices an offence, who reports it, which enforcer acts on
//! it, who judges it and what is done about it. Each link is a person deciding
//! by their own lights (see `crate::decide`), so a law is only as strong as
//! the willingness of the people asked to uphold it.
//!
//! OWNER: enforce agent. Contract in docs/DESIGN.md §Enforcement.
use crate::statute::*;
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnforceState {}
/// Per-person state: for enforcers, their tally toward quotas and so on.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LegalState {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trail {
    /// Done in the open: seen by whoever is there.
    Seen,
    /// Done behind closed doors: seen only by those present inside, found by searches.
    Private,
    /// Leaves no witness but a hole in the books: found by clerks and registers.
    Paper,
}

/// An offence that has just been committed.
#[derive(Clone, Debug)]
pub struct Offence {
    pub offender: PersonId,
    pub statute: StatuteId,
    pub clause: usize,
    pub place: PlaceId,
    pub trail: Trail,
    /// What the offender gained, in coins.
    pub gain: f32,
    /// Value of goods or property involved (for confiscation and fines that depend on it).
    pub stake: f32,
    /// "buying ale", "being out after curfew"...
    pub what: String,
    /// The sanction the clause prescribes, with the fine already evaluated.
    pub sanction: Sanction,
    pub fine: f32,
    /// Someone directly harmed (the robbed stallholder), who has their own reasons to report.
    pub victim: Option<PersonId>,
}

/// The chance, as the would-be offender sees it, that doing this here and now leads to punishment.
pub fn detection_chance(_w: &World, _offender: PersonId, _sid: StatuteId, _place: PlaceId, _trail: Trail) -> f32 { 0.05 }
/// An offence has happened. Run the chain: witnesses, reports, an enforcer's
/// choice, bribery, judgment, punishment. Returns true if the offender was
/// stopped in the act (so the act itself did not go ahead).
pub fn report_offence(_w: &mut World, _off: Offence) -> bool { false }
/// Carry out a sanction (fine, jail, confiscation, exile, execution, status change, sterilisation).
pub fn punish(_w: &mut World, _p: PersonId, _sid: StatuteId, _sanction: &Sanction, _fine: f32, _reason: &str, _by: Option<PersonId>) {}
/// Free a prisoner (end of sentence, pardon, jailbreak).
pub fn release(_w: &mut World, _p: PersonId, _why: &str) {}

/// Apply constitutional clauses about enforcement (called by `law::enact` / `law::repeal`).
pub fn apply_unit(_w: &mut World, _jur: JurId, _def: &UnitDef, _source: Option<StatuteId>) {}
pub fn apply_instrument(_w: &mut World, _jur: JurId, _def: &InstrumentDef, _source: Option<StatuteId>) {}
pub fn set_clerks(_w: &mut World, _jur: JurId, _count: u32, _wage: f32) {}
/// Disband a unit by name. True if one was found.
pub fn disband(_w: &mut World, _name: &str) -> bool { false }

/// Enforcers of units whose writ runs at `place` and who are on duty now.
pub fn on_duty_at(_w: &World, _place: PlaceId) -> Vec<PersonId> { Vec::new() }
/// Where an enforcer on shift should be standing now (their unit's posting for this hour), if on shift.
/// `life` walks them there with `Activity::Patrol`.
pub fn post_for(_w: &mut World, _enforcer: PersonId) -> Option<PlaceId> { None }
/// All enforcers on the payroll of `jur` and its descendants.
pub fn enforcers_of(_w: &World, _jur: JurId) -> Vec<PersonId> { Vec::new() }
/// The armed force an enforcer brings to a confrontation.
pub fn force_of(_w: &World, _enforcer: PersonId) -> f32 { 1.0 }

/// Morning: staffing and pay rules of units, quotas, bounties, inspectors, home searches, releases.
pub fn start_of_day(_w: &mut World) {}
/// Every slot: postings and patrols; checkpoints and watchposts; reports waiting to be acted on.
pub fn on_slot(_w: &mut World) {}
