//! The standing institutions of government: jurisdictions, offices, bodies,
//! lawmaking procedures, enforcement units and bills in progress.
//!
//! Nothing here is special to the player. The player begins as the holder of
//! one office and is subject to the same rules as anyone else who holds one:
//! they can be outvoted, replaced when a term ends, or overthrown.
use crate::statute::*;
use crate::types::*;
use serde::{Deserialize, Serialize};

/// Someone who can hold office and sponsor bills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Actor {
    Player,
    Person(PersonId),
}

/// A territory with its own statute book, treasury and institutions. The town
/// is the root; each ward is a child. A statute binds acts done within its
/// jurisdiction's places, and is enforced only by units of that jurisdiction
/// or an ancestor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Jurisdiction {
    pub id: JurId,
    pub name: String,
    pub parent: Option<JurId>,
    pub treasury: f32,
    /// How ordinary statutes are made here. A ward with no proposers has no
    /// legislature of its own and is governed entirely from above.
    pub procedure: ProcedureDef,
    /// How offices, bodies and procedures themselves are changed.
    pub amend: ProcedureDef,
    pub court: CourtDef,
    pub revenue_today: f32,
    pub spending_today: f32,
    /// Coins it owed today and could not pay.
    pub shortfall_today: f32,
    /// Public amenity built up by public works, 0..=100.
    pub amenity: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Office {
    pub id: OfficeId,
    pub jur: JurId,
    pub def: OfficeDef,
    pub holder: Option<Actor>,
    pub since_day: i32,
    /// Day on which the holder's term ends and the selection rule runs again.
    pub term_ends: Option<i32>,
    pub abolished: bool,
    pub created_by: Option<StatuteId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Body {
    pub id: BodyId,
    pub jur: JurId,
    pub def: BodyDef,
    pub members: Vec<Actor>,
    pub term_ends: Option<i32>,
    pub abolished: bool,
    pub created_by: Option<StatuteId>,
}

/// An enforcement unit. Its people are employees of the `Watch` firm `firm`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub jur: JurId,
    pub def: UnitDef,
    pub firm: FirmId,
    pub disbanded: bool,
    pub created_by: Option<StatuteId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instrument {
    pub jur: JurId,
    pub def: InstrumentDef,
    pub created_by: Option<StatuteId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BillState {
    /// Waiting for the current step to open.
    Queued,
    /// A body or the people are voting; polls close at `closes` (absolute slot).
    Voting { closes: u32, yes: f32, no: f32, voted: Vec<PersonId> },
    /// Waiting on an office holder's assent or veto.
    Assent { office: OfficeId, due: u32 },
    Passed,
    Failed { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillKind { Enact, Repeal }

/// What a forecast says a change would do, in brief.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ForecastSummary {
    pub horizon_days: u32,
    /// Mean change in wellbeing across adults.
    pub mean_delta: f32,
    pub share_better: f32,
    pub share_worse: f32,
    /// Plain-language observations ("food price +12%", "treasury -40 a day").
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bill {
    pub id: BillId,
    pub kind: BillKind,
    /// The draft to enact, or (for a repeal) a copy of the statute to strike.
    pub statute: Statute,
    pub target: Option<StatuteId>,
    pub sponsor: Actor,
    pub jur: JurId,
    /// True if it must follow the amending procedure.
    pub constitutional: bool,
    /// Index of the current step in the procedure.
    pub step: usize,
    pub state: BillState,
    pub introduced_day: i32,
    /// What happened to it so far, in plain words.
    pub history: Vec<String>,
    pub forecast: Option<ForecastSummary>,
}

/// Choices the player makes for the townsperson they are playing as. `None`
/// leaves the person to decide for themselves.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Overrides {
    /// How to vote on each open bill.
    pub votes: Vec<(BillId, bool)>,
    /// Whether to stand in the next election for an office (by name).
    pub stand: Vec<(String, bool)>,
    pub protest: Option<bool>,
    pub rebel: Option<bool>,
    pub emigrate: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayerState {
    /// The townsperson the player is living as, if any. While this is set the
    /// player has that person's rights and no others.
    pub possessing: Option<PersonId>,
    pub overrides: Overrides,
    /// Times the player has been removed from office.
    pub deposed: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Polity {
    pub jurisdictions: Vec<Jurisdiction>,
    /// The root jurisdiction (the town).
    pub town: JurId,
    /// The ward jurisdictions, in a fixed order.
    pub wards: Vec<JurId>,
    pub offices: Vec<Office>,
    pub bodies: Vec<Body>,
    pub units: Vec<Unit>,
    pub instruments: Vec<Instrument>,
    pub bills: Vec<Bill>,
    pub player: PlayerState,
    pub next_bill: u32,
}

impl Polity {
    pub fn jur(&self, id: JurId) -> &Jurisdiction { &self.jurisdictions[id.ix()] }
    pub fn jur_mut(&mut self, id: JurId) -> &mut Jurisdiction { &mut self.jurisdictions[id.ix()] }
    pub fn office(&self, id: OfficeId) -> &Office { &self.offices[id.ix()] }
    pub fn office_mut(&mut self, id: OfficeId) -> &mut Office { &mut self.offices[id.ix()] }
    pub fn body(&self, id: BodyId) -> &Body { &self.bodies[id.ix()] }
    pub fn body_mut(&mut self, id: BodyId) -> &mut Body { &mut self.bodies[id.ix()] }
    /// Find a jurisdiction by name; "" means the town.
    pub fn jur_by_name(&self, name: &str) -> Option<JurId> {
        if name.trim().is_empty() { return Some(self.town); }
        self.jurisdictions.iter().find(|j| j.name.eq_ignore_ascii_case(name.trim())).map(|j| j.id)
    }
    /// A live office by name, searching `jur` first and then its ancestors.
    pub fn office_by_name(&self, jur: JurId, name: &str) -> Option<OfficeId> {
        let mut cur = Some(jur);
        while let Some(j) = cur {
            if let Some(o) = self.offices.iter().find(|o| !o.abolished && o.jur == j && o.def.name.eq_ignore_ascii_case(name.trim())) { return Some(o.id); }
            cur = self.jurisdictions[j.ix()].parent;
        }
        self.offices.iter().find(|o| !o.abolished && o.def.name.eq_ignore_ascii_case(name.trim())).map(|o| o.id)
    }
    pub fn body_by_name(&self, jur: JurId, name: &str) -> Option<BodyId> {
        let mut cur = Some(jur);
        while let Some(j) = cur {
            if let Some(b) = self.bodies.iter().find(|b| !b.abolished && b.jur == j && b.def.name.eq_ignore_ascii_case(name.trim())) { return Some(b.id); }
            cur = self.jurisdictions[j.ix()].parent;
        }
        self.bodies.iter().find(|b| !b.abolished && b.def.name.eq_ignore_ascii_case(name.trim())).map(|b| b.id)
    }
    /// `jur` and its ancestors, nearest first.
    pub fn chain(&self, jur: JurId) -> Vec<JurId> {
        let mut out = vec![jur];
        let mut cur = self.jurisdictions[jur.ix()].parent;
        while let Some(j) = cur { out.push(j); cur = self.jurisdictions[j.ix()].parent; }
        out
    }
    /// Is `inner` the same as, or inside, `outer`?
    pub fn within(&self, inner: JurId, outer: JurId) -> bool { self.chain(inner).contains(&outer) }
    /// Offices currently held by an actor.
    pub fn offices_of(&self, a: Actor) -> Vec<OfficeId> { self.offices.iter().filter(|o| !o.abolished && o.holder == Some(a)).map(|o| o.id).collect() }
    pub fn bodies_of(&self, a: Actor) -> Vec<BodyId> { self.bodies.iter().filter(|b| !b.abolished && b.members.contains(&a)).map(|b| b.id).collect() }
    pub fn has_power(&self, a: Actor, jur: JurId, power: Power) -> bool {
        self.offices.iter().any(|o| !o.abolished && o.holder == Some(a) && o.def.powers.contains(&power) && self.within(jur, o.jur))
    }
    /// The actor the player currently acts as for civic purposes.
    pub fn player_actor(&self) -> Actor {
        match self.player.possessing { Some(p) => Actor::Person(p), None => Actor::Player }
    }
}
