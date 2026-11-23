//! The founders: households and the people in them, the firms and who owns
//! them, who rents from whom, who works where, and who knows whom.
use super::names::{FEMALE, MALE, SURNAMES};
use crate::types::*;

/// Adults of working age left without a job at the founding, for the watch and the clerk's desk to recruit.
pub const RESERVE: usize = 4;
/// Share of households that rent their home.
pub const RENTING_SHARE: f32 = 0.4;
/// Coins per day asked for a rented house.
pub const RENT: f32 = 1.6;
/// The founding population is kept within these bounds.
pub const POPULATION: (u32, u32) = (85, 95);
/// People this young or younger do not run a business.
const OWNER_AGE: f32 = 17.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Family, Couple, Single, Elders, Parent }

/// A household before anyone in it has a name.
#[derive(Clone, Copy)]
struct Plan {
    kind: Kind,
    kids: u32,
    /// Elders: a couple rather than one person alone.
    pair: bool,
    /// Family: a grandparent lives with them.
    grandparent: bool,
}
impl Plan {
    fn size(&self) -> u32 {
        match self.kind {
            Kind::Family => 2 + self.kids + self.grandparent as u32,
            Kind::Couple => 2,
            Kind::Single => 1,
            Kind::Elders => 1 + self.pair as u32,
            Kind::Parent => 1 + self.kids,
        }
    }
}

/// Settle the town. Expects the map and its places to exist.
pub(super) fn settle(w: &mut World) {
    let homes: Vec<PlaceId> = w.places.iter().filter(|p| p.kind == PlaceKind::Home).map(|p| p.id).collect();
    let plans = plan_households(w, homes.len());
    let mut surnames = SURNAMES.to_vec();
    w.rng.shuffle(&mut surnames);
    for (plan, &home) in plans.iter().zip(&homes) { found_household(w, plan, home, &mut surnames); }
    found_firms(w);
    let owners = assign_owners(w);
    assign_homes(w, &homes, &owners);
    staff_firms(w);
    befriend(w);
}

// ------------------------------------------------------------- households

/// Decide what households there will be: 14 families, 5 couples, 6 people
/// living alone, 2 households of elders and 2 lone parents. That makes 46
/// adults of working age - the 42 the firms need and a few to spare.
fn plan_households(w: &mut World, homes: usize) -> Vec<Plan> {
    let blank = |kind| Plan { kind, kids: 0, pair: false, grandparent: false };
    let mut plans = Vec::new();
    for (kind, n) in [(Kind::Family, 14), (Kind::Couple, 5), (Kind::Single, 6), (Kind::Elders, 2), (Kind::Parent, 2)] {
        for _ in 0..n {
            let mut p = blank(kind);
            match kind {
                Kind::Family => p.kids = w.rng.int(1, 3) as u32 + w.rng.chance(0.3) as u32,
                Kind::Parent => p.kids = w.rng.int(1, 2) as u32,
                Kind::Elders => p.pair = w.rng.chance(0.6),
                _ => {}
            }
            plans.push(p);
        }
    }
    w.rng.shuffle(&mut plans);
    // Households that can run a business come first; elders last.
    plans.sort_by_key(|p| p.kind == Kind::Elders);
    plans.truncate(homes);

    // Three families have a grandparent under their roof.
    let mut families: Vec<usize> = (0..plans.len()).filter(|&i| plans[i].kind == Kind::Family).collect();
    w.rng.shuffle(&mut families);
    for &i in families.iter().take(3) { plans[i].grandparent = true; }

    // Keep the town the intended size by adding or removing a child here and there.
    let mut total: u32 = plans.iter().map(Plan::size).sum();
    while total < POPULATION.0 {
        let room: Vec<usize> = families.iter().copied().filter(|&i| plans[i].kids < 4).collect();
        if room.is_empty() { break; }
        plans[room[w.rng.index(room.len())]].kids += 1;
        total += 1;
    }
    while total > POPULATION.1 {
        let big: Vec<usize> = families.iter().copied().filter(|&i| plans[i].kids > 1).collect();
        if big.is_empty() { break; }
        plans[big[w.rng.index(big.len())]].kids -= 1;
        total -= 1;
    }
    plans
}

fn found_household(w: &mut World, plan: &Plan, home: PlaceId, surnames: &mut Vec<&'static str>) {
    let surname = surnames.pop().unwrap_or("Newcome");
    let id = HouseholdId(w.households.len() as u32);
    w.households.push(Household { id, name: surname.to_string(), members: Vec::new(), home: Some(home), money: 0.0, stock: [0.0; N_GOODS], arrears: 0.0, wealth: 0.0, wealth_rank: 0, wealth_pct: 50.0, econ: Default::default() });
    w.places[home.ix()].name = format!("{surname} House");
    match plan.kind {
        Kind::Family => {
            let (wife, husband) = couple(w, id, surname, 24.0, 46.0, 17.0);
            children(w, id, surname, &[wife, husband], plan.kids);
            if plan.grandparent {
                // The husband's parent shares the family name; the wife's keeps their own.
                let (child, name) = if w.rng.chance(0.5) { (husband, surname) } else { (wife, surnames.pop().unwrap_or(surname)) };
                let age = (w.age(child) + w.rng.range(21.0, 32.0)).clamp(ELDER_AGE + 1.0, 90.0);
                let elder = add_person(w, id, None, age, name);
                link_parent(w, elder, child);
            }
        }
        Kind::Couple => { couple(w, id, surname, 19.0, 55.0, 17.0); }
        Kind::Single => { let age = w.rng.range(18.0, 50.0); add_person(w, id, None, age, surname); }
        Kind::Elders => {
            if plan.pair { couple(w, id, surname, 61.0, 76.0, ELDER_AGE + 1.0); } else { let age = w.rng.range(61.0, 80.0); add_person(w, id, None, age, surname); }
        }
        Kind::Parent => {
            let age = w.rng.range(26.0, 45.0);
            let parent = add_person(w, id, None, age, surname);
            children(w, id, surname, &[parent], plan.kids);
        }
    }
}

/// A married pair (wife, husband) whose ages centre somewhere in `lo..hi`.
fn couple(w: &mut World, hh: HouseholdId, surname: &str, lo: f32, hi: f32, youngest: f32) -> (PersonId, PersonId) {
    let age = w.rng.range(lo, hi);
    let a = (age + w.rng.range(-3.0, 3.0)).clamp(youngest, 85.0);
    let b = (age + w.rng.range(-3.0, 4.0)).clamp(youngest, 85.0);
    let wife = add_person(w, hh, Some(Sex::Female), a, surname);
    let husband = add_person(w, hh, Some(Sex::Male), b, surname);
    w.persons[wife.ix()].spouse = Some(husband);
    w.persons[husband.ix()].spouse = Some(wife);
    (wife, husband)
}

fn children(w: &mut World, hh: HouseholdId, surname: &str, parents: &[PersonId], n: u32) {
    let youngest = parents.iter().map(|p| w.age(*p)).fold(f32::INFINITY, f32::min);
    for _ in 0..n {
        let oldest = (youngest - 18.0).clamp(1.0, 15.5);
        let age = w.rng.range(0.5, oldest);
        let kid = add_person(w, hh, None, age, surname);
        for &p in parents { link_parent(w, p, kid); }
    }
}

fn link_parent(w: &mut World, parent: PersonId, child: PersonId) {
    match w.persons[parent.ix()].sex {
        Sex::Female => w.persons[child.ix()].mother = Some(parent),
        Sex::Male => w.persons[child.ix()].father = Some(parent),
    }
    w.persons[parent.ix()].children.push(child);
}

/// A new person of the given age, at home. Their genes are drawn
/// independently around the middle of each scale, and what they express is
/// their genes nudged by the accidents of upbringing; nothing ties either to
/// their sex, family or fortune.
fn add_person(w: &mut World, hh: HouseholdId, sex: Option<Sex>, age: f32, surname: &str) -> PersonId {
    let sex = sex.unwrap_or_else(|| if w.rng.chance(0.5) { Sex::Female } else { Sex::Male });
    let list: &[&str] = if sex == Sex::Female { &FEMALE } else { &MALE };
    let mut name = list[w.rng.index(list.len())];
    for _ in 0..8 {
        if !w.households[hh.ix()].members.iter().any(|m| w.persons[m.ix()].name == name) { break; }
        name = list[w.rng.index(list.len())];
    }
    let id = PersonId(w.persons.len() as u32);
    let home = w.households[hh.ix()].home.unwrap_or(w.map.plaza);
    let birth_day = w.day() - (age * DAYS_PER_YEAR as f32).round() as i32;
    let mut p = Person::new(id, name, surname, sex, birth_day, hh, home);
    for i in 0..N_PREFS {
        let g = w.rng.gauss(0.5, 0.2).clamp(0.0, 1.0);
        p.genes.0[i] = g;
        p.prefs.0[i] = (g + w.rng.gauss(0.0, 0.08)).clamp(0.0, 1.0);
    }
    p.needs = Needs {
        hunger: w.rng.range(10.0, 40.0),
        energy: w.rng.range(70.0, 100.0),
        social: w.rng.range(50.0, 80.0),
        fun: w.rng.range(50.0, 80.0),
        health: w.rng.gauss(85.0 - (age - 45.0).max(0.0) * 0.6, 8.0).clamp(30.0, 100.0),
    };
    p.education = if age < 6.0 { 0.0 } else { w.rng.gauss(((age - 6.0) * 0.055).min(0.55), 0.15).clamp(0.0, 1.0) };
    p.skill = ((age - ADULT_AGE) / 25.0).clamp(0.0, 1.0);
    p.enrolled = (6.0..ADULT_AGE).contains(&age);
    p.until = w.slot;
    p.doing = "At home".to_string();
    w.persons.push(p);
    w.households[hh.ix()].members.push(id);
    id
}

// ------------------------------------------------------------------ firms

fn firm_kind(kind: PlaceKind) -> Option<FirmKind> {
    Some(match kind {
        PlaceKind::Farm => FirmKind::Farm,
        PlaceKind::Lumberyard => FirmKind::Lumberyard,
        PlaceKind::Workshop => FirmKind::Workshop,
        PlaceKind::Brewery => FirmKind::Brewery,
        PlaceKind::Tavern => FirmKind::Tavern,
        PlaceKind::Clinic => FirmKind::Clinic,
        PlaceKind::School => FirmKind::School,
        _ => return None,
    })
}

/// The most people a firm of this kind has room for.
pub fn capacity(kind: FirmKind) -> u32 {
    match kind {
        FirmKind::Farm => 7,
        FirmKind::Lumberyard | FirmKind::Workshop => 6,
        FirmKind::Brewery => 5,
        FirmKind::Tavern => 4,
        FirmKind::Clinic | FirmKind::School => 3,
        FirmKind::Watch => 30,
        FirmKind::Works => 40,
        FirmKind::Office | FirmKind::Academy => 6,
    }
}

/// Hired hands a firm of this kind starts with, besides its owner.
pub fn founding_hands(kind: FirmKind) -> u32 {
    match kind {
        FirmKind::Farm | FirmKind::Lumberyard | FirmKind::Workshop => 3,
        FirmKind::Brewery | FirmKind::Tavern => 2,
        FirmKind::Clinic | FirmKind::School => 1,
        _ => 0,
    }
}

/// The price a firm of this kind opens at: the reference price of what it
/// makes, or of a cup of ale, a treatment, a day's schooling.
pub fn founding_price(kind: FirmKind) -> f32 {
    match kind {
        FirmKind::Tavern => 2.2,
        FirmKind::Clinic => 7.0,
        FirmKind::School => 0.8,
        k => k.output().map(Good::base_price).unwrap_or(0.0),
    }
}

/// A firm in every building made for one. The academy stands empty until research is funded.
fn found_firms(w: &mut World) {
    let town = w.polity.town;
    for pi in 0..w.places.len() {
        let Some(kind) = firm_kind(w.places[pi].kind) else { continue };
        let id = FirmId(w.firms.len() as u32);
        let price = founding_price(kind);
        let producer = kind.output().is_some();
        w.firms.push(Firm {
            id, kind, name: w.places[pi].name.clone(), place: w.places[pi].id, owner: Owner::None, jur: town, manager: None, workers: Vec::new(),
            open: true, suspended: false, illicit: None,
            cash: 90.0,
            stock: if kind == FirmKind::Farm { 30.0 } else if producer { 12.0 } else { 0.0 },
            input: if kind.input().is_some() { 14.0 } else { 0.0 },
            price, sell_price: price, wage_offer: 9.0, vacancies: 0, capacity: capacity(kind),
            today: Ledger::default(), yesterday: Ledger::default(),
            avg_sales: if kind == FirmKind::Farm { 18.0 } else { 8.0 }, avg_profit: 0.0, effort_now: 0.0, econ: Default::default(),
        });
        w.places[pi].firm = Some(id);
    }
}

fn able(w: &World, h: HouseholdId) -> Vec<PersonId> {
    w.households[h.ix()].members.iter().copied().filter(|m| { let a = w.age(*m); (OWNER_AGE..ELDER_AGE).contains(&a) }).collect()
}

/// Give each firm to a household, whose eldest able member runs it. Returns the owning households.
fn assign_owners(w: &mut World) -> Vec<HouseholdId> {
    use FirmKind::*;
    let order = [Clinic, School, Farm, Farm, Farm, Farm, Farm, Lumberyard, Workshop, Workshop, Brewery, Tavern];
    let day = w.day();
    let mut owners = Vec::new();
    let mut next = 0;
    for kind in order {
        let Some(fi) = w.firms.iter().position(|f| f.kind == kind && f.owner == Owner::None) else { continue };
        while next < w.households.len() && able(w, HouseholdId(next as u32)).is_empty() { next += 1; }
        if next >= w.households.len() { break; }
        let h = HouseholdId(next as u32);
        next += 1;
        let Some(boss) = able(w, h).into_iter().min_by_key(|m| (w.persons[m.ix()].birth_day, m.0)) else { continue };
        let surname = w.households[h.ix()].name.clone();
        let place = w.firms[fi].place;
        let name = match kind {
            Workshop => format!("{surname} Workshop"),
            Clinic => format!("{surname} Clinic"),
            School => format!("{surname} School"),
            _ => w.places[place.ix()].name.clone(),
        };
        w.places[place.ix()].name = name.clone();
        w.places[place.ix()].owner = Owner::Household(h);
        let f = &mut w.firms[fi];
        f.name = name;
        f.owner = Owner::Household(h);
        f.manager = Some(boss);
        f.workers.push(boss);
        let schooled = matches!(kind, Clinic | School).then(|| 0.7 + w.rng.f32() * 0.25);
        let p = &mut w.persons[boss.ix()];
        p.job = Some(Job { firm: FirmId(fi as u32), wage: 0.0, role: kind.role(), since_day: day, owner: true });
        p.skill = (p.skill + 0.3).clamp(0.0, 1.0);
        if let Some(e) = schooled { p.education = e; }
        p.income.avg = 9.0;
        w.households[h.ix()].money = w.rng.range(160.0, 300.0);
        owners.push(h);
    }
    owners
}

/// Who owns each home. Households with a business own theirs; of the rest,
/// enough rent to make up the renting share, from two landlord families who
/// also hold the houses standing empty.
fn assign_homes(w: &mut World, homes: &[PlaceId], owners: &[HouseholdId]) {
    let n = w.households.len();
    let mut landlords: Vec<HouseholdId> = owners.iter().skip(2).copied().collect();
    if landlords.is_empty() { landlords = owners.to_vec(); }
    w.rng.shuffle(&mut landlords);
    landlords.truncate(2);

    let mut others: Vec<usize> = (0..n).filter(|i| !owners.contains(&HouseholdId(*i as u32))).collect();
    w.rng.shuffle(&mut others);
    let renters = if landlords.is_empty() { 0 } else { ((n as f32 * RENTING_SHARE).round() as usize).min(others.len()) };
    let mut renting = vec![false; n];
    for &i in others.iter().take(renters) { renting[i] = true; }

    let mut let_out = 0;
    for i in 0..n {
        let h = HouseholdId(i as u32);
        let Some(home) = w.households[i].home else { continue };
        let pl = &mut w.places[home.ix()];
        if renting[i] {
            pl.owner = Owner::Household(landlords[let_out % landlords.len()]);
            pl.tenant = Some(h);
            pl.rent = RENT;
            let_out += 1;
        } else {
            pl.owner = Owner::Household(h);
        }
        let size = w.households[i].members.len() as f32;
        let purse = w.rng.range(25.0, 90.0);
        let food = size * w.rng.range(2.0, 3.5);
        let wares = size * w.rng.range(0.6, 1.2);
        let wood = size * w.rng.range(0.25, 0.6);
        let hh = &mut w.households[i];
        if hh.money == 0.0 { hh.money = purse; }
        hh.stock[Good::Food as usize] = food;
        hh.stock[Good::Wares as usize] = wares;
        hh.stock[Good::Wood as usize] = wood;
    }
    for &home in homes.iter().skip(n) {
        let pl = &mut w.places[home.ix()];
        pl.name = "Vacant House".to_string();
        if !landlords.is_empty() {
            pl.owner = Owner::Household(landlords[let_out % landlords.len()]);
            pl.rent = RENT;
            let_out += 1;
        }
    }
}

/// Hire the founding hands from among adults of working age, keeping a few
/// back for public service. The clinic and the school take the best schooled.
fn staff_firms(w: &mut World) {
    let day = w.day();
    let mut pool: Vec<PersonId> = w.persons.iter().filter(|p| p.job.is_none() && (ADULT_AGE..ELDER_AGE).contains(&p.age_on(day))).map(|p| p.id).collect();
    w.rng.shuffle(&mut pool);
    let mut order: Vec<usize> = (0..w.firms.len()).collect();
    order.sort_by_key(|&i| !matches!(w.firms[i].kind, FirmKind::Clinic | FirmKind::School));
    let mut hands: Vec<u32> = order.iter().map(|&i| founding_hands(w.firms[i].kind)).collect();

    // If hands are short, the largest crews go without one each until the numbers fit.
    let spare = pool.len().saturating_sub(RESERVE) as u32;
    let mut wanted: u32 = hands.iter().sum();
    while wanted > spare {
        let Some(j) = (0..hands.len()).max_by_key(|&j| hands[j]) else { break };
        if hands[j] == 0 { break; }
        hands[j] -= 1;
        wanted -= 1;
    }

    for (j, &fi) in order.iter().enumerate() {
        let kind = w.firms[fi].kind;
        for _ in 0..hands[j] {
            if pool.is_empty() { break; }
            let k = if matches!(kind, FirmKind::Clinic | FirmKind::School) {
                (0..pool.len()).fold(0, |best, k| if w.persons[pool[k].ix()].education > w.persons[pool[best].ix()].education { k } else { best })
            } else {
                pool.len() - 1
            };
            let who = pool.remove(k);
            let wage = w.rng.gauss(9.0, 0.8).clamp(7.0, 11.0);
            let p = &mut w.persons[who.ix()];
            p.job = Some(Job { firm: FirmId(fi as u32), wage, role: kind.role(), since_day: day, owner: false });
            p.income.avg = wage;
            w.firms[fi].workers.push(who);
        }
    }
}

// ------------------------------------------------------------- friendship

/// Give everyone old enough a few friends of their own generation, mostly
/// from among neighbours and workmates.
fn befriend(w: &mut World) {
    let day = w.day();
    let n = w.persons.len();
    let ages: Vec<f32> = w.persons.iter().map(|p| p.age_on(day)).collect();
    let doors: Vec<(i32, i32)> = w.persons.iter().map(|p| w.households[p.household.ix()].home.map(|h| w.places[h.ix()].door).unwrap_or((0, 0))).collect();
    let firms: Vec<Option<FirmId>> = w.persons.iter().map(|p| p.job.as_ref().map(|j| j.firm)).collect();
    let mut peers: Vec<(usize, f32)> = Vec::new();
    for a in 0..n {
        if ages[a] < 6.0 { continue; }
        let child = ages[a] < ADULT_AGE;
        let span = if child { 4.0 } else { 18.0 };
        peers.clear();
        for b in 0..n {
            if b == a || ages[b] < 6.0 || (ages[b] < ADULT_AGE) != child || (ages[b] - ages[a]).abs() >= span { continue; }
            if w.persons[a].household == w.persons[b].household { continue; }
            let near = (doors[a].0 - doors[b].0).abs() + (doors[a].1 - doors[b].1).abs() <= 14;
            let workmates = firms[a].is_some() && firms[a] == firms[b];
            peers.push((b, 1.0 + if near { 3.0 } else { 0.0 } + if workmates { 4.0 } else { 0.0 }));
        }
        if peers.is_empty() { continue; }
        let total: f32 = peers.iter().map(|(_, wt)| wt).sum();
        for _ in 0..3 {
            let mut r = w.rng.f32() * total;
            let mut pick = peers[peers.len() - 1].0;
            for &(b, wt) in &peers {
                if r < wt { pick = b; break; }
                r -= wt;
            }
            let affinity = w.rng.range(0.3, 0.8);
            acquaint(&mut w.persons[a], PersonId(pick as u32), affinity);
            acquaint(&mut w.persons[pick], PersonId(a as u32), affinity);
        }
    }
}

fn acquaint(p: &mut Person, other: PersonId, affinity: f32) {
    match p.friends.iter_mut().find(|f| f.id == other) {
        Some(f) => f.affinity = f.affinity.max(affinity),
        None => p.friends.push(Friend { id: other, affinity }),
    }
}
