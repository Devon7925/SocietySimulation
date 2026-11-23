use super::*;
use crate::polity::Actor;
use crate::scope;
use crate::statute::{Act, Action, Clause, Power, Statute};

const SEEDS: [u64; 8] = [1, 2, 3, 7, 42, 1234, 99_991, 0xDEAD_BEEF];

fn overlaps(a: &Rect, b: &Rect) -> bool { a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h }
fn tile_at(w: &World, x: i32, y: i32) -> u8 { w.map.tiles[(y * w.map.w + x) as usize] }
fn count(w: &World, kind: PlaceKind) -> usize { w.places.iter().filter(|p| p.kind == kind).count() }
fn is_area(kind: PlaceKind) -> bool { matches!(kind, PlaceKind::Street | PlaceKind::Gate | PlaceKind::Forest | PlaceKind::Plaza) }

/// `cargo test -p society worldgen::tests::dump -- --ignored --nocapture` prints a town.
#[test]
#[ignore]
fn dump() {
    for seed in SEEDS {
        let mut w = generate(seed);
        let homes = count(&w, PlaceKind::Home);
        let mut per_ward = Vec::new();
        for ward in &w.polity.wards {
            per_ward.push((w.places.iter().filter(|p| p.ward == *ward && p.kind == PlaceKind::Home).count(), w.persons.iter().filter(|p| w.place(p.loc).ward == *ward).count()));
        }
        let mut more = 0;
        while more < 400 && add_home(&mut w, "").is_some() { more += 1; }
        println!("seed {seed}: {homes} homes, {} people, wards {per_ward:?}, room for {more} more", w.persons.len());
    }
    let w = generate(1);
    for y in 0..w.map.h {
        let row: String = (0..w.map.w).map(|x| [' ', '.', '~', '^', ':', '#', '+', '"'][tile_at(&w, x, y) as usize]).collect();
        println!("{row}");
    }
    for (i, ward) in w.polity.wards.iter().enumerate() {
        let homes = w.places.iter().filter(|p| p.ward == *ward && p.kind == PlaceKind::Home).count();
        let people = w.persons.iter().filter(|p| w.place(p.loc).ward == *ward).count();
        let others: Vec<&str> = w.places.iter().filter(|p| p.ward == *ward && p.kind != PlaceKind::Home).map(|p| p.name.as_str()).collect();
        println!("{}: {homes} homes, {people} people; {others:?}", names::WARD_NAMES[i]);
    }
    let day = w.day();
    let kids = w.persons.iter().filter(|p| p.age_on(day) < ADULT_AGE).count();
    let elders = w.persons.iter().filter(|p| p.age_on(day) >= ELDER_AGE).count();
    println!("{} people: {kids} children, {elders} elders; {} households", w.persons.len(), w.households.len());
    let plaza = w.map.plaza;
    let far = w.places.iter().map(|p| w.travel_slots(p.id, plaza)).max();
    println!("farthest from the plaza: {far:?} slots");
}

#[test]
fn same_seed_same_town() {
    let a = serde_json::to_string(&generate(11)).unwrap();
    let b = serde_json::to_string(&generate(11)).unwrap();
    assert!(a == b, "the same seed must give byte-identical worlds");
    let c = serde_json::to_string(&generate(12)).unwrap();
    assert!(a != c, "different seeds must give different worlds");
    let back: World = serde_json::from_str(&a).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), a);
}

#[test]
fn clock_and_starting_positions() {
    let w = generate(5);
    assert_eq!((w.day(), w.hour()), (0, 6.0));
    assert_eq!(w.depth, 0);
    assert!(!w.town_name.is_empty());
    for p in &w.persons {
        assert!(p.alive && p.present);
        assert_eq!(Some(p.loc), w.households[p.household.ix()].home);
        assert_eq!(p.activity, Activity::Idle);
        assert_eq!(p.until, w.slot);
        assert!(p.travel.is_none() && p.next.is_none());
    }
    assert_eq!(w.chronicle.len(), 1);
}

#[test]
fn the_town_has_its_buildings() {
    for seed in SEEDS {
        let w = generate(seed);
        for (kind, n) in [
            (PlaceKind::Townhall, 1), (PlaceKind::Market, 1), (PlaceKind::Tavern, 1), (PlaceKind::Chapel, 1), (PlaceKind::Watchhouse, 1), (PlaceKind::Clinic, 1),
            (PlaceKind::School, 1), (PlaceKind::Academy, 1), (PlaceKind::Workshop, 2), (PlaceKind::Brewery, 1), (PlaceKind::Lumberyard, 1), (PlaceKind::Farm, 5),
            (PlaceKind::Home, map::N_HOMES), (PlaceKind::Plaza, 1), (PlaceKind::Park, 1), (PlaceKind::Forest, 1), (PlaceKind::Street, 4), (PlaceKind::Gate, 3),
        ] {
            assert_eq!(count(&w, kind), n, "seed {seed}: {kind:?}");
        }
        assert_eq!((w.map.w, w.map.h), (MAP_W, MAP_H));
        assert_eq!(w.map.tiles.len(), (MAP_W * MAP_H) as usize);
        assert_eq!(w.place(w.map.plaza).kind, PlaceKind::Plaza);
        assert_eq!(w.place(w.map.market).kind, PlaceKind::Market);
        assert_eq!(w.place(w.map.forest).kind, PlaceKind::Forest);
        assert!(w.map.gates.iter().all(|g| w.place(*g).kind == PlaceKind::Gate));
        for (i, pl) in w.places.iter().enumerate() { assert_eq!(pl.id.ix(), i); }
        // Every farm has a field of its own.
        for farm in w.places.iter().filter(|p| p.kind == PlaceKind::Farm) {
            let f = farm.field.expect("a farm has a field");
            for y in f.y..f.y + f.h { for x in f.x..f.x + f.w { assert_eq!(tile_at(&w, x, y), tile::FIELD); } }
        }
        // Public places belong to the town.
        for kind in [PlaceKind::Townhall, PlaceKind::Market, PlaceKind::Watchhouse, PlaceKind::Chapel, PlaceKind::Academy, PlaceKind::Plaza, PlaceKind::Park] {
            assert!(w.places.iter().filter(|p| p.kind == kind).all(|p| p.owner == Owner::State(w.polity.town)), "seed {seed}: {kind:?}");
        }
        let academy = w.place_of_kind(PlaceKind::Academy).unwrap();
        assert!(w.place(academy).firm.is_none());
        assert!(w.firms.iter().all(|f| f.kind != FirmKind::Academy));
    }
}

#[test]
fn buildings_sit_on_their_tiles_and_do_not_overlap() {
    for seed in SEEDS {
        let w = generate(seed);
        let solid: Vec<&Place> = w.places.iter().filter(|p| !is_area(p.kind)).collect();
        for (i, a) in solid.iter().enumerate() {
            assert!(a.rect.w > 0 && a.rect.h > 0);
            for b in &solid[i + 1..] { assert!(!overlaps(&a.rect, &b.rect), "seed {seed}: {} overlaps {}", a.name, b.name); }
            for b in &solid { if let Some(f) = b.field { assert!(!overlaps(&a.rect, &f), "seed {seed}: {} stands in a field", a.name); } }
            for y in a.rect.y..a.rect.y + a.rect.h {
                for x in a.rect.x..a.rect.x + a.rect.w {
                    let t = tile_at(&w, x, y);
                    if a.kind == PlaceKind::Park { assert!(t == tile::PARK || t == tile::WATER); } else { assert_eq!(t, tile::BUILDING, "seed {seed}: {}", a.name); }
                }
            }
        }
        assert!(w.map.tiles.contains(&tile::WATER) && w.map.tiles.contains(&tile::TREE));
    }
}

#[test]
fn every_place_has_a_ward_and_a_reachable_door() {
    for seed in SEEDS {
        let w = generate(seed);
        let n = w.places.len();
        assert_eq!(w.map.travel_n, n);
        assert_eq!(w.map.travel.len(), n * n);
        let plaza = w.map.plaza;
        let plaza_door = w.place(plaza).door;
        for pl in &w.places {
            assert!(w.polity.wards.contains(&pl.ward), "seed {seed}: {} has no ward", pl.name);
            let (x, y) = pl.door;
            assert!(x >= 0 && y >= 0 && x < w.map.w && y < w.map.h);
            assert!(matches!(tile_at(&w, x, y), tile::ROAD | tile::PLAZA), "seed {seed}: the door of {} is not on a road", pl.name);
            let cost = map::path_cost(&w.map, pl.door, plaza_door);
            assert!(cost.is_some(), "seed {seed}: no way from {} to the plaza", pl.name);
            let slots = w.travel_slots(pl.id, plaza);
            if pl.id == plaza { assert_eq!(slots, 0); } else { assert!((1..=8).contains(&slots), "seed {seed}: {} is {slots} slots from the plaza", pl.name); }
            // About thirteen road tiles to the slot, never quicker than the crow flies.
            let crow = ((x - plaza_door.0).abs().max((y - plaza_door.1).abs())) as u32;
            assert!(slots * map::TILES_PER_SLOT >= crow, "seed {seed}: {} is too near", pl.name);
        }
        for a in 0..n {
            for b in 0..n {
                let t = w.map.travel[a * n + b];
                if a == b { assert_eq!(t, 0); } else { assert!(t >= 1); assert_eq!(t, w.map.travel[b * n + a]); }
            }
        }
        // The gates are the far ends of the main roads.
        let (west, east) = (w.map.gates[0], w.map.gates[1]);
        assert_eq!(w.travel_slots(west, east), 6);
    }
}

#[test]
fn wards_follow_the_compass() {
    let w = generate(3);
    assert_eq!(w.polity.wards.len(), 4);
    assert_eq!(w.map.streets.len(), 4);
    for (i, ward) in w.polity.wards.iter().enumerate() {
        let street = w.place(w.map.streets[i]);
        assert_eq!(street.kind, PlaceKind::Street);
        assert_eq!(street.ward, *ward);
        assert_eq!(w.street_of(*ward), street.id);
        assert_eq!(w.polity.jur(*ward).parent, Some(w.polity.town));
        assert_eq!(w.polity.jur(*ward).name, names::WARD_NAMES[i]);
        assert!(w.places.iter().any(|p| p.ward == *ward && p.kind == PlaceKind::Home), "no homes in {}", names::WARD_NAMES[i]);
    }
    let (north, east, south, west) = (w.polity.wards[0], w.polity.wards[1], w.polity.wards[2], w.polity.wards[3]);
    assert_eq!(w.place(w.map.forest).ward, north);
    assert_eq!(w.place(w.map.plaza).ward, north);
    assert_eq!(w.map.gates.iter().map(|g| w.place(*g).ward).collect::<Vec<_>>(), vec![west, east, south]);
    assert_eq!(w.place(w.place_of_kind(PlaceKind::Lumberyard).unwrap()).ward, north);
    assert_eq!(w.jurs_at(w.map.forest), vec![north, w.polity.town]);
}

#[test]
fn population_has_children_adults_and_elders() {
    for seed in SEEDS {
        let w = generate(seed);
        let day = w.day();
        let n = w.persons.len() as u32;
        assert!((people::POPULATION.0..=people::POPULATION.1).contains(&n), "seed {seed}: {n} people");
        let kids = w.persons.iter().filter(|p| p.age_on(day) < ADULT_AGE).count();
        let elders = w.persons.iter().filter(|p| p.age_on(day) >= ELDER_AGE).count();
        let adults = w.persons.len() - kids - elders;
        assert!(kids >= 20 && adults >= 40 && elders >= 4, "seed {seed}: {kids} children, {adults} adults, {elders} elders");
        assert_eq!(w.households.len(), 29);
        assert_eq!(w.stats.population, n);
        for (i, p) in w.persons.iter().enumerate() {
            assert_eq!(p.id.ix(), i);
            assert!(w.households[p.household.ix()].members.contains(&p.id));
            assert!(!p.name.is_empty() && !p.surname.is_empty());
            assert!((0.0..=1.0).contains(&p.education) && (0.0..=1.0).contains(&p.skill));
            assert!(p.genes.0.iter().chain(p.prefs.0.iter()).all(|v| (0.0..=1.0).contains(v)));
            assert!(p.prefs != p.genes);
            assert_eq!(p.enrolled, (6.0..ADULT_AGE).contains(&p.age_on(day)));
        }
        // Nobody shares a full name with someone under the same roof.
        for h in &w.households {
            for (i, a) in h.members.iter().enumerate() {
                for b in &h.members[i + 1..] { assert_ne!(w.person(*a).full_name(), w.person(*b).full_name(), "seed {seed}"); }
            }
        }
    }
}

#[test]
fn preferences_are_spread_and_uncorrelated_with_station() {
    let mut all = Vec::new();
    let mut owners = Vec::new();
    for seed in SEEDS {
        let w = generate(seed);
        for p in &w.persons {
            all.extend_from_slice(&p.genes.0);
            if p.job.as_ref().is_some_and(|j| j.owner) { owners.extend_from_slice(&p.prefs.0); }
        }
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let m = mean(&all);
    let sd = (all.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / all.len() as f32).sqrt();
    assert!((m - 0.5).abs() < 0.02, "mean gene {m}");
    assert!((sd - 0.2).abs() < 0.03, "spread of genes {sd}");
    assert!((mean(&owners) - 0.5).abs() < 0.04, "owners' mean preference {}", mean(&owners));
}

#[test]
fn kinship_is_consistent() {
    for seed in SEEDS {
        let w = generate(seed);
        let day = w.day();
        for p in &w.persons {
            if let Some(s) = p.spouse {
                let s = w.person(s);
                assert_eq!(s.spouse, Some(p.id));
                assert_eq!(s.household, p.household);
                assert_ne!(s.sex, p.sex);
                assert!(p.age_on(day) >= ADULT_AGE);
            }
            for c in &p.children {
                let c = w.person(*c);
                assert_eq!(if p.sex == Sex::Female { c.mother } else { c.father }, Some(p.id));
                assert!(p.age_on(day) - c.age_on(day) >= 17.0, "seed {seed}: {} is too young to be the parent of {}", p.full_name(), c.full_name());
            }
            for parent in [p.mother, p.father].into_iter().flatten() { assert!(w.person(parent).children.contains(&p.id)); }
            if p.age_on(day) < ADULT_AGE {
                let parents: Vec<PersonId> = [p.mother, p.father].into_iter().flatten().collect();
                assert!(!parents.is_empty(), "seed {seed}: a child with no parent");
                assert!(parents.iter().all(|q| w.person(*q).household == p.household));
            }
        }
    }
}

#[test]
fn friendships_are_mutual_and_local() {
    let w = generate(21);
    let day = w.day();
    let mut workmates = 0;
    for p in &w.persons {
        if p.age_on(day) >= 6.0 { assert!(!p.friends.is_empty(), "{} has no friends", p.full_name()); } else { assert!(p.friends.is_empty()); }
        for f in &p.friends {
            let q = w.person(f.id);
            assert_ne!(q.id, p.id);
            assert_ne!(q.household, p.household);
            assert!((0.3..=0.8).contains(&f.affinity));
            assert_eq!(q.affinity(p.id), f.affinity);
            assert_eq!(p.age_on(day) < ADULT_AGE, q.age_on(day) < ADULT_AGE);
            if p.job.is_some() && p.job.as_ref().map(|j| j.firm) == q.job.as_ref().map(|j| j.firm) { workmates += 1; }
        }
        assert_eq!(p.friends.iter().map(|f| f.id).collect::<std::collections::BTreeSet<_>>().len(), p.friends.len());
    }
    assert!(workmates >= 10, "only {workmates} friendships between workmates");
}

#[test]
fn every_private_firm_has_an_owner_household_and_a_manager() {
    for seed in SEEDS {
        let w = generate(seed);
        assert_eq!(w.firms.len(), 12);
        let mut owners = std::collections::BTreeSet::new();
        for (i, f) in w.firms.iter().enumerate() {
            assert_eq!(f.id.ix(), i);
            let Owner::Household(h) = f.owner else { panic!("seed {seed}: {} has no owner household", f.name) };
            assert!(owners.insert(h), "seed {seed}: a household owns two firms");
            let pl = w.place(f.place);
            assert_eq!(pl.firm, Some(f.id));
            assert_eq!(pl.owner, f.owner);
            assert_eq!(pl.name, f.name);
            assert_eq!(f.jur, w.polity.town);
            let boss = w.person(f.manager.expect("a manager"));
            assert_eq!(boss.household, h);
            let job = boss.job.as_ref().expect("the manager works there");
            assert!(job.owner && job.wage == 0.0 && job.firm == f.id && job.role == f.kind.role());
            assert_eq!(f.workers.first(), Some(&boss.id));
            assert_eq!(f.workers.len() as u32, 1 + people::founding_hands(f.kind), "seed {seed}: {}", f.name);
            assert!(f.workers.len() as u32 <= f.capacity);
            for wk in &f.workers[1..] {
                let j = w.person(*wk).job.as_ref().expect("on the books");
                assert!(!j.owner && j.firm == f.id && (7.0..=11.0).contains(&j.wage) && j.role == f.kind.role());
            }
            assert!(f.open && !f.suspended && f.illicit.is_none());
            assert_eq!(f.capacity, people::capacity(f.kind));
            assert_eq!((f.price, f.sell_price, f.wage_offer, f.cash), (people::founding_price(f.kind), f.price, 9.0, 90.0));
            assert!(f.price > 0.0);
            assert_eq!(f.input > 0.0, f.kind.input().is_some());
            // The owners' households are the better off.
            assert!(w.household(h).money >= 160.0);
        }
        let by_kind = |k| w.firms.iter().filter(|f| f.kind == k).count();
        assert_eq!((by_kind(FirmKind::Farm), by_kind(FirmKind::Workshop), by_kind(FirmKind::Tavern)), (5, 2, 1));
        // Everyone with a job is on exactly one firm's books.
        for p in &w.persons {
            let listed = w.firms.iter().filter(|f| f.workers.contains(&p.id)).count();
            assert_eq!(listed, p.job.is_some() as usize);
            if p.job.is_some() { let a = p.age_on(w.day()); assert!((ADULT_AGE..ELDER_AGE).contains(&a)); }
        }
        // A healer and a teacher know their letters.
        for kind in [FirmKind::Clinic, FirmKind::School] {
            let f = w.firm(w.firm_of_kind(kind).unwrap());
            assert!(w.person(f.manager.unwrap()).education >= 0.7);
        }
    }
}

#[test]
fn a_few_adults_are_left_free_for_the_watch() {
    for seed in SEEDS {
        let w = generate(seed);
        let day = w.day();
        let free = w.persons.iter().filter(|p| p.job.is_none() && (ADULT_AGE..ELDER_AGE).contains(&p.age_on(day))).count();
        assert!((3..=5).contains(&free), "seed {seed}: {free} adults without work");
        assert!(w.stats.unemployment < 0.2, "seed {seed}: unemployment {}", w.stats.unemployment);
    }
}

#[test]
fn every_household_has_a_home_and_about_two_in_five_rent() {
    for seed in SEEDS {
        let w = generate(seed);
        let mut seen = std::collections::BTreeSet::new();
        let mut renting = 0;
        let mut landlords = std::collections::BTreeSet::new();
        for h in &w.households {
            assert!(!h.members.is_empty());
            let home = w.place(h.home.expect("a home"));
            assert_eq!(home.kind, PlaceKind::Home);
            assert!(seen.insert(home.id), "seed {seed}: two households in one house");
            assert_eq!(home.name, format!("{} House", h.name));
            assert!(h.money >= 25.0 && h.stock[Good::Food as usize] >= 2.0 * h.members.len() as f32);
            assert!(h.wealth >= h.money && h.wealth_rank >= 1);
            match home.tenant {
                Some(t) => {
                    assert_eq!(t, h.id);
                    assert!(home.rent > 0.0);
                    let Owner::Household(l) = home.owner else { panic!("a rented house has a landlord") };
                    assert_ne!(l, h.id);
                    landlords.insert(l);
                    renting += 1;
                }
                None => assert_eq!(home.owner, Owner::Household(h.id)),
            }
        }
        let share = renting as f32 / w.households.len() as f32;
        assert!((0.33..=0.47).contains(&share), "seed {seed}: {share} of households rent");
        assert_eq!(landlords.len(), 2, "seed {seed}");
        // The landlords are households with a business, and hold the empty houses too.
        for l in &landlords { assert!(w.firms.iter().any(|f| f.owner == Owner::Household(*l))); }
        for empty in w.places.iter().filter(|p| p.kind == PlaceKind::Home && !seen.contains(&p.id)) {
            let Owner::Household(l) = empty.owner else { panic!("an empty house has an owner") };
            assert!(landlords.contains(&l) && empty.tenant.is_none() && empty.rent > 0.0);
        }
        let ranks: std::collections::BTreeSet<u32> = w.households.iter().map(|h| h.wealth_rank).collect();
        assert_eq!(ranks.len(), w.households.len());
    }
}

#[test]
fn the_constitution_is_a_sovereign_and_four_wards() {
    let w = generate(8);
    let p = &w.polity;
    assert_eq!(p.jurisdictions.len(), 5);
    let town = p.jur(p.town);
    assert_eq!((town.name.as_str(), town.parent, town.treasury), (w.town_name.as_str(), None, charter::FOUNDING_TREASURY));
    assert_eq!(p.jur_by_name(""), Some(p.town));
    assert_eq!(p.jur_by_name("southbank"), Some(p.wards[2]));
    assert_eq!(p.offices.len(), 1);
    let sovereign = p.office(p.office_by_name(p.wards[1], charter::SOVEREIGN).unwrap());
    assert_eq!(sovereign.holder, Some(Actor::Player));
    assert!(matches!(sovereign.def.selection, crate::statute::Selection::Fixed));
    for power in [Power::Decree, Power::Command, Power::Pardon, Power::Judge] {
        for ward in &p.wards { assert!(p.has_power(Actor::Player, *ward, power)); }
    }
    assert_eq!(town.procedure.proposers.len(), 1);
    assert_eq!(town.procedure.steps.len(), 1);
    assert!(!town.procedure.constitutional && town.amend.constitutional);
    assert_eq!(town.amend.steps.len(), 1);
    for ward in &p.wards {
        let j = p.jur(*ward);
        assert!(j.procedure.proposers.is_empty() && j.procedure.steps.is_empty() && j.amend.proposers.is_empty());
        assert_eq!(j.treasury, 0.0);
    }
    assert!(p.bodies.is_empty() && p.bills.is_empty());
    assert_eq!(p.next_bill, 1);
    assert_eq!(w.stats.treasury, charter::FOUNDING_TREASURY);
}

#[test]
fn founding_statutes_say_what_the_design_says() {
    let w = generate(4);
    let statutes = founding_statutes();
    assert_eq!(statutes.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["The Common Peace", "The Tithe", "The Town Watch"]);
    let someone = w.present_adults()[0];
    for s in &statutes {
        assert!(s.jurisdiction.is_empty() && !s.summary.is_empty());
        let json = serde_json::to_string(s).unwrap();
        let back: Statute = serde_json::from_str(&json).unwrap();
        assert_eq!(back.clauses.len(), s.clauses.len());
    }
    // Theft: three times the value taken, and two days.
    let Clause::Conduct(c) = &statutes[0].clauses[0] else { panic!("the Common Peace is a rule of conduct") };
    assert_eq!(c.action, Action::Steal);
    assert_eq!(c.sanction.jail_days, 2.0);
    let act = Act::new(Action::Steal, w.map.market, 5.0).good(Good::Food, 2.0, 2.0);
    assert_eq!(scope::act_num(&w, &c.sanction.fine, someone, &act, -1.0), 12.0);
    assert!(scope::act_bool(&w, &c.when, someone, &act, false) && scope::person_bool(&w, &c.who, someone, false));
    // A tenth of income.
    let Clause::Levy(l) = &statutes[1].clauses[0] else { panic!("the Tithe is a levy") };
    assert_eq!(l.base, crate::statute::LevyBase::Income);
    assert!((scope::person_num(&w, &l.rate, someone, -1.0) - 0.1).abs() < 1e-6);
    // A watch of three under the Sovereign, posted out of doors and at the tavern, and a clerk.
    let Clause::Unit(u) = &statutes[2].clauses[0] else { panic!("the Town Watch is a unit") };
    assert_eq!((u.staff, u.wage, u.commander.as_str(), u.armed), (3, 10.0, charter::SOVEREIGN, true));
    assert!(w.polity.office_by_name(w.polity.town, &u.commander).is_some());
    assert!(scope::place_bool(&w, &u.posts, w.map.plaza, false));
    assert!(scope::place_bool(&w, &u.posts, w.map.streets[2], false));
    assert!(!scope::place_bool(&w, &u.posts, w.households[0].home.unwrap(), true));
    assert!(scope::person_bool(&w, &u.eligibility, someone, false));
    assert!(matches!(statutes[2].clauses[1], Clause::Clerks { count: 1, .. }));
}

#[test]
fn add_home_builds_on_free_ground() {
    let mut w = generate(6);
    let before = w.places.len();
    let built = w.map.tiles.iter().filter(|t| **t == tile::BUILDING).count();
    let id = add_home(&mut w, "New House").expect("room for another house");
    assert_eq!(id.ix(), before);
    assert_eq!(w.places.len(), before + 1);
    let home = w.place(id).clone();
    assert_eq!((home.kind, home.name.as_str(), home.owner, home.tenant, home.firm), (PlaceKind::Home, "New House", Owner::None, None, None));
    assert_eq!((home.rect.w, home.rect.h, home.value), (4, 3, map::base_value(PlaceKind::Home)));
    assert!(w.polity.wards.contains(&home.ward));
    assert_eq!(w.map.tiles.iter().filter(|t| **t == tile::BUILDING).count(), built + 12);
    assert!(matches!(tile_at(&w, home.door.0, home.door.1), tile::ROAD | tile::PLAZA));
    for other in w.places.iter().filter(|p| p.id != id && !is_area(p.kind)) {
        assert!(!overlaps(&home.rect, &other.rect), "the new house overlaps {}", other.name);
        if let Some(f) = other.field { assert!(!overlaps(&home.rect, &f)); }
    }
    // It is close in, and the walking times now cover it.
    assert_eq!(w.map.travel_n, before + 1);
    assert_eq!(w.map.travel.len(), (before + 1) * (before + 1));
    let to_plaza = w.travel_slots(id, w.map.plaza);
    assert!((1..=3).contains(&to_plaza), "{to_plaza} slots from the plaza");
    assert_eq!(w.travel_slots(w.map.plaza, id), to_plaza);
    assert_eq!(w.travel_slots(w.map.gates[0], w.map.gates[1]), 6);
    // The world it was built in is still deterministic.
    let mut twin = generate(6);
    assert_eq!(add_home(&mut twin, "New House"), Some(id));
    assert_eq!(serde_json::to_string(&twin).unwrap(), serde_json::to_string(&w).unwrap());
}

#[test]
fn add_home_never_overlaps_and_stops_when_the_town_is_full() {
    let mut w = generate(9);
    let mut built = 0;
    while built < 400 && add_home(&mut w, "").is_some() { built += 1; }
    assert!(built >= 10, "only room for {built} more houses");
    assert!(built < 400, "the town never fills up");
    assert!(add_home(&mut w, "One too many").is_none());
    let solid: Vec<&Place> = w.places.iter().filter(|p| !is_area(p.kind)).collect();
    for (i, a) in solid.iter().enumerate() {
        for b in &solid[i + 1..] { assert!(!overlaps(&a.rect, &b.rect), "{} overlaps {}", a.name, b.name); }
        for b in &solid { if let Some(f) = b.field { assert!(!overlaps(&a.rect, &f)); } }
    }
    assert_eq!(w.places.last().unwrap().name, "House");
    assert_eq!(w.map.travel_n, w.places.len());
    // Every one of them can still be walked to.
    let plaza_door = w.place(w.map.plaza).door;
    for pl in &w.places { assert!(map::path_cost(&w.map, pl.door, plaza_door).is_some(), "{} is cut off", pl.name); }
}

#[test]
fn add_home_reads_places_as_well_as_tiles() {
    // A house whose tiles were never stamped still keeps its ground.
    let mut w = generate(10);
    let first = add_home(&mut w, "First").unwrap();
    let rect = w.place(first).rect;
    for y in rect.y..rect.y + rect.h { for x in rect.x..rect.x + rect.w { let i = (y * w.map.w + x) as usize; w.map.tiles[i] = tile::GRASS; } }
    for _ in 0..20 {
        let Some(next) = add_home(&mut w, "Next") else { break };
        assert!(!overlaps(&w.place(next).rect, &rect));
    }
}

#[test]
fn add_home_in_a_town_without_roads_finds_no_lot() {
    let (mut w, _) = crate::testkit::tiny_world();
    let before = w.places.len();
    assert!(add_home(&mut w, "Nowhere").is_none());
    assert_eq!(w.places.len(), before);
    // Give it a road and it builds, in the ward of its nearest neighbour.
    for x in 0..w.map.w { let i = (14 * w.map.w + x) as usize; w.map.tiles[i] = tile::ROAD; }
    let id = add_home(&mut w, "By the road").expect("a lot by the new road");
    let home = w.place(id).clone();
    assert!(w.polity.wards.contains(&home.ward));
    for other in w.places.iter().filter(|p| p.id != id && !is_area(p.kind)) { assert!(!overlaps(&home.rect, &other.rect)); }
    assert!(w.travel_slots(id, w.map.plaza) >= 1);
}
