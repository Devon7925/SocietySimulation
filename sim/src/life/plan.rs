//! What each person does with their day. Nobody follows a script: every time
//! an activity ends, the person looks at the hour, their needs, their purse
//! and the law, and picks what to do next.
use super::moves::{begin, plan, resent, set_out, taken, walk};
use super::{flag, refs, voice};
use crate::enforce::{self, Trail};
use crate::law::{self, Duty, Verdict};
use crate::statute::{Act, Action, Clause, Conduct};
use crate::types::*;
use crate::{decide, economy, gov, scope};

macro_rules! think {
    ($w:expr, $p:expr, $($arg:tt)*) => { if !$w.is_fork() { let t = format!($($arg)*); $w.persons[$p.ix()].thought = t; } };
}
macro_rules! note {
    ($w:expr, $p:expr, $kind:expr, $($arg:tt)*) => { if !$w.is_fork() { let t = format!($($arg)*); $w.note($p, $kind, t); } };
}

/// The working day of each kind of firm, in hours.
pub fn shift_hours(kind: FirmKind) -> (f32, f32) {
    match kind {
        FirmKind::Farm | FirmKind::Lumberyard | FirmKind::Workshop | FirmKind::Brewery => (7.0, 17.0),
        FirmKind::Tavern => (14.0, 23.0),
        FirmKind::Clinic | FirmKind::Office | FirmKind::Academy => (8.0, 17.0),
        FirmKind::School => (8.0, 15.0),
        FirmKind::Works => (7.0, 16.0),
        FirmKind::Watch => (6.0, 18.0),
    }
}
/// Only the tavern and the watch keep the rest day as a working day.
pub fn operates_today(kind: FirmKind, weekday: u32) -> bool {
    weekday != REST_DAY || matches!(kind, FirmKind::Tavern | FirmKind::Watch)
}

fn in_window(hr: f32, start: f32, end: f32) -> bool { if start <= end { hr >= start && hr < end } else { hr >= start || hr < end } }
fn hours_until(hr: f32, target: f32) -> f32 { (target - hr + 24.0) % 24.0 }
fn mealtime(hr: f32) -> bool { (5.0..8.5).contains(&hr) || (11.7..14.0).contains(&hr) || (17.3..20.5).contains(&hr) }
fn market_open(hr: f32) -> bool { (7.0..19.0).contains(&hr) }
/// The hour at which pupils set out for school.
const SCHOOL_OPENS: f32 = 7.6;

/// The facts a decision starts from, read once.
#[derive(Clone, Copy)]
pub(super) struct Ctx {
    pub pid: PersonId,
    pub slot: u32,
    pub hr: f32,
    pub wd: u32,
    pub age: f32,
    pub kid: bool,
    pub hh: HouseholdId,
    pub home: Option<PlaceId>,
    pub fork: bool,
}

fn ctx(w: &mut World, pid: PersonId) -> Ctx {
    let day = w.day();
    let p = &mut w.persons[pid.ix()];
    if p.mind.day != day { p.mind.new_day(day); }
    let age = p.age_on(day);
    let hh = p.household;
    Ctx { pid, slot: w.slot, hr: w.hour(), wd: w.weekday(), age, kid: age < ADULT_AGE, hh, home: w.households[hh.ix()].home, fork: w.is_fork() }
}

fn rint(w: &mut World, lo: u32, hi: u32) -> u32 { w.rng.int(lo as i32, hi as i32) as u32 }
fn stock(w: &World, c: &Ctx, g: Good) -> f32 { w.households[c.hh.ix()].stock[g as usize] }
fn money(w: &World, c: &Ctx) -> f32 { w.households[c.hh.ix()].money }
fn me<'a>(w: &'a World, c: &Ctx) -> &'a Person { &w.persons[c.pid.ix()] }
fn has(w: &World, c: &Ctx, f: u32) -> bool { w.persons[c.pid.ix()].mind.has(f) }
fn mark(w: &mut World, c: &Ctx, f: u32) { w.persons[c.pid.ix()].mind.set(f); }

/// The clause behind a duty.
fn conduct(w: &World, d: Duty) -> Option<&Conduct> {
    match w.statute(d.statute)?.clauses.get(d.clause)? { Clause::Conduct(c) => Some(c), _ => None }
}

/// A duty binds this person and keeping it would cost them `cost` coins. They
/// weigh that against the sanction (`decide::would_break`); the law is told
/// what they chose. True if they keep it.
fn keep_duty(w: &mut World, pid: PersonId, d: Duty, cost: f32, place: PlaceId, trail: Trail) -> bool {
    let at_stake = match conduct(w, d) {
        Some(c) => { let fine = scope::person_num(w, &c.sanction.fine, pid, 0.0).max(0.0); decide::sanction_cost(w, pid, &c.sanction, fine) }
        None => 0.0,
    };
    let p_detect = enforce::detection_chance(w, pid, d.statute, place, trail);
    let breaks = decide::would_break(w, pid, d.statute, cost, p_detect, at_stake);
    law::duty_outcome(w, pid, d, !breaks, cost);
    if !breaks && cost > 0.0 { resent(w, pid, d.statute, cost); }
    !breaks
}

// ------------------------------------------------------------------ morning

/// The day's standing decisions: which children go to school, and what those
/// bound to hold a job make of that.
pub(super) fn morning_decisions(w: &mut World) {
    let day = w.day();
    let school = refs(w).school;
    for i in 0..w.persons.len() {
        let pid = PersonId(i as u32);
        let (age, has_job) = { let p = &w.persons[i]; if !p.present { continue; } (p.age_on(day), p.job.is_some()) };
        if (6.0..ADULT_AGE).contains(&age) { enrol(w, pid, school); }
        else if age < 6.0 && w.persons[i].enrolled { w.persons[i].enrolled = false; }
        if age >= ADULT_AGE && has_job {
            // Someone bound to hold a job, who has one, keeps the rule; if the job pays less than they would work for, against their wishes.
            if let Some(d) = law::duty(w, pid, Action::HoldJob) {
                let wage = w.persons[i].job.as_ref().map(|j| if j.owner { f32::MAX } else { j.wage }).unwrap_or(0.0);
                let short = (economy::reservation_wage(w, pid) - wage).max(0.0);
                law::duty_outcome(w, pid, d, true, short);
                if short > 0.0 { resent(w, pid, d.statute, short); }
            }
        }
    }
}

/// The eldest present adult of a household (who answers for the children), found without allocating.
pub(super) fn head_of(w: &World, hh: HouseholdId) -> Option<PersonId> {
    let day = w.day();
    let mut best: Option<(i32, PersonId)> = None;
    for &m in &w.households[hh.ix()].members {
        let p = &w.persons[m.ix()];
        if !p.present || p.age_on(day) < ADULT_AGE { continue; }
        if best.map(|b| p.birth_day < b.0).unwrap_or(true) { best = Some((p.birth_day, m)); }
    }
    best.map(|b| b.1)
}

/// A parent decides whether a child goes to school today: whether a day of
/// lessons is worth its fee to them, given what the household can spare, what
/// schooling did for the parent, and any rule that requires it.
fn enrol(w: &mut World, child: PersonId, school: Option<FirmId>) {
    let ci = child.ix();
    let hh = w.persons[ci].household;
    let Some(sf) = school.filter(|f| { let f = &w.firms[f.ix()]; f.open && !f.suspended }) else { w.persons[ci].enrolled = false; return };
    let Some(parent) = head_of(w, hh) else { w.persons[ci].enrolled = false; return };
    let fee = { let f = &w.firms[sf.ix()]; if f.sell_price > 0.0 { f.sell_price } else { f.price } }.max(0.0) * (1.0 - law::subsidy_rate(w, parent, crate::statute::Priced::Education)).clamp(0.0, 1.0);
    let (worth, felt_fee, can_pay) = {
        let (pa, ch, h) = (&w.persons[parent.ix()], &w.persons[ci], &w.households[hh.ix()]);
        let n = h.members.len().max(1) as f32;
        let col = w.stats.cost_of_living.max(1.0);
        let worth = decide::daily_scale(w, parent) * 0.25 * (0.4 + 0.6 * pa.prefs[Pref::Family]) * (0.4 + 0.6 * pa.prefs[Pref::Patience]) * (0.5 + pa.education) * (1.2 - ch.education);
        // A coin weighs more in a household with little put by.
        let cushion = 10.0 * col * n;
        let pinch = ((cushion - h.money) / cushion).clamp(0.0, 1.0);
        (worth, fee * (1.0 + 3.0 * pinch), h.money >= fee + w.stats.price[Good::Food as usize] * n)
    };
    let mut send = can_pay && worth >= felt_fee;
    if let Some(d) = law::duty(w, child, Action::AttendSchool) {
        if send { law::duty_outcome(w, parent, d, true, 0.0); }
        else {
            let place = w.firms[sf.ix()].place;
            send = can_pay && keep_duty(w, parent, d, (felt_fee - worth).max(0.2), place, Trail::Paper);
            if !can_pay { law::duty_outcome(w, parent, d, false, felt_fee); }
            if send && !w.persons[ci].enrolled {
                let (name, t) = (w.persons[ci].name.clone(), voice::title(w, d.statute));
                note!(w, parent, "law", "Sent {name} to school because \"{t}\" requires it. We can ill spare the fee.");
            }
        }
    }
    w.persons[ci].enrolled = send;
}

// ------------------------------------------------------------------- sleep

fn sleep_window(w: &World, c: &Ctx) -> (f32, f32) {
    let h = c.pid.0.wrapping_mul(2_654_435_761);
    let mut wake = 5.4 + ((h >> 8) & 0xFF) as f32 / 255.0;
    let mut bed = 21.0 + 1.8 * ((h >> 16) & 0xFF) as f32 / 255.0;
    if let Some(j) = &me(w, c).job {
        if let Some(f) = w.firms.get(j.firm.ix()) { if shift_hours(f.kind).1 > 22.0 { wake = 8.5; bed = 0.8; } }
    }
    if c.age < 12.0 { bed = bed.min(20.5); }
    (wake, bed)
}

fn sleep(w: &mut World, c: &Ctx, nap: Option<u32>) {
    let (wake, _) = sleep_window(w, c);
    let slots = match nap { Some(n) => n, None => ((hours_until(c.hr, wake) * SLOTS_PER_HOUR as f32).round() as u32 + rint(w, 0, 1)).max(2) };
    match c.home {
        Some(home) => {
            if set_out(w, c.pid, home, plan(w, Activity::Sleep, slots, "Asleep at home", 0), 10.0) || taken(w, c.pid) { return; }
            begin(w, c.pid, plan(w, Activity::Sleep, slots, "Dozing where I am until I may go home", 0));
        }
        None => {
            let green = refs(w).park.unwrap_or(w.map.plaza);
            let line = if w.places[green.ix()].kind == PlaceKind::Park { "Sleeping rough on the green" } else { "Sleeping rough in the plaza" };
            if set_out(w, c.pid, green, plan(w, Activity::Sleep, slots, line, 0), 10.0) || taken(w, c.pid) { return; }
            begin(w, c.pid, plan(w, Activity::Sleep, slots, "Sleeping where I can", 0));
        }
    }
}

fn stay_home(w: &mut World, c: &Ctx, line: Option<&str>, slots: u32) {
    let Some(home) = c.home else {
        let plaza = w.map.plaza;
        if !set_out(w, c.pid, plaza, plan(w, Activity::Idle, slots, "With nowhere to go", 0), 2.0) && !taken(w, c.pid) {
            begin(w, c.pid, plan(w, Activity::Idle, 2, "Waiting where I am", 0));
        }
        return;
    };
    let (temperance, done_ale) = { let p = me(w, c); (p.prefs[Pref::Temperance], p.mind.has(flag::HOME_ALE)) };
    let mut then = match line {
        Some(l) => plan(w, Activity::Home, slots, l, 0),
        None if c.kid => plan(w, Activity::Home, slots, "Playing at home", 0),
        None => { let l = voice::pick(w, c.pid, &["Resting at home", "Doing chores at home", "Mending and tidying at home", "At home with the family"]); plan(w, Activity::Home, slots, l, 0) }
    };
    if line.is_none() && c.age >= 18.0 && c.hr >= 18.0 && stock(w, c, Good::Ale) >= 0.5 && temperance < 0.55 && !done_ale {
        mark(w, c, flag::HOME_ALE);
        then = plan(w, Activity::Home, slots, "Having a cup of ale at home", 1);
    } else if line.is_none() && c.age >= 50.0 && c.hr > 8.0 && c.hr < 18.0 && w.rng.chance(0.4) {
        then = plan(w, Activity::Garden, slots, "Tending the garden", 0);
    }
    // Getting home is worth a good deal, even at some risk.
    if !set_out(w, c.pid, home, then, 6.0) && !taken(w, c.pid) {
        begin(w, c.pid, plan(w, Activity::Idle, 2, "Waiting where I am", 0));
    }
}

fn leave(w: &mut World, c: &Ctx) {
    let loc = me(w, c).loc;
    let gate = w.map.gates.iter().copied().min_by_key(|g| w.travel_slots(loc, *g));
    match gate {
        Some(g) => walk(w, c.pid, g, plan(w, Activity::Leaving, 4, "Leaving town", 0)),
        None => begin(w, c.pid, plan(w, Activity::Leaving, 4, "Leaving town", 0)),
    }
}

// -------------------------------------------------------------------- food

/// A meal went by with nothing to eat: counted once for each meal of the day, and written down.
pub(super) fn missed_meal(w: &mut World, pid: PersonId, why: &str) {
    let hr = w.hour();
    let f = voice::meal_flag(hr);
    let m = &mut w.persons[pid.ix()].mind;
    if m.has(f) { return; }
    m.set(f);
    m.missed = m.missed.saturating_add(1);
    let meal = voice::meal_name(hr).to_lowercase();
    note!(w, pid, "hardship", "No {meal} today: {why}");
}

fn eat(w: &mut World, c: &Ctx, urgent: bool) {
    let q = if c.age < 12.0 { 0.3 } else { 0.5 };
    let (loc, lunch, hunger) = { let p = me(w, c); (p.loc, p.mind.lunch, p.needs.hunger) };
    let at_home = c.home == Some(loc);
    let arg_q = (q * 100.0) as i64;
    if lunch >= q && !at_home { begin(w, c.pid, plan(w, Activity::Eat, 1, "Eating a packed meal", 1000 + arg_q)); return; }
    if stock(w, c, Good::Food) >= q {
        match c.home {
            Some(home) => {
                let line = if c.fork { String::new() } else { format!("{} at home", voice::meal_name(c.hr)) };
                set_out(w, c.pid, home, Plan { activity: Activity::Eat, slots: 2, doing: line, arg: arg_q }, 8.0);
            }
            None => begin(w, c.pid, plan(w, Activity::Eat, 1, "Eating what little we have", 2000 + arg_q)),
        }
        return;
    }
    let food_price = w.stats.price[Good::Food as usize];
    if c.age >= 14.0 && market_open(c.hr) && money(w, c) >= food_price * 0.6 && shop_free(w, c) { go_shop(w, c, "We are out of food."); return; }
    if !urgent && hunger < 70.0 { return; }
    if c.age < 10.0 { return; }
    hardship(w, c);
}

/// Nothing to eat and no way to buy it: kin and friends first, then whatever is left.
fn hardship(w: &mut World, c: &Ctx) {
    {
        let m = &mut w.persons[c.pid.ix()].mind;
        if m.hard_n >= 2 { return; }
        m.hard_n += 1;
    }
    missed_meal(w, c.pid, if money(w, c) < w.stats.price[Good::Food as usize] { "nothing in the house and no coin for the market." } else { "nothing in the house." });
    // First, those who care for us.
    let mut kin: Vec<PersonId> = Vec::with_capacity(6);
    {
        let p = me(w, c);
        kin.extend(p.mother);
        kin.extend(p.father);
        kin.extend(p.children.iter().copied());
        let mut friends: Vec<Friend> = p.friends.clone();
        friends.sort_by(|a, b| b.affinity.total_cmp(&a.affinity).then(a.id.cmp(&b.id)));
        kin.extend(friends.iter().map(|f| f.id));
        let day = w.day();
        kin.retain(|k| { let o = &w.persons[k.ix()]; o.present && o.household != c.hh && o.age_on(day) >= ADULT_AGE && o.jailed_until.is_none() });
        kin.truncate(4);
    }
    for k in kin {
        let kh = w.persons[k.ix()].household;
        let (their_food, their_n) = { let h = &w.households[kh.ix()]; (h.stock[Good::Food as usize], h.members.len() as f32) };
        if their_food <= their_n * 2.0 { continue; }
        let care = decide::regard(w, k, c.pid);
        if !w.rng.chance((0.15 + care).min(0.95)) { continue; }
        let g = (their_food * 0.25).min(2.0);
        w.households[kh.ix()].stock[Good::Food as usize] -= g;
        w.households[c.hh.ix()].stock[Good::Food as usize] += g;
        if !c.fork {
            let (giver, taker) = (w.persons[k.ix()].full_name(), me(w, c).full_name());
            w.note(c.pid, "hardship", format!("{giver} shared {g:.1} food with us. We had nothing."));
            w.note(k, "kindness", format!("Gave {g:.1} food to {taker}, who had none."));
        }
        eat(w, c, true);
        return;
    }
    let (hunger, conformity, community) = { let p = me(w, c); (p.needs.hunger, p.prefs[Pref::Conformity], p.prefs[Pref::Community]) };
    if hunger > 86.0 && c.age >= 12.0 {
        let bent = (1.0 - conformity * 0.9 - community * 0.2 + if hunger > 95.0 { 0.4 } else { 0.0 }).clamp(0.0, 1.0);
        if w.rng.chance(bent) {
            think!(w, c.pid, "I cannot watch us starve. I will take what we need.");
            let market = w.map.market;
            if set_out(w, c.pid, market, plan(w, Activity::Steal, 1, "Loitering by the food stalls", Good::Food as i64 * 100 + 20), 10.0) || taken(w, c.pid) { return; }
        }
    }
    if (6.0..18.0).contains(&c.hr) && w.rng.chance(0.6) {
        think!(w, c.pid, "Nothing in the larder. Perhaps the forest will feed us.");
        let (forest, slots) = (w.map.forest, rint(w, 7, 11));
        if set_out(w, c.pid, forest, plan(w, Activity::Forage, slots, "Foraging in the forest", 0), 6.0) || taken(w, c.pid) { return; }
    }
    if (8.0..20.0).contains(&c.hr) {
        think!(w, c.pid, "I have no choice but to beg.");
        let plaza = w.map.plaza;
        set_out(w, c.pid, plaza, plan(w, Activity::Beg, 6, "Begging in the plaza", 0), 4.0);
    }
}

// ---------------------------------------------------------------- shopping

fn shop_free(w: &World, c: &Ctx) -> bool { w.life.shop_until.get(c.hh.ix()).map(|u| *u <= c.slot).unwrap_or(true) }

fn needs_shopping(w: &World, c: &Ctx) -> bool {
    let h = &w.households[c.hh.ix()];
    if !shop_free(w, c) || h.money < 1.0 { return false; }
    let n = h.members.len() as f32;
    h.stock[Good::Food as usize] < n * 0.85 * 1.5 || (h.stock[Good::Wares as usize] < n * 0.35 && h.money > 25.0) || (h.stock[Good::Wood as usize] < n * 0.15 && h.money > 15.0)
}

fn go_shop(w: &mut World, c: &Ctx, why: &str) {
    let ix = c.hh.ix();
    if w.life.shop_until.len() <= ix { w.life.shop_until.resize(w.households.len().max(ix + 1), 0); }
    w.life.shop_until[ix] = c.slot + 10;
    think!(w, c.pid, "{why}");
    let (market, slots) = (w.map.market, rint(w, 1, 2));
    if !set_out(w, c.pid, market, plan(w, Activity::Shop, slots, "Shopping at the market", 0), 8.0) { w.life.shop_until[ix] = 0; }
}

// -------------------------------------------------------------------- work

fn on_shift(w: &World, c: &Ctx) -> bool {
    let Some(j) = &me(w, c).job else { return false };
    let Some(f) = w.firms.get(j.firm.ix()) else { return false };
    if !f.open || f.suspended || !operates_today(f.kind, c.wd) || j.role == Role::Enforcer { return false; }
    let (st, en) = shift_hours(f.kind);
    c.hr >= st && c.hr < en - 0.2
}

/// Enforcers stand where their unit posts them.
fn patrol(w: &mut World, c: &Ctx) {
    let Some(post) = enforce::post_for(w, c.pid) else { return };
    if taken(w, c.pid) || post.ix() >= w.places.len() { return; }
    let slots = rint(w, 2, 4);
    pack_lunch(w, c, 0.5);
    if !has(w, c, flag::WORK_NOTED) {
        mark(w, c, flag::WORK_NOTED);
        let at = voice::place_label(w, post);
        note!(w, c.pid, "work", "On watch today. First post: {at}.");
    }
    let line = if c.fork { String::new() } else { format!("On watch at {}", voice::place_label(w, post)) };
    walk(w, c.pid, post, Plan { activity: Activity::Patrol, slots, doing: line, arg: 0 });
}

/// Leaving home for the day: take something to eat along, if the larder can spare it.
fn pack_lunch(w: &mut World, c: &Ctx, q: f32) {
    let p = me(w, c);
    if c.home != Some(p.loc) || p.mind.lunch >= q || stock(w, c, Good::Food) < q * 3.0 { return; }
    w.households[c.hh.ix()].stock[Good::Food as usize] -= q;
    w.persons[c.pid.ix()].mind.lunch += q;
}

fn work(w: &mut World, c: &Ctx) {
    let Some((firm, wage, role, owner)) = me(w, c).job.as_ref().map(|j| (j.firm, j.wage, j.role, j.owner)) else { return };
    if role == Role::Enforcer { return; }
    let Some(f) = w.firms.get(firm.ix()) else { return };
    if !f.open || f.suspended || !operates_today(f.kind, c.wd) { return; }
    let (st, en) = shift_hours(f.kind);
    let place = f.place;
    let (loc, worked, barred_until) = { let p = me(w, c); (p.loc, p.work.slots_today, p.mind.no_work_until) };
    let lead = (w.travel_slots(loc, place) as f32 / SLOTS_PER_HOUR as f32).max(0.25);
    if c.hr < st - lead || c.hr >= en - 0.2 || barred_until > c.slot { return; }
    // The working day comes in blocks, with a break for dinner.
    let end_hr = if st < 11.0 && en > 14.0 && c.hr < 12.0 { 12.0 } else { en };
    let slots = (((end_hr - c.hr) * SLOTS_PER_HOUR as f32).round() as i32).clamp(1, 8) as u32;
    let day_pay = if owner { decide::daily_scale(w, c.pid) } else { wage };
    let value = (day_pay * slots as f32 / 32.0).max(1.0);
    let mut act = Act::new(Action::Work, place, value);
    act.wage = wage;
    act.hours_today = worked as f32 / SLOTS_PER_HOUR as f32;
    act.firm = Some(firm);
    match law::attempt(w, c.pid, &act) {
        Verdict::Lawful | Verdict::Violated(_) => {}
        Verdict::Refrained(sid) => {
            w.persons[c.pid.ix()].mind.no_work_until = c.slot + 8;
            resent(w, c.pid, sid, value);
            think!(w, c.pid, "The law says I may not work now.");
            if !c.fork && !has(w, c, flag::BARRED_NOTED) {
                mark(w, c, flag::BARRED_NOTED);
                let t = voice::title(w, sid);
                w.note(c.pid, "law", format!("Kept from my work by \"{t}\". That is pay I will not see."));
            }
            return;
        }
        Verdict::Stopped(_) => { w.persons[c.pid.ix()].mind.no_work_until = c.slot + 8; return; }
    }
    if taken(w, c.pid) { return; }
    pack_lunch(w, c, 0.5);
    let line = if c.fork { String::new() } else { voice::work_line(role, &w.firms[firm.ix()].name) };
    set_out(w, c.pid, place, Plan { activity: Activity::Work, slots, doing: line, arg: firm.0 as i64 }, day_pay.max(1.0));
}

fn seek_work(w: &mut World, c: &Ctx) {
    if has(w, c, flag::SOUGHT) || c.hr < 6.5 || c.hr > 15.0 || c.age >= 72.0 { return; }
    mark(w, c, flag::SOUGHT);
    let mut floor = economy::reservation_wage(w, c.pid);
    // A rule that requires holding a job: take whatever is going, or defy it.
    if c.age >= ADULT_AGE {
        if let Some(d) = law::duty(w, c.pid, Action::HoldJob) {
            let below = economy::best_vacancy(w, c.pid, floor).is_none();
            if below {
                let any = economy::best_vacancy(w, c.pid, 0.0).map(|v| v.2);
                let here = me(w, c).loc;
                match any {
                    Some(net) => {
                        if keep_duty(w, c.pid, d, (floor - net).max(0.5), here, Trail::Paper) {
                            floor = 0.0;
                            let t = voice::title(w, d.statute);
                            note!(w, c.pid, "law", "\"{t}\" says I must work, so I will take what there is, though it pays {net:.1} a day.");
                        }
                    }
                    None => { law::duty_outcome(w, c.pid, d, false, 0.0); }
                }
                if taken(w, c.pid) { return; }
            }
        }
    }
    match economy::best_vacancy(w, c.pid, floor) {
        Some((firm, _wage, net)) if firm.ix() < w.firms.len() => {
            let (place, name) = { let f = &w.firms[firm.ix()]; (f.place, if c.fork { String::new() } else { f.name.clone() }) };
            think!(w, c.pid, "I hear {name} is hiring.");
            let line = if c.fork { String::new() } else { format!("Asking for work at {name}") };
            set_out(w, c.pid, place, Plan { activity: Activity::Apply, slots: 1, doing: line, arg: firm.0 as i64 }, net * 3.0);
        }
        _ => {
            if c.fork { return; }
            match economy::best_vacancy(w, c.pid, 0.0) {
                Some((_, _, net)) => { think!(w, c.pid, "The only work going pays {net:.1} a day. Not worth my while."); }
                None => { think!(w, c.pid, "There is no work to be had."); }
            }
        }
    }
}

// ------------------------------------------------------------------ duties

fn must_attend(w: &mut World, c: &Ctx) {
    let Some(d) = law::duty(w, c.pid, Action::Attend) else { return };
    let kind = conduct(w, d).map(|k| k.place.trim().to_ascii_lowercase()).unwrap_or_default();
    let place = if kind == "home" { c.home } else { w.places.iter().find(|p| p.kind.name() == kind).map(|p| p.id) }.unwrap_or(w.map.plaza);
    let decided = { let m = &me(w, c).mind; m.has(flag::ATTEND_ASKED) && m.attend_sid == d.statute };
    if !decided {
        let cost = decide::daily_scale(w, c.pid) * (0.2 + 0.3 * me(w, c).prefs[Pref::Autonomy]);
        let keeps = keep_duty(w, c.pid, d, cost, place, Trail::Seen);
        let m = &mut w.persons[c.pid.ix()].mind;
        m.attend_sid = d.statute;
        m.flags = (m.flags | flag::ATTEND_ASKED) & !flag::ATTEND_GO;
        if keeps {
            m.set(flag::ATTEND_GO);
            if !c.fork {
                let (t, at) = (voice::title(w, d.statute), voice::place_label(w, place));
                let line = match voice::vary(w, c.pid, 2) { 0 => format!("Went to {at} as \"{t}\" requires. I had other plans for the hour."), _ => format!("Summoned to {at} under \"{t}\". I went, not gladly.") };
                w.note(c.pid, "law", line);
            }
        } else {
            think!(w, c.pid, "They may summon me. I am not going.");
        }
    }
    if taken(w, c.pid) || !has(w, c, flag::ATTEND_GO) { return; }
    let line = if c.fork { String::new() } else { format!("Attending as required by \"{}\"", voice::title(w, d.statute)) };
    walk(w, c.pid, place, Plan { activity: Activity::Attend, slots: 2, doing: line, arg: d.statute.0 as i64 });
}

fn corvee(w: &mut World, c: &Ctx) {
    if c.kid || c.hr < 7.0 || c.hr > 17.0 { return; }
    let Some(d) = law::duty(w, c.pid, Action::PublicLabor) else { return };
    let per_week = conduct(w, d).map(|k| k.hours_per_week).unwrap_or(0.0);
    let owed = (per_week * SLOTS_PER_HOUR as f32) as i32 - me(w, c).mind.labor_week as i32;
    if owed <= 0 { return; }
    let site = refs(w).park.unwrap_or(w.map.plaza);
    if !has(w, c, flag::CORVEE_ASKED) {
        mark(w, c, flag::CORVEE_ASKED);
        // Unpaid hours cost what the time is worth to them, more to those who mind labour.
        let cost = decide::daily_scale(w, c.pid) / 32.0 * owed.min(8) as f32 * (1.5 - me(w, c).prefs[Pref::Diligence]);
        if keep_duty(w, c.pid, d, cost, site, Trail::Paper) {
            mark(w, c, flag::CORVEE_GO);
            let t = voice::title(w, d.statute);
            note!(w, c.pid, "law", "Gave my hours to the public works, as \"{t}\" demands. Nobody pays me for them.");
        } else {
            think!(w, c.pid, "Let them dig their own ditches.");
        }
    }
    if taken(w, c.pid) || !has(w, c, flag::CORVEE_GO) { return; }
    set_out(w, c.pid, site, plan(w, Activity::Corvee, owed.min(8) as u32, "Doing compulsory labour on the public works", owed.min(8) as i64), 3.0);
}

/// Where the law requires people to tell the watch what they have seen.
fn inform(w: &mut World, c: &Ctx) {
    if c.kid || has(w, c, flag::INFORM_ASKED) || me(w, c).felt.disorder <= 0.0 || !(7.0..20.0).contains(&c.hr) { return; }
    let Some(d) = law::duty(w, c.pid, Action::Inform) else { return };
    mark(w, c, flag::INFORM_ASKED);
    let Some(house) = refs(w).watchhouse else { return };
    let cost = decide::daily_scale(w, c.pid) * (0.1 + 0.3 * me(w, c).prefs[Pref::Autonomy]);
    if !keep_duty(w, c.pid, d, cost, house, Trail::Paper) || taken(w, c.pid) { return; }
    set_out(w, c.pid, house, plan(w, Activity::Attend, 1, "Telling the watch what I saw", d.statute.0 as i64), 2.0);
}

fn vote(w: &mut World, c: &Ctx) {
    if !(8.0..17.6).contains(&c.hr) || (has(w, c, flag::BALLOT_ASKED) && !has(w, c, flag::BALLOT_GO)) { return; }
    if !gov::has_ballot(w, c.pid) { return; }
    let hall = refs(w).townhall.unwrap_or(w.map.plaza);
    if !has(w, c, flag::BALLOT_ASKED) {
        mark(w, c, flag::BALLOT_ASKED);
        let goes = match law::duty(w, c.pid, Action::Vote) {
            Some(d) => { let cost = decide::daily_scale(w, c.pid) * 0.1; keep_duty(w, c.pid, d, cost, hall, Trail::Paper) }
            None => {
                // People turn out when they care about what is on the ballot, and when turning out is what one does.
                let p = me(w, c);
                let keen = w.polity.bills.iter().filter(|b| matches!(b.state, crate::polity::BillState::Voting { .. })).map(|b| p.opinion(b.statute.id).unwrap_or(0.0).abs()).fold(0.0f32, f32::max);
                let turnout = 0.4 + 0.25 * p.prefs[Pref::Conformity] + 0.15 * p.prefs[Pref::Community] + 0.3 * keen;
                w.rng.chance(turnout)
            }
        };
        if goes { mark(w, c, flag::BALLOT_GO); }
    }
    if taken(w, c.pid) || !has(w, c, flag::BALLOT_GO) { return; }
    set_out(w, c.pid, hall, plan(w, Activity::Vote, 1, "Casting a vote at the Town Hall", 0), 3.0);
}

fn protest(w: &mut World, c: &Ctx, wish: Option<bool>) {
    if c.kid || c.hr < 9.0 || c.hr > 16.5 || wish == Some(false) { return; }
    if me(w, c).mind.protest_ask_at > c.slot { return; }
    w.persons[c.pid.ix()].mind.protest_ask_at = c.slot + 8;
    let wants = wish == Some(true) || gov::wants_to_protest(w, c.pid);
    if !wants || taken(w, c.pid) { return; }
    let plaza = w.map.plaza;
    let value = decide::daily_scale(w, c.pid) * 0.4 * (1.0 + 2.0 * (-me(w, c).wellbeing.total).max(0.0));
    match law::attempt(w, c.pid, &Act::new(Action::Protest, plaza, value)) {
        Verdict::Lawful | Verdict::Violated(_) => {}
        Verdict::Refrained(sid) => {
            resent(w, c.pid, sid, value);
            think!(w, c.pid, "I would be in the plaza shouting, if it were not forbidden.");
            return;
        }
        Verdict::Stopped(_) => return,
    }
    if taken(w, c.pid) { return; }
    if !c.fork {
        let p = me(w, c);
        let worst = p.opinions.iter().filter(|(s, _)| w.statute(*s).map(|s| s.is_active()).unwrap_or(false)).fold(None, |a: Option<(StatuteId, f32)>, b| if a.map(|x| b.1 < x.1).unwrap_or(true) { Some(*b) } else { a });
        let cause = worst.filter(|x| x.1 < -0.2).map(|x| voice::title(w, x.0));
        let first = !p.mind.has(flag::PROTEST_NOTED);
        w.persons[c.pid.ix()].thought = match &cause { Some(t) => format!("Enough. \"{t}\" has to go."), None => "Things cannot go on like this.".to_string() };
        if first {
            mark(w, c, flag::PROTEST_NOTED);
            w.note(c.pid, "politics", match &cause { Some(t) => format!("Joined the protest in the plaza against \"{t}\"."), None => "Joined the protest in the plaza.".to_string() });
        }
    }
    let slots = rint(w, 6, 11);
    set_out(w, c.pid, plaza, plan(w, Activity::Protest, slots, "Protesting in the plaza", 0), value.max(8.0));
}

// ------------------------------------------------------------------ errands

fn errands(w: &mut World, c: &Ctx) {
    if market_open(c.hr) && needs_shopping(w, c) && (!c.kid || head_of(w, c.hh).is_none()) { go_shop(w, c, "Time to go to market."); return; }
    if me(w, c).needs.health < 55.0 && (8.0..16.5).contains(&c.hr) && !has(w, c, flag::CLINIC) {
        mark(w, c, flag::CLINIC);
        let Some(firm) = refs(w).clinic else { return };
        let (place, price, ok, name) = { let f = &w.firms[firm.ix()]; (f.place, if f.sell_price > 0.0 { f.sell_price } else { f.price }, f.open && !f.suspended, if c.fork { String::new() } else { f.name.clone() }) };
        if !ok { return; }
        if money(w, c) <= price + 4.0 {
            note!(w, c.pid, "health", "I am not well, but the healer's {price:.1} coins is more than we can spare.");
            return;
        }
        if !visit_ok(w, c, place, 8.0, None) { return; }
        think!(w, c.pid, "I am not well. I should see the healer.");
        let line = if c.fork { String::new() } else { format!("Seeing the healer at {name}") };
        set_out(w, c.pid, place, Plan { activity: Activity::Treatment, slots: 3, doing: line, arg: firm.0 as i64 }, 10.0);
    }
}

/// May they go there? (Rules about visiting places; the person decides whether to mind them.)
fn visit_ok(w: &mut World, c: &Ctx, place: PlaceId, value: f32, whom: Option<PersonId>) -> bool {
    let mut act = Act::new(Action::Visit, place, value);
    act.other = whom;
    match law::attempt(w, c.pid, &act) {
        Verdict::Lawful | Verdict::Violated(_) => !taken(w, c.pid),
        Verdict::Refrained(sid) => {
            resent(w, c.pid, sid, value);
            if !c.fork {
                let (at, t) = (voice::place_label(w, place), voice::title(w, sid));
                w.persons[c.pid.ix()].thought = format!("I would go to {at}, but \"{t}\" forbids it.");
            }
            false
        }
        Verdict::Stopped(_) => false,
    }
}

// ------------------------------------------------------------------ leisure

#[derive(Clone, Copy, PartialEq)]
enum Pastime { Tavern, Visit, Stroll, Chapel, Home, Pilfer }

/// A friend to call on, nearer friends more often.
fn pick_friend(w: &mut World, c: &Ctx) -> Option<PersonId> {
    // In a town this size people know roughly who is in: they call on friends who are at home and up.
    let ok = |w: &World, f: &Friend| {
        let o = &w.persons[f.id.ix()];
        o.present && o.jailed_until.is_none() && o.household != c.hh && o.travel.is_none() && o.activity != Activity::Sleep && w.households[o.household.ix()].home == Some(o.loc)
    };
    let total: f32 = me(w, c).friends.iter().filter(|f| ok(w, f)).map(|f| f.affinity * f.affinity).sum();
    if total <= 0.0 { return None; }
    let mut x = w.rng.f32() * total;
    let mut last = None;
    for f in &me(w, c).friends {
        if !ok(w, f) { continue; }
        last = Some(f.id);
        x -= f.affinity * f.affinity;
        if x <= 0.0 { break; }
    }
    last
}

/// Slots left before the day's work or lessons call, if they are still ahead today.
fn slots_before_duty(w: &World, c: &Ctx) -> Option<u32> {
    let p = me(w, c);
    let starts = if let Some(j) = &p.job {
        let f = w.firms.get(j.firm.ix())?;
        if !f.open || f.suspended || !operates_today(f.kind, c.wd) || j.role == Role::Enforcer { return None; }
        shift_hours(f.kind).0 - (w.travel_slots(p.loc, f.place) as f32 / SLOTS_PER_HOUR as f32).max(0.25)
    } else if c.kid && p.enrolled && c.wd != REST_DAY { SCHOOL_OPENS } else { return None };
    if c.hr >= starts { return None; }
    Some((((starts - c.hr) * SLOTS_PER_HOUR as f32).ceil() as u32).max(1))
}

fn leisure(w: &mut World, c: &Ctx) {
    let dark = c.hr >= 20.5 || c.hr < 5.5;
    if c.kid && (c.hr >= 19.0 || c.hr < 6.5) { let s = rint(w, 3, 5); stay_home(w, c, Some("At home with the family"), s); return; }
    // With work or lessons soon to start there is no time to go anywhere.
    let spare = slots_before_duty(w, c).unwrap_or(u32::MAX);
    if spare <= 4 { stay_home(w, c, None, spare); return; }
    let r = refs(w);
    let mut opts = [(Pastime::Home, f32::MIN); 6];
    let mut n = 0;
    let mut push = |o: Pastime, s: f32| { opts[n] = (o, s); n += 1; };
    {
        let p = me(w, c);
        let h = &w.households[c.hh.ix()];
        let (fun, social) = (p.needs.fun, p.needs.social);
        let pr = &p.prefs;
        if let Some(t) = r.tavern.map(|t| &w.firms[t.ix()]) {
            let price = if t.sell_price > 0.0 { t.sell_price } else { t.price };
            if !c.kid && t.open && !t.suspended && c.hr >= 15.0 && c.hr < 22.6 && h.money > price * 2.0 + 6.0 {
                push(Pastime::Tavern, (100.0 - fun) / 100.0 * 1.1 + pr[Pref::Sociability] * 0.8 + (1.0 - pr[Pref::Temperance]) * 0.9 - pr[Pref::Patience] * 0.4 - if p.mind.has(flag::TAVERN) { 1.2 } else { 0.0 } + if c.wd >= 5 { 0.25 } else { 0.0 });
            }
        }
        if !p.friends.is_empty() && c.hr >= 9.0 && c.hr < 20.5 {
            push(Pastime::Visit, (100.0 - social) / 100.0 + pr[Pref::Sociability] * 0.5 - if p.mind.has(flag::VISITED) { 0.8 } else { 0.0 });
        }
        if !dark {
            let amenity = w.polity.jurisdictions.get(w.polity.town.ix()).map(|j| j.amenity).unwrap_or(0.0);
            push(Pastime::Stroll, 0.45 + (100.0 - fun) / 160.0 + if c.kid { 0.9 } else { 0.0 } + amenity / 120.0);
        }
        if c.wd == REST_DAY && c.hr >= 7.5 && c.hr < 10.5 && !c.kid && r.chapel.is_some() && !p.mind.has(flag::CHAPEL) {
            push(Pastime::Chapel, 0.5 + pr[Pref::Continuity] * 1.6 + pr[Pref::Community] * 0.3);
        }
        push(Pastime::Home, 0.62 + if p.needs.energy < 40.0 { 0.8 } else { 0.0 } + if dark { 0.5 } else { 0.0 } + if c.age > 60.0 { 0.2 } else { 0.0 });
        // The light-fingered look for their chance.
        if !c.kid && pr[Pref::Wealth] > 0.72 && pr[Pref::Community] < 0.3 && pr[Pref::Conformity] < 0.34 && market_open(c.hr) && !p.mind.has(flag::PILFER) {
            push(Pastime::Pilfer, 0.9 + pr[Pref::Risk] * 0.5);
        }
    }
    let mut best = (Pastime::Home, f32::MIN);
    for &(o, s) in opts.iter().take(n) { let s = s + w.rng.f32() * 0.5; if s > best.1 { best = (o, s); } }
    let (sociability, temperance, continuity) = { let pr = &me(w, c).prefs; (pr[Pref::Sociability], pr[Pref::Temperance], pr[Pref::Continuity]) };
    match best.0 {
        Pastime::Tavern => {
            mark(w, c, flag::TAVERN);
            if let Some(t) = r.tavern {
                let place = w.firms[t.ix()].place;
                if visit_ok(w, c, place, 3.0 + (1.0 - temperance) * 3.0, None) {
                    let name = if c.fork { String::new() } else { w.firms[t.ix()].name.clone() };
                    if !c.fork { w.persons[c.pid.ix()].thought = match voice::vary(w, c.pid, 3) { 0 => "A drink and some company would do me good.".to_string(), 1 => format!("Off to {name}."), _ => "I have earned a drink.".to_string() }; }
                    let slots = rint(w, 4, 9).min(spare);
                    let line = if c.fork { String::new() } else { format!("Drinking at {name}") };
                    if set_out(w, c.pid, place, Plan { activity: Activity::Tavern, slots, doing: line, arg: t.0 as i64 }, 3.0 + sociability * 4.0) { return; }
                }
            }
        }
        Pastime::Visit => {
            mark(w, c, flag::VISITED);
            if let Some(fr) = pick_friend(w, c) {
                if let Some(their) = w.households[w.persons[fr.ix()].household.ix()].home {
                    if visit_ok(w, c, their, 2.0, Some(fr)) {
                        let name = if c.fork { String::new() } else { w.persons[fr.ix()].full_name() };
                        think!(w, c.pid, "I should call on {}.", w.persons[fr.ix()].name);
                        let slots = rint(w, 3, 7).min(spare);
                        let line = if c.fork { String::new() } else { format!("Visiting {name}") };
                        if set_out(w, c.pid, their, Plan { activity: Activity::Visit, slots, doing: line, arg: fr.0 as i64 }, 2.0 + sociability * 3.0) { return; }
                    }
                }
            }
        }
        Pastime::Stroll => {
            let park = if w.rng.chance(0.5) { r.park } else { None };
            let place = park.unwrap_or(w.map.plaza);
            if visit_ok(w, c, place, 2.0, None) {
                if !c.fork && voice::vary(w, c.pid, 2) == 0 { w.persons[c.pid.ix()].thought = if c.kid { "Out to play.".to_string() } else { voice::pick(w, c.pid, &["A walk will clear my head.", "Some air, I think.", "I will see who is about."]).to_string() }; }
                let slots = rint(w, 3, 6).min(spare);
                let line = match (c.kid, park.is_some()) { (true, true) => "Playing on the green", (true, false) => "Playing in the plaza", (false, true) => "Strolling on the green", (false, false) => "Passing the time in the plaza" };
                if set_out(w, c.pid, place, plan(w, if c.kid { Activity::Play } else { Activity::Stroll }, slots, line, 0), 2.0) { return; }
            }
        }
        Pastime::Chapel => {
            mark(w, c, flag::CHAPEL);
            if let Some(chapel) = r.chapel {
                if visit_ok(w, c, chapel, 2.0 + continuity * 6.0, None) {
                    think!(w, c.pid, "Restday. To the chapel.");
                    let slots = rint(w, 5, 7);
                    if set_out(w, c.pid, chapel, plan(w, Activity::Chapel, slots, "At the Restday gathering", 0), 2.0 + continuity * 5.0) { return; }
                }
            }
        }
        Pastime::Pilfer => {
            mark(w, c, flag::PILFER);
            let (market, good) = (w.map.market, if w.rng.chance(0.5) { Good::Wares } else { Good::Ale });
            if set_out(w, c.pid, market, plan(w, Activity::Steal, 1, "Lingering by the stalls", good as i64 * 100 + 10), 4.0) { return; }
        }
        Pastime::Home => {}
    }
    if taken(w, c.pid) { return; }
    let slots = rint(w, 3, 6).min(spare);
    stay_home(w, c, None, slots);
}

// --------------------------------------------------------------- the choice

/// The current activity is over. Decide what comes next.
pub(super) fn choose(w: &mut World, pid: PersonId) {
    let c = &ctx(w, pid);
    w.persons[pid.ix()].civic.protesting = false;
    // After each possibility: if it claimed the person (or the law did), stop.
    macro_rules! step { ($e:expr) => { $e; if taken(w, pid) { return; } }; }

    // The player, living as this person, decides the large things themselves.
    let over = if w.polity.player.possessing == Some(pid) { Some(w.polity.player.overrides.clone()) } else { None };
    let (wish_protest, wish_rebel, wish_leave) = match &over { Some(o) => (o.protest, o.rebel, o.emigrate), None => (None, None, None) };

    if me(w, c).activity == Activity::Leaving {
        if wish_leave == Some(false) && !me(w, c).doing.contains("exile") { w.persons[pid.ix()].activity = Activity::Idle; }
        else { leave(w, c); return; }
    } else if wish_leave == Some(true) {
        w.polity.player.overrides.emigrate = None;
        let here = me(w, c).loc;
        let value = decide::daily_scale(w, pid) * 10.0;
        if law::attempt(w, pid, &Act::new(Action::Emigrate, here, value)).proceeds() && !taken(w, pid) {
            note!(w, pid, "home", "I have made up my mind to leave this town.");
            leave(w, c);
            return;
        }
        if taken(w, pid) { return; }
        think!(w, pid, "I would leave, but they do not let people go.");
    }
    if c.age < 6.0 {
        // The very young are fed and kept at home by the household.
        if me(w, c).needs.hunger > 45.0 && stock(w, c, Good::Food) >= 0.25 { begin(w, pid, plan(w, Activity::Eat, 1, "Being fed", 2025)); return; }
        stay_home(w, c, Some(if c.age < 2.0 { "In the cradle" } else { "Playing at home" }), 4);
        return;
    }

    let (energy, enforcer) = { let p = me(w, c); (p.needs.energy, matches!(&p.job, Some(j) if j.role == Role::Enforcer)) };
    let rebelling = wish_rebel.unwrap_or(me(w, c).civic.rebelling);
    // Enforcers keep the hours of their unit, night or day, stopping only to eat.
    if enforcer && energy >= 8.0 && !rebelling {
        let hunger = me(w, c).needs.hunger;
        if hunger > 78.0 { step!(eat(w, c, true)); }
        else if (12.0..13.2).contains(&c.hr) && !has(w, c, flag::LUNCHED) { mark(w, c, flag::LUNCHED); if hunger > 28.0 { step!(eat(w, c, false)); } }
        step!(patrol(w, c));
    }
    let (wake, bed) = sleep_window(w, c);
    if !in_window(c.hr, wake, bed) { sleep(w, c, None); return; }
    if energy < 8.0 { sleep(w, c, Some(8)); return; }
    // Off a night watch: sleep by day.
    if enforcer && energy < 30.0 { let n = (((85.0 - energy) / 3.15).ceil() as u32).clamp(4, 32); sleep(w, c, Some(n)); return; }

    if rebelling && c.age >= 14.0 && me(w, c).needs.hunger < 90.0 {
        let (plaza, slots) = (w.map.plaza, rint(w, 4, 8));
        think!(w, pid, "No more of this. We end it today.");
        walk(w, pid, plaza, plan(w, Activity::Rebel, slots, "In the plaza with the rising", 0));
        return;
    }
    step!(must_attend(w, c));
    if me(w, c).needs.hunger > 78.0 { step!(eat(w, c, true)); }
    let shift = on_shift(w, c);
    if shift && (12.0..13.2).contains(&c.hr) && !has(w, c, flag::LUNCHED) {
        mark(w, c, flag::LUNCHED);
        if me(w, c).needs.hunger > 28.0 { step!(eat(w, c, false)); }
    }
    if !shift && !c.kid { step!(vote(w, c)); }
    if !shift && me(w, c).needs.hunger > if mealtime(c.hr) { 36.0 } else { 66.0 } { step!(eat(w, c, false)); }
    // Someone who means to protest today leaves their work to do it.
    step!(protest(w, c, wish_protest));
    if me(w, c).job.is_some() { step!(work(w, c)); }
    if c.kid && c.age >= 6.0 && me(w, c).enrolled && c.wd != REST_DAY && (SCHOOL_OPENS..13.8).contains(&c.hr) { step!(school(w, c)); }
    if c.age >= 14.0 {
        step!(corvee(w, c));
        step!(inform(w, c));
        step!(errands(w, c));
        let (jobless, enrolled) = { let p = me(w, c); (p.job.is_none(), p.enrolled) };
        if jobless && !c.kid { step!(seek_work(w, c)); }
        // Children of households with nothing put by look for work too.
        if jobless && c.kid && !enrolled && money(w, c) / (w.households[c.hh.ix()].members.len().max(1) as f32) < 6.0 { step!(seek_work(w, c)); }
    }
    leisure(w, c);
}

fn school(w: &mut World, c: &Ctx) {
    let Some(sf) = refs(w).school else { return };
    let (place, open) = { let f = &w.firms[sf.ix()]; (f.place, f.open && !f.suspended) };
    if !open { return; }
    pack_lunch(w, c, 0.3);
    // Lessons come in blocks, with a break at noon to eat.
    let end_hr = if c.hr < 12.0 { 12.0 } else { 14.0 };
    let slots = (((end_hr - c.hr) * SLOTS_PER_HOUR as f32).round() as i32).clamp(1, 8) as u32;
    let line = if c.fork { String::new() } else { format!("At lessons in {}", w.firms[sf.ix()].name) };
    set_out(w, c.pid, place, Plan { activity: Activity::School, slots, doing: line, arg: sf.0 as i64 }, 3.0);
}
