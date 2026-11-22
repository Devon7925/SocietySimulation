//! Government as institutions: offices and bodies and how they are filled,
//! how bills become law, how townspeople in office legislate, and how
//! governments fall.
//!
//! OWNER: gov agent. Contract in docs/DESIGN.md §Government.
use crate::polity::*;
use crate::statute::*;
use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GovState {}
/// Per-person civic state (candidacies, how they voted, protest and revolt intentions).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Civic {
    /// Standing in the plaza in protest right now.
    pub protesting: bool,
    /// In open revolt.
    pub rebelling: bool,
}

// ---- constitution (called by `law::enact`, `worldgen`, and uprisings)
pub fn create_jurisdiction(_w: &mut World, _name: &str, _parent: Option<JurId>) -> JurId { JurId(0) }
/// Create or replace (by name) an office in `jur`.
pub fn apply_office(_w: &mut World, _jur: JurId, _def: &OfficeDef, _source: Option<StatuteId>) -> OfficeId { OfficeId(0) }
pub fn apply_body(_w: &mut World, _jur: JurId, _def: &BodyDef, _source: Option<StatuteId>) -> BodyId { BodyId(0) }
pub fn apply_procedure(_w: &mut World, _jur: JurId, _def: &ProcedureDef) {}
/// Abolish an office or body by name. True if one was found.
pub fn abolish(_w: &mut World, _jur: JurId, _name: &str) -> bool { false }
/// Put someone in an office (or vacate it with None). `how` is for the chronicle: "elected", "appointed"...
pub fn seat(_w: &mut World, _office: OfficeId, _holder: Option<Actor>, _how: &str) {}

// ---- lawmaking
/// May this actor introduce a bill in `jur`? (They hold a proposing office, sit
/// in a proposing body, hold a decree power, or an initiative is open to them.)
pub fn can_propose(_w: &World, _a: Actor, _jur: JurId, _constitutional: bool) -> bool { false }
/// Introduce a bill. With a decree power it becomes law at once; otherwise it
/// starts down the jurisdiction's procedure. Err explains why it cannot be introduced.
pub fn introduce(_w: &mut World, _s: Statute, _sponsor: Actor, _kind: BillKind, _target: Option<StatuteId>) -> Result<BillId, String> { Err("not implemented".into()) }
/// The player (or the person they are playing) answers a pending assent / veto decision.
pub fn player_assent(_w: &mut World, _bill: BillId, _approve: bool) -> Result<(), String> { Ok(()) }
/// Is `p` entitled to vote in the step this bill is at?
pub fn may_vote(_w: &World, _p: PersonId, _bill: BillId) -> bool { false }
/// Record a ballot cast in person at the town hall (called by `life` when someone goes to vote).
pub fn cast_ballots(_w: &mut World, _p: PersonId) {}
/// Is there anything open today that `p` may vote on (bills or elections) and has not yet?
pub fn has_ballot(_w: &World, _p: PersonId) -> bool { false }

// ---- unrest
/// Does this person mean to go and protest today? Called by `life` in the
/// daytime; `life` then runs `law::attempt(Protest)`, walks them to the plaza
/// with `Activity::Protest` and keeps `Civic::protesting` true while they stand there.
/// During an uprising `Civic::rebelling` is set by this module and `life`
/// sends such people to the plaza with `Activity::Rebel`.
pub fn wants_to_protest(_w: &mut World, _p: PersonId) -> bool { false }

// ---- player
/// Start or stop living as a townsperson. Err if not allowed (the player holds office, the person is dead...).
pub fn possess(_w: &mut World, _p: Option<PersonId>) -> Result<(), String> { Ok(()) }

// ---- time
/// Morning: terms ending, selections due, sessions of bodies, townspeople drafting bills, bills advancing.
pub fn start_of_day(_w: &mut World) {}
/// Every slot: polls closing, assents falling due.
pub fn on_slot(_w: &mut World) {}
/// Night: protest and unrest tallies, uprisings and their outcome.
pub fn end_of_day(_w: &mut World) {}
