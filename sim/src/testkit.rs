//! A tiny hand-built world for unit tests. It uses no module's generator, so
//! every module can test against it on its own.
//!
//! Layout: the town "Testford" with two wards (North, South). North has the
//! plaza, market, town hall, watch house, tavern, a farm and two homes; South
//! has a workshop and two homes. Four households, ten people. One office,
//! "Sovereign", held by the player with every power, who alone makes law.
use crate::polity::*;
use crate::statute::*;
use crate::types::*;

pub const NORTH: JurId = JurId(1);
pub const SOUTH: JurId = JurId(2);
pub const TOWN: JurId = JurId(0);

fn place(w: &mut World, kind: PlaceKind, name: &str, ward: JurId, x: i32, y: i32) -> PlaceId {
    let id = PlaceId(w.places.len() as u32);
    w.places.push(Place { id, kind, name: name.to_string(), ward, rect: Rect { x, y, w: 4, h: 3 }, door: (x + 2, y + 3), owner: Owner::None, tenant: None, rent: 0.0, value: 150.0, firm: None, field: None });
    id
}

fn firm(w: &mut World, kind: FirmKind, name: &str, place: PlaceId, owner: Owner, capacity: u32) -> FirmId {
    let id = FirmId(w.firms.len() as u32);
    let price = kind.output().map(|g| g.base_price()).unwrap_or(2.0);
    w.firms.push(Firm {
        id, kind, name: name.to_string(), place, owner, jur: TOWN, manager: None, workers: Vec::new(), open: true, suspended: false, illicit: None,
        cash: 80.0, stock: 20.0, input: 10.0, price, sell_price: price, wage_offer: 9.0, vacancies: 0, capacity,
        today: Ledger::default(), yesterday: Ledger::default(), avg_sales: 8.0, avg_profit: 2.0, effort_now: 0.0, econ: Default::default(),
    });
    w.places[place.ix()].firm = Some(id);
    w.places[place.ix()].owner = owner;
    id
}

fn household(w: &mut World, name: &str, home: PlaceId, money: f32, owns: bool) -> HouseholdId {
    let id = HouseholdId(w.households.len() as u32);
    w.households.push(Household { id, name: name.to_string(), members: Vec::new(), home: Some(home), money, stock: [6.0, 1.0, 2.0, 0.5], arrears: 0.0, wealth: money, wealth_rank: 1, wealth_pct: 50.0, econ: Default::default() });
    if owns { w.places[home.ix()].owner = Owner::Household(id); }
    w.places[home.ix()].name = format!("{name} House");
    id
}

fn person(w: &mut World, name: &str, hh: HouseholdId, sex: Sex, age: f32) -> PersonId {
    let id = PersonId(w.persons.len() as u32);
    let surname = w.households[hh.ix()].name.clone();
    let home = w.households[hh.ix()].home.unwrap();
    let birth = w.day() - (age * DAYS_PER_YEAR as f32) as i32;
    let mut p = Person::new(id, name, &surname, sex, birth, hh, home);
    p.education = if age >= 16.0 { 0.5 } else { 0.1 };
    p.skill = if age >= 16.0 { 0.5 } else { 0.0 };
    p.income.avg = if age >= 16.0 { 9.0 } else { 0.0 };
    p.until = w.slot;
    w.persons.push(p);
    w.households[hh.ix()].members.push(id);
    id
}

fn employ(w: &mut World, p: PersonId, f: FirmId, wage: f32, owner: bool) {
    let role = w.firms[f.ix()].kind.role();
    let day = w.day();
    w.persons[p.ix()].job = Some(Job { firm: f, wage, role, since_day: day, owner });
    w.firms[f.ix()].workers.push(p);
    if owner { w.firms[f.ix()].manager = Some(p); }
}

/// Ids of the notable things in [`tiny_world`].
pub struct Tiny {
    pub plaza: PlaceId,
    pub market: PlaceId,
    pub townhall: PlaceId,
    pub watchhouse: PlaceId,
    pub tavern: PlaceId,
    pub farm: FirmId,
    pub workshop: FirmId,
    pub tavern_firm: FirmId,
    /// Farm-owning couple with two children (North).
    pub ada: PersonId,
    pub bram: PersonId,
    pub cai: PersonId,
    pub dara: PersonId,
    /// Farmhand couple, tenants of the first household (North).
    pub edith: PersonId,
    pub finn: PersonId,
    /// Workshop owner, single (South).
    pub greta: PersonId,
    /// Crafter couple with an elder (South).
    pub hugo: PersonId,
    pub ida: PersonId,
    pub joss: PersonId,
    pub sovereign: OfficeId,
}

/// Build the tiny world. The clock stands at 06:00 on day 0 (a Moonday).
pub fn tiny_world() -> (World, Tiny) {
    let mut w = World::empty(7);
    w.town_name = "Testford".into();
    let sovereign_proc = ProcedureDef { constitutional: false, proposers: vec![Proposer::Office { name: "Sovereign".into() }], steps: vec![Step::Assent { office: "Sovereign".into() }], veto: None };
    let none = ProcedureDef::default();
    for (i, (name, parent, treasury)) in [("Testford", None, 200.0), ("North", Some(TOWN), 0.0), ("South", Some(TOWN), 0.0)].into_iter().enumerate() {
        let root = i == 0;
        w.polity.jurisdictions.push(Jurisdiction {
            id: JurId(i as u32), name: name.into(), parent, treasury,
            procedure: if root { sovereign_proc.clone() } else { none.clone() },
            amend: if root { ProcedureDef { constitutional: true, ..sovereign_proc.clone() } } else { none.clone() },
            court: CourtDef::Summary, revenue_today: 0.0, spending_today: 0.0, shortfall_today: 0.0, amenity: 0.0,
        });
    }
    w.polity.town = TOWN;
    w.polity.wards = vec![NORTH, SOUTH];
    w.polity.offices.push(Office {
        id: OfficeId(0), jur: TOWN,
        def: OfficeDef { name: "Sovereign".into(), powers: vec![Power::Decree, Power::Command, Power::Pardon, Power::Judge], selection: Selection::Fixed, term_days: 0, salary: 0.0, eligibility: "is_adult".into(), holder: "player".into() },
        holder: Some(Actor::Player), since_day: 0, term_ends: None, abolished: false, created_by: None,
    });
    w.polity.next_bill = 1;

    let street_n = place(&mut w, PlaceKind::Street, "North streets", NORTH, 0, 0);
    let street_s = place(&mut w, PlaceKind::Street, "South streets", SOUTH, 0, 20);
    let plaza = place(&mut w, PlaceKind::Plaza, "Plaza", NORTH, 10, 8);
    let market = place(&mut w, PlaceKind::Market, "Market Square", NORTH, 16, 8);
    let forest = place(&mut w, PlaceKind::Forest, "Forest", NORTH, 10, 0);
    let gate = place(&mut w, PlaceKind::Gate, "West Gate", NORTH, 0, 10);
    let townhall = place(&mut w, PlaceKind::Townhall, "Town Hall", NORTH, 10, 3);
    let watchhouse = place(&mut w, PlaceKind::Watchhouse, "Watch House", NORTH, 16, 3);
    let tavern_pl = place(&mut w, PlaceKind::Tavern, "The Test Tavern", NORTH, 22, 8);
    let farm_pl = place(&mut w, PlaceKind::Farm, "Oakfield Farm", NORTH, 2, 3);
    let home_a = place(&mut w, PlaceKind::Home, "House", NORTH, 22, 3);
    let home_b = place(&mut w, PlaceKind::Home, "House", NORTH, 28, 3);
    let shop_pl = place(&mut w, PlaceKind::Workshop, "South Workshop", SOUTH, 10, 22);
    let home_c = place(&mut w, PlaceKind::Home, "House", SOUTH, 16, 22);
    let home_d = place(&mut w, PlaceKind::Home, "House", SOUTH, 22, 22);
    for pl in [townhall, watchhouse, plaza, market] { w.places[pl.ix()].owner = Owner::State(TOWN); }
    w.places[farm_pl.ix()].field = Some(Rect { x: 2, y: 8, w: 6, h: 4 });
    w.places[farm_pl.ix()].value = 420.0;
    w.map = Map { w: 34, h: 28, tiles: vec![tile::GRASS; 34 * 28], travel: vec![1; w.places.len() * w.places.len()], travel_n: w.places.len(), streets: vec![street_n, street_s], gates: vec![gate], plaza, market, forest };

    let h1 = household(&mut w, "Ashdown", home_a, 220.0, true);
    let h2 = household(&mut w, "Birch", home_b, 40.0, false);
    let h3 = household(&mut w, "Carver", home_c, 180.0, true);
    let h4 = household(&mut w, "Dunmore", home_d, 60.0, true);
    w.places[home_b.ix()].owner = Owner::Household(h1);
    w.places[home_b.ix()].tenant = Some(h2);
    w.places[home_b.ix()].rent = 1.6;

    let farm = firm(&mut w, FirmKind::Farm, "Oakfield Farm", farm_pl, Owner::Household(h1), 6);
    let workshop = firm(&mut w, FirmKind::Workshop, "South Workshop", shop_pl, Owner::Household(h3), 5);
    let tavern_firm = firm(&mut w, FirmKind::Tavern, "The Test Tavern", tavern_pl, Owner::Household(h3), 3);

    let ada = person(&mut w, "Ada", h1, Sex::Female, 38.0);
    let bram = person(&mut w, "Bram", h1, Sex::Male, 40.0);
    let cai = person(&mut w, "Cai", h1, Sex::Male, 12.0);
    let dara = person(&mut w, "Dara", h1, Sex::Female, 7.0);
    let edith = person(&mut w, "Edith", h2, Sex::Female, 29.0);
    let finn = person(&mut w, "Finn", h2, Sex::Male, 31.0);
    let greta = person(&mut w, "Greta", h3, Sex::Female, 45.0);
    let hugo = person(&mut w, "Hugo", h4, Sex::Male, 35.0);
    let ida = person(&mut w, "Ida", h4, Sex::Female, 33.0);
    let joss = person(&mut w, "Joss", h4, Sex::Male, 68.0);
    for (a, b) in [(ada, bram), (edith, finn), (hugo, ida)] { w.persons[a.ix()].spouse = Some(b); w.persons[b.ix()].spouse = Some(a); }
    for kid in [cai, dara] { w.persons[kid.ix()].mother = Some(ada); w.persons[kid.ix()].father = Some(bram); w.persons[ada.ix()].children.push(kid); w.persons[bram.ix()].children.push(kid); }
    w.persons[hugo.ix()].father = Some(joss);
    w.persons[joss.ix()].children.push(hugo);
    for (a, b) in [(ada, edith), (bram, hugo), (finn, hugo), (greta, ida)] { w.persons[a.ix()].befriend(b, 0.6); w.persons[b.ix()].befriend(a, 0.6); }

    employ(&mut w, ada, farm, 0.0, true);
    employ(&mut w, bram, farm, 9.0, false);
    employ(&mut w, edith, farm, 9.0, false);
    employ(&mut w, finn, farm, 9.0, false);
    employ(&mut w, greta, workshop, 0.0, true);
    employ(&mut w, hugo, workshop, 9.0, false);
    employ(&mut w, ida, tavern_firm, 9.0, false);
    w.firms[tavern_firm.ix()].manager = Some(ida);

    w.stats.population = 10;
    w.stats.adults = 8;
    w.stats.median_wage = 9.0;
    w.stats.median_income = 9.0;
    w.stats.median_wealth = 120.0;
    w.stats.median_rent = 1.6;
    w.stats.cost_of_living = 5.0;
    w.stats.price = [2.0, 2.0, 6.5, 3.0];
    w.stats.drink_price = 2.2;
    w.stats.clinic_price = 7.0;
    w.stats.school_price = 0.8;
    w.stats.treasury = 200.0;
    for h in 0..w.households.len() { w.households[h].wealth = w.households[h].money; w.households[h].wealth_rank = h as u32 + 1; }

    (w, Tiny { plaza, market, townhall, watchhouse, tavern: tavern_pl, farm, workshop, tavern_firm, ada, bram, cai, dara, edith, finn, greta, hugo, ida, joss, sovereign: OfficeId(0) })
}

/// A simple statute for tests: forbid `action` for everyone, with a fine.
pub fn ban(action: Action, when: &str, fine: f32) -> Statute {
    Statute::draft(&format!("Ban on {}", action.name()), vec![Clause::Conduct(Conduct { label: String::new(), mode: Mode::Forbid, action, who: "true".into(), when: when.into(), place: String::new(), hours_per_week: 0.0, sanction: Sanction { fine: fine.to_string(), ..Default::default() } })])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope;
    #[test]
    fn tiny_world_is_coherent() {
        let (w, t) = tiny_world();
        assert_eq!(w.present().len(), 10);
        assert_eq!(w.present_adults().len(), 8);
        assert!(w.is_adult(t.ada) && !w.is_adult(t.cai));
        assert!(scope::person_bool(&w, "is_owner && job == \"farmer\" && ward == \"North\"", t.ada, false));
        assert!(scope::person_bool(&w, "!owns_home && children == 0", t.edith, false));
        assert!(scope::person_bool(&w, "is_landlord", t.bram, false));
        assert!(scope::place_bool(&w, "type == \"farm\" && owner.surname == \"Ashdown\" && workers == 4", w.firm(t.farm).place, false));
        assert!(w.polity.has_power(Actor::Player, SOUTH, Power::Decree));
        let json = serde_json::to_string(&w).unwrap();
        let back: World = serde_json::from_str(&json).unwrap();
        assert_eq!(back.persons.len(), 10);
        let s = ban(Action::Consume, "good == \"ale\"", 5.0);
        let js = serde_json::to_string(&s).unwrap();
        let s2: Statute = serde_json::from_str(&js).unwrap();
        assert_eq!(s2.clauses.len(), 1);
    }
}
