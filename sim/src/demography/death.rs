//! Mortality, and settling what a person leaves behind when they die or go.
use super::households::*;
use crate::polity::Actor;
use crate::statute::{Act, Action, EventKind};
use crate::types::*;
use crate::{decide, economy, gov, law};

/// Chance a day that a person in their prime falls ill.
pub const ILLNESS_RATE: f32 = 0.006;
/// Share of its value a property fetches when its owners are leaving and cannot wait.
pub const HURRIED_SALE: f32 = 0.5;

/// Chance of dying today.
///
/// The hazard of age doubles about every six years (a tenth of a percent a
/// day at 55, most of a percent at 70, nearly three percent at 80); poor health
/// multiplies it (four times at health 50) and near collapse adds a hazard
/// that spares no age, which is how starvation kills. Better medicine divides
/// the whole. In a town of ninety with a few elders that comes to a death
/// every week or two.
pub fn death_hazard(age: f32, health: f32, medicine: f32) -> f32 {
    let senescence = 0.0030 * (0.12 * (age - 60.0)).exp();
    let background = 0.00015 + if age < 2.0 { 0.0005 } else { 0.0 };
    let frail = ((75.0 - health) / 25.0).max(0.0);
    let acute = ((20.0 - health) / 20.0).clamp(0.0, 1.0);
    let h = (background + senescence) * (1.0 + 3.0 * frail * frail) + 0.3 * acute * acute;
    (h / medicine.max(0.25).powf(1.5)).clamp(0.0, 1.0)
}

/// Each morning: age wears on the body, some fall ill, some die.
pub(super) fn mortality(w: &mut World) {
    let day = w.day();
    let medicine = w.tech[Field::Medicine].max(0.25);
    let mut ill: Vec<PersonId> = Vec::new();
    let mut dying: Vec<(PersonId, &'static str)> = Vec::new();
    for i in 0..w.persons.len() {
        if !w.persons[i].present { continue; }
        let (r_ill, severity, r_death) = (w.rng.f32(), w.rng.range(12.0, 38.0), w.rng.f32());
        let p = &mut w.persons[i];
        let age = p.age_on(day);
        // The best health the body can hold falls with age.
        let ceiling = 100.0 - (age - 45.0).max(0.0) * 0.9;
        if p.needs.health > ceiling { p.needs.health = (p.needs.health - 1.0).max(ceiling); }
        let p_ill = ILLNESS_RATE * (1.0 + (age - 40.0).max(0.0) / 15.0) / medicine;
        if r_ill < p_ill {
            p.needs.health = (p.needs.health - severity).max(1.0);
            ill.push(p.id);
        }
        let health = p.needs.health;
        if health <= 0.0 || r_death < death_hazard(age, health, medicine) {
            let cause = if p.needs.hunger >= 90.0 && health < 30.0 { "hunger" }
                else if health < 45.0 { "illness" }
                else if age >= ELDER_AGE { "old age" }
                else { "a sudden fever" };
            dying.push((p.id, cause));
        }
    }
    for id in ill { w.note(id, "health", "Woke feeling ill."); }
    for (id, cause) in dying { die(w, id, cause); }
}

/// How `q` would speak of `p`.
fn spoken_of(w: &World, q: PersonId, p: PersonId) -> String {
    let (me, them) = (w.person(q), w.person(p));
    let female = them.sex == Sex::Female;
    let tie = if me.spouse == Some(p) { if female { "My wife" } else { "My husband" } }
        else if me.mother == Some(p) { "My mother" }
        else if me.father == Some(p) { "My father" }
        else if me.children.contains(&p) { if female { "My daughter" } else { "My son" } }
        else if shares_parent(w, q, p) { if female { "My sister" } else { "My brother" } }
        else if me.affinity(p) >= 0.3 { "My friend" }
        else { "" };
    if tie.is_empty() { them.full_name() } else { format!("{tie} {}", them.name) }
}

/// Those in town who care what becomes of `p`, how much, and how they speak of them.
fn those_who_care(w: &World, p: PersonId, at_least: f32) -> Vec<(PersonId, f32, String)> {
    let mut out = Vec::new();
    for q in &w.persons {
        if !q.present || q.id == p { continue; }
        let r = decide::regard(w, q.id, p);
        if r >= at_least { out.push((q.id, r, spoken_of(w, q.id, p))); }
    }
    out
}

/// Someone to run a firm whose manager is gone: one of the owning family who
/// works there if there is one (they take it on as an owner, living on the
/// profit), otherwise the ablest hand, kept on their wage.
fn succeed_manager(w: &mut World, f: FirmId) {
    let day = w.day();
    let firm = &w.firms[f.ix()];
    let family = match firm.owner { Owner::Household(h) => Some(h), _ => None };
    let mut best: Option<(bool, f32, PersonId)> = None;
    for &id in &firm.workers {
        let Some(q) = w.persons.get(id.ix()) else { continue };
        if !q.present || q.age_on(day) < ADULT_AGE { continue; }
        let key = (family == Some(q.household), q.skill + q.education);
        if best.is_none_or(|b| key > (b.0, b.1)) { best = Some((key.0, key.1, id)); }
    }
    w.firms[f.ix()].manager = best.map(|b| b.2);
    if let Some((true, _, m)) = best {
        if let Some(job) = w.persons[m.ix()].job.as_mut() {
            if job.firm == f { job.owner = true; job.wage = 0.0; }
        }
    }
}

/// Undo every tie that would otherwise point at someone who is no longer
/// here: their job and any firm they managed, offices and seats, the player's
/// hold on them, their marriage, their place in others' friendships.
fn settle_ties(w: &mut World, p: PersonId, how: &str) {
    // Work.
    economy::leave_job(w, p, "");
    w.persons[p.ix()].job = None;
    for f in 0..w.firms.len() {
        w.firms[f].workers.retain(|x| *x != p);
        if w.firms[f].manager == Some(p) { succeed_manager(w, FirmId(f as u32)); }
    }
    // Office: the seat falls vacant, and filling it is the government's business.
    let actor = Actor::Person(p);
    for o in 0..w.polity.offices.len() {
        if w.polity.offices[o].holder != Some(actor) { continue; }
        gov::seat(w, OfficeId(o as u32), None, how);
        if w.polity.offices[o].holder == Some(actor) { w.polity.offices[o].holder = None; }
    }
    for b in w.polity.bodies.iter_mut() { b.members.retain(|a| *a != actor); }
    if w.polity.player.possessing == Some(p) {
        let _ = gov::possess(w, None);
        if w.polity.player.possessing == Some(p) {
            w.polity.player.possessing = None;
            w.polity.player.overrides = Default::default();
        }
    }
    // Marriage and friendship.
    if let Some(s) = w.persons[p.ix()].spouse.take() {
        if w.persons[s.ix()].spouse == Some(p) { w.persons[s.ix()].spouse = None; }
    }
    for q in w.persons.iter_mut() {
        if q.id != p { q.friends.retain(|f| f.id != p); }
    }
    // Whatever they were in the middle of.
    let me = &mut w.persons[p.ix()];
    me.travel = None;
    me.next = None;
    me.jailed_until = None;
    me.enrolled = false;
    me.civic.protesting = false;
    me.civic.rebelling = false;
    me.activity = Activity::Idle;
    me.doing.clear();
    me.thought.clear();
}

/// Levy the inheritance tax on an estate held in household `h`'s purse.
/// `law::inheritance_tax` works out the tax and credits the treasury; the
/// purse is debited here, unless the law has already taken it.
fn estate_tax(w: &mut World, deceased: PersonId, h: HouseholdId, estate: f32) -> f32 {
    if estate <= 0.0 { return 0.0; }
    let before = w.households[h.ix()].money;
    let tax = law::inheritance_tax(w, deceased, estate);
    let tax = if tax.is_finite() { tax.clamp(0.0, estate) } else { 0.0 };
    let taken = (before - w.households[h.ix()].money).max(0.0);
    if taken < tax { w.households[h.ix()].money -= tax - taken; }
    tax
}

pub fn die(w: &mut World, p: PersonId, cause: &str) {
    if p.ix() >= w.persons.len() || !w.persons[p.ix()].alive { return; }
    if !w.persons[p.ix()].present {
        // Died somewhere else: there is nothing in town left to settle.
        w.persons[p.ix()].alive = false;
        return;
    }
    let day = w.day();
    let (name, age, h) = { let q = &w.persons[p.ix()]; (q.full_name(), q.age_on(day), q.household) };
    let mourners = those_who_care(w, p, 0.2);

    // The estate: their share of the common purse, less what the law takes of it.
    let adults = w.adults_of(h).len().max(1) as f32;
    let share = if age >= ADULT_AGE { w.households[h.ix()].money.max(0.0) / adults } else { 0.0 };
    let tax = estate_tax(w, p, h, share);

    settle_ties(w, p, "died");
    {
        let me = &mut w.persons[p.ix()];
        me.alive = false;
        me.present = false;
    }
    remove_member(w, p);
    w.demo.deaths += 1;
    w.demo.deaths_today += 1;

    // Grief, in proportion to how much each cared.
    let unnatural = !matches!(cause, "old age" | "illness" | "a sudden fever");
    for (q, regard, spoken) in mourners {
        let m = &mut w.persons[q.ix()];
        m.needs.social = (m.needs.social - 25.0 * regard).max(0.0);
        m.needs.fun = (m.needs.fun - 35.0 * regard).max(0.0);
        // A death by hunger or violence also leaves those close to it feeling less safe.
        if unnatural { m.felt.disorder += regard; }
        w.note(q, "grief", format!("{spoken} has died."));
    }
    w.chronicle_about("people", format!("{name}, aged {}, died of {cause}.", age.floor().max(0.0) as i32), vec![p], None);
    if tax > 0.05 {
        if let Some(heir) = w.head_of(h) { w.note(heir, "money", format!("The town took {tax:.1} coins of what {name} left.")); }
    }
    law::emit(w, EventKind::Death, p);
    after_loss(w, h, Some(p));
}

/// A household has lost a member. If nobody is left, what it had passes on;
/// if only children are left, someone takes them in.
fn after_loss(w: &mut World, h: HouseholdId, lost: Option<PersonId>) {
    if w.households[h.ix()].members.is_empty() {
        pass_estate(w, h, lost);
    } else if w.adults_of(h).is_empty() && !w.members(h).is_empty() {
        foster(w, h);
    }
}

/// An emptied household's money, stores and property go to the children of
/// the last of it who live elsewhere in town, or failing them to the town.
fn pass_estate(w: &mut World, h: HouseholdId, lost: Option<PersonId>) {
    let mut heirs: Vec<HouseholdId> = Vec::new();
    if let Some(d) = lost {
        for c in &w.persons[d.ix()].children {
            if let Some(child) = w.persons.get(c.ix()) {
                if child.present && child.household != h { heirs.push(child.household); }
            }
        }
    }
    let props = w.properties_of(h);
    let (money, stock) = {
        let hh = &mut w.households[h.ix()];
        (std::mem::take(&mut hh.money).max(0.0), std::mem::take(&mut hh.stock))
    };
    let who = lost.map(|d| w.persons[d.ix()].full_name()).unwrap_or_else(|| format!("the {} household", w.households[h.ix()].name));
    if heirs.is_empty() {
        let town = w.polity.town;
        if town.ix() < w.polity.jurisdictions.len() {
            to_treasury(w, town, money);
            for pl in &props { transfer_property(w, *pl, Owner::State(town)); }
            if money > 0.05 || !props.is_empty() {
                w.chronicle_about("people", format!("With no heirs in town, what {who} left passed to the town."), lost.into_iter().collect(), None);
            }
        } else {
            // No town to take it: it lies where it was.
            w.households[h.ix()].money = money;
        }
    } else {
        let n = heirs.len() as f32;
        for heir in &heirs {
            let hh = &mut w.households[heir.ix()];
            hh.money += money / n;
            for g in 0..N_GOODS { hh.stock[g] += stock[g] / n; }
        }
        for (i, pl) in props.iter().enumerate() { transfer_property(w, *pl, Owner::Household(heirs[i % heirs.len()])); }
        let mut told = heirs.clone();
        told.sort();
        told.dedup();
        for heir in told {
            let part = money * heirs.iter().filter(|x| **x == heir).count() as f32 / n;
            if let Some(head) = w.head_of(heir) { w.note(head, "money", format!("We inherited {part:.0} coins and what else {who} left.")); }
        }
    }
    vacate(w, h);
}

/// Who should take in these children: a living parent elsewhere, grown
/// brothers and sisters, grandparents, aunts and uncles; failing kin, the
/// closest friends of their parents; failing those, whoever in town cares most.
fn find_guardians(w: &World, h: HouseholdId, kids: &[PersonId]) -> Option<HouseholdId> {
    let day = w.day();
    let able = |q: PersonId| w.persons.get(q.ix()).is_some_and(|x| x.present && x.age_on(day) >= ADULT_AGE && x.household != h && !w.demo.is_leaving(x.household));
    let mut offers: Vec<(f32, PersonId)> = Vec::new();
    for &k in kids {
        let kid = w.person(k);
        for par in [kid.mother, kid.father].into_iter().flatten() {
            let Some(parent) = w.persons.get(par.ix()) else { continue };
            offers.push((10.0, par));
            for s in &parent.children { offers.push((8.0, *s)); }
            for g in [parent.mother, parent.father].into_iter().flatten() {
                offers.push((7.0, g));
                if let Some(grand) = w.persons.get(g.ix()) { for u in &grand.children { offers.push((6.0, *u)); } }
            }
            for f in &parent.friends { offers.push((f.affinity, f.id)); }
        }
        for f in &kid.friends { offers.push((0.5 * f.affinity, f.id)); }
    }
    let mut best: Option<(f32, PersonId)> = None;
    for (score, q) in offers {
        if able(q) && best.is_none_or(|b| score > b.0 || (score == b.0 && q < b.1)) { best = Some((score, q)); }
    }
    if best.is_none() {
        if let Some(&first) = kids.first() {
            for q in &w.persons {
                if !able(q.id) { continue; }
                let r = decide::regard(w, q.id, first);
                if best.is_none_or(|b| r > b.0) { best = Some((r, q.id)); }
            }
        }
    }
    best.map(|b| w.person(b.1).household)
}

/// Children left with no adult go to live with kin or friends, taking what their household had with them.
fn foster(w: &mut World, h: HouseholdId) -> bool {
    let kids = w.members(h);
    let Some(to) = find_guardians(w, h, &kids) else { return false };
    let (from_name, to_name) = (w.households[h.ix()].name.clone(), w.households[to.ix()].name.clone());
    let hosts = w.adults_of(to);
    merge_households(w, h, to);
    for k in &kids { w.note(*k, "family", format!("We have gone to live with the {to_name} household.")); }
    for a in &hosts { w.note(*a, "family", format!("We have taken in the {from_name} children.")); }
    w.chronicle_about("people", format!("The orphaned {from_name} children were taken in by the {to_name} household."), kids, None);
    true
}

/// Households that would pay `price` for a place its owners must part with,
/// keenest first: (what they would gain beyond the price, household, its head).
fn bidders(w: &World, seller: HouseholdId, place: PlaceId, price: f32) -> Vec<(f32, HouseholdId, PersonId)> {
    let pl = w.place(place);
    let cost = w.stats.cost_of_living.max(1.0);
    let mut out = Vec::new();
    for h in &w.households {
        if h.id == seller || h.members.is_empty() || w.demo.is_leaving(h.id) { continue; }
        let Some(head) = w.head_of(h.id) else { continue };
        let cushion = 20.0 * cost * w.members(h.id).len() as f32;
        if h.money < price + cushion { continue; }
        // What holding it would bring in a day: rent no longer paid, rent received, or the business's profit.
        let per_day = if pl.tenant == Some(h.id) { pl.rent }
            else if let Some(f) = pl.firm.and_then(|f| w.firms.get(f.ix())) { if f.kind.is_org() { 0.0 } else { f.avg_profit.max(0.0) } }
            else if pl.kind == PlaceKind::Home { if pl.tenant.is_some() { pl.rent } else { 0.5 * pl.rent.max(w.stats.median_rent) } }
            else { 0.0 };
        let horizon = 40.0 + 80.0 * w.person(head).prefs[Pref::Patience];
        let surplus = per_day * horizon - price;
        if surplus > 0.0 { out.push((surplus, h.id, head)); }
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    out
}

/// A household leaving for good sells what it owns to whoever in town finds
/// it worth a hurried price (the coin goes into the purse it carries away);
/// what nobody will buy is left to the town.
fn sell_or_abandon(w: &mut World, h: HouseholdId) {
    let town = w.polity.town;
    let seller_name = w.households[h.ix()].name.clone();
    for place in w.properties_of(h) {
        let price = (w.place(place).value * HURRIED_SALE).max(0.0);
        let mut sold = false;
        for (surplus, buyer, head) in bidders(w, h, place, price).into_iter().take(3) {
            let mut act = Act::new(Action::OwnProperty, place, surplus);
            act.property = Some(place);
            act.price = price;
            act.properties_owned = w.properties_of(buyer).len() as u32 + 1;
            if !law::attempt(w, head, &act).proceeds() { continue; }
            if w.households[buyer.ix()].money < price { continue; }
            w.households[buyer.ix()].money -= price;
            w.households[h.ix()].money += price;
            transfer_property(w, place, Owner::Household(buyer));
            let what = w.place(place).name.clone();
            w.note(head, "money", format!("Bought {what} from the departing {seller_name} household for {price:.0} coins."));
            sold = true;
            break;
        }
        if !sold {
            let to = if town.ix() < w.polity.jurisdictions.len() { Owner::State(town) } else { Owner::None };
            transfer_property(w, place, to);
        }
    }
}

pub fn depart(w: &mut World, p: PersonId, why: &str) {
    if p.ix() >= w.persons.len() || !w.persons[p.ix()].present { return; }
    let day = w.day();
    let (name, adult, h) = { let q = &w.persons[p.ix()]; (q.full_name(), q.age_on(day) >= ADULT_AGE, q.household) };
    let together = w.demo.is_leaving(h);
    let adults_before = w.adults_of(h).len().max(1) as f32;
    let left_behind: Vec<(PersonId, f32, String)> = those_who_care(w, p, 0.25).into_iter()
        .filter(|(q, _, _)| !(together && w.persons[q.ix()].household == h)).collect();

    settle_ties(w, p, "left town");
    w.persons[p.ix()].present = false;
    remove_member(w, p);
    w.demo.departures += 1;
    w.demo.departures_today += 1;
    for (q, regard, spoken) in left_behind {
        let m = &mut w.persons[q.ix()];
        m.needs.social = (m.needs.social - 12.0 * regard).max(0.0);
        w.note(q, "grief", format!("{spoken} has left town for good."));
    }
    let reason = if why.trim().is_empty() { String::new() } else { format!(" ({})", why.trim()) };
    let from = if w.town_name.is_empty() { "town".to_string() } else { w.town_name.clone() };

    if w.households[h.ix()].members.is_empty() {
        // The last of the household to go. What it owns is sold or left to the
        // town, and the purse leaves with them: coin going out of the town.
        sell_or_abandon(w, h);
        let hh = &mut w.households[h.ix()];
        let carried = std::mem::take(&mut hh.money).max(0.0);
        hh.stock = [0.0; N_GOODS];
        let hh_name = hh.name.clone();
        w.demo.coin_out += carried;
        vacate(w, h);
        w.demo.leaving.retain(|l| l.household != h);
        let text = if together { format!("The {hh_name} household left {from} for good{reason}.") } else { format!("{name} left {from} for good{reason}.") };
        w.chronicle_about("people", text, vec![p], None);
    } else if !together {
        // One person going alone takes an adult's share of the purse out of town.
        if adult {
            let hh = &mut w.households[h.ix()];
            let share = hh.money.max(0.0) / adults_before;
            hh.money -= share;
            w.demo.coin_out += share;
        }
        w.chronicle_about("people", format!("{name} left {from} for good{reason}."), vec![p], None);
        after_loss(w, h, None);
    }
}
