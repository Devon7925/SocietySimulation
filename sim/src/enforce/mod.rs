//! Enforcement: who notices an offence, who reports it, which enforcer acts on
//! it, who judges it and what is done about it. Each link is a person deciding
//! by their own lights (see `crate::decide`), so a law is only as strong as
//! the willingness of the people asked to uphold it.
//!
//! OWNER: enforce agent. Contract in docs/DESIGN.md §Enforcement.
//!
//! Layout:
//!  * `units`       - the watch: founding, pay, shifts, postings, supervision;
//!  * `instruments` - informers, checkpoints, registers, searches, watchposts, inspectors;
//!  * `detect`      - what an offender expects, built from the same parts as the real chain;
//!  * `chain`       - the real chain: witnesses, reports, the enforcer's choice, bribes;
//!  * `court`       - summary judgment, judges and juries;
//!  * `punish`      - carrying out sanctions, jail and release;
//!  * `money`       - the few places coins move.
use crate::statute::*;
use crate::types::*;
use serde::{Deserialize, Serialize};

mod chain;
mod court;
mod detect;
mod instruments;
mod money;
mod punish;
#[cfg(test)]
mod tests;
mod units;

pub use chain::Case;
pub use detect::{clerks_of, paper_chance, register_of};
pub use instruments::inspectors_of;
pub use units::{apply_unit_with_firm, direct, eligible_for, unit_of};

/// What enforcement did in one day.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tally {
    /// Offences handed to `report_offence`.
    pub offences: u32,
    /// Offences someone told the watch about.
    pub reports: u32,
    pub punished: u32,
    pub acquitted: u32,
    pub bribes: u32,
    /// Times an enforcer or clerk saw an offence and let it pass.
    pub ignored: u32,
    /// Charges brought against people who had done nothing.
    pub false_charges: u32,
    pub searches: u32,
    pub checks: u32,
    /// Coins taken in fines and forfeits.
    pub fines: f32,
}

/// A commander's order to a unit: stand here until further notice.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Directive {
    pub unit: UnitId,
    pub place: PlaceId,
    /// Absolute slot at which the order lapses.
    pub until: u32,
}

/// Lookup tables rebuilt each morning and whenever units, instruments or the statute book change.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub ok: bool,
    pub n_statutes: usize,
    /// For each unit (by `UnitId`), the places it may be posted to.
    pub posts: Vec<Vec<PlaceId>>,
    /// For each place: bit 1 = a checkpoint, bit 2 = a watchpost.
    pub marks: Vec<u8>,
    pub any_marks: bool,
    /// Rules a check can test on the spot: being out of doors, being at this place.
    pub standing: Vec<(StatuteId, usize)>,
    /// Every rule that forbids something: what a charge can be brought under.
    pub chargeable: Vec<(StatuteId, usize)>,
    /// Some unit pays by results (bounty, fine share or quota).
    pub by_results: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnforceState {
    /// Reports waiting for an enforcer to act on them.
    pub pending: Vec<Case>,
    /// Offences nobody has acted on that could still come to light: things done
    /// behind closed doors in recent days (found by searches) and goods carried
    /// today (found at checkpoints).
    pub latent: Vec<Case>,
    /// Where offences were lately reported or stopped: (place, slot).
    pub hot: Vec<(PlaceId, u32)>,
    pub directed: Vec<Directive>,
    /// The clerks' office of each jurisdiction that has one.
    pub clerks: Vec<(JurId, FirmId)>,
    /// The inspectors' office of each jurisdiction that has one.
    pub inspectors: Vec<(JurId, FirmId)>,
    pub prisoners: Vec<PersonId>,
    pub today: Tally,
    pub yesterday: Tally,
    pub cache: Cache,
    /// A bribe is being offered: the chain must not open another negotiation inside it.
    #[serde(skip)]
    pub bribing: bool,
    /// Depth of nested duty-to-inform handling.
    #[serde(skip)]
    pub nested: u8,
}

/// Per-person state: for enforcers, their tally toward quotas and so on.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LegalState {
    /// Convictions this enforcer has brought since the week began.
    pub cases_week: u32,
    /// Bounties and fine shares earned and not yet paid (settled each morning).
    pub bonus_due: f32,
    pub bribes_taken: u32,
    /// Charges they brought against people who had done nothing.
    pub false_charges: u32,
    /// Times a superior caught them looking away.
    pub reprimands: u32,
    /// Where they are posted for this stretch of their shift, and until when.
    pub post: Option<PlaceId>,
    pub post_until: u32,
    /// Slot at which their present spell in jail began.
    pub jailed_since: Option<u32>,
    /// The checkpoint or watchpost they are standing at, and whether they have been checked there.
    pub at_mark: Option<PlaceId>,
    pub checked: bool,
    /// The statute they were last charged under, and the day.
    pub last_charge: Option<(StatuteId, i32)>,
    /// The last day on which being stopped by the watch went in their diary.
    pub stopped_day: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trail {
    /// Done in the open: seen by whoever is there.
    Seen,
    /// Done behind closed doors: seen only by those present inside, found by searches.
    Private,
    /// Leaves no witness but a hole in the books: found by clerks and registers.
    Paper,
}

/// An offence that has just been committed.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub fn detection_chance(w: &World, offender: PersonId, sid: StatuteId, place: PlaceId, trail: Trail) -> f32 { detect::detection_chance(w, offender, sid, place, trail) }
/// An offence has happened. Run the chain: witnesses, reports, an enforcer's
/// choice, bribery, judgment, punishment. Returns true if the offender was
/// stopped in the act (so the act itself did not go ahead).
pub fn report_offence(w: &mut World, off: Offence) -> bool { chain::report_offence(w, off) }
/// Carry out a sanction (fine, jail, confiscation, exile, execution, status change, sterilisation).
pub fn punish(w: &mut World, p: PersonId, sid: StatuteId, sanction: &Sanction, fine: f32, reason: &str, by: Option<PersonId>) {
    punish::carry_out(w, p, sid, sanction, fine, reason, by, 0.0, false);
}
/// Free a prisoner (end of sentence, pardon, jailbreak).
pub fn release(w: &mut World, p: PersonId, why: &str) { punish::release(w, p, why) }

/// Apply constitutional clauses about enforcement (called by `law::enact` / `law::repeal`).
pub fn apply_unit(w: &mut World, jur: JurId, def: &UnitDef, source: Option<StatuteId>) { units::apply_unit_with_firm(w, jur, def, source, None); }
pub fn apply_instrument(w: &mut World, jur: JurId, def: &InstrumentDef, source: Option<StatuteId>) { instruments::apply(w, jur, def, source) }
pub fn set_clerks(w: &mut World, jur: JurId, count: u32, wage: f32) { units::set_clerks(w, jur, count, wage) }
/// Disband a unit by name. True if one was found.
pub fn disband(w: &mut World, name: &str) -> bool { units::disband(w, name) }

/// Enforcers of units whose writ runs at `place` and who are on duty now.
pub fn on_duty_at(w: &World, place: PlaceId) -> Vec<PersonId> { units::on_duty_at(w, place) }
/// Where an enforcer on shift should be standing now (their unit's posting for this hour), if on shift.
/// `life` walks them there with `Activity::Patrol`.
pub fn post_for(w: &mut World, enforcer: PersonId) -> Option<PlaceId> { units::post_for(w, enforcer) }
/// All enforcers on the payroll of `jur` and its descendants.
pub fn enforcers_of(w: &World, jur: JurId) -> Vec<PersonId> { units::enforcers_of(w, jur) }
/// The armed force an enforcer brings to a confrontation.
pub fn force_of(w: &World, enforcer: PersonId) -> f32 { units::force_of(w, enforcer) }

/// Morning: staffing and pay rules of units, quotas, bounties, inspectors, home searches, releases.
pub fn start_of_day(w: &mut World) {
    w.enforce.yesterday = std::mem::take(&mut w.enforce.today);
    instruments::rebuild(w);
    units::sync(w);
    units::settle_pay(w);
    punish::morning(w);
    chain::forget(w);
    instruments::run_searches(w);
}
/// Every slot: postings and patrols; checkpoints and watchposts; reports waiting to be acted on.
pub fn on_slot(w: &mut World) {
    if !w.enforce.cache.ok || w.enforce.cache.n_statutes != w.statutes.len() { instruments::rebuild(w); }
    if !w.enforce.prisoners.is_empty() { punish::tick(w); }
    if !w.enforce.pending.is_empty() { chain::resolve_pending(w); }
    if w.enforce.cache.any_marks { instruments::scan_marked(w); }
    if w.polity.units.is_empty() { return; }
    if w.slot.is_multiple_of(SLOTS_PER_HOUR) { units::presence(w); }
    if w.enforce.cache.by_results && w.slot % 8 == 5 { chain::consider_charges(w); }
}
