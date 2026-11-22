//! Core data model. Everything the simulation knows lives in [`World`], which
//! is plain data: `Clone` (so the world can be forked for forecasts) and
//! serde-serialisable (saves, and the JSON API to the browser).
//!
//! Entities live in `Vec`s indexed by their id; ids are never reused. Dead
//! people and defunct firms stay in place with a flag so indices stay valid.
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------- time

/// One slot is 15 minutes of town time.
pub const SLOT_MINUTES: u32 = 15;
pub const SLOTS_PER_HOUR: u32 = 4;
pub const SLOTS_PER_DAY: u32 = 96;
pub const DAYS_PER_WEEK: u32 = 7;
/// Weekday 6 is the customary rest day.
pub const REST_DAY: u32 = 6;
pub const DAYS_PER_YEAR: u32 = 28;
pub const ADULT_AGE: f32 = 16.0;
pub const ELDER_AGE: f32 = 60.0;
pub const WEEKDAYS: [&str; 7] = ["Moonday", "Tillday", "Midweek", "Thatchday", "Fairday", "Marketday", "Restday"];

// -------------------------------------------------------------------- ids

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
        #[serde(transparent)]
        pub struct $name(pub u32);
        impl $name {
            #[inline]
            pub fn ix(self) -> usize { self.0 as usize }
        }
    };
}
id_type!(PersonId);
id_type!(HouseholdId);
id_type!(PlaceId);
id_type!(FirmId);
id_type!(JurId);
id_type!(OfficeId);
id_type!(BodyId);
id_type!(StatuteId);
id_type!(BillId);
id_type!(UnitId);

// ------------------------------------------------------------ preferences

/// The base preferences and dispositions every person has, each in 0..=1.
///
/// This is the *only* place individual desires are specified. Everything else
/// a person wants - a job, an office, a law, a revolt - is derived by asking
/// what serves these, never assigned directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum Pref {
    // What they want (weights on the components of wellbeing).
    /// Food, rest, health, shelter, household goods.
    Comfort = 0,
    /// Free time, fun and company.
    Leisure,
    /// Accumulated money and property.
    Wealth,
    /// The wellbeing of their own household and close kin.
    Family,
    /// The wellbeing of friends, neighbours and townsfolk at large.
    Community,
    /// Not being commanded, forbidden, or watched.
    Autonomy,
    /// Dislike of inequality and of rules that treat people unequally.
    Fairness,
    /// Dislike of crime, disorder and danger.
    Security,
    /// Dislike of change from the customs and rules they grew up with.
    Continuity,
    /// Standing in the eyes of others: office, rank, respect.
    Esteem,
    // How they decide.
    /// Willingness to gamble on uncertain outcomes.
    Risk,
    /// Weight placed on the future relative to today.
    Patience,
    /// Pull toward doing what others do and what authority asks.
    Conformity,
    /// Willingness to work (inverse of the felt cost of labour).
    Diligence,
    /// Restraint with drink and other indulgences.
    Temperance,
    /// Appetite for company.
    Sociability,
}
pub const N_PREFS: usize = 16;
pub const ALL_PREFS: [Pref; N_PREFS] = [
    Pref::Comfort, Pref::Leisure, Pref::Wealth, Pref::Family, Pref::Community, Pref::Autonomy, Pref::Fairness, Pref::Security,
    Pref::Continuity, Pref::Esteem, Pref::Risk, Pref::Patience, Pref::Conformity, Pref::Diligence, Pref::Temperance, Pref::Sociability,
];
/// The first ten preferences are the weights on wellbeing components.
pub const N_WANTS: usize = 10;
impl Pref {
    pub fn name(self) -> &'static str {
        ["comfort", "leisure", "wealth", "family", "community", "autonomy", "fairness", "security", "continuity", "esteem", "risk", "patience", "conformity", "diligence", "temperance", "sociability"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Prefs(pub [f32; N_PREFS]);
impl Default for Prefs {
    fn default() -> Self { Prefs([0.5; N_PREFS]) }
}
impl std::ops::Index<Pref> for Prefs {
    type Output = f32;
    #[inline]
    fn index(&self, p: Pref) -> &f32 { &self.0[p as usize] }
}
impl std::ops::IndexMut<Pref> for Prefs {
    #[inline]
    fn index_mut(&mut self, p: Pref) -> &mut f32 { &mut self.0[p as usize] }
}

/// A person's wellbeing, component by component. Components are roughly in
/// -1..=1 (0 = neutral). `own` is the self-regarding part (what others'
/// altruism looks at); `total` is the full weighted sum the person maximises.
/// Maintained by `life::wellbeing`; see docs/DESIGN.md for the definitions.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Wellbeing {
    /// Smoothed components, indexed like the first `N_WANTS` preferences.
    pub parts: [f32; N_WANTS],
    pub own: f32,
    pub total: f32,
}

/// Things that happened to a person today that bear on their wellbeing.
/// Any module may add to these; `life::end_of_day` folds them into
/// [`Wellbeing`] and clears them.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Felt {
    /// Coin-equivalent value of things they wanted to do and did not because a rule forbade it, or were made to do.
    pub constrained: f32,
    /// How much they were watched, searched, checked or informed on (events, weighted by intrusiveness).
    pub watched: f32,
    /// Harm suffered from crime (coin-equivalent) today.
    pub victimized: f32,
    /// Punishment suffered (coin-equivalent, jail days valued at daily income).
    pub punished: f32,
    /// Disorder witnessed: crimes, riots, brawls seen.
    pub disorder: f32,
    /// Coin-equivalent of being treated worse than others by a rule that singles out their group.
    pub slighted: f32,
    /// Respect received (holding office, being deferred to, winning a vote).
    pub honored: f32,
}

// ----------------------------------------------------------------- people

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex { Female, Male }

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Needs {
    /// 0 = full, 100 = starving.
    pub hunger: f32,
    /// 100 = rested.
    pub energy: f32,
    /// 100 = content with company.
    pub social: f32,
    /// 100 = entertained.
    pub fun: f32,
    /// 100 = healthy; 0 = dead.
    pub health: f32,
}
impl Default for Needs {
    fn default() -> Self { Needs { hunger: 25.0, energy: 85.0, social: 65.0, fun: 65.0, health: 90.0 } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { Farmer, Lumberjack, Crafter, Brewer, Tavernkeeper, Healer, Teacher, Enforcer, Clerk, Laborer, Scholar }
impl Role {
    pub fn name(self) -> &'static str {
        match self { Role::Farmer => "farmer", Role::Lumberjack => "lumberjack", Role::Crafter => "crafter", Role::Brewer => "brewer", Role::Tavernkeeper => "tavernkeeper", Role::Healer => "healer", Role::Teacher => "teacher", Role::Enforcer => "enforcer", Role::Clerk => "clerk", Role::Laborer => "laborer", Role::Scholar => "scholar" }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub firm: FirmId,
    /// Coins per full day. 0 for an owner-manager, who lives on the profit.
    pub wage: f32,
    pub role: Role,
    pub since_day: i32,
    pub owner: bool,
}

/// What a person is doing right now. `Person::doing` holds the human-readable line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Idle, Travel, Sleep, Eat, Work, Patrol, School, Shop, Tavern, Visit, Host, Stroll, Play, Garden, Home, Chapel,
    Protest, Vote, Apply, Treatment, Forage, Beg, Steal, Corvee, Attend, Jailed, Campaign, Rebel, Leaving,
}

/// A walk in progress. While travelling, `Person::loc` is the destination's
/// ward street (`Map::streets`), so "who is in this building" stays truthful.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Travel {
    pub from: PlaceId,
    pub to: PlaceId,
    pub depart: u32,
    pub arrive: u32,
}

/// What to begin on arriving somewhere.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub activity: Activity,
    /// Slots the activity will last.
    pub slots: u32,
    pub doing: String,
    /// Free-form payload for the planner (a firm id, a friend id, a good...).
    #[serde(default)]
    pub arg: i64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Income {
    pub wage: f32,
    pub profit: f32,
    pub rent: f32,
    pub transfer: f32,
    pub other: f32,
    /// Taxes paid today.
    pub tax: f32,
    /// Smoothed total gross income per day.
    pub avg: f32,
    /// Smoothed income per day not earned by labour (rent, profit, transfers).
    pub unearned_avg: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeKind { Wage, Profit, Rent, Transfer, Other }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiaryEntry {
    pub slot: u32,
    pub text: String,
    /// work | money | crime | law | politics | hardship | kindness | leisure | health | home | grief | family
    pub kind: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Friend {
    pub id: PersonId,
    /// 0..=1
    pub affinity: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub name: String,
    pub surname: String,
    pub sex: Sex,
    /// May be negative for people born before the game began.
    pub birth_day: i32,
    pub alive: bool,
    /// False once they have died, emigrated or been exiled.
    pub present: bool,
    pub household: HouseholdId,

    /// Heritable values behind the preferences (never observable in-world).
    pub genes: Prefs,
    /// Expressed base preferences: genes shaped by upbringing, schooling and peers.
    pub prefs: Prefs,
    pub needs: Needs,
    /// 0..=1
    pub education: f32,
    /// 0..=1 experience at work.
    pub skill: f32,
    pub job: Option<Job>,

    pub loc: PlaceId,
    pub activity: Activity,
    /// Absolute slot at which the current activity ends.
    pub until: u32,
    pub doing: String,
    pub thought: String,
    pub travel: Option<Travel>,
    pub next: Option<Plan>,

    pub spouse: Option<PersonId>,
    pub mother: Option<PersonId>,
    pub father: Option<PersonId>,
    pub children: Vec<PersonId>,
    pub friends: Vec<Friend>,

    /// Bitset over `World::status_names` (legal statuses defined by statutes).
    pub statuses: u64,
    pub convictions: u32,
    /// Absolute slot of release, if in jail.
    pub jailed_until: Option<u32>,
    pub sterile: bool,
    pub immigrant: bool,
    pub enrolled: bool,

    pub income: Income,
    pub felt: Felt,
    pub wellbeing: Wellbeing,
    /// 0..=1: how far they regard the government as rightful. Derived from how
    /// they have fared under it and how they have been treated by it.
    pub trust: f32,
    /// Their view of each live statute or pending bill, -1..=1.
    pub opinions: Vec<(StatuteId, f32)>,
    pub diary: Vec<DiaryEntry>,
    /// Recent occasions on which they did, or wanted to do, something (newest last, capped).
    #[serde(default)]
    pub wishes: Vec<crate::statute::Wish>,

    /// Per-module state. Each struct is owned by the named module.
    #[serde(default)]
    pub mind: crate::life::Mind,
    #[serde(default)]
    pub fiscal: crate::law::FiscalState,
    #[serde(default)]
    pub civic: crate::gov::Civic,
    #[serde(default)]
    pub legal: crate::enforce::LegalState,
    #[serde(default)]
    pub work: crate::economy::WorkState,
}

impl Person {
    /// A newborn-blank person; callers fill in what they know.
    pub fn new(id: PersonId, name: &str, surname: &str, sex: Sex, birth_day: i32, household: HouseholdId, loc: PlaceId) -> Person {
        Person {
            id, name: name.to_string(), surname: surname.to_string(), sex, birth_day, alive: true, present: true, household,
            genes: Prefs::default(), prefs: Prefs::default(), needs: Needs::default(), education: 0.0, skill: 0.0, job: None,
            loc, activity: Activity::Idle, until: 0, doing: String::new(), thought: String::new(), travel: None, next: None,
            spouse: None, mother: None, father: None, children: Vec::new(), friends: Vec::new(),
            statuses: 0, convictions: 0, jailed_until: None, sterile: false, immigrant: false, enrolled: false,
            income: Income::default(), felt: Felt::default(), wellbeing: Wellbeing::default(), trust: 0.5, opinions: Vec::new(), diary: Vec::new(), wishes: Vec::new(),
            mind: Default::default(), fiscal: Default::default(), civic: Default::default(), legal: Default::default(), work: Default::default(),
        }
    }
    pub fn full_name(&self) -> String { format!("{} {}", self.name, self.surname) }
    pub fn age_on(&self, day: i32) -> f32 { (day - self.birth_day) as f32 / DAYS_PER_YEAR as f32 }
    pub fn has_status(&self, ix: usize) -> bool { ix < 64 && self.statuses & (1u64 << ix) != 0 }
    pub fn set_status(&mut self, ix: usize, on: bool) {
        if ix < 64 { if on { self.statuses |= 1u64 << ix } else { self.statuses &= !(1u64 << ix) } }
    }
    pub fn opinion(&self, s: StatuteId) -> Option<f32> { self.opinions.iter().find(|(id, _)| *id == s).map(|(_, v)| *v) }
    pub fn set_opinion(&mut self, s: StatuteId, v: f32) {
        match self.opinions.iter_mut().find(|(id, _)| *id == s) { Some(e) => e.1 = v, None => self.opinions.push((s, v)) }
    }
    pub fn affinity(&self, other: PersonId) -> f32 { self.friends.iter().find(|f| f.id == other).map(|f| f.affinity).unwrap_or(0.0) }
    pub fn befriend(&mut self, other: PersonId, delta: f32) {
        match self.friends.iter_mut().find(|f| f.id == other) {
            Some(f) => f.affinity = (f.affinity + delta).clamp(0.0, 1.0),
            None => self.friends.push(Friend { id: other, affinity: delta.clamp(0.0, 1.0) }),
        }
    }
}

// ------------------------------------------------------------- households

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum Good { Food = 0, Wood, Wares, Ale }
pub const N_GOODS: usize = 4;
pub const ALL_GOODS: [Good; N_GOODS] = [Good::Food, Good::Wood, Good::Wares, Good::Ale];
impl Good {
    pub fn name(self) -> &'static str { ["food", "wood", "wares", "ale"][self as usize] }
    pub fn from_name(s: &str) -> Option<Good> { ALL_GOODS.iter().copied().find(|g| g.name() == s) }
    /// Reference price in coins per unit, used to seed markets and anchor outside trade.
    pub fn base_price(self) -> f32 { [2.0, 2.0, 6.5, 3.0][self as usize] }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Household {
    pub id: HouseholdId,
    pub name: String,
    pub members: Vec<PersonId>,
    pub home: Option<PlaceId>,
    /// The common purse. Individuals earn; the household holds and spends.
    pub money: f32,
    /// Stocks of each good, indexed by `Good as usize`.
    pub stock: [f32; N_GOODS],
    /// Days of rent unpaid.
    pub arrears: f32,
    /// Money plus property value (refreshed daily by `stats`).
    pub wealth: f32,
    /// 1 = richest household (refreshed daily).
    pub wealth_rank: u32,
    /// 0 = poorest, 100 = richest (refreshed daily).
    pub wealth_pct: f32,
    #[serde(default)]
    pub econ: crate::economy::HouseholdEcon,
}

// ------------------------------------------------------------------ places

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    Home, Farm, Lumberyard, Workshop, Brewery, Tavern, Clinic, School, Academy, Market, Townhall, Watchhouse, Chapel,
    Plaza, Park, Forest, Street, Gate,
}
impl PlaceKind {
    pub fn name(self) -> &'static str {
        match self {
            PlaceKind::Home => "home", PlaceKind::Farm => "farm", PlaceKind::Lumberyard => "lumberyard", PlaceKind::Workshop => "workshop", PlaceKind::Brewery => "brewery",
            PlaceKind::Tavern => "tavern", PlaceKind::Clinic => "clinic", PlaceKind::School => "school", PlaceKind::Academy => "academy", PlaceKind::Market => "market",
            PlaceKind::Townhall => "townhall", PlaceKind::Watchhouse => "watchhouse", PlaceKind::Chapel => "chapel", PlaceKind::Plaza => "plaza", PlaceKind::Park => "park",
            PlaceKind::Forest => "forest", PlaceKind::Street => "street", PlaceKind::Gate => "gate",
        }
    }
    /// Out of doors: anyone present can see what happens here.
    pub fn outdoor(self) -> bool { matches!(self, PlaceKind::Plaza | PlaceKind::Park | PlaceKind::Forest | PlaceKind::Street | PlaceKind::Gate | PlaceKind::Market) }
    /// Open to the public (as opposed to a private home or a workplace).
    pub fn public(self) -> bool { self.outdoor() || matches!(self, PlaceKind::Tavern | PlaceKind::Chapel | PlaceKind::Townhall | PlaceKind::Clinic | PlaceKind::School) }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum Owner {
    None,
    Household(HouseholdId),
    /// Public property of a jurisdiction.
    State(JurId),
    /// Held in common by the workers of the firm that occupies it.
    Coop(FirmId),
}
impl Owner {
    pub fn kind_name(self) -> &'static str { match self { Owner::None => "none", Owner::Household(_) => "private", Owner::State(_) => "state", Owner::Coop(_) => "coop" } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Place {
    pub id: PlaceId,
    pub kind: PlaceKind,
    pub name: String,
    /// The ward (lowest-level jurisdiction) this place lies in.
    pub ward: JurId,
    /// Footprint in tiles.
    pub rect: Rect,
    /// The road tile people step onto when leaving.
    pub door: (i32, i32),
    pub owner: Owner,
    /// The household renting it, if it is a rented home.
    pub tenant: Option<HouseholdId>,
    /// Coins per day asked of the tenant.
    pub rent: f32,
    pub value: f32,
    pub firm: Option<FirmId>,
    /// A farm's field, where its people actually work.
    pub field: Option<Rect>,
}

/// Tile kinds for the renderer and for walking distances.
pub mod tile {
    pub const GRASS: u8 = 0;
    pub const ROAD: u8 = 1;
    pub const WATER: u8 = 2;
    pub const TREE: u8 = 3;
    pub const FIELD: u8 = 4;
    pub const BUILDING: u8 = 5;
    pub const PLAZA: u8 = 6;
    pub const PARK: u8 = 7;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Map {
    pub w: i32,
    pub h: i32,
    /// Row-major tile kinds (`tile::*`).
    pub tiles: Vec<u8>,
    /// `travel[a * n + b]`: slots needed to walk from place a to place b (n = number of places at generation; see `travel_slots`).
    pub travel: Vec<u8>,
    pub travel_n: usize,
    /// One street pseudo-place per ward, indexed by ward order in `Polity::wards`.
    pub streets: Vec<PlaceId>,
    pub gates: Vec<PlaceId>,
    pub plaza: PlaceId,
    pub market: PlaceId,
    pub forest: PlaceId,
}

// ------------------------------------------------------------------- firms

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmKind {
    Farm, Lumberyard, Workshop, Brewery, Tavern, Clinic, School,
    /// An enforcement unit (see `polity::Unit`); wages come from its jurisdiction.
    Watch,
    /// Clerks of a jurisdiction: keep registers and audit taxes.
    Office,
    /// A public works crew.
    Works,
    /// Scholars funded to do research.
    Academy,
}
impl FirmKind {
    pub fn name(self) -> &'static str {
        match self { FirmKind::Farm => "farm", FirmKind::Lumberyard => "lumberyard", FirmKind::Workshop => "workshop", FirmKind::Brewery => "brewery", FirmKind::Tavern => "tavern", FirmKind::Clinic => "clinic", FirmKind::School => "school", FirmKind::Watch => "watch", FirmKind::Office => "office", FirmKind::Works => "works", FirmKind::Academy => "academy" }
    }
    /// Public bodies that sell nothing and are paid from a treasury.
    pub fn is_org(self) -> bool { matches!(self, FirmKind::Watch | FirmKind::Office | FirmKind::Works | FirmKind::Academy) }
    pub fn output(self) -> Option<Good> { match self { FirmKind::Farm => Some(Good::Food), FirmKind::Lumberyard => Some(Good::Wood), FirmKind::Workshop => Some(Good::Wares), FirmKind::Brewery => Some(Good::Ale), _ => None } }
    pub fn input(self) -> Option<Good> { match self { FirmKind::Workshop => Some(Good::Wood), FirmKind::Brewery => Some(Good::Food), FirmKind::Tavern => Some(Good::Ale), _ => None } }
    pub fn role(self) -> Role {
        match self { FirmKind::Farm => Role::Farmer, FirmKind::Lumberyard => Role::Lumberjack, FirmKind::Workshop => Role::Crafter, FirmKind::Brewery => Role::Brewer, FirmKind::Tavern => Role::Tavernkeeper, FirmKind::Clinic => Role::Healer, FirmKind::School => Role::Teacher, FirmKind::Watch => Role::Enforcer, FirmKind::Office => Role::Clerk, FirmKind::Works => Role::Laborer, FirmKind::Academy => Role::Scholar }
    }
}

/// A firm's books for one day.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub sold: f32,
    pub revenue: f32,
    pub produced: f32,
    pub wages: f32,
    pub input_cost: f32,
    pub tax: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Firm {
    pub id: FirmId,
    pub kind: FirmKind,
    pub name: String,
    pub place: PlaceId,
    pub owner: Owner,
    /// The jurisdiction that pays for it, for public bodies and state firms.
    pub jur: JurId,
    pub manager: Option<PersonId>,
    pub workers: Vec<PersonId>,
    pub open: bool,
    /// Not operating because operating would be unlawful and the manager complies.
    pub suspended: bool,
    /// Operating in defiance of a statute.
    pub illicit: Option<StatuteId>,
    pub cash: f32,
    /// Output inventory.
    pub stock: f32,
    /// Input inventory.
    pub input: f32,
    /// The price the firm would like to charge.
    pub price: f32,
    /// The price it actually sells at today (after any legal limit it honours).
    pub sell_price: f32,
    pub wage_offer: f32,
    pub vacancies: u32,
    pub capacity: u32,
    pub today: Ledger,
    pub yesterday: Ledger,
    pub avg_sales: f32,
    pub avg_profit: f32,
    /// Effort of workers on site this slot (reset by `economy::on_slot`).
    pub effort_now: f32,
    #[serde(default)]
    pub econ: crate::economy::FirmEcon,
}

// --------------------------------------------------------- world settings

/// How people form a view of something they have not lived through.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum ForecastMode {
    /// Fork the world, apply the change, run it forward and compare wellbeing.
    Fork { horizon_days: u32, samples: u32 },
    /// No foresight: people judge only by how changes have treated them so far,
    /// and judge proposals by their most direct, first-order effect on them.
    Experience,
    /// Fork, but each person trusts the forecast only as far as their education
    /// lets them follow it, leaning on experience for the rest.
    Blend { horizon_days: u32, samples: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Share of a child's preferences set by inheritance (the rest by upbringing, schooling, peers and chance).
    pub heritability: f32,
    pub forecast: ForecastMode,
    /// Spread of individual error in reading a forecast (shrinks with education).
    pub forecast_noise: f32,
    /// Whether townspeople in office draft their own bills.
    pub npc_legislation: bool,
    pub immigration: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings { heritability: 0.5, forecast: ForecastMode::Blend { horizon_days: 21, samples: 1 }, forecast_noise: 0.35, npc_legislation: true, immigration: true }
    }
}

/// Multipliers that research moves. Everything starts at 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum Field {
    /// Farm output per unit of effort.
    Agronomy = 0,
    /// Lumberyard output.
    Forestry,
    /// Workshop and brewery output.
    Craft,
    /// Recovery from illness; lower mortality.
    Medicine,
    /// How much a day of school teaches.
    Pedagogy,
    /// Cheaper construction.
    Building,
    /// Record-keeping: how well clerks audit and registers work.
    Administration,
    /// How much a watcher, post or checkpoint notices.
    Surveillance,
    /// The force an armed enforcer brings to bear.
    Arms,
    /// Better terms from the travelling trader.
    Commerce,
}
pub const N_FIELDS: usize = 10;
pub const ALL_FIELDS: [Field; N_FIELDS] = [Field::Agronomy, Field::Forestry, Field::Craft, Field::Medicine, Field::Pedagogy, Field::Building, Field::Administration, Field::Surveillance, Field::Arms, Field::Commerce];
impl Field {
    pub fn name(self) -> &'static str { ["agronomy", "forestry", "craft", "medicine", "pedagogy", "building", "administration", "surveillance", "arms", "commerce"][self as usize] }
    pub fn from_name(s: &str) -> Option<Field> { ALL_FIELDS.iter().copied().find(|f| f.name() == s) }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tech(pub [f32; N_FIELDS]);
impl Default for Tech { fn default() -> Self { Tech([1.0; N_FIELDS]) } }
impl std::ops::Index<Field> for Tech { type Output = f32; fn index(&self, f: Field) -> &f32 { &self.0[f as usize] } }
impl std::ops::IndexMut<Field> for Tech { fn index_mut(&mut self, f: Field) -> &mut f32 { &mut self.0[f as usize] } }

// ------------------------------------------------------------------- stats

/// Town-wide aggregates, refreshed at the end of each day by `stats`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub day: i32,
    pub population: u32,
    pub adults: u32,
    pub mean_wellbeing: f32,
    pub median_wage: f32,
    pub median_income: f32,
    pub median_wealth: f32,
    pub median_rent: f32,
    pub cost_of_living: f32,
    pub unemployment: f32,
    pub gini: f32,
    /// Cheapest price on offer for each good.
    pub price: [f32; N_GOODS],
    pub drink_price: f32,
    pub clinic_price: f32,
    pub school_price: f32,
    pub treasury: f32,
    pub revenue: f32,
    pub spending: f32,
    pub offences: u32,
    pub punished: u32,
    pub bribes: u32,
    pub thefts: u32,
    pub protesters: u32,
    /// Mean trust in the government, 0..=1.
    pub trust: f32,
    pub births: u32,
    pub deaths: u32,
    pub arrivals: u32,
    pub departures: u32,
    pub enforcers: u32,
    pub homeless: u32,
    pub money_supply: f32,
    /// Mean of each expressed preference across living adults.
    pub pref_mean: [f32; N_PREFS],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChronicleEntry {
    pub slot: u32,
    /// law | politics | economy | crime | people | research | unrest
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub persons: Vec<PersonId>,
    #[serde(default)]
    pub statute: Option<StatuteId>,
}

// ------------------------------------------------------------------- world

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub seed: u64,
    pub rng: Rng,
    /// Absolute slot count since the first midnight.
    pub slot: u32,
    pub town_name: String,
    pub settings: Settings,
    /// 0 for the real world; >0 inside a forecast fork. Forks must not fork again,
    /// must not write to the chronicle, and skip NPC legislation.
    pub depth: u8,

    pub map: Map,
    pub places: Vec<Place>,
    pub persons: Vec<Person>,
    pub households: Vec<Household>,
    pub firms: Vec<Firm>,
    pub polity: crate::polity::Polity,
    pub statutes: Vec<crate::statute::Statute>,
    /// Names of legal statuses; a person's `statuses` bitset indexes this.
    pub status_names: Vec<String>,
    pub tech: Tech,

    pub stats: Stats,
    pub history: Vec<Stats>,
    pub chronicle: Vec<ChronicleEntry>,
    /// The last day whose morning routines have run (-1 before the first).
    #[serde(default = "minus_one")]
    pub day_begun: i32,

    /// Per-module state. Each struct is owned by the named module.
    #[serde(default)]
    pub econ: crate::economy::EconState,
    #[serde(default)]
    pub law: crate::law::LawState,
    #[serde(default)]
    pub enforce: crate::enforce::EnforceState,
    #[serde(default)]
    pub gov: crate::gov::GovState,
    #[serde(default)]
    pub life: crate::life::LifeState,
    #[serde(default)]
    pub demo: crate::demography::DemoState,
    #[serde(default)]
    pub research: crate::research::ResearchState,
    #[serde(default)]
    pub forecast: crate::forecast::ForecastState,
}

fn minus_one() -> i32 { -1 }

impl World {
    /// A world with nothing in it: no map, no people, no government.
    pub fn empty(seed: u64) -> World {
        World {
            seed, rng: Rng::new(seed), slot: 6 * SLOTS_PER_HOUR, town_name: String::new(), settings: Settings::default(), depth: 0,
            map: Map::default(), places: Vec::new(), persons: Vec::new(), households: Vec::new(), firms: Vec::new(),
            polity: Default::default(), statutes: Vec::new(), status_names: Vec::new(), tech: Tech::default(),
            stats: Stats::default(), history: Vec::new(), chronicle: Vec::new(), day_begun: -1,
            econ: Default::default(), law: Default::default(), enforce: Default::default(), gov: Default::default(), life: Default::default(),
            demo: Default::default(), research: Default::default(), forecast: Default::default(),
        }
    }
    /// Active statutes' clauses: (statute, clause index, clause).
    pub fn active_clauses(&self) -> impl Iterator<Item = (&crate::statute::Statute, usize, &crate::statute::Clause)> {
        self.statutes.iter().filter(|s| s.is_active()).flat_map(|s| s.clauses.iter().enumerate().map(move |(i, c)| (s, i, c)))
    }
    pub fn statute(&self, id: StatuteId) -> Option<&crate::statute::Statute> { self.statutes.iter().find(|s| s.id == id) }
    pub fn statute_mut(&mut self, id: StatuteId) -> Option<&mut crate::statute::Statute> { self.statutes.iter_mut().find(|s| s.id == id) }
    /// The ward a place lies in, and the jurisdictions whose statutes bind there (ward first, then ancestors).
    pub fn jurs_at(&self, place: PlaceId) -> Vec<JurId> { self.polity.chain(self.places[place.ix()].ward) }

    // ---- time
    #[inline] pub fn day(&self) -> i32 { (self.slot / SLOTS_PER_DAY) as i32 }
    #[inline] pub fn slot_of_day(&self) -> u32 { self.slot % SLOTS_PER_DAY }
    #[inline] pub fn hour(&self) -> f32 { self.slot_of_day() as f32 / SLOTS_PER_HOUR as f32 }
    #[inline] pub fn weekday(&self) -> u32 { (self.slot / SLOTS_PER_DAY) % DAYS_PER_WEEK }
    #[inline] pub fn is_rest_day(&self) -> bool { self.weekday() == REST_DAY }
    #[inline] pub fn is_night(&self) -> bool { let h = self.hour(); !(5.0..22.0).contains(&h) }
    /// Absolute slot for `hour` today.
    pub fn slot_at(&self, hour: f32) -> u32 { self.day() as u32 * SLOTS_PER_DAY + (hour * SLOTS_PER_HOUR as f32).round() as u32 }
    pub fn is_fork(&self) -> bool { self.depth > 0 }

    // ---- access
    #[inline] pub fn person(&self, id: PersonId) -> &Person { &self.persons[id.ix()] }
    #[inline] pub fn person_mut(&mut self, id: PersonId) -> &mut Person { &mut self.persons[id.ix()] }
    #[inline] pub fn household(&self, id: HouseholdId) -> &Household { &self.households[id.ix()] }
    #[inline] pub fn household_mut(&mut self, id: HouseholdId) -> &mut Household { &mut self.households[id.ix()] }
    #[inline] pub fn place(&self, id: PlaceId) -> &Place { &self.places[id.ix()] }
    #[inline] pub fn place_mut(&mut self, id: PlaceId) -> &mut Place { &mut self.places[id.ix()] }
    #[inline] pub fn firm(&self, id: FirmId) -> &Firm { &self.firms[id.ix()] }
    #[inline] pub fn firm_mut(&mut self, id: FirmId) -> &mut Firm { &mut self.firms[id.ix()] }
    pub fn hh_of(&self, p: PersonId) -> &Household { &self.households[self.persons[p.ix()].household.ix()] }
    pub fn hh_of_mut(&mut self, p: PersonId) -> &mut Household { let h = self.persons[p.ix()].household; &mut self.households[h.ix()] }
    pub fn age(&self, p: PersonId) -> f32 { self.persons[p.ix()].age_on(self.day()) }
    pub fn is_adult(&self, p: PersonId) -> bool { self.age(p) >= ADULT_AGE }

    /// Ids of everyone alive and in town.
    pub fn present(&self) -> Vec<PersonId> { self.persons.iter().filter(|p| p.present).map(|p| p.id).collect() }
    pub fn present_adults(&self) -> Vec<PersonId> { let d = self.day(); self.persons.iter().filter(|p| p.present && p.age_on(d) >= ADULT_AGE).map(|p| p.id).collect() }
    /// Present members of a household.
    pub fn members(&self, h: HouseholdId) -> Vec<PersonId> { self.households[h.ix()].members.iter().copied().filter(|m| self.persons[m.ix()].present).collect() }
    pub fn adults_of(&self, h: HouseholdId) -> Vec<PersonId> { let d = self.day(); self.households[h.ix()].members.iter().copied().filter(|m| { let p = &self.persons[m.ix()]; p.present && p.age_on(d) >= ADULT_AGE }).collect() }
    /// The eldest present adult of a household: who answers for it.
    pub fn head_of(&self, h: HouseholdId) -> Option<PersonId> { self.adults_of(h).into_iter().min_by_key(|m| self.persons[m.ix()].birth_day) }
    pub fn properties_of(&self, h: HouseholdId) -> Vec<PlaceId> { self.places.iter().filter(|p| p.owner == Owner::Household(h)).map(|p| p.id).collect() }
    pub fn home_of(&self, p: PersonId) -> Option<PlaceId> { self.hh_of(p).home }
    /// People currently inside/at a place (not counting those still walking to it).
    pub fn people_at(&self, place: PlaceId) -> Vec<PersonId> { self.persons.iter().filter(|p| p.present && p.loc == place && p.travel.is_none()).map(|p| p.id).collect() }
    pub fn firm_of_kind(&self, kind: FirmKind) -> Option<FirmId> { self.firms.iter().find(|f| f.kind == kind && f.open).map(|f| f.id) }
    pub fn place_of_kind(&self, kind: PlaceKind) -> Option<PlaceId> { self.places.iter().find(|p| p.kind == kind).map(|p| p.id) }

    /// Slots to walk between two places (at least 1 if they differ).
    pub fn travel_slots(&self, a: PlaceId, b: PlaceId) -> u32 {
        if a == b { return 0; }
        let n = self.map.travel_n;
        if a.ix() < n && b.ix() < n { return self.map.travel[a.ix() * n + b.ix()].max(1) as u32; }
        // Places built after generation: estimate from door positions (about 15 tiles per slot).
        let (pa, pb) = (&self.places[a.ix()], &self.places[b.ix()]);
        let d = (pa.door.0 - pb.door.0).abs() + (pa.door.1 - pb.door.1).abs();
        ((d as f32 / 13.0).ceil() as u32).max(1)
    }
    /// The street pseudo-place of a ward.
    pub fn street_of(&self, ward: JurId) -> PlaceId {
        let ix = self.polity.wards.iter().position(|w| *w == ward).unwrap_or(0);
        self.map.streets.get(ix).copied().unwrap_or(self.map.plaza)
    }

    // ---- logging
    /// A line in a person's own diary.
    pub fn note(&mut self, p: PersonId, kind: &str, text: impl Into<String>) {
        if self.depth > 0 { return; }
        let slot = self.slot;
        let d = &mut self.persons[p.ix()].diary;
        d.push(DiaryEntry { slot, text: text.into(), kind: kind.to_string() });
        if d.len() > 120 { let cut = d.len() - 120; d.drain(0..cut); }
    }
    /// A line in the town chronicle.
    pub fn chronicle(&mut self, kind: &str, text: impl Into<String>) {
        self.chronicle_about(kind, text, Vec::new(), None);
    }
    pub fn chronicle_about(&mut self, kind: &str, text: impl Into<String>, persons: Vec<PersonId>, statute: Option<StatuteId>) {
        if self.depth > 0 { return; }
        self.chronicle.push(ChronicleEntry { slot: self.slot, kind: kind.to_string(), text: text.into(), persons, statute });
        if self.chronicle.len() > 600 { let cut = self.chronicle.len() - 600; self.chronicle.drain(0..cut); }
    }

    // ---- statuses
    pub fn status_ix(&self, name: &str) -> Option<usize> { self.status_names.iter().position(|s| s.eq_ignore_ascii_case(name)) }
    /// Index of a status name, registering it if new (up to 64).
    pub fn status_ix_or_add(&mut self, name: &str) -> Option<usize> {
        if let Some(i) = self.status_ix(name) { return Some(i); }
        if self.status_names.len() >= 64 { return None; }
        self.status_names.push(name.to_string());
        Some(self.status_names.len() - 1)
    }
}
