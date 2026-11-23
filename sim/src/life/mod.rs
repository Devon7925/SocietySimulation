//! Daily life: needs, the choice of what to do next, and wellbeing - the one
//! number each person is trying to raise, built from their base preferences.
//!
//! OWNER: life agent. Contract in docs/DESIGN.md §Life.
//!
//! Layout:
//!  * `needs`     - what a slot does to hunger, rest, company, fun and health;
//!  * `moves`     - walking between places, arriving, beginning an activity;
//!  * `plan`      - the choice of what to do next (the daily planner);
//!  * `acts`      - what happens when an activity starts and when it ends;
//!  * `wellbeing` - the ten components, their nightly smoothing, and trust;
//!  * `opinion`   - views of statutes and bills;
//!  * `voice`     - the words people use about their own day.
//!
//! Nothing here gives anyone a goal. A person has needs, circumstances and
//! sixteen base preferences; what they do follows from those.
use crate::types::*;
use serde::{Deserialize, Serialize};

mod acts;
mod moves;
mod needs;
mod opinion;
mod plan;
mod voice;
mod wellbeing;
#[cfg(test)]
mod tests;

pub use plan::{operates_today, shift_hours};

/// Places and firms the planner looks up constantly (found once a morning).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Refs {
    pub valid: bool,
    pub day: i32,
    pub n_places: u32,
    pub n_firms: u32,
    pub park: Option<PlaceId>,
    pub chapel: Option<PlaceId>,
    pub townhall: Option<PlaceId>,
    pub watchhouse: Option<PlaceId>,
    pub tavern: Option<FirmId>,
    pub clinic: Option<FirmId>,
    pub school: Option<FirmId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LifeState {
    pub refs: Refs,
    /// Per household: the slot until which one of them is out doing the marketing.
    pub shop_until: Vec<u32>,
    /// Each statute's count of punishments as of last night, to tell which
    /// rules people were punished under today.
    pub punished_seen: Vec<(StatuteId, u32)>,
}

/// What a person has lived through under one statute in force.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Lived {
    pub sid: StatuteId,
    /// Day they first lived under it.
    pub since: i32,
    /// Their wellbeing when it took effect.
    pub wb_at: f32,
    /// Its direct cost (-) or payment (+) to them, in days of income per day.
    pub direct: f32,
    /// Things it stopped them doing or made them do, in days of income (fades).
    pub hit: f32,
    pub known: bool,
}

/// Flags in `Mind::flags`: things already done or decided today.
pub(crate) mod flag {
    pub const SOUGHT: u32 = 1 << 0;
    pub const LUNCHED: u32 = 1 << 1;
    pub const TAVERN: u32 = 1 << 2;
    pub const VISITED: u32 = 1 << 3;
    pub const CHAPEL: u32 = 1 << 4;
    pub const CLINIC: u32 = 1 << 5;
    pub const PILFER: u32 = 1 << 6;
    pub const HOME_ALE: u32 = 1 << 7;
    pub const PROTEST_NOTED: u32 = 1 << 8;
    pub const BALLOT_ASKED: u32 = 1 << 9;
    pub const BALLOT_GO: u32 = 1 << 10;
    pub const CORVEE_ASKED: u32 = 1 << 11;
    pub const CORVEE_GO: u32 = 1 << 12;
    pub const ATTEND_ASKED: u32 = 1 << 13;
    pub const ATTEND_GO: u32 = 1 << 14;
    pub const SCHOOL_PAID: u32 = 1 << 15;
    pub const WORK_NOTED: u32 = 1 << 16;
    pub const MISSED_BREAKFAST: u32 = 1 << 17;
    pub const MISSED_DINNER: u32 = 1 << 18;
    pub const MISSED_SUPPER: u32 = 1 << 19;
    pub const VOTED: u32 = 1 << 20;
    pub const INFORM_ASKED: u32 = 1 << 21;
    pub const BARRED_NOTED: u32 = 1 << 22;
    pub const SHOP_NOTED: u32 = 1 << 23;
}

/// Per-person planner state and the history wellbeing and trust are judged against.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Mind {
    /// The day `flags` and the daily counters belong to.
    pub day: i32,
    pub flags: u32,
    /// Food carried for a meal away from home.
    pub lunch: f32,
    pub drinks: u8,
    pub meals: u8,
    /// Meals gone without today.
    pub missed: u8,
    /// Desperate measures tried today.
    pub hard_n: u8,
    /// Slots spent today at work, on compelled labour or locked up.
    pub toil: u16,
    /// Not to try working again before this slot (the law forbade it).
    pub no_work_until: u32,
    /// Slot after which they may be asked again whether they mean to protest.
    pub protest_ask_at: u32,
    /// The statute whose summons they have already weighed today.
    pub attend_sid: StatuteId,
    /// Slots of compulsory labour done this week.
    pub labor_week: u16,
    /// Sums of hunger, energy, company and fun over today's slots.
    pub sums: [f32; 4],
    pub ticks: u16,

    /// False until the first night's wellbeing has been taken.
    pub seen: bool,
    /// Their usual wellbeing: a slow average, what "how they used to fare" means.
    pub base: f32,
    /// How unsettled their own circumstances have lately been (fades).
    pub upheaval: f32,
    pub snap_job: Option<FirmId>,
    pub snap_home: Option<PlaceId>,
    pub snap_household: HouseholdId,
    pub snap_size: u8,
    pub income_base: f32,
    /// The transfer payments they have come to expect a day.
    pub transfer_base: f32,
    pub last_pay: f32,
    pub lived: Vec<Lived>,
    /// The statute that most recently stopped them doing something, and when.
    pub barred: Option<(StatuteId, u32)>,
}

impl Mind {
    fn new_day(&mut self, day: i32) {
        self.day = day;
        self.flags = 0;
        self.drinks = 0;
        self.meals = 0;
        self.missed = 0;
        self.hard_n = 0;
    }
    #[inline]
    pub(crate) fn has(&self, f: u32) -> bool { self.flags & f != 0 }
    #[inline]
    pub(crate) fn set(&mut self, f: u32) { self.flags |= f; }
}

/// The planner's look-up table, refreshed when the day or the town has changed.
pub(crate) fn refs(w: &mut World) -> Refs {
    let r = w.life.refs;
    if r.valid && r.day == w.day() && r.n_places as usize == w.places.len() && r.n_firms as usize == w.firms.len() { return r; }
    let firm = |k: FirmKind| w.firms.iter().find(|f| f.kind == k && f.open && !f.suspended).or_else(|| w.firms.iter().find(|f| f.kind == k && f.open)).map(|f| f.id);
    let r = Refs {
        valid: true, day: w.day(), n_places: w.places.len() as u32, n_firms: w.firms.len() as u32,
        park: w.place_of_kind(PlaceKind::Park), chapel: w.place_of_kind(PlaceKind::Chapel),
        townhall: w.place_of_kind(PlaceKind::Townhall), watchhouse: w.place_of_kind(PlaceKind::Watchhouse),
        tavern: firm(FirmKind::Tavern), clinic: firm(FirmKind::Clinic), school: firm(FirmKind::School),
    };
    w.life.refs = r;
    r
}

/// Morning: per-person resets and the day's standing decisions.
pub fn start_of_day(w: &mut World) {
    w.life.refs.valid = false;
    refs(w);
    for s in w.life.shop_until.iter_mut() { *s = 0; }
    let day = w.day();
    let monday = w.weekday() == 0;
    for p in w.persons.iter_mut() {
        if !p.present { continue; }
        p.mind.new_day(day);
        if monday { p.mind.labor_week = 0; }
    }
    plan::morning_decisions(w);
}

/// One slot of one person's life: needs tick; arrive if a walk is done; when
/// the current activity ends, choose the next.
pub fn update_person(w: &mut World, pid: PersonId) {
    let slot = w.slot;
    let i = pid.ix();
    // ---- every slot (kept cheap: no allocation, no searching)
    let arriving = {
        let Some(p) = w.persons.get_mut(i) else { return };
        if !p.present { return; }
        let homeless = w.households[p.household.ix()].home.is_none();
        needs::tick(p, homeless, (slot / SLOTS_PER_DAY) as i32, w.tech[Field::Pedagogy]);
        match p.activity {
            Activity::Work => {
                if let Some(j) = &p.job {
                    if let Some(f) = w.firms.get_mut(j.firm.ix()) {
                        if f.place == p.loc { f.effort_now += p.work.effort; p.work.slots_today += 1; }
                    }
                }
            }
            Activity::Patrol => p.work.slots_today += 1,
            Activity::Travel => if matches!(&p.next, Some(n) if n.activity == Activity::Patrol) { p.work.slots_today += 1; p.mind.toil += 1; },
            _ => {}
        }
        match p.travel {
            Some(t) => { if slot < t.arrive { return; } true }
            None => { if slot < p.until { return; } false }
        }
    };
    // ---- something changes
    if arriving { moves::arrive(w, pid); } else { turn(w, pid); }
    // Whatever happened, nobody is left with nothing to do.
    let p = &mut w.persons[i];
    if p.present && p.travel.is_none() && p.until <= slot {
        if let Some(u) = p.jailed_until { p.activity = Activity::Jailed; p.until = u.max(slot + 1); }
        else { if matches!(p.activity, Activity::Travel) { p.activity = Activity::Idle; } p.until = slot + 1 + (pid.0 & 1); }
    }
}

/// An activity has run its course: finish it and pick the next.
fn turn(w: &mut World, pid: PersonId) {
    let slot = w.slot;
    let i = pid.ix();
    let ended = w.persons[i].activity;
    acts::on_end(w, pid, ended);
    if moves::busy(w, pid) && w.persons[i].jailed_until.is_none() { return; }
    if let Some(u) = w.persons[i].jailed_until {
        if slot < u {
            let fork = w.is_fork();
            let p = &mut w.persons[i];
            if p.activity == Activity::Protest { p.civic.protesting = false; }
            if p.activity != Activity::Jailed && !fork { p.doing = "Locked in the Watch House".to_string(); }
            p.activity = Activity::Jailed;
            p.until = u;
            return;
        }
        crate::enforce::release(w, pid, "sentence served");
        let p = &mut w.persons[i];
        p.jailed_until = None;
        p.needs.hunger = p.needs.hunger.min(50.0);
        if p.activity == Activity::Jailed { p.activity = Activity::Idle; }
        if moves::busy(w, pid) { return; }
    }
    plan::choose(w, pid);
}

/// Night: fold the day into each person's wellbeing and trust; refresh opinions of statutes.
pub fn end_of_day(w: &mut World) {
    voice::evening_notes(w);
    wellbeing::nightly(w);
    opinion::nightly(w);
    if w.weekday() == REST_DAY { wellbeing::tend_friendships(w); }
}

/// Send a person walking to a place to begin `plan` on arrival (immediately if already there).
///
/// Someone stepping out of a building of their own accord at an hour when
/// being outside is forbidden decides first whether to obey (`law::attempt`);
/// if they obey, or are stopped, they stay where they are. People being
/// marched somewhere (to jail, out of town) are not asked.
pub fn go(w: &mut World, p: PersonId, to: PlaceId, plan: Plan) {
    if p.ix() >= w.persons.len() || to.ix() >= w.places.len() { return; }
    moves::set_out(w, p, to, plan, 2.0);
}

/// Wellbeing as it stands this instant, from current state (no smoothing).
pub fn wellbeing_now(w: &World, p: PersonId) -> Wellbeing {
    wellbeing::now(w, p)
}

/// Recompute everyone's opinion of one statute or bill now (used when a bill is introduced).
pub fn form_opinions(w: &mut World, sid: StatuteId) {
    opinion::form(w, sid);
}
