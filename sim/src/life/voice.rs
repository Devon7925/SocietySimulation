//! The words people use about their own day: the line that says what they are
//! doing, the passing thought, the diary. All of it is derived from the
//! person's actual situation, and none of it is written inside a forecast.
use super::flag;
use crate::types::*;

/// A choice among `n` wordings that varies by person and moment without
/// drawing on the world's random stream (so wording never alters events).
pub(super) fn vary(w: &World, pid: PersonId, n: usize) -> usize {
    let x = (w.slot as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add((pid.0 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    let x = (x ^ (x >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((x >> 33) as usize) % n.max(1)
}
pub(super) fn pick<'a>(w: &World, pid: PersonId, opts: &[&'a str]) -> &'a str {
    opts[vary(w, pid, opts.len())]
}

pub(super) fn title(w: &World, sid: StatuteId) -> String {
    w.statute(sid).map(|s| s.title.clone()).unwrap_or_else(|| "the law".to_string())
}

/// How a place is spoken of: "the plaza", "Oakfield Farm".
pub(super) fn place_label(w: &World, place: PlaceId) -> String {
    let pl = &w.places[place.ix()];
    match pl.kind {
        PlaceKind::Plaza => "the plaza".to_string(),
        PlaceKind::Market => "the market".to_string(),
        PlaceKind::Park => "the green".to_string(),
        PlaceKind::Forest => "the forest".to_string(),
        PlaceKind::Street => "the street".to_string(),
        PlaceKind::Townhall | PlaceKind::Watchhouse | PlaceKind::Chapel | PlaceKind::Clinic | PlaceKind::School | PlaceKind::Academy if !pl.name.starts_with("The ") => format!("the {}", pl.name),
        _ => pl.name.clone(),
    }
}

/// The line shown while someone is on their way.
pub(super) fn walking(w: &World, pid: PersonId, to: PlaceId, act: Activity) -> String {
    let home = w.hh_of(pid).home == Some(to);
    match act {
        Activity::Sleep if home => "Heading home to bed".to_string(),
        Activity::Sleep => "Looking for somewhere to sleep".to_string(),
        Activity::Work => format!("Walking to work at {}", w.places[to.ix()].name),
        Activity::Patrol => "Walking the beat".to_string(),
        Activity::School => "Walking to school".to_string(),
        Activity::Jailed => format!("Being marched to {}", place_label(w, to)),
        Activity::Leaving => "Leaving town for good".to_string(),
        Activity::Eat if home => "Walking home to eat".to_string(),
        _ if home => "Walking home".to_string(),
        _ => format!("Walking to {}", place_label(w, to)),
    }
}

pub(super) fn meal_name(hr: f32) -> &'static str {
    if hr < 10.0 { "Breakfast" } else if hr < 15.5 { "Dinner" } else { "Supper" }
}
pub(super) fn meal_flag(hr: f32) -> u32 {
    if hr < 10.0 { flag::MISSED_BREAKFAST } else if hr < 15.5 { flag::MISSED_DINNER } else { flag::MISSED_SUPPER }
}

pub(super) fn work_line(role: Role, firm: &str) -> String {
    match role {
        Role::Farmer => format!("Working the fields at {firm}"),
        Role::Lumberjack => format!("Felling timber for {firm}"),
        Role::Crafter => format!("At the bench in {firm}"),
        Role::Brewer => format!("Brewing at {firm}"),
        Role::Tavernkeeper => format!("Serving at {firm}"),
        Role::Healer => format!("Tending the sick at {firm}"),
        Role::Teacher => format!("Teaching at {firm}"),
        Role::Enforcer => "On patrol".to_string(),
        Role::Clerk => format!("Keeping the ledgers at {firm}"),
        Role::Laborer => format!("Labouring for {firm}"),
        Role::Scholar => format!("At study in {firm}"),
    }
}

/// A passing thought, taken from whatever weighs most on the person just now
/// or, failing any trouble, from what they have to look forward to.
pub(super) fn muse(w: &mut World, pid: PersonId) {
    if w.is_fork() { return; }
    let text = {
        let p = &w.persons[pid.ix()];
        let h = &w.households[p.household.ix()];
        let day = w.day();
        let age = p.age_on(day);
        let adult = age >= ADULT_AGE;
        let n = h.members.len().max(1) as f32;
        let food_price = w.stats.price[Good::Food as usize].max(0.1);
        let v = vary(w, pid, 3);
        // Troubles first, the sharpest first.
        if p.needs.hunger > 70.0 && h.stock[Good::Food as usize] < 0.5 {
            ["My stomach aches and the larder is empty.", "Nothing in the house to eat. Nothing.", "I cannot think of anything but food."][v].to_string()
        } else if h.home.is_none() {
            ["No roof over us. That is what it has come to.", "I would give a great deal for a door to close behind me.", "Another night out of doors ahead."][v].to_string()
        } else if adult && h.arrears > 1.0 {
            ["The rent is behind and I do not see how we catch up.", "The landlord will not wait for ever.", "We owe rent we do not have."][v].to_string()
        } else if p.needs.health < 45.0 {
            ["I feel poorly. I ought to see the healer, if we can spare the coins.", "This sickness is not lifting.", "Every step is an effort today."][v].to_string()
        } else if let Some(sid) = p.mind.barred.filter(|(_, at)| w.slot.saturating_sub(*at) < 24).map(|(s, _)| s) {
            let t = title(w, sid);
            [format!("\"{t}\" stood in my way today."), format!("I had other plans, but \"{t}\" says no."), format!("Who is helped by \"{t}\"? Not me.")][v].clone()
        } else if adult && h.money < food_price * n * 2.0 {
            ["We are down to our last coins.", "The purse is nearly empty.", "I count the coins twice and there are no more of them."][v].to_string()
        } else if adult && age < 62.0 && p.job.is_none() && h.money < w.stats.cost_of_living.max(1.0) * n * 20.0 {
            ["Another day without work.", "Somebody in this town must need a pair of hands.", "No work, and the savings will not last."][v].to_string()
        } else if p.needs.energy < 25.0 {
            ["I am worn out. Bed cannot come soon enough.", "My legs are lead.", "I could sleep standing up."][v].to_string()
        } else if let Some(line) = pay_thought(w, pid, v) {
            line
        } else if let Some(line) = law_thought(w, pid, v) {
            line
        } else if p.needs.social < 35.0 && !p.friends.is_empty() {
            let f = p.friends.iter().fold(p.friends[0], |a, b| if b.affinity > a.affinity { *b } else { a });
            let name = &w.persons[f.id.ix()].name;
            [format!("It is too long since I saw {name}."), format!("I should call on {name}."), "I have kept too much to myself lately.".to_string()][v].clone()
        } else if adult && p.needs.fun < 35.0 && p.prefs[Pref::Temperance] < 0.5 && w.hour() < 18.0 {
            ["A cup at the tavern tonight would set me right.", "I mean to have a drink this evening.", "All work and no cheer. Tonight I go out."][v].to_string()
        } else if w.weekday() + 1 == REST_DAY && w.hour() > 12.0 {
            ["Restday tomorrow, at last.", "One more evening and then a day to ourselves.", "Tomorrow nobody will ask anything of me."][v].to_string()
        } else if !adult {
            ["I wonder who is out playing.", "I hope there is something good for supper.", "When I am grown I shall do as I please."][v].to_string()
        } else if p.wellbeing.total > 0.25 {
            ["Things are going well enough for us.", "I have little to complain of.", "A good season, this."][v].to_string()
        } else if p.wellbeing.total < -0.15 {
            ["It was easier once than it is now.", "I do not know how much longer we can go on like this.", "Something has to change."][v].to_string()
        } else {
            ["One day much like another.", "There is always something wanting doing.", "We get by."][v].to_string()
        }
    };
    w.persons[pid.ix()].thought = text;
}

/// What they make of their pay, when it is worth remarking on.
fn pay_thought(w: &World, pid: PersonId, v: usize) -> Option<String> {
    let p = &w.persons[pid.ix()];
    let j = p.job.as_ref()?;
    if vary(w, pid, 5) > 1 { return None; }
    let median = w.stats.median_wage.max(1.0);
    let firm = &w.firms.get(j.firm.ix())?.name;
    if j.owner {
        let profit = w.firms[j.firm.ix()].avg_profit;
        return Some(if profit > median { [format!("{firm} is paying its way and more."), "Trade is good. Long may it last.".to_string(), "The books look healthy this week.".to_string()][v].clone() }
            else if profit < 0.5 { [format!("{firm} barely pays its way."), "If trade does not pick up I shall have to let someone go.".to_string(), "More going out than coming in.".to_string()][v].clone() }
            else { return None });
    }
    let wage = j.wage;
    if wage < median * 0.8 {
        Some([format!("{wage:.1} coins a day is thin pay for this work."), "Others earn more for less. I know it.".to_string(), format!("{firm} gets my days cheap.")][v].clone())
    } else if wage > median * 1.2 {
        Some([format!("{wage:.1} a day. I should hold on to this place."), "The pay is good here.".to_string(), "I am better paid than most, and I know it.".to_string()][v].clone())
    } else if p.mind.day == w.day() && p.mind.toil > 30 {
        Some(["A long day for the money.".to_string(), "The pay keeps us fed, no more.".to_string(), "Honest pay for honest work, I suppose.".to_string()][v].clone())
    } else { None }
}

/// What they make of the rule they like least or best, when they feel strongly.
fn law_thought(w: &World, pid: PersonId, v: usize) -> Option<String> {
    let p = &w.persons[pid.ix()];
    if p.opinions.is_empty() || vary(w, pid, 4) != 0 { return None; }
    let mut worst: Option<(StatuteId, f32)> = None;
    let mut best: Option<(StatuteId, f32)> = None;
    for &(sid, op) in &p.opinions {
        if !w.statute(sid).map(|s| s.is_active()).unwrap_or(false) { continue; }
        if worst.map(|x| op < x.1).unwrap_or(true) { worst = Some((sid, op)); }
        if best.map(|x| op > x.1).unwrap_or(true) { best = Some((sid, op)); }
    }
    if let Some((sid, op)) = worst { if op < -0.4 { let t = title(w, sid); return Some([format!("I cannot abide \"{t}\"."), format!("\"{t}\" does us nothing but harm."), format!("Whoever thought of \"{t}\" never had to live under it.")][v].clone()); } }
    if let Some((sid, op)) = best { if op > 0.5 { let t = title(w, sid); return Some([format!("\"{t}\" has done right by us."), format!("Say what you like, \"{t}\" was well made."), format!("I would not see \"{t}\" undone.")][v].clone()); } }
    if p.trust < 0.2 { return Some(["I do not trust those who rule this town.", "They make rules for us and never ask.", "Those in the Town Hall look after themselves."][v].to_string()); }
    None
}

/// Lines written at the close of the day: pay, and going to bed hungry.
pub(super) fn evening_notes(w: &mut World) {
    if w.is_fork() { return; }
    let day = w.day();
    let payday_talk = w.weekday() + 1 == REST_DAY;
    let median = w.stats.median_wage.max(1.0);
    for i in 0..w.persons.len() {
        let pid = PersonId(i as u32);
        let (wage, profit, tax, last, missed, firm) = {
            let p = &w.persons[i];
            if !p.present || p.age_on(day) < 10.0 { continue; }
            let missed = if p.mind.day == day { p.mind.missed } else { 0 };
            (p.income.wage, p.income.profit, p.income.tax, p.mind.last_pay, missed, p.job.as_ref().map(|j| j.firm))
        };
        let firm_name = firm.and_then(|f| w.firms.get(f.ix())).map(|f| f.name.clone()).unwrap_or_else(|| "work".to_string());
        if wage > 0.05 {
            let changed = last <= 0.05 || (wage - last).abs() > 0.05 * last;
            if changed || payday_talk {
                let view = if wage < median * 0.8 { " Less than most earn here." } else if wage > median * 1.2 { " Better than most are paid." } else { "" };
                let taxed = if tax > 0.05 { format!(" {tax:.1} went in tax.") } else { String::new() };
                let line = if last > 0.05 && wage < last * 0.95 { format!("Only {wage:.1} coins for the day at {firm_name}; it was {last:.1} before.{taxed}") }
                    else if last > 0.05 && wage > last * 1.05 { format!("Paid {wage:.1} coins at {firm_name}, up from {last:.1}.{taxed}{view}") }
                    else { match vary(w, pid, 3) { 0 => format!("Paid {wage:.1} coins for the day at {firm_name}.{taxed}{view}"), 1 => format!("A day's pay from {firm_name}: {wage:.1} coins.{taxed}{view}"), _ => format!("{wage:.1} coins earned at {firm_name}.{taxed}{view}") } };
                w.note(pid, "money", line);
            }
            w.persons[i].mind.last_pay = wage;
        } else if profit > 0.05 && payday_talk {
            w.note(pid, "money", format!("{firm_name} brought me {profit:.1} coins today."));
        }
        if missed >= 2 {
            w.note(pid, "hardship", match vary(w, pid, 2) { 0 => format!("To bed hungry. {missed} meals missed today."), _ => format!("{missed} meals went by with nothing on the table.") });
        }
    }
}
