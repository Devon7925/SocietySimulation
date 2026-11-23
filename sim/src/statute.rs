//! The law format. A [`Statute`] is a list of [`Clause`]s; this is what the
//! law-drafting model compiles English into, what townspeople in office
//! generate when they legislate, and what the simulation executes.
//!
//! Everything here is plain serde data with lenient defaults, so a drafter may
//! omit fields that do not matter. `law::validate` checks a draft and explains
//! what is wrong in words a drafter can act on; `law::describe` turns a
//! statute back into plain English.
//!
//! Expressions (type [`Expr`]) are strings in the language of `crate::expr`,
//! evaluated against the scopes defined in `crate::scope`.
use crate::polity::Actor;
use crate::types::*;
use serde::{Deserialize, Serialize};

pub type Expr = String;

fn t() -> Expr { "true".to_string() }
fn zero() -> Expr { "0".to_string() }
fn one() -> Expr { "1".to_string() }
fn half() -> f32 { 0.5 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatuteStatus { #[default] Draft, Pending, Active, Repealed, Rejected }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatuteStats {
    /// Coins collected (taxes, fees, fines) since enactment.
    pub revenue: f32,
    /// Coins paid out (transfers, subsidies, bounties, salaries it created).
    pub paid_out: f32,
    pub offences: u32,
    /// Times someone wanted to do what it forbids and held back.
    pub refrained: u32,
    pub reported: u32,
    pub punished: u32,
    pub fines: f32,
    /// Tax known to have been hidden and never recovered.
    pub evaded: f32,
    pub bribes: u32,
    /// Enforcers who saw an offence and chose to let it pass.
    pub ignored: u32,
    pub today_offences: u32,
    pub today_refrained: u32,
    /// Smoothed share of occasions on which the rule was broken rather than kept (0..=1).
    pub flout_rate: f32,
    /// Mean opinion of the statute among adults, -1..=1 (refreshed daily).
    pub approval: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Statute {
    #[serde(default)]
    pub id: StatuteId,
    pub title: String,
    /// One or two plain sentences on what it does.
    #[serde(default)]
    pub summary: String,
    /// The words it was proposed in.
    #[serde(default)]
    pub text: String,
    /// Name of the jurisdiction it applies in: "" for the whole town, or a ward's name.
    #[serde(default)]
    pub jurisdiction: String,
    pub clauses: Vec<Clause>,
    /// Parts of the proposal the mechanics cannot express.
    #[serde(default)]
    pub unmodeled: Vec<String>,
    #[serde(default)]
    pub notes: String,

    // --- lifecycle, managed by the simulation ---
    #[serde(default)]
    pub jur: JurId,
    #[serde(default)]
    pub status: StatuteStatus,
    #[serde(default)]
    pub proposed_day: i32,
    #[serde(default)]
    pub enacted_day: Option<i32>,
    #[serde(default)]
    pub repealed_day: Option<i32>,
    #[serde(default)]
    pub sponsor: Option<Actor>,
    /// How it became law: "decree", "vote of the Council", "referendum", "uprising"...
    #[serde(default)]
    pub passed_by: String,
    #[serde(default)]
    pub stats: StatuteStats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Clause {
    /// Forbid or require an action.
    Conduct(Conduct),
    /// A tax, fee or tariff.
    Levy(Levy),
    /// A regular payment from the treasury to people.
    Transfer(Transfer),
    /// The treasury pays part of the price of something.
    Subsidy(Subsidy),
    /// A legal minimum or maximum price, wage or rent.
    Price(PriceRule),
    /// A legal status (licence, rank, class, registration) and how it is gained and lost.
    Status(StatusRule),
    /// When something happens or on a schedule, do things. The general-purpose mechanism.
    Event(EventRule),
    /// A one-off change of ownership carried out when the statute takes effect.
    Property(PropertyTransfer),
    /// Create or replace an office (by name).
    Office(OfficeDef),
    /// Create or replace a deliberative body (by name).
    Body(BodyDef),
    /// How laws are made in this jurisdiction.
    Procedure(ProcedureDef),
    /// Create or replace an enforcement unit (by name).
    Unit(UnitDef),
    /// Switch a tool of detection on or off.
    Instrument(InstrumentDef),
    /// How guilt is decided.
    Court(CourtDef),
    /// The jurisdiction employs clerks, who audit taxes and keep its registers.
    Clerks { count: u32, #[serde(default = "ten")] wage: f32 },
    /// An examination that measures something about a person, imperfectly, and puts the result on record.
    Assessment(AssessmentDef),
    /// Abolish an office, body or unit by name.
    Abolish { name: String },
    /// Fund research in a field.
    Research { field: Field, per_day: f32 },
    /// The state offers jobs on public works.
    PublicWorks(PublicWorks),
    /// Strike new coin into the treasury.
    Mint { amount: f32, #[serde(default)] period: Period },
    /// Strike statutes from the book.
    Repeal { statutes: Vec<StatuteId> },
    /// A purely symbolic act.
    Declaration { text: String, #[serde(default)] rename_town: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Period { #[default] Once, Day, Week, Year }

// ---------------------------------------------------------------- conduct

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Things people do of their own accord (usually forbidden, conditionally).
    Buy, Sell, Produce, Consume, Work, Employ, BeOutside, Visit, Protest, OwnProperty, Emigrate, Immigrate, HaveChild, Marry,
    Steal, Forage, Bribe, StandForOffice, Petition,
    // Things people must be made to do (required).
    AttendSchool, HoldJob, PublicLabor, Attend, Vote, Inform,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Buy => "buy", Action::Sell => "sell", Action::Produce => "produce", Action::Consume => "consume", Action::Work => "work", Action::Employ => "employ",
            Action::BeOutside => "be_outside", Action::Visit => "visit", Action::Protest => "protest", Action::OwnProperty => "own_property", Action::Emigrate => "emigrate",
            Action::Immigrate => "immigrate", Action::HaveChild => "have_child", Action::Marry => "marry", Action::Steal => "steal", Action::Forage => "forage",
            Action::Bribe => "bribe", Action::StandForOffice => "stand_for_office", Action::Petition => "petition", Action::AttendSchool => "attend_school",
            Action::HoldJob => "hold_job", Action::PublicLabor => "public_labor", Action::Attend => "attend", Action::Vote => "vote", Action::Inform => "inform",
        }
    }
    /// Actions that only make sense as requirements.
    pub fn is_duty(self) -> bool { matches!(self, Action::AttendSchool | Action::HoldJob | Action::PublicLabor | Action::Attend | Action::Vote | Action::Inform) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode { #[default] Forbid, Require }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sanction {
    /// Coins.
    #[serde(default = "zero")]
    pub fine: Expr,
    #[serde(default)]
    pub jail_days: f32,
    /// Seize the goods or property involved.
    #[serde(default)]
    pub confiscate: bool,
    #[serde(default)]
    pub exile: bool,
    #[serde(default)]
    pub execute: bool,
    /// Bar from having (further) children.
    #[serde(default)]
    pub sterilize: bool,
    /// Give or remove a legal status ("" for none).
    #[serde(default)]
    pub add_status: String,
    #[serde(default)]
    pub remove_status: String,
    /// The fine falls on the whole household rather than the offender alone.
    #[serde(default)]
    pub collective: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conduct {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub mode: Mode,
    pub action: Action,
    /// Who the rule binds (person scope).
    #[serde(default = "t")]
    pub who: Expr,
    /// The circumstances in which it binds (act scope: person + act + place + moment).
    #[serde(default = "t")]
    pub when: Expr,
    /// For `attend`: the kind of place they must be at.
    #[serde(default)]
    pub place: String,
    /// For `public_labor`: unpaid hours owed each week.
    #[serde(default)]
    pub hours_per_week: f32,
    #[serde(default)]
    pub sanction: Sanction,
}

/// Something a person is about to do, as the law sees it. Built by whichever
/// module makes the person act and passed to `law::attempt`.
#[derive(Clone, Debug)]
pub struct Act {
    pub action: Action,
    pub place: PlaceId,
    pub good: Option<Good>,
    pub price: f32,
    pub quantity: f32,
    pub wage: f32,
    pub hours_today: f32,
    /// The other party: the worker being employed, the intended spouse, the trading partner.
    pub other: Option<PersonId>,
    pub firm: Option<FirmId>,
    pub property: Option<PlaceId>,
    /// For own_property: how many properties the household would then hold.
    pub properties_owned: u32,
    /// What doing this is worth to the actor, in coins. Drives both the
    /// decision to break a rule and the felt cost of obeying it.
    pub value: f32,
}
impl Act {
    pub fn new(action: Action, place: PlaceId, value: f32) -> Self {
        Act { action, place, good: None, price: 0.0, quantity: 1.0, wage: 0.0, hours_today: 0.0, other: None, firm: None, property: None, properties_owned: 0, value }
    }
    pub fn good(mut self, g: Good, price: f32, qty: f32) -> Self { self.good = Some(g); self.price = price; self.quantity = qty; self }
}

/// A remembered occasion on which a person did, or wanted to do, something.
/// People's sense of what a proposed rule would cost them comes from replaying these.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wish {
    pub action: Action,
    pub place_kind: PlaceKind,
    pub ward: JurId,
    pub good: Option<Good>,
    pub price: f32,
    pub quantity: f32,
    pub wage: f32,
    pub hours_today: f32,
    pub hour: f32,
    pub weekday: u32,
    pub value: f32,
    pub day: i32,
}

// ----------------------------------------------------------------- fiscal

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevyBase {
    /// All personal income from work, profit and rent.
    Income, Wage, Profit, RentIncome,
    /// Fraction of net worth, charged weekly.
    Wealth,
    /// Fraction of the value of buildings owned, charged weekly.
    Property,
    /// A flat number of coins per person per week (`rate` is coins, not a fraction).
    Head,
    /// Fraction of each sale, taken from the seller.
    Sale, Import, Export, Inheritance,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Levy {
    #[serde(default)]
    pub label: String,
    pub base: LevyBase,
    /// A fraction of the base (or coins, for `head`), evaluated per taxpayer; may be progressive.
    pub rate: Expr,
    #[serde(default = "t")]
    pub who: Expr,
    /// For sale / import / export: which good (none = all).
    #[serde(default)]
    pub good: Option<Good>,
    /// A caught evader pays the tax plus this multiple of it.
    #[serde(default = "two")]
    pub evasion_fine_multiple: f32,
    #[serde(default)]
    pub evasion_jail_days: f32,
}
fn two() -> f32 { 2.0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transfer {
    #[serde(default)]
    pub label: String,
    /// Coins per payment, per recipient.
    pub amount: Expr,
    #[serde(default = "per_day")]
    pub period: Period,
    #[serde(default = "t")]
    pub who: Expr,
}
fn per_day() -> Period { Period::Day }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priced { Food, Wood, Wares, Ale, Healthcare, Education, Rent, Wage }
impl Priced {
    pub fn of_good(g: Good) -> Priced { match g { Good::Food => Priced::Food, Good::Wood => Priced::Wood, Good::Wares => Priced::Wares, Good::Ale => Priced::Ale } }
    pub fn name(self) -> &'static str { match self { Priced::Food => "food", Priced::Wood => "wood", Priced::Wares => "wares", Priced::Ale => "ale", Priced::Healthcare => "healthcare", Priced::Education => "education", Priced::Rent => "rent", Priced::Wage => "wage" } }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subsidy {
    #[serde(default)]
    pub label: String,
    /// What is subsidised. `wage` subsidises employers of matching workers.
    pub target: Priced,
    /// Share of the price the treasury pays, 0..=1.
    #[serde(default = "one")]
    pub fraction: Expr,
    /// Which buyers (tenants, patients, pupils' parents, workers) qualify.
    #[serde(default = "t")]
    pub who: Expr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceRule {
    #[serde(default)]
    pub label: String,
    pub target: Priced,
    /// Coins; "" for no floor.
    #[serde(default)]
    pub min: Expr,
    /// Coins; "" for no ceiling.
    #[serde(default)]
    pub max: Expr,
    /// Whose transactions it covers (the buyer, tenant or worker).
    #[serde(default = "t")]
    pub who: Expr,
    #[serde(default)]
    pub sanction: Sanction,
}

// --------------------------------------------------------------- statuses

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum StatusGain {
    /// Everyone for whom `when` holds has it.
    Auto { when: Expr },
    /// Anyone matching `who` may buy it for `fee`; people buy when it is worth it to them.
    Purchase { fee: Expr, #[serde(default = "t")] who: Expr },
    /// The holder of an office (or a body) confers it on whom they choose.
    Appoint { by_office: String, #[serde(default)] count: u32 },
    /// Drawn by lot from those matching `pool`.
    Lottery { pool: Expr, count: u32, #[serde(default)] period: Period },
    /// The `top` highest scorers by `score` among those matching `pool`.
    Rank { score: Expr, top: u32, #[serde(default = "t")] pool: Expr, #[serde(default)] period: Period },
    /// Children are born with it if a parent holds it.
    Birth,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum StatusLoss {
    When { when: Expr },
    AfterDays { days: u32 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusRule {
    /// A short snake_case name, used in expressions as `has("name")`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub gain: Vec<StatusGain>,
    #[serde(default)]
    pub lose: Vec<StatusLoss>,
}

// ------------------------------------------------------------ event rules

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind { Birth, Death, Marriage, ComingOfAge, Conviction, Hire, Immigration, Election, Enactment }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum Trigger {
    /// At `hour` every day / on `weekday` each week / on the first day of each year / once on enactment.
    Schedule { period: Period, #[serde(default)] weekday: u32, #[serde(default = "nine")] hour: f32 },
    /// Whenever such an event happens; the person it happens to is the `subject`.
    Event { event: EventKind },
}
fn nine() -> f32 { 9.0 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick { #[default] All, Random, Top, Bottom, Subject }

/// Chooses people.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Selector {
    /// Person scope. Ignored for `pick: subject`.
    #[serde(default = "t")]
    pub filter: Expr,
    #[serde(default)]
    pub pick: Pick,
    /// For top / bottom: the expression to rank by.
    #[serde(default)]
    pub by: Expr,
    /// For random / top / bottom: how many.
    #[serde(default = "one_u32")]
    pub n: u32,
}
fn one_u32() -> u32 { 1 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "party", rename_all = "snake_case")]
pub enum Party {
    Treasury,
    /// The person the triggering event happened to.
    Subject,
    People(Selector),
    /// Whoever holds the named office.
    Holder { office: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Recipient {
    State,
    Person { id: PersonId },
    /// Workers of a business become a co-operative; residents of a home become its owners.
    Occupants,
    /// Sold to the highest bidder, proceeds to the treasury.
    Auction,
    /// Households that own nothing, occupants first.
    Landless,
    People(Selector),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Effect {
    /// Move coins. Amount is per payer->payee pair; people pay what they can.
    Pay { from: Party, to: Party, amount: Expr },
    SetStatus { whom: Selector, status: String, #[serde(default = "yes")] on: bool },
    Punish { whom: Selector, sanction: Sanction, #[serde(default)] reason: String },
    /// Change who owns the properties matching `select` (place scope).
    MoveProperty { select: Expr, to: Recipient, #[serde(default)] compensation: f32 },
    /// Install the selected person in the named office.
    Seat { office: String, whom: Selector },
    /// The two selected households exchange homes.
    SwapHomes { a: Selector, b: Selector },
    /// The treasury buys goods at market and hands them out.
    Give { whom: Selector, good: Good, quantity: Expr },
}
fn yes() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRule {
    #[serde(default)]
    pub label: String,
    pub trigger: Trigger,
    /// Evaluated in the town scope (plus `subject.*` for event triggers); effects run only if true.
    #[serde(default = "t")]
    pub condition: Expr,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyTransfer {
    #[serde(default)]
    pub label: String,
    /// Place scope.
    pub select: Expr,
    pub to: Recipient,
    /// Fraction of value paid to the previous owner from the treasury.
    #[serde(default)]
    pub compensation: f32,
}

// ----------------------------------------------------------- constitution

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteMethod {
    /// Most first choices wins.
    Plurality,
    /// Top two face a second round unless one has a majority.
    Runoff,
    /// Instant runoff over ranked ballots.
    Ranked,
    /// Vote for every acceptable candidate; most approvals wins.
    Approval,
    /// Points by rank.
    Borda,
    /// Seats in proportion to votes for each list (bodies only).
    Proportional,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Selection {
    /// Nobody replaces the holder automatically.
    Fixed,
    Election {
        #[serde(default = "plurality")]
        method: VoteMethod,
        /// Who may vote (person scope).
        #[serde(default = "adult")]
        franchise: Expr,
        /// How much each ballot counts (person scope), e.g. "1" or "wealth".
        #[serde(default = "one")]
        weight: Expr,
        /// Who may stand (person scope), in addition to the office's `eligibility`.
        #[serde(default = "adult")]
        candidacy: Expr,
    },
    /// Drawn by lot from those matching `pool`.
    Lottery { #[serde(default = "adult")] pool: Expr },
    /// The eldest living child of the last holder, failing that their nearest kin.
    Heredity,
    /// Chosen by the holder of another office, or by a body's vote.
    Appointment { appointer: String },
    /// Whoever scores highest on `score` among the eligible.
    Merit { score: Expr },
    /// The sitting members (or the outgoing holder) choose.
    Cooptation,
    /// Sold to the highest bidder; proceeds to the treasury.
    Auction,
    /// Held by whoever commands the strongest armed following; changes hands by coup.
    Force,
}
fn plurality() -> VoteMethod { VoteMethod::Plurality }
fn adult() -> Expr { "is_adult".to_string() }
impl Default for Selection { fn default() -> Self { Selection::Fixed } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    /// May enact and repeal statutes alone, bypassing the lawmaking procedure.
    Decree,
    /// Directs enforcement units that name this office as commander.
    Command,
    /// May release prisoners and cancel sentences.
    Pardon,
    /// Decides cases when the court is a judge.
    Judge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OfficeDef {
    pub name: String,
    #[serde(default)]
    pub powers: Vec<Power>,
    #[serde(default)]
    pub selection: Selection,
    /// 0 = for life.
    #[serde(default)]
    pub term_days: u32,
    /// Coins per day from the treasury.
    #[serde(default)]
    pub salary: f32,
    /// Who may hold it (person scope).
    #[serde(default = "adult")]
    pub eligibility: Expr,
    /// First holder: "player", a person id as a number in a string ("12"), "" to fill by the selection rule, or "keep" to leave the current holder.
    #[serde(default)]
    pub holder: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyDef {
    pub name: String,
    pub seats: u32,
    #[serde(default)]
    pub selection: Selection,
    #[serde(default)]
    pub term_days: u32,
    #[serde(default)]
    pub salary: f32,
    #[serde(default = "adult")]
    pub eligibility: Expr,
    /// Elected bodies: one member per ward (true) or all chosen at large (false).
    #[serde(default)]
    pub by_ward: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "snake_case")]
pub enum Proposer {
    Office { name: String },
    /// Any member of the body.
    Body { name: String },
    /// Any person matching `franchise` who gathers signatures from this fraction of them.
    Initiative { #[serde(default = "adult")] franchise: Expr, signatures: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// The office holder must assent.
    Assent { office: String },
    /// The body votes; passes with more than `threshold` of votes cast.
    Body { name: String, #[serde(default = "half")] threshold: f32 },
    /// A vote of the people.
    Referendum {
        #[serde(default = "adult")]
        franchise: Expr,
        #[serde(default = "one")]
        weight: Expr,
        #[serde(default = "half")]
        threshold: f32,
        /// Minimum share of the franchise that must vote for the result to stand.
        #[serde(default)]
        turnout: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VetoDef {
    pub office: String,
    /// A body that can override, and the share of its votes needed ("" for an absolute veto).
    #[serde(default)]
    pub override_body: String,
    #[serde(default = "two_thirds")]
    pub override_threshold: f32,
}
fn two_thirds() -> f32 { 0.667 }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProcedureDef {
    /// If true this is the procedure for changing offices, bodies and procedures
    /// themselves; otherwise the procedure for ordinary statutes.
    #[serde(default)]
    pub constitutional: bool,
    pub proposers: Vec<Proposer>,
    /// Every step must pass, in order.
    pub steps: Vec<Step>,
    #[serde(default)]
    pub veto: Option<VetoDef>,
}

// ------------------------------------------------------------ enforcement

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shift { #[default] Split, Day, Evening, Night }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitDef {
    pub name: String,
    pub staff: u32,
    /// Coins per day.
    #[serde(default = "ten")]
    pub wage: f32,
    /// Share of each fine that goes to the enforcer who brought the case.
    #[serde(default)]
    pub fine_share: f32,
    /// Coins per conviction.
    #[serde(default)]
    pub bounty: f32,
    /// Convictions expected of each enforcer per week (0 = none), and the pay docked per shortfall.
    #[serde(default)]
    pub quota_per_week: f32,
    #[serde(default)]
    pub quota_penalty: f32,
    /// Which places they patrol (place scope).
    #[serde(default = "t")]
    pub posts: Expr,
    #[serde(default)]
    pub shift: Shift,
    /// Titles of the statutes they enforce; empty = all of their jurisdiction's.
    #[serde(default)]
    pub mandate: Vec<String>,
    /// Office that commands them ("" = nobody; they follow the statute book as they see fit).
    #[serde(default)]
    pub commander: String,
    #[serde(default = "yes")]
    pub armed: bool,
    #[serde(default = "adult")]
    pub eligibility: Expr,
}
fn ten() -> f32 { 10.0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstrumentKind {
    /// Whoever reports an offence that leads to conviction gets `share` of the fine plus `flat` coins.
    InformerReward { #[serde(default)] share: f32, #[serde(default)] flat: f32 },
    /// Enforcers stop and check everyone passing through matching places.
    Checkpoint { places: Expr },
    /// Clerks keep a register; offences that leave a paper trail in it are far likelier to surface.
    Registry { of: Register },
    /// Homes of matching people are searched this many times a week on average.
    Search { who: Expr, per_week: f32 },
    /// A fixed post that watches matching places around the clock.
    Watchpost { places: Expr },
    /// Inspectors who watch the enforcers themselves.
    Inspectors { count: u32, #[serde(default = "ten")] wage: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Register { Births, Property, Income, Trade, Residence }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstrumentDef {
    #[serde(default)]
    pub label: String,
    #[serde(flatten)]
    pub kind: InstrumentKind,
    #[serde(default = "yes")]
    pub on: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CourtDef {
    /// The enforcer who makes the arrest decides guilt and applies the sanction.
    Summary,
    /// The holder of an office with the `judge` power hears each case.
    Judge { office: String },
    /// A panel drawn by lot from those matching `pool` votes on each case.
    Jury { size: u32, #[serde(default = "adult")] pool: Expr },
}
impl Default for CourtDef { fn default() -> Self { CourtDef::Summary } }

/// An official test. Clerks examine the people it covers and record a score
/// from 0 to 1 that later statutes can read as `score("name")`. The score is
/// the person's true disposition seen through a fog: a cheap, hurried test by
/// poor record-keepers says little; a careful one says more, never everything.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssessmentDef {
    /// A short snake_case name, used in expressions as `score("name")`.
    pub name: String,
    /// Which disposition the test tries to measure.
    pub measures: Pref,
    /// Who is examined (person scope).
    #[serde(default = "adult")]
    pub who: Expr,
    /// How often each person is re-examined (`once` = a single time).
    #[serde(default)]
    pub period: Period,
    /// Coins of clerks' time spent on each examination, from the treasury. More buys a sharper test.
    #[serde(default = "one_f32")]
    pub cost: f32,
    /// What befalls those who refuse to sit it.
    #[serde(default)]
    pub sanction: Sanction,
}
fn one_f32() -> f32 { 1.0 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Project { #[default] Amenities, Housing, None }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicWorks {
    #[serde(default)]
    pub label: String,
    pub jobs: u32,
    pub wage: f32,
    #[serde(default)]
    pub project: Project,
    #[serde(default = "adult")]
    pub who: Expr,
}

impl Statute {
    /// A blank draft with the given title and clauses.
    pub fn draft(title: &str, clauses: Vec<Clause>) -> Statute {
        Statute { id: StatuteId(0), title: title.to_string(), summary: String::new(), text: String::new(), jurisdiction: String::new(), clauses, unmodeled: Vec::new(), notes: String::new(), jur: JurId(0), status: StatuteStatus::Draft, proposed_day: 0, enacted_day: None, repealed_day: None, sponsor: None, passed_by: String::new(), stats: StatuteStats::default() }
    }
    pub fn is_active(&self) -> bool { self.status == StatuteStatus::Active }
    /// Does it change offices, bodies or the lawmaking procedure?
    pub fn is_constitutional(&self) -> bool {
        self.clauses.iter().any(|c| matches!(c, Clause::Office(_) | Clause::Body(_) | Clause::Procedure(_)) || matches!(c, Clause::Abolish { .. }))
    }
}
