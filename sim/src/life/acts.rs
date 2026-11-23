//! What happens when an activity begins (the meal is eaten, the shopping is
//! done, the drink is poured) and when it ends (the forager comes home with
//! what they found).
use super::moves::resent;
use super::needs::MEAL;
use super::{flag, plan::{head_of, missed_meal}, voice};
use crate::law::{self, Verdict};
use crate::statute::{Act, Action, Priced};
use crate::types::*;
use crate::{decide, demography, economy, gov};

macro_rules! think {
    ($w:expr, $p:expr, $($arg:tt)*) => { if !$w.is_fork() { let t = format!($($arg)*); $w.persons[$p.ix()].thought = t; } };
}
macro_rules! note {
    ($w:expr, $p:expr, $kind:expr, $($arg:tt)*) => { if !$w.is_fork() { let t = format!($($arg)*); $w.note($p, $kind, t); } };
}

/// Is the person still doing what was just begun? (A purchase or an offence
/// in the middle of it may have had them marched off.)
fn still(w: &World, pid: PersonId, act: Activity, until: u32) -> bool {
    let p = &w.persons[pid.ix()];
    p.present && p.travel.is_none() && p.activity == act && p.until == until && p.jailed_until.is_none()
}

/// The activity came to nothing: stand about briefly and think again.
fn cut(w: &mut World, pid: PersonId, line: &str) {
    let (slot, fork) = (w.slot, w.is_fork());
    let p = &mut w.persons[pid.ix()];
    p.activity = Activity::Idle;
    p.until = slot + 1;
    if !fork { p.doing = line.to_string(); }
}

pub(super) fn on_start(w: &mut World, pid: PersonId, act: Activity, arg: i64) {
    let i = pid.ix();
    let until = w.persons[i].until;
    match act {
        Activity::Eat => meal(w, pid, arg, until),
        Activity::Shop => shopping(w, pid, until),
        Activity::Work => start_work(w, pid),
        Activity::Patrol => { let e = economy::effort_of(w, pid); w.persons[i].work.effort = e; }
        Activity::Tavern => tavern(w, pid, arg, until),
        Activity::Treatment => treatment(w, pid, arg, until),
        Activity::Apply => apply(w, pid, arg),
        Activity::Vote => {
            gov::cast_ballots(w, pid);
            let p = &mut w.persons[i];
            // Having a say is part of what makes a government theirs.
            p.trust = (p.trust + 0.01).min(1.0);
            p.mind.set(flag::VOTED);
        }
        Activity::School => lessons(w, pid, arg, until),
        Activity::Steal => steal(w, pid, arg, until),
        Activity::Forage => {
            let (loc, hunger) = { let p = &w.persons[i]; (p.loc, p.needs.hunger) };
            let value = 3.0 + hunger / 20.0;
            let v = law::attempt(w, pid, &Act::new(Action::Forage, loc, value).good(Good::Food, w.stats.price[Good::Food as usize], 0.75));
            if !v.proceeds() {
                if let Verdict::Refrained(sid) = v { resent(w, pid, sid, value); }
                if still(w, pid, act, until) { cut(w, pid, "Turning back from the forest"); think!(w, pid, "Gathering in the forest is forbidden."); }
            }
        }
        Activity::Visit => visit(w, pid, arg, until),
        Activity::Chapel | Activity::Stroll | Activity::Play => {
            mingle(w, pid);
            if voice::vary(w, pid, 3) == 0 { voice::muse(w, pid); }
        }
        Activity::Protest => { w.persons[i].civic.protesting = true; mingle(w, pid); }
        Activity::Leaving => {
            let at_gate = w.places[w.persons[i].loc.ix()].kind == PlaceKind::Gate || w.map.gates.is_empty();
            if at_gate {
                let why = if w.persons[i].doing.contains("exile") { "exiled" } else { "emigrated" };
                demography::depart(w, pid, why);
            }
        }
        Activity::Home => {
            if arg == 1 { home_ale(w, pid, until); }
            else if voice::vary(w, pid, 2) == 0 { voice::muse(w, pid); }
        }
        Activity::Garden => voice::muse(w, pid),
        // `arg` = the slots of labour this spell discharges.
        Activity::Corvee => { let m = &mut w.persons[i].mind; m.labor_week = m.labor_week.saturating_add(arg.clamp(0, 96) as u16); }
        _ => {}
    }
}

pub(super) fn on_end(w: &mut World, pid: PersonId, act: Activity) {
    let i = pid.ix();
    match act {
        Activity::Forage => {
            // What the forest gives is found, not bought: goods enter the town here.
            let got = w.rng.range(0.4, 1.1);
            let hh = w.persons[i].household;
            w.households[hh.ix()].stock[Good::Food as usize] += got;
            note!(w, pid, "hardship", "Gathered roots and berries in the forest ({got:.1} food).");
        }
        Activity::Beg => {
            // Alms pass from purse to purse; no coin is made.
            let day = w.day();
            let mine = w.persons[i].household;
            let mut got = 0.0;
            for _ in 0..4 {
                let d = PersonId(w.rng.index(w.persons.len()) as u32);
                let (ok, dh) = { let o = &w.persons[d.ix()]; (o.present && d != pid && o.household != mine && o.age_on(day) >= ADULT_AGE, o.household) };
                if !ok || w.households[dh.ix()].money <= 40.0 { continue; }
                let care = decide::regard(w, d, pid);
                if !w.rng.chance((care * 4.0).min(0.9)) { continue; }
                let g = w.rng.range(0.5, 2.0);
                w.households[dh.ix()].money -= g;
                got += g;
            }
            w.households[mine.ix()].money += got;
            if got > 0.0 { note!(w, pid, "hardship", "Begged in the plaza. Kind souls gave {got:.1} coins."); }
            else { note!(w, pid, "hardship", "Begged in the plaza. Nobody gave."); }
        }
        Activity::Corvee => {
            let town = w.polity.town;
            if let Some(j) = w.polity.jurisdictions.get_mut(town.ix()) { j.amenity = (j.amenity + 0.05).min(100.0); }
        }
        Activity::Tavern => {
            if w.persons[i].mind.drinks >= 4 {
                let p = &mut w.persons[i];
                p.needs.energy = (p.needs.energy - 10.0).max(0.0);
                think!(w, pid, "My head. I had more than was good for me.");
            }
        }
        Activity::Protest => w.persons[i].civic.protesting = false,
        _ => {}
    }
}

// -------------------------------------------------------------------- food

/// `arg` = source * 1000 + quantity * 100; source 0 the larder at home, 1 a packed meal, 2 the household's stock eaten where they stand.
fn meal(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let i = pid.ix();
    let (src, q) = (arg / 1000, (arg % 1000) as f32 / 100.0);
    let hh = w.persons[i].household;
    let have = if src == 1 { w.persons[i].mind.lunch } else { w.households[hh.ix()].stock[Good::Food as usize] };
    if have + 1e-4 < q {
        cut(w, pid, "Finding the larder bare");
        think!(w, pid, "There is nothing to eat at home.");
        missed_meal(w, pid, "the larder was bare when I got home.");
        return;
    }
    let (loc, hunger) = { let p = &w.persons[i]; (p.loc, p.needs.hunger) };
    let price = w.stats.price[Good::Food as usize];
    let value = price * q * (1.0 + hunger / 20.0);
    let v = law::attempt(w, pid, &Act::new(Action::Consume, loc, value).good(Good::Food, price, q));
    if !v.proceeds() {
        if let Verdict::Refrained(sid) = v {
            resent(w, pid, sid, value);
            if !w.is_fork() { let t = voice::title(w, sid); w.persons[i].thought = format!("I am hungry, and \"{t}\" says I may not eat."); }
        }
        if still(w, pid, Activity::Eat, until) { cut(w, pid, "Going without"); }
        return;
    }
    if !still(w, pid, Activity::Eat, until) { return; }
    if src == 1 { w.persons[i].mind.lunch -= q; } else { w.households[hh.ix()].stock[Good::Food as usize] -= q; }
    let p = &mut w.persons[i];
    p.needs.hunger = (p.needs.hunger - MEAL).max(0.0);
    p.mind.meals = p.mind.meals.saturating_add(1);
}

fn home_ale(w: &mut World, pid: PersonId, until: u32) {
    let i = pid.ix();
    let hh = w.persons[i].household;
    let (loc, temperance) = { let p = &w.persons[i]; (p.loc, p.prefs[Pref::Temperance]) };
    let price = w.stats.price[Good::Ale as usize];
    let value = 2.0 + (1.0 - temperance) * 4.0;
    let v = law::attempt(w, pid, &Act::new(Action::Consume, loc, value).good(Good::Ale, price, 0.5));
    if !still(w, pid, Activity::Home, until) { return; }
    if v.proceeds() && w.households[hh.ix()].stock[Good::Ale as usize] >= 0.5 {
        w.households[hh.ix()].stock[Good::Ale as usize] -= 0.5;
        let p = &mut w.persons[i];
        p.needs.fun = (p.needs.fun + 14.0).min(100.0);
        p.mind.drinks = p.mind.drinks.saturating_add(1);
        if !w.is_fork() { let line = voice::pick(w, pid, &["A cup of ale by the fire.", "Had a quiet cup of ale at home.", "Poured myself some of our ale this evening."]); w.note(pid, "leisure", line); }
    } else {
        if let Verdict::Refrained(sid) = v {
            resent(w, pid, sid, value);
            think!(w, pid, "There is ale in the house and I may not touch it.");
        }
        if !w.is_fork() { w.persons[i].doing = "Resting at home".to_string(); }
    }
}

// ---------------------------------------------------------------- shopping

fn shopping(w: &mut World, pid: PersonId, until: u32) {
    let i = pid.ix();
    let day = w.day();
    let hh = w.persons[i].household;
    let h = hh.ix();
    let (mut n, mut need, mut fond, mut adults) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for &m in &w.households[h].members {
        let o = &w.persons[m.ix()];
        if !o.present { continue; }
        let age = o.age_on(day);
        n += 1.0;
        need += if age < 12.0 { 0.6 } else { 1.0 };
        if age >= ADULT_AGE { adults += 1.0; fond += 1.0 - o.prefs[Pref::Temperance]; }
    }
    let fond = if adults > 0.0 { fond / adults } else { 0.0 };
    let px = w.stats.price;
    let price = |g: Good| px[g as usize].max(0.1);
    let before = w.households[h].money;
    let mut got = [0.0f32; N_GOODS];
    let food = w.households[h].stock[Good::Food as usize];
    let mut want = need * 3.2 - food;
    if before < need * price(Good::Food) * 3.0 { want = want.min(need * 1.6 - food); }
    if want > 0.3 {
        let limit = (Good::Food.base_price() * 6.0).min((price(Good::Food) * 1.3).max(before / need.max(1.0)));
        got[Good::Food as usize] = economy::buy_good(w, pid, Good::Food, want, limit);
        if !still(w, pid, Activity::Shop, until) { return; }
    }
    let rent = w.households[h].home.map(|hm| { let pl = &w.places[hm.ix()]; if pl.tenant == Some(hh) { pl.rent } else { 0.0 } }).unwrap_or(0.0);
    // What is left once food and rent for some days are put aside.
    let spare = |w: &World| (w.households[h].money - need * price(Good::Food) * 4.0 - rent * 4.0).max(0.0);
    let wares = w.households[h].stock[Good::Wares as usize];
    if wares < n * 0.45 {
        let q = (n - wares).min(spare(w) * 0.45 / price(Good::Wares));
        if q > 0.2 { got[Good::Wares as usize] = economy::buy_good(w, pid, Good::Wares, q, price(Good::Wares) * 1.25); if !still(w, pid, Activity::Shop, until) { return; } }
    }
    if w.households[h].stock[Good::Wood as usize] < n * 0.25 {
        let q = (n * 0.6).min(spare(w) * 0.2 / price(Good::Wood));
        if q > 0.2 { got[Good::Wood as usize] = economy::buy_good(w, pid, Good::Wood, q, price(Good::Wood) * 1.25); if !still(w, pid, Activity::Shop, until) { return; } }
    }
    if fond > 0.42 && w.households[h].stock[Good::Ale as usize] < 0.5 {
        let q = (spare(w) * 0.14 * fond * 2.0 / price(Good::Ale)).min(1.5);
        if q >= 0.5 { got[Good::Ale as usize] = economy::buy_good(w, pid, Good::Ale, q, price(Good::Ale) * 1.3); if !still(w, pid, Activity::Shop, until) { return; } }
    }
    // Done for now. A household that came away without the food it went for does not go straight back.
    let empty_handed = want > 0.3 && got[Good::Food as usize] < want * 0.25;
    let again = if empty_handed { w.slot + 12 } else { 0 };
    if let Some(u) = w.life.shop_until.get_mut(h) { *u = again; }
    if w.is_fork() { return; }
    let spent = before - w.households[h].money;
    let mut parts = String::new();
    for g in ALL_GOODS {
        let v = got[g as usize];
        if v > 0.05 { if !parts.is_empty() { parts.push_str(", "); } parts.push_str(&format!("{v:.1} {}", g.name())); }
    }
    if !parts.is_empty() {
        let line = match voice::vary(w, pid, 3) { 0 => format!("Bought {parts} at the market for {spent:.1} coins."), 1 => format!("At the market: {parts}. {spent:.1} coins in all."), _ => format!("Marketing done: {parts}, {spent:.1} coins.") };
        w.note(pid, "money", line);
    } else if want > 0.3 && !w.persons[i].mind.has(flag::SHOP_NOTED) {
        w.persons[i].mind.set(flag::SHOP_NOTED);
        let poor = before < price(Good::Food);
        w.note(pid, "hardship", if poor { "Went to market but could not afford food." } else { "Went to market; could not get what we needed." });
        w.persons[i].thought = if poor { "Food on every stall and not a coin to buy it with.".to_string() } else { "I came home from market with an empty basket.".to_string() };
    }
}

// -------------------------------------------------------------------- work

fn start_work(w: &mut World, pid: PersonId) {
    let i = pid.ix();
    let effort = economy::effort_of(w, pid);
    w.persons[i].work.effort = effort;
    let Some(firm) = w.persons[i].job.as_ref().map(|j| j.firm) else { return };
    // Days side by side make workmates into friends, slowly.
    let n = w.firms[firm.ix()].workers.len();
    if n > 1 {
        let other = w.firms[firm.ix()].workers[w.rng.index(n)];
        let here = w.persons[i].loc;
        let near = { let o = &w.persons[other.ix()]; other != pid && o.present && o.loc == here && o.travel.is_none() && o.household != w.persons[i].household };
        if near { w.persons[i].befriend(other, 0.004); w.persons[other.ix()].befriend(pid, 0.004); }
    }
    if w.is_fork() { return; }
    if !w.persons[i].mind.has(flag::WORK_NOTED) {
        w.persons[i].mind.set(flag::WORK_NOTED);
        let name = w.firms[firm.ix()].name.clone();
        let line = match voice::vary(w, pid, 3) { 0 => format!("Started the day's work at {name}."), 1 => format!("At {name} for the day."), _ => format!("To work at {name}.") };
        w.note(pid, "work", line);
    }
    if voice::vary(w, pid, 2) == 0 { voice::muse(w, pid); }
}

fn apply(w: &mut World, pid: PersonId, arg: i64) {
    let firm = FirmId(arg as u32);
    if firm.ix() >= w.firms.len() { return; }
    let hired = economy::apply_for(w, pid, firm);
    if w.is_fork() { return; }
    let name = w.firms[firm.ix()].name.clone();
    if hired {
        let wage = w.persons[pid.ix()].job.as_ref().map(|j| j.wage).unwrap_or(0.0);
        w.note(pid, "work", format!("Asked for work at {name} and was taken on, at {wage:.1} coins a day."));
        w.persons[pid.ix()].thought = "Work at last. That is a weight off.".to_string();
    } else {
        w.note(pid, "work", match voice::vary(w, pid, 2) { 0 => format!("Asked for work at {name}. Nothing came of it."), _ => format!("{name} had no place for me.") });
        w.persons[pid.ix()].thought = "No luck finding work today.".to_string();
    }
}

fn treatment(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let firm = FirmId(arg as u32);
    if firm.ix() >= w.firms.len() { return; }
    let hh = w.persons[pid.ix()].household;
    let before = w.households[hh.ix()].money;
    let staffed = economy::staff_present(w, firm) > 0;
    let seen = staffed && economy::buy_service(w, pid, firm, Priced::Healthcare);
    if !still(w, pid, Activity::Treatment, until) { return; }
    if seen {
        let gain = 30.0 * w.tech[Field::Medicine];
        let p = &mut w.persons[pid.ix()];
        p.needs.health = (p.needs.health + gain).min(100.0);
        let paid = before - w.households[hh.ix()].money;
        note!(w, pid, "health", "Saw the healer at {} ({paid:.1} coins).", w.firms[firm.ix()].name);
    } else {
        cut(w, pid, "Turned away at the clinic");
        note!(w, pid, "health", "{}", if staffed { "Could not pay the healer." } else { "Went to the clinic. No healer was in." });
    }
}

fn lessons(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let firm = FirmId(arg as u32);
    if firm.ix() >= w.firms.len() { return; }
    let i = pid.ix();
    if !w.persons[i].mind.has(flag::SCHOOL_PAID) {
        // The day's fee, paid by whoever answers for the child.
        let payer = head_of(w, w.persons[i].household);
        let paid = payer.map(|a| economy::buy_service(w, a, firm, Priced::Education)).unwrap_or(false);
        if !still(w, pid, Activity::School, until) { return; }
        if !paid {
            w.persons[i].enrolled = false;
            cut(w, pid, "Sent home from school");
            if let Some(a) = payer { note!(w, a, "money", "Could not pay {}'s school fee today.", w.persons[i].name); }
            return;
        }
        w.persons[i].mind.set(flag::SCHOOL_PAID);
    }
    // No lessons without a teacher (allowing for the teacher still being on the way first thing).
    if w.hour() >= 8.5 && economy::staff_present(w, firm) == 0 {
        cut(w, pid, "Waiting at the school - no teacher came");
        think!(w, pid, "No teacher today.");
    }
}

// ------------------------------------------------------------------ leisure

fn tavern(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let i = pid.ix();
    mingle(w, pid);
    let firm = FirmId(arg as u32);
    let open = w.firms.get(firm.ix()).map(|f| f.open && !f.suspended).unwrap_or(false);
    if !open || economy::staff_present(w, firm) == 0 {
        if !w.is_fork() { w.persons[i].doing = "Sitting in the quiet tavern".to_string(); }
        return;
    }
    let hh = w.persons[i].household;
    let (temperance, patience) = { let pr = &w.persons[i].prefs; (pr[Pref::Temperance], pr[Pref::Patience]) };
    let want = 1 + ((1.0 - temperance) * 2.4 * w.rng.f32()).round() as u32;
    let food = w.stats.price[Good::Food as usize];
    let before = w.households[hh.ix()].money;
    let (mut had, mut dry, mut refused) = (0u32, false, false);
    for _ in 0..want {
        let (price, input) = { let f = &w.firms[firm.ix()]; (if f.sell_price > 0.0 { f.sell_price } else { f.price }, f.input) };
        // What one more cup is worth to them tonight.
        let worth = 1.2 + (1.0 - temperance) * 5.0 + (100.0 - w.persons[i].needs.fun) / 40.0 - patience * 1.2;
        if input < 0.5 { dry = true; break; }
        if price > worth || w.households[hh.ix()].money < price + food * 2.0 { break; }
        // The economy sees to the sale, the law on buying and drinking included.
        let served = economy::buy_service(w, pid, firm, Priced::Ale);
        if !still(w, pid, Activity::Tavern, until) { return; }
        if !served { refused = true; break; }
        had += 1;
        let p = &mut w.persons[i];
        p.mind.drinks = p.mind.drinks.saturating_add(1);
        p.needs.fun = (p.needs.fun + 13.0).min(100.0);
        p.needs.social = (p.needs.social + 5.0).min(100.0);
    }
    if w.is_fork() { return; }
    let name = w.firms[firm.ix()].name.clone();
    if had > 0 {
        let paid = before - w.households[hh.ix()].money;
        let cups = if had == 1 { "One drink".to_string() } else { format!("{had} drinks") };
        w.note(pid, "leisure", match voice::vary(w, pid, 2) { 0 => format!("{cups} at {name} ({paid:.1} coins)."), _ => format!("An evening at {name}: {} for {paid:.1} coins.", cups.to_lowercase()) });
    } else if dry {
        w.persons[i].doing = format!("At {name} - the barrels are dry");
    } else if refused {
        w.persons[i].thought = "A drink would be welcome, but there is none to be had for me.".to_string();
    }
}

/// Fall in with someone who happens to be at the same place.
pub(super) fn mingle(w: &mut World, pid: PersonId) {
    let i = pid.ix();
    let day = w.day();
    let (loc, hh, kid, age) = { let p = &w.persons[i]; let a = p.age_on(day); (p.loc, p.household, a < ADULT_AGE, a) };
    if age < 6.0 { return; }
    let n = w.persons.len();
    for _ in 0..6 {
        let o = PersonId(w.rng.index(n) as u32);
        let fits = { let q = &w.persons[o.ix()]; o != pid && q.present && q.travel.is_none() && q.loc == loc && q.household != hh && (q.age_on(day) < ADULT_AGE) == kid && q.activity != Activity::Sleep };
        if !fits { continue; }
        let d = 0.02 * (0.5 + w.persons[i].prefs[Pref::Sociability]);
        w.persons[i].befriend(o, d);
        w.persons[o.ix()].befriend(pid, d);
        let p = &mut w.persons[i];
        p.needs.social = (p.needs.social + 4.0).min(100.0);
        return;
    }
}

fn visit(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let i = pid.ix();
    let fr = PersonId(arg as u32);
    if fr.ix() >= w.persons.len() || fr == pid { cut(w, pid, "Nobody home"); return; }
    let loc = w.persons[i].loc;
    let home = { let f = &w.persons[fr.ix()]; f.present && f.travel.is_none() && f.loc == loc && !matches!(f.activity, Activity::Sleep | Activity::Jailed) };
    if !home {
        let line = if w.is_fork() { String::new() } else { format!("Knocking at {}'s door - nobody home", w.persons[fr.ix()].name) };
        cut(w, pid, &line);
        return;
    }
    w.persons[i].befriend(fr, 0.02);
    w.persons[fr.ix()].befriend(pid, 0.02);
    let fork = w.is_fork();
    let my_name = if fork { String::new() } else { w.persons[i].name.clone() };
    {
        let f = &mut w.persons[fr.ix()];
        f.needs.social = (f.needs.social + 12.0).min(100.0);
        if matches!(f.activity, Activity::Home | Activity::Idle | Activity::Garden) {
            f.activity = Activity::Host;
            f.until = until;
            if !fork { f.doing = format!("Hosting {my_name}"); }
        }
    }
    let p = &mut w.persons[i];
    p.needs.social = (p.needs.social + 8.0).min(100.0);
    if !fork {
        let name = w.persons[fr.ix()].full_name();
        let line = match voice::vary(w, pid, 3) { 0 => format!("Called on {name}."), 1 => format!("Spent a while with {name}."), _ => format!("Sat with {name} and talked.") };
        w.note(pid, "leisure", line);
    }
}

// ------------------------------------------------------------------- theft

/// `arg` = good * 100 + quantity * 10.
fn steal(w: &mut World, pid: PersonId, arg: i64, until: u32) {
    let i = pid.ix();
    let good = ALL_GOODS[((arg / 100) as usize).min(N_GOODS - 1)];
    let q = (arg % 100) as f32 / 10.0;
    let px = economy::market_price(w, good);
    let (loc, hh, hunger) = { let p = &w.persons[i]; (p.loc, p.household, p.needs.hunger) };
    let need = good == Good::Food && hunger > 80.0;
    let value = px * q * if need { 5.0 } else { 1.3 };
    match law::attempt(w, pid, &Act::new(Action::Steal, loc, value).good(good, px, q)) {
        Verdict::Lawful | Verdict::Violated(_) => {}
        Verdict::Refrained(_) => { think!(w, pid, "I could not bring myself to do it."); return; }
        Verdict::Stopped(_) => return,
    }
    if !still(w, pid, Activity::Steal, until) { return; }
    let before = w.households[hh.ix()].stock[good as usize];
    let (got, _from) = economy::steal_goods(w, pid, good, q);
    if got <= 0.0 { if !w.is_fork() { w.persons[i].doing = "Finding nothing to take".to_string(); } return; }
    // The goods end up at the thief's home, whether or not the economy already put them there.
    let short = before + got - w.households[hh.ix()].stock[good as usize];
    if short > 1e-4 { w.households[hh.ix()].stock[good as usize] += short; }
    if need { note!(w, pid, "crime", "Took {got:.1} {} from the market without paying. We have to eat.", good.name()); }
    else { note!(w, pid, "crime", "Lifted {got:.1} {} from a market stall.", good.name()); }
}
