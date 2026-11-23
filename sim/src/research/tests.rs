use super::*;
use crate::statute::{Statute, StatuteStatus};
use crate::testkit::{tiny_world, Tiny, TOWN};

/// A hand-built academy for the town, registered as the one it keeps.
fn academy(w: &mut World) -> FirmId {
    let id = FirmId(w.firms.len() as u32);
    w.firms.push(Firm {
        id, kind: FirmKind::Academy, name: "Testford Academy".into(), place: w.map.plaza, owner: Owner::State(TOWN), jur: TOWN, manager: None, workers: Vec::new(),
        open: true, suspended: false, illicit: None, cash: 0.0, stock: 0.0, input: 0.0, price: 0.0, sell_price: 0.0, wage_offer: 0.0, vacancies: 0, capacity: 0,
        today: Ledger::default(), yesterday: Ledger::default(), avg_sales: 0.0, avg_profit: 0.0, effort_now: 0.0, econ: Default::default(),
    });
    w.research.academies.push(Academy { jur: TOWN, firm: id });
    id
}

fn make_scholar(w: &mut World, p: PersonId, firm: FirmId) {
    for f in w.firms.iter_mut() { f.workers.retain(|x| *x != p); }
    let day = w.day();
    w.persons[p.ix()].job = Some(Job { firm, wage: 12.0, role: Role::Scholar, since_day: day, owner: false });
    w.persons[p.ix()].education = 0.7;
    w.firms[firm.ix()].workers.push(p);
}

fn fund(w: &mut World, lines: &[(Field, f32)]) -> StatuteId {
    let id = crate::law::next_id(w);
    let mut s = Statute::draft("The Endowment", lines.iter().map(|(field, per_day)| Clause::Research { field: *field, per_day: *per_day }).collect());
    s.id = id;
    s.jur = TOWN;
    s.status = StatuteStatus::Active;
    w.statutes.push(s);
    id
}

/// One day: the scholars put in a full day, night falls, morning comes.
fn day(w: &mut World, scholars: &[PersonId]) {
    for s in scholars { w.persons[s.ix()].work.slots_today = FULL_DAY_SLOTS as u32; }
    w.slot += SLOTS_PER_DAY;
    start_of_day(w);
}

fn staffed() -> (World, Tiny, FirmId, Vec<PersonId>) {
    let (mut w, t) = tiny_world();
    let firm = academy(&mut w);
    make_scholar(&mut w, t.greta, firm);
    make_scholar(&mut w, t.hugo, firm);
    let scholars = vec![t.greta, t.hugo];
    (w, t, firm, scholars)
}

#[test]
fn no_progress_without_funding() {
    let (mut w, _t, firm, scholars) = staffed();
    w.firms[firm.ix()].capacity = 3;
    for _ in 0..10 { day(&mut w, &scholars); }
    assert!(w.research.progress.iter().all(|p| *p == 0.0));
    assert!(w.research.level.iter().all(|l| *l == 0));
    assert_eq!(w.firm(firm).capacity, 0, "an unfunded academy offers no posts");
}

#[test]
fn no_progress_without_scholars_at_work() {
    let (mut w, _t, firm, scholars) = staffed();
    fund(&mut w, &[(Field::Medicine, 30.0)]);
    // Funded, but nobody turns up.
    for _ in 0..5 { day(&mut w, &[]); }
    assert!(w.research.progress.iter().all(|p| *p == 0.0));
    assert_eq!(w.firm(firm).capacity, 2, "30 a day pays two scholars at 12.6");
    assert!((w.firm(firm).wage_offer - scholar_wage(&w)).abs() < 1e-4);
    // Funded, and an academy with nobody in it.
    for s in &scholars { w.persons[s.ix()].job = None; }
    w.firms[firm.ix()].workers.clear();
    for _ in 0..5 { day(&mut w, &scholars); }
    assert!(w.research.progress.iter().all(|p| *p == 0.0));
}

#[test]
fn funded_and_staffed_work_goes_to_the_funded_fields() {
    let (mut w, _t, _firm, scholars) = staffed();
    fund(&mut w, &[(Field::Medicine, 30.0), (Field::Agronomy, 10.0)]);
    day(&mut w, &scholars);
    let (med, agr) = (w.research.progress[Field::Medicine as usize], w.research.progress[Field::Agronomy as usize]);
    assert!(med > 0.0 && agr > 0.0);
    assert!((med / agr - 3.0).abs() < 0.01, "work follows the money: {med} to {agr}");
    for f in ALL_FIELDS {
        if f != Field::Medicine && f != Field::Agronomy { assert_eq!(w.research.progress[f as usize], 0.0); }
    }
    assert_eq!(w.research.scholars, 2);
    // Abler scholars get more done.
    let (mut w2, _t2, _f2, scholars2) = staffed();
    for s in &scholars2 { w2.persons[s.ix()].education = 0.2; }
    fund(&mut w2, &[(Field::Medicine, 30.0), (Field::Agronomy, 10.0)]);
    day(&mut w2, &scholars2);
    assert!(w2.research.progress[Field::Medicine as usize] < med);
}

#[test]
fn a_small_budget_offers_one_post_at_what_it_has() {
    let (mut w, _t, firm, _scholars) = staffed();
    fund(&mut w, &[(Field::Craft, 5.0)]);
    day(&mut w, &[]);
    assert_eq!(w.firm(firm).capacity, 1);
    assert!((w.firm(firm).wage_offer - 5.0).abs() < 1e-4);
}

#[test]
fn funding_that_stops_closes_the_posts_and_keeps_what_was_learnt() {
    let (mut w, _t, firm, scholars) = staffed();
    let sid = fund(&mut w, &[(Field::Craft, 40.0)]);
    for _ in 0..30 { day(&mut w, &scholars); }
    assert!(w.research.level[Field::Craft as usize] >= 2);
    let (tech, level, banked) = (w.tech, w.research.level, w.research.progress);
    w.statute_mut(sid).unwrap().status = StatuteStatus::Repealed;
    for _ in 0..30 { day(&mut w, &scholars); }
    assert_eq!(w.firm(firm).capacity, 0);
    assert_eq!(w.tech.0, tech.0, "unfunded fields do not regress");
    assert_eq!(w.research.level, level);
    assert_eq!(w.research.progress, banked);
}

#[test]
fn each_step_costs_more_and_tech_never_falls() {
    for l in 0..40 { assert!(step_cost(l + 1) > step_cost(l)); }
    let (mut w, _t, _firm, scholars) = staffed();
    fund(&mut w, &[(Field::Forestry, 40.0)]);
    let i = Field::Forestry as usize;
    let mut tech = w.tech;
    let mut step_days: Vec<u32> = Vec::new();
    let mut since = 0;
    for _ in 0..400 {
        let level = w.research.level[i];
        day(&mut w, &scholars);
        since += 1;
        if w.research.level[i] > level { step_days.push(since); since = 0; }
        for f in ALL_FIELDS { assert!(w.tech[f] >= tech[f], "{} fell", f.name()); }
        tech = w.tech;
    }
    assert!(step_days.len() >= 6, "steps taken: {step_days:?}");
    assert!(step_days.last().unwrap() > step_days.first().unwrap(), "later steps take longer: {step_days:?}");
    assert!(w.tech[Field::Forestry] > 1.0);
    assert!(ALL_FIELDS.iter().filter(|f| **f != Field::Forestry).all(|f| w.tech[*f] == 1.0));
    assert!(w.chronicle.iter().any(|c| c.kind == "research"));
}

#[test]
fn step_sizes_are_a_few_percent_sometimes_nothing_sometimes_much() {
    let (mut w, _t) = tiny_world();
    let (mut none, mut small, mut large) = (0, 0, 0);
    for _ in 0..600 {
        let before = w.tech[Field::Arms];
        advance(&mut w, Field::Arms, step_cost(0));
        let gain = w.tech[Field::Arms] / before - 1.0;
        if gain < 1e-6 { none += 1 } else if gain < 0.07 { small += 1 } else { large += 1 }
        w.tech[Field::Arms] = 1.0;
        w.research.level[Field::Arms as usize] = 0;
    }
    assert!(small > 350, "{none} {small} {large}");
    assert!(none > 40 && none < 150, "{none} {small} {large}");
    assert!(large > 25 && large < 110, "{none} {small} {large}");
}

#[test]
fn same_seed_same_breakthroughs_in_the_world_and_in_a_fork() {
    let run = |depth: u8| {
        let (mut w, _t, _firm, scholars) = staffed();
        w.depth = depth;
        fund(&mut w, &[(Field::Medicine, 25.0), (Field::Commerce, 25.0)]);
        for _ in 0..200 { day(&mut w, &scholars); }
        w
    };
    let (a, b, fork) = (run(0), run(0), run(1));
    assert!(a.research.level[Field::Medicine as usize] >= 3);
    assert_eq!(a.tech.0, b.tech.0);
    assert_eq!(a.research.level, b.research.level);
    let lines = |w: &World| w.chronicle.iter().filter(|c| c.kind == "research").map(|c| c.text.clone()).collect::<Vec<_>>();
    assert_eq!(lines(&a), lines(&b));
    assert!(!lines(&a).is_empty());
    // A fork learns exactly what the world would, and writes none of it down.
    assert_eq!(a.tech.0, fork.tech.0);
    assert_eq!(a.research.level, fork.research.level);
    assert!(fork.chronicle.is_empty());
}

#[test]
fn hours_noted_at_night_or_wages_on_the_ledger_stand_in_for_cleared_slots() {
    // Payroll has cleared the slots, but the night's note remembers them.
    let (mut w, _t, firm, scholars) = staffed();
    fund(&mut w, &[(Field::Pedagogy, 30.0)]);
    for s in &scholars { w.persons[s.ix()].work.slots_today = 16; }
    end_of_day(&mut w);
    for s in &scholars { w.persons[s.ix()].work.slots_today = 0; }
    w.slot += SLOTS_PER_DAY;
    start_of_day(&mut w);
    let half = w.research.progress[Field::Pedagogy as usize];
    assert!(half > 0.0);
    // No note and no slots, but the ledger shows the whole wage bill was paid.
    let (mut w2, _t2, firm2, scholars2) = staffed();
    assert_eq!(firm, firm2);
    fund(&mut w2, &[(Field::Pedagogy, 30.0)]);
    w2.firms[firm2.ix()].yesterday.wages = 24.0;
    w2.slot += SLOTS_PER_DAY;
    start_of_day(&mut w2);
    let full = w2.research.progress[Field::Pedagogy as usize];
    assert!(full > half * 1.5, "{full} against {half}");
    assert_eq!(scholars2.len(), 2);
}

#[test]
fn an_academy_is_asked_for_only_when_there_is_funding() {
    let (mut w, _t) = tiny_world();
    start_of_day(&mut w);
    assert!(w.research.academies.is_empty());
    // With funding one is asked of the economy; whatever comes back is used only if it is an academy.
    fund(&mut w, &[(Field::Arms, 20.0)]);
    let farm_capacity = w.firms[0].capacity;
    start_of_day(&mut w);
    assert!(w.research.academies.iter().all(|a| w.firm(a.firm).kind == FirmKind::Academy));
    assert_eq!(w.firms[0].capacity, farm_capacity);
}
