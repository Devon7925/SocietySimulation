use super::*;
use crate::polity::Actor;
use crate::rng::Rng;
use crate::testkit::{tiny_world, Tiny, NORTH, TOWN};

// ---------------------------------------------------------------- helpers

/// Every coin in town: purses, tills and treasuries.
fn coin(w: &World) -> f32 {
    w.households.iter().map(|h| h.money).sum::<f32>() + w.firms.iter().map(|f| f.cash).sum::<f32>() + w.polity.jurisdictions.iter().map(|j| j.treasury).sum::<f32>()
}

/// Nothing anywhere points at someone who is not there.
fn check(w: &World) {
    for h in &w.households {
        assert!(h.money.is_finite() && h.money > -1e-2, "purse of {} is {}", h.name, h.money);
        for m in &h.members {
            let p = w.person(*m);
            assert!(p.present && p.alive, "{} is on a household roll but gone", p.full_name());
            assert_eq!(p.household, h.id);
        }
        if h.members.is_empty() { assert!(h.home.is_none(), "empty household {} keeps a home", h.name); }
    }
    for p in &w.persons {
        if !p.present {
            assert!(p.job.is_none(), "{} is gone but employed", p.full_name());
            continue;
        }
        assert!(p.alive);
        assert_eq!(w.household(p.household).members.iter().filter(|m| **m == p.id).count(), 1, "{} not on their household's roll once", p.full_name());
        if let Some(s) = p.spouse {
            assert!(w.person(s).present, "{} is married to someone gone", p.full_name());
            assert_eq!(w.person(s).spouse, Some(p.id));
        }
        for f in &p.friends { assert!(w.person(f.id).present, "{} keeps a friend who is gone", p.full_name()); }
        if let Some(j) = &p.job { assert!(w.firm(j.firm).workers.contains(&p.id)); }
        for v in p.prefs.0.iter().chain(p.genes.0.iter()) { assert!((0.0..=1.0).contains(v)); }
    }
    for f in &w.firms {
        for x in &f.workers { assert!(w.person(*x).present, "{} employs someone gone", f.name); }
        if let Some(m) = f.manager { assert!(w.person(m).present, "{} is managed by someone gone", f.name); }
        if let Owner::Household(h) = f.owner { assert!(!w.household(h).members.is_empty(), "{} is owned by nobody", f.name); }
    }
    for pl in &w.places {
        if let Owner::Household(h) = pl.owner { assert!(!w.household(h).members.is_empty(), "{} is owned by nobody", pl.name); }
        if let Some(t) = pl.tenant { assert_eq!(w.household(t).home, Some(pl.id), "{} is let to a household that does not live there", pl.name); }
    }
    for o in &w.polity.offices {
        if let Some(Actor::Person(p)) = o.holder { assert!(w.person(p).present); }
    }
}

fn add_home(w: &mut World, owner: Owner) -> PlaceId {
    let id = PlaceId(w.places.len() as u32);
    w.places.push(Place { id, kind: PlaceKind::Home, name: "House".into(), ward: NORTH, rect: Rect { x: 0, y: 0, w: 4, h: 3 }, door: (2, 3), owner, tenant: None, rent: 1.6, value: 150.0, firm: None, field: None });
    id
}

/// Days pass with only this module running. The fed and rested mend a few
/// points of health a day, which is `life`'s part.
fn live(w: &mut World, days: u32) {
    for _ in 0..days {
        w.slot += SLOTS_PER_DAY;
        for p in w.persons.iter_mut() { if p.present { p.needs.health = (p.needs.health + 3.0).min(100.0); } }
        start_of_day(w);
        check(w);
    }
}

/// A household of its own in a house of its own, its adults at work on the farm.
fn settle(w: &mut World, t: &Tiny, adults: &[(Sex, f32)], kids: usize, married: bool) -> HouseholdId {
    let surname = names::surname(w);
    let h = new_household(w, &surname);
    let home = add_home(w, Owner::Household(h));
    w.households[h.ix()].home = Some(home);
    w.households[h.ix()].money = w.rng.range(40.0, 200.0);
    let day = w.day();
    let grown: Vec<PersonId> = adults.iter().map(|(sex, age)| new_person(w, h, Some(*sex), *age, &surname)).collect();
    for &a in &grown {
        let p = &mut w.persons[a.ix()];
        if p.age_on(day) < ELDER_AGE {
            p.income.avg = 9.0;
            p.job = Some(Job { firm: t.farm, wage: 9.0, role: Role::Farmer, since_day: day, owner: false });
            w.firms[t.farm.ix()].workers.push(a);
        } else {
            p.income.avg = 6.0;
        }
    }
    if married && grown.len() == 2 {
        w.persons[grown[0].ix()].spouse = Some(grown[1]);
        w.persons[grown[1].ix()].spouse = Some(grown[0]);
    }
    let youngest = grown.iter().map(|a| w.person(*a).age_on(day)).fold(f32::MAX, f32::min);
    let mother = grown.iter().copied().find(|a| w.person(*a).sex == Sex::Female);
    let father = grown.iter().copied().find(|a| w.person(*a).sex == Sex::Male);
    for _ in 0..kids {
        let oldest = (youngest - 18.0).min(15.5);
        if oldest < 0.5 { break; }
        let age = w.rng.range(0.5, oldest);
        let c = new_person(w, h, None, age, &surname);
        w.persons[c.ix()].mother = mother;
        w.persons[c.ix()].father = father;
        for a in &grown { w.persons[a.ix()].children.push(c); }
    }
    h
}

/// The tiny world grown to about ninety people in the mix the real town is founded with.
fn ninety(seed: u64) -> (World, Tiny) {
    let (mut w, t) = tiny_world();
    w.rng = Rng::new(seed);
    w.settings.immigration = false;
    for _ in 0..12 {
        let age = w.rng.range(24.0, 46.0);
        let (her, his) = ((age + w.rng.range(-3.0, 3.0)).max(17.0), (age + w.rng.range(-3.0, 4.0)).max(17.0));
        let kids = w.rng.int(1, 3) as usize;
        settle(&mut w, &t, &[(Sex::Female, her), (Sex::Male, his)], kids, true);
    }
    for _ in 0..4 {
        let age = w.rng.range(19.0, 55.0);
        settle(&mut w, &t, &[(Sex::Female, age), (Sex::Male, age + 1.0)], 0, true);
    }
    for i in 0..5 {
        let age = w.rng.range(18.0, 50.0);
        settle(&mut w, &t, &[(if i % 2 == 0 { Sex::Female } else { Sex::Male }, age)], 0, false);
    }
    for i in 0..4 {
        let age = w.rng.range(61.0, 76.0);
        if i < 2 { settle(&mut w, &t, &[(Sex::Female, age), (Sex::Male, age + 2.0)], 0, true); }
        else { settle(&mut w, &t, &[(if i == 2 { Sex::Female } else { Sex::Male }, age + 3.0)], 0, false); }
    }
    for _ in 0..2 {
        let age = w.rng.range(26.0, 45.0);
        settle(&mut w, &t, &[(Sex::Female, age)], 2, false);
    }
    // Friends of about their own age under other roofs, as at the founding.
    let day = w.day();
    for a in 0..w.persons.len() {
        let (age, hh) = (w.persons[a].age_on(day), w.persons[a].household);
        if age < 6.0 { continue; }
        let peers: Vec<PersonId> = w.persons.iter().filter(|b| b.household != hh && (b.age_on(day) < ADULT_AGE) == (age < ADULT_AGE) && (b.age_on(day) - age).abs() < if age < ADULT_AGE { 4.0 } else { 18.0 }).map(|b| b.id).collect();
        for _ in 0..3 {
            if peers.is_empty() { break; }
            let b = peers[w.rng.index(peers.len())];
            let aff = w.rng.range(0.3, 0.8);
            let a = PersonId(a as u32);
            if w.persons[a.ix()].affinity(b) < aff {
                let (d1, d2) = (aff - w.persons[a.ix()].affinity(b), aff - w.persons[b.ix()].affinity(a));
                w.persons[a.ix()].befriend(b, d1);
                w.persons[b.ix()].befriend(a, d2);
            }
        }
    }
    check(&w);
    (w, t)
}

// ------------------------------------------------------- making people

#[test]
fn new_person_is_valid() {
    let (mut w, _t) = tiny_world();
    let h = new_household(&mut w, "Quill");
    assert_eq!(w.household(h).name, "Quill");
    assert!(w.household(h).members.is_empty() && w.household(h).home.is_none() && w.household(h).money == 0.0);
    for i in 0..300 {
        let age = (i % 85) as f32 + 0.5;
        let want = match i % 3 { 0 => Some(Sex::Female), 1 => Some(Sex::Male), _ => None };
        let id = new_person(&mut w, h, want, age, "Quill");
        let p = w.person(id);
        assert_eq!(p.id.ix(), w.persons.len() - 1);
        assert!(p.alive && p.present);
        assert_eq!(p.household, h);
        assert!(w.household(h).members.contains(&id));
        assert_eq!(p.surname, "Quill");
        if let Some(s) = want { assert_eq!(p.sex, s); }
        let list: &[&str] = if p.sex == Sex::Female { &names::FEMALE } else { &names::MALE };
        assert!(list.contains(&p.name.as_str()));
        assert!((w.age(id) - age).abs() < 0.05);
        assert_eq!(w.is_adult(id), age >= ADULT_AGE);
        for k in 0..N_PREFS { assert!((0.0..=1.0).contains(&p.genes.0[k]) && (0.0..=1.0).contains(&p.prefs.0[k])); }
        assert!((0.0..=1.0).contains(&p.education) && (0.0..=1.0).contains(&p.skill) && (0.0..=1.0).contains(&p.trust));
        if age < 6.0 { assert_eq!(p.education, 0.0); }
        if age < ADULT_AGE { assert_eq!(p.skill, 0.0); }
        assert!((30.0..=100.0).contains(&p.needs.health) && (0.0..=100.0).contains(&p.needs.hunger));
        assert!(p.loc.ix() < w.places.len());
        assert!(p.job.is_none() && p.spouse.is_none() && p.children.is_empty() && !p.immigrant);
    }
    // Strangers are drawn around the middle, each preference on its own.
    let means = pref_means_of(&w, |p| p.surname == "Quill");
    for m in means { assert!((m - 0.5).abs() < 0.06, "{m}"); }
    // An id that names no household gets a household made for it rather than a panic.
    let stray = new_person(&mut w, HouseholdId(9999), None, 30.0, "Rook");
    assert_eq!(w.hh_of(stray).name, "Rook");
    check(&w);
}

#[test]
fn pref_means_cover_the_adults_in_town() {
    let (mut w, t) = tiny_world();
    for p in w.persons.iter_mut() { p.prefs[Pref::Risk] = 0.2; }
    w.persons[t.cai.ix()].prefs[Pref::Risk] = 1.0; // a child: not counted
    assert!((pref_means(&w)[Pref::Risk as usize] - 0.2).abs() < 1e-5);
    w.persons[t.ada.ix()].prefs[Pref::Risk] = 1.0;
    assert!((pref_means(&w)[Pref::Risk as usize] - 0.3).abs() < 1e-5);
    w.persons[t.ada.ix()].present = false;
    assert!((pref_means(&w)[Pref::Risk as usize] - 0.2).abs() < 1e-5);
    w.persons[t.cai.ix()].genes[Pref::Risk] = 0.9;
    assert!(gene_means(&w)[Pref::Risk as usize] > 0.5);
}

// ---------------------------------------------------------- inheritance

/// Mean of a child's genes and expressed preferences over many births to Ada and Bram.
fn many_children(w: &mut World, t: &Tiny, father: bool, n: usize) -> ([f32; N_PREFS], [f32; N_PREFS], f32) {
    let (mut g, mut e) = ([0.0f32; N_PREFS], [0.0f32; N_PREFS]);
    let mut spread = 0.0;
    for _ in 0..n {
        let (genes, prefs) = inherit(w, t.ada, if father { Some(t.bram) } else { None });
        for k in 0..N_PREFS {
            assert!((0.0..=1.0).contains(&genes.0[k]) && (0.0..=1.0).contains(&prefs.0[k]));
            g[k] += genes.0[k] / n as f32;
            e[k] += prefs.0[k] / n as f32;
        }
        spread += (genes.0[0] - 0.7).powi(2) / n as f32;
    }
    (g, e, spread.sqrt())
}

#[test]
fn inheritance_follows_heritability() {
    let (mut w, t) = tiny_world();
    // Parents whose genes say one thing and whose ways say another.
    w.persons[t.ada.ix()].genes = Prefs([0.8; N_PREFS]);
    w.persons[t.bram.ix()].genes = Prefs([0.6; N_PREFS]);
    w.persons[t.ada.ix()].prefs = Prefs([0.2; N_PREFS]);
    w.persons[t.bram.ix()].prefs = Prefs([0.3; N_PREFS]);

    w.settings.heritability = 1.0;
    let (g, e, _) = many_children(&mut w, &t, true, 2000);
    for k in 0..N_PREFS {
        assert!((g[k] - 0.7).abs() < 0.02, "genes are the parents' mean: {}", g[k]);
        assert!((e[k] - 0.7).abs() < 0.02, "fully heritable: expressed follows genes, not the parents' ways: {}", e[k]);
    }
    let (genes, prefs) = inherit(&mut w, t.ada, Some(t.bram));
    assert_eq!(genes, prefs);

    w.settings.heritability = 0.0;
    let (g, e, _) = many_children(&mut w, &t, true, 2000);
    for k in 0..N_PREFS {
        assert!((g[k] - 0.7).abs() < 0.02, "genes pass on whatever is expressed: {}", g[k]);
        assert!((e[k] - 0.25).abs() < 0.02, "not heritable: expressed follows upbringing: {}", e[k]);
    }

    w.settings.heritability = 0.5;
    let (_, e, _) = many_children(&mut w, &t, true, 2000);
    for k in 0..N_PREFS { assert!((e[k] - 0.475).abs() < 0.02, "half and half: {}", e[k]); }
}

#[test]
fn a_lone_mothers_child_takes_her_genes_with_wider_mutation() {
    let (mut w, t) = tiny_world();
    w.persons[t.ada.ix()].genes = Prefs([0.7; N_PREFS]);
    w.persons[t.bram.ix()].genes = Prefs([0.7; N_PREFS]);
    let (g2, _, spread2) = many_children(&mut w, &t, true, 3000);
    let (g1, _, spread1) = many_children(&mut w, &t, false, 3000);
    assert!((g1[0] - 0.7).abs() < 0.03 && (g2[0] - 0.7).abs() < 0.03);
    assert!(spread1 > spread2 * 1.25, "one known parent: {spread1}; two: {spread2}");
}

#[test]
fn who_has_the_children_shifts_the_towns_preferences_over_generations() {
    let (mut w, _t) = tiny_world();
    w.settings.heritability = 0.5;
    let h = new_household(&mut w, "Pell");
    let mut generation: Vec<PersonId> = (0..120).map(|i| new_person(&mut w, h, Some(if i % 2 == 0 { Sex::Female } else { Sex::Male }), 25.0, "Pell")).collect();
    let mean = |w: &World, ids: &[PersonId], pref: Pref, genes: bool| ids.iter().map(|p| if genes { w.person(*p).genes[pref] } else { w.person(*p).prefs[pref] }).sum::<f32>() / ids.len() as f32;
    let start = (mean(&w, &generation, Pref::Family, false), mean(&w, &generation, Pref::Family, true), mean(&w, &generation, Pref::Risk, false));
    for _ in 0..5 {
        // Only the half who most value family have children; they pair off at random and have four each.
        let mut women: Vec<PersonId> = generation.iter().copied().filter(|p| w.person(*p).sex == Sex::Female).collect();
        let mut men: Vec<PersonId> = generation.iter().copied().filter(|p| w.person(*p).sex == Sex::Male).collect();
        for side in [&mut women, &mut men] {
            side.sort_by(|a, b| w.person(*b).prefs[Pref::Family].partial_cmp(&w.person(*a).prefs[Pref::Family]).unwrap());
            side.truncate(side.len() / 2);
        }
        w.rng.shuffle(&mut men);
        let mut next = Vec::new();
        for (m, f) in women.iter().zip(men.iter()) {
            for i in 0..4 {
                let (genes, prefs) = inherit(&mut w, *m, Some(*f));
                let c = new_person(&mut w, h, Some(if i % 2 == 0 { Sex::Female } else { Sex::Male }), 0.0, "Pell");
                w.persons[c.ix()].genes = genes;
                w.persons[c.ix()].prefs = prefs;
                next.push(c);
            }
        }
        generation = next;
    }
    let end = (mean(&w, &generation, Pref::Family, false), mean(&w, &generation, Pref::Family, true), mean(&w, &generation, Pref::Risk, false));
    assert!(end.0 > start.0 + 0.15, "expressed family feeling rose from {} to {}", start.0, end.0);
    assert!(end.1 > start.1 + 0.10, "and so did the genes behind it: {} to {}", start.1, end.1);
    assert!((end.2 - start.2).abs() < 0.08, "a preference nobody selected on stayed put: {} to {}", start.2, end.2);
}

#[test]
fn children_drift_toward_those_who_raise_and_teach_them() {
    let (mut w, t) = tiny_world();
    w.settings.heritability = 0.5;
    let k = Pref::Diligence;
    for p in w.persons.iter_mut() { p.genes[k] = 0.5; p.prefs[k] = 0.5; p.friends.clear(); }
    // Cai and Dara are raised by the same hard-working parents; only Dara goes to school, under an idle teacher.
    for p in [t.ada, t.bram] { w.persons[p.ix()].prefs[k] = 0.9; }
    let school = FirmId(w.firms.len() as u32);
    let mut f = w.firms[t.tavern_firm.ix()].clone();
    f.id = school;
    f.kind = FirmKind::School;
    f.workers = vec![t.greta];
    w.firms.push(f);
    w.persons[t.greta.ix()].prefs[k] = 0.0;
    w.persons[t.dara.ix()].enrolled = true;
    for _ in 0..40 { people::upbringing(&mut w); }
    let (cai, dara) = (w.person(t.cai).prefs[k], w.person(t.dara).prefs[k]);
    assert!(cai > 0.55 && cai < 0.75, "raised by the diligent, a child becomes more so, within what the genes allow: {cai}");
    assert!(dara < cai - 0.02, "the teacher pulls the other way: {dara} against {cai}");
    assert_eq!(w.person(t.ada).prefs[k], 0.9, "adults are who they are");
    assert_eq!(w.person(t.cai).genes[k], 0.5, "genes never move");
    // A friend's pull.
    let before = w.person(t.cai).prefs[Pref::Risk];
    w.persons[t.cai.ix()].befriend(t.dara, 0.9);
    w.persons[t.dara.ix()].prefs[Pref::Risk] = 1.0;
    for p in [t.ada, t.bram] { w.persons[p.ix()].prefs[Pref::Risk] = before; }
    for _ in 0..20 { people::upbringing(&mut w); }
    assert!(w.person(t.cai).prefs[Pref::Risk] > before + 0.01);
    // Fully heritable preferences do not drift at all.
    w.settings.heritability = 1.0;
    let fixed = w.person(t.cai).prefs;
    for _ in 0..20 { people::upbringing(&mut w); }
    assert_eq!(w.person(t.cai).prefs, fixed);
}

// ---------------------------------------------------------------- death

#[test]
fn hazard_rises_with_age_and_sickness_and_falls_with_medicine() {
    let mut last = 0.0;
    for age in [5.0, 20.0, 40.0, 55.0, 65.0, 75.0, 85.0, 95.0] {
        let h = death_hazard(age, 90.0, 1.0);
        assert!(h > last, "age {age}");
        last = h;
    }
    assert!(death_hazard(30.0, 90.0, 1.0) < 0.0005);
    assert!(death_hazard(70.0, 90.0, 1.0) > 0.004 && death_hazard(70.0, 90.0, 1.0) < 0.012);
    assert!(death_hazard(30.0, 40.0, 1.0) > 4.0 * death_hazard(30.0, 90.0, 1.0));
    assert!(death_hazard(30.0, 5.0, 1.0) > 0.1, "near collapse kills at any age");
    assert!(death_hazard(70.0, 60.0, 1.5) < 0.6 * death_hazard(70.0, 60.0, 1.0));
    assert!(death_hazard(110.0, 0.0, 0.01) <= 1.0);
}

#[test]
fn the_starving_die_and_the_well_mostly_do_not() {
    let (mut w, t) = tiny_world();
    w.persons[t.finn.ix()].needs.health = 0.0;
    w.persons[t.finn.ix()].needs.hunger = 100.0;
    death::mortality(&mut w);
    assert!(!w.person(t.finn).alive);
    assert!(w.chronicle.iter().any(|c| c.text.contains("Finn Birch") && c.text.contains("hunger")));
    assert!(w.person(t.edith).felt.disorder > 0.0, "a death by hunger unsettles those close to it");
    assert!(w.person(t.edith).diary.iter().any(|d| d.kind == "grief" && d.text.contains("My husband Finn")));
    check(&w);
}

#[test]
fn death_leaves_no_dangling_references() {
    let (mut w, t) = tiny_world();
    w.polity.offices[0].holder = Some(Actor::Person(t.ada));
    w.polity.player.possessing = Some(t.ada);
    let before = coin(&w);
    die(&mut w, t.ada, "illness");
    check(&w);
    let ada = w.person(t.ada);
    assert!(!ada.alive && !ada.present && ada.job.is_none() && ada.spouse.is_none());
    assert_eq!(w.person(t.bram).spouse, None);
    assert!(!w.household(ada.household).members.contains(&t.ada));
    assert!(!w.firm(t.farm).workers.contains(&t.ada));
    assert_eq!(w.firm(t.farm).manager, Some(t.bram), "her husband, who works the farm, takes it on");
    assert!(w.person(t.bram).job.as_ref().unwrap().owner);
    assert_eq!(w.person(t.edith).affinity(t.ada), 0.0);
    assert_eq!(w.polity.offices[0].holder, None, "her seat is vacant");
    assert_eq!(w.polity.player.possessing, None);
    assert!((coin(&w) - before).abs() < 1e-3, "no coin made or lost");
    assert_eq!(w.demo.deaths, 1);
    assert!(w.person(t.bram).diary.iter().any(|d| d.kind == "grief" && d.text == "My wife Ada has died."));
    assert!(w.person(t.cai).diary.iter().any(|d| d.text == "My mother Ada has died."));
    assert!(w.person(t.bram).needs.fun < 65.0);
    assert!(w.chronicle.iter().any(|c| c.text == "Ada Ashdown, aged 38, died of illness."));
    // Dying twice changes nothing.
    die(&mut w, t.ada, "illness");
    assert_eq!(w.demo.deaths, 1);
    // Someone who has already left town can die without touching it.
    depart(&mut w, t.greta, "");
    die(&mut w, t.greta, "old age");
    assert!(!w.person(t.greta).alive);
    assert_eq!(w.demo.deaths, 1);
    check(&w);
}

#[test]
fn an_estate_with_no_heirs_passes_to_the_town() {
    let (mut w, t) = tiny_world();
    let before = coin(&w);
    let treasury = w.polity.jur(TOWN).treasury;
    let h = w.person(t.greta).household;
    let owned = w.properties_of(h);
    assert_eq!(owned.len(), 3, "her house, the workshop and the tavern");
    die(&mut w, t.greta, "old age");
    check(&w);
    assert!((coin(&w) - before).abs() < 1e-3);
    assert!((w.polity.jur(TOWN).treasury - treasury - 180.0).abs() < 1e-3);
    assert_eq!(w.household(h).money, 0.0);
    assert!(w.household(h).members.is_empty() && w.household(h).home.is_none());
    for pl in owned { assert_eq!(w.place(pl).owner, Owner::State(TOWN)); }
    assert_eq!(w.firm(t.workshop).owner, Owner::State(TOWN));
    assert_eq!(w.firm(t.workshop).manager, Some(t.hugo), "the remaining hand keeps the workshop running");
    assert_eq!(w.person(t.hugo).job.as_ref().unwrap().wage, 9.0);
    assert_eq!(w.firm(t.tavern_firm).manager, Some(t.ida));
}

#[test]
fn an_estate_passes_to_children_living_elsewhere() {
    let (mut w, t) = tiny_world();
    // Joss keeps his own house, with savings and a cottage he lets.
    let h = new_household(&mut w, "Dunmore");
    let home = add_home(&mut w, Owner::Household(h));
    let cottage = add_home(&mut w, Owner::Household(h));
    let old = w.person(t.joss).household;
    w.households[old.ix()].members.retain(|m| *m != t.joss);
    w.persons[t.joss.ix()].household = h;
    w.households[h.ix()].members.push(t.joss);
    w.households[h.ix()].home = Some(home);
    w.households[h.ix()].money = 90.0;
    w.households[h.ix()].stock[Good::Food as usize] = 4.0;
    check(&w);
    let before = coin(&w);
    let hugo_purse = w.hh_of(t.hugo).money;
    die(&mut w, t.joss, "old age");
    check(&w);
    assert!((coin(&w) - before).abs() < 1e-3);
    assert!((w.hh_of(t.hugo).money - hugo_purse - 90.0).abs() < 1e-3);
    let heir = w.person(t.hugo).household;
    assert_eq!(w.place(home).owner, Owner::Household(heir));
    assert_eq!(w.place(cottage).owner, Owner::Household(heir));
    assert!(w.hh_of(t.hugo).stock[Good::Food as usize] >= 10.0);
    assert!(w.person(t.hugo).diary.iter().any(|d| d.text.contains("inherited 90 coins")));
    assert!(w.person(t.hugo).diary.iter().any(|d| d.text == "My father Joss has died."));
}

#[test]
fn orphans_are_taken_in_with_what_their_household_had() {
    let (mut w, t) = tiny_world();
    let before = coin(&w);
    let old = w.person(t.cai).household;
    die(&mut w, t.ada, "illness");
    check(&w);
    assert_eq!(w.person(t.cai).household, old, "one parent is enough");
    die(&mut w, t.bram, "illness");
    check(&w);
    // No kin in town: their mother's friend Edith and their father's friend Hugo are equally close; the Birches take them.
    let new = w.person(t.edith).household;
    assert_eq!(w.person(t.cai).household, new);
    assert_eq!(w.person(t.dara).household, new);
    assert!(w.household(new).members.contains(&t.cai) && w.household(new).members.contains(&t.dara));
    assert!(w.household(old).members.is_empty() && w.household(old).home.is_none());
    assert_eq!(w.household(old).money, 0.0);
    assert!((w.household(new).money - 260.0).abs() < 1e-3, "the purses are joined");
    assert!((coin(&w) - before).abs() < 1e-3);
    assert!(w.properties_of(old).is_empty());
    assert_eq!(w.properties_of(new).len(), 3, "the farm, the Ashdown house and the house the Birches rented");
    let home = w.household(new).home.unwrap();
    assert_eq!(w.place(home).tenant, None, "nobody is their own tenant");
    assert_eq!(w.firm(t.farm).owner, Owner::Household(new));
    assert!(w.firm(t.farm).manager.is_some());
    assert!(w.chronicle.iter().any(|c| c.text == "The orphaned Ashdown children were taken in by the Birch household."));
    assert!(w.person(t.dara).diary.iter().any(|d| d.text == "We have gone to live with the Birch household."));
}

#[test]
fn kin_come_before_friends_for_orphans() {
    let (mut w, t) = tiny_world();
    // Greta is Bram's sister.
    let granny = new_person(&mut w, HouseholdId(2), Some(Sex::Female), 70.0, "Carver");
    for c in [t.bram, t.greta] {
        w.persons[c.ix()].mother = Some(granny);
        w.persons[granny.ix()].children.push(c);
    }
    die(&mut w, granny, "old age");
    die(&mut w, t.ada, "illness");
    die(&mut w, t.bram, "illness");
    check(&w);
    assert_eq!(w.person(t.cai).household, w.person(t.greta).household, "their aunt takes them");
    assert!(close_kin(&w, t.cai, t.greta) && close_kin(&w, t.bram, t.greta) && close_kin(&w, t.cai, t.dara) && close_kin(&w, t.hugo, t.joss));
    assert!(!close_kin(&w, t.ada, t.edith) && !close_kin(&w, t.ada, t.greta));
}

// ------------------------------------------------------------ departure

#[test]
fn one_who_leaves_alone_takes_their_share_and_leaves_no_loose_ends() {
    let (mut w, t) = tiny_world();
    let before = coin(&w);
    depart(&mut w, t.finn, "exiled");
    check(&w);
    let finn = w.person(t.finn);
    assert!(finn.alive && !finn.present && finn.job.is_none() && finn.spouse.is_none());
    assert_eq!(w.person(t.edith).spouse, None);
    assert!(!w.firm(t.farm).workers.contains(&t.finn));
    assert_eq!(w.person(t.hugo).affinity(t.finn), 0.0);
    assert!((w.hh_of(t.edith).money - 20.0).abs() < 1e-3, "he took half the purse of 40");
    assert!((w.demo.coin_out - 20.0).abs() < 1e-3);
    assert!((coin(&w) + w.demo.coin_out - before).abs() < 1e-3, "the only coin gone is what he carried out");
    assert_eq!(w.demo.departures, 1);
    assert!(w.chronicle.iter().any(|c| c.text == "Finn Birch left Testford for good (exiled)."));
    assert!(w.person(t.edith).diary.iter().any(|d| d.text == "My husband Finn has left town for good."));
    depart(&mut w, t.finn, "exiled");
    assert_eq!(w.demo.departures, 1);
}

#[test]
fn a_household_that_leaves_takes_its_purse_and_its_property_is_sold_or_left_to_the_town() {
    let (mut w, t) = tiny_world();
    let before = coin(&w);
    let h = w.person(t.hugo).household;
    let home = w.household(h).home.unwrap();
    w.demo.leaving.push(Leaving { household: h, since: w.day() });
    depart(&mut w, t.hugo, "emigrated");
    assert!((w.household(h).money - 60.0).abs() < 1e-3, "the purse goes with the last of them");
    assert_eq!(w.demo.coin_out, 0.0);
    depart(&mut w, t.ida, "emigrated");
    depart(&mut w, t.joss, "emigrated");
    check(&w);
    assert!(w.household(h).members.is_empty() && w.household(h).home.is_none());
    assert_eq!(w.household(h).money, 0.0);
    // Nobody in town found the house worth the price, so it went to the town and they left with their 60 coins.
    assert_eq!(w.place(home).owner, Owner::State(TOWN));
    assert!((w.demo.coin_out - 60.0).abs() < 1e-3);
    assert!((coin(&w) + w.demo.coin_out - before).abs() < 1e-3);
    assert!(w.demo.leaving.is_empty());
    assert_eq!(w.firm(t.tavern_firm).manager, None, "the tavern has lost its keeper and nobody is left to take over");
    assert!(w.firm(t.tavern_firm).workers.is_empty());
    assert_eq!(w.demo.departures, 3);
    assert_eq!(w.chronicle.iter().filter(|c| c.text == "The Dunmore household left Testford for good (emigrated).").count(), 1);
    for p in [t.hugo, t.ida, t.joss] { assert!(w.person(p).alive && !w.person(p).present); }
}

#[test]
fn a_sitting_tenant_with_savings_buys_from_a_departing_landlord() {
    let (mut w, t) = tiny_world();
    let before = coin(&w);
    let (landlord, tenant) = (w.person(t.ada).household, w.person(t.edith).household);
    let rented = w.household(tenant).home.unwrap();
    w.households[tenant.ix()].money = 400.0;
    let before = before + 360.0;
    w.demo.leaving.push(Leaving { household: landlord, since: w.day() });
    for p in [t.ada, t.bram, t.cai, t.dara] { depart(&mut w, p, "emigrated"); }
    check(&w);
    // Rent of 1.6 a day saved is worth more than half the house's value of 150 to a household that can spare it.
    assert_eq!(w.place(rented).owner, Owner::Household(tenant));
    assert_eq!(w.place(rented).tenant, None);
    assert!((w.household(tenant).money - 325.0).abs() < 1e-3);
    assert!((w.demo.coin_out - (220.0 + 75.0)).abs() < 1e-3, "the sellers carried the price away with their purse");
    assert!((coin(&w) + w.demo.coin_out - before).abs() < 1e-3);
    // The farm found no buyer at half its value and is the town's now.
    assert_eq!(w.firm(t.farm).owner, Owner::State(TOWN));
    assert!(w.person(t.finn).diary.iter().any(|d| d.kind == "money" && d.text.contains("from the departing Ashdown household for 75 coins")));
}

// ------------------------------------------------------- births, marriage

#[test]
fn couples_have_children_when_one_more_is_worth_it() {
    let (mut w, t) = tiny_world();
    // The Birches: young, childless, both earning, with room.
    let worth = child_worth(&w, t.edith, t.finn).expect("a first child is worth having");
    assert!((worth - 16.5).abs() < 0.01, "{worth}");
    // The Ashdowns already have two, and care for family a little less than most.
    for p in [t.ada, t.bram] { w.persons[p.ix()].prefs[Pref::Family] = 0.4; }
    assert!(child_worth(&w, t.ada, t.bram).is_none());
    w.persons[t.ada.ix()].prefs[Pref::Family] = 0.95;
    w.persons[t.bram.ix()].prefs[Pref::Family] = 0.95;
    assert!(child_worth(&w, t.ada, t.bram).is_some(), "those who prize family want a third");
    // No income and no savings: they cannot feed another.
    for p in [t.edith, t.finn] { w.persons[p.ix()].income.avg = 0.0; }
    let birch = w.person(t.edith).household;
    w.households[birch.ix()].money = 5.0;
    assert!(child_worth(&w, t.edith, t.finn).is_none());
    w.households[birch.ix()].money = 2000.0;
    let from_savings = child_worth(&w, t.edith, t.finn).unwrap();
    assert!(from_savings < worth, "living off savings makes a child less attractive: {from_savings}");
    for p in [t.edith, t.finn] { w.persons[p.ix()].income.avg = 9.0; }
    // No home, no child.
    let h = w.person(t.edith).household;
    let home = w.households[h.ix()].home.take();
    assert!(child_worth(&w, t.edith, t.finn).is_none());
    w.households[h.ix()].home = home;

    let before = w.persons.len();
    let mut days = 0;
    while w.person(t.edith).children.is_empty() && days < 380 { w.slot += SLOTS_PER_DAY; family::births(&mut w); days += 1; }
    assert!(days > 3 && days < 380, "a small daily chance: {days} days");
    let c = *w.person(t.edith).children.first().unwrap();
    let child = w.person(c);
    assert_eq!(w.age(c), 0.0);
    assert_eq!((child.mother, child.father), (Some(t.edith), Some(t.finn)));
    assert_eq!(child.surname, "Birch");
    assert_eq!(child.household, w.person(t.edith).household);
    assert!(w.person(t.finn).children.contains(&c));
    assert!(w.demo.births >= 1 && w.persons.len() > before);
    assert!(w.person(t.finn).diary.iter().any(|d| d.kind == "family" && d.text.contains("was born today")));
    assert!(child_worth(&w, t.edith, t.finn).is_none(), "not while the last is still a babe in arms");
    check(&w);
    // The sterile and the too old have none.
    let (mut w, t) = tiny_world();
    w.persons[t.finn.ix()].sterile = true;
    w.persons[t.ida.ix()].birth_day -= 20 * DAYS_PER_YEAR as i32;
    for _ in 0..300 { w.slot += SLOTS_PER_DAY; family::births(&mut w); }
    assert!(w.person(t.edith).children.is_empty() && w.person(t.ida).children.is_empty());
}

#[test]
fn dear_friends_marry_and_join_households() {
    let (mut w, t) = tiny_world();
    // A single man of Greta's age, renting nothing and owning nothing.
    let h = new_household(&mut w, "Vance");
    let odo = new_person(&mut w, h, Some(Sex::Male), 44.0, "Vance");
    w.households[h.ix()].money = 30.0;
    w.persons[odo.ix()].income.avg = 9.0;
    let kid = new_person(&mut w, h, Some(Sex::Female), 2.0, "Vance");
    w.persons[kid.ix()].father = Some(odo);
    w.persons[odo.ix()].children.push(kid);
    for p in [odo, t.greta] { w.persons[p.ix()].prefs[Pref::Family] = 0.6; w.persons[p.ix()].prefs[Pref::Autonomy] = 0.4; }
    w.persons[odo.ix()].befriend(t.greta, 0.65);
    w.persons[t.greta.ix()].befriend(odo, 0.9);
    for _ in 0..40 { w.slot += SLOTS_PER_DAY; family::courtship(&mut w); }
    assert_eq!(w.person(odo).spouse, None, "not until each holds the other dear");
    w.persons[odo.ix()].befriend(t.greta, 0.25);
    assert!(marriage_gain(&w, odo, t.greta) > 0.0 && marriage_gain(&w, t.greta, odo) > 0.0);
    let before = coin(&w);
    let mut days = 0;
    while w.person(odo).spouse.is_none() && days < 250 { w.slot += SLOTS_PER_DAY; family::courtship(&mut w); days += 1; }
    assert!(days < 250);
    check(&w);
    assert_eq!(w.person(odo).spouse, Some(t.greta));
    assert_eq!(w.person(t.greta).spouse, Some(odo));
    let home = w.person(t.greta).household;
    assert_eq!(w.person(odo).household, home, "he moves in with her: she has the house");
    assert_eq!(w.person(kid).household, home, "and his daughter with him");
    assert!(w.household(h).members.is_empty());
    assert!((w.household(home).money - 210.0).abs() < 1e-3, "one purse now");
    assert!((coin(&w) - before).abs() < 1e-3);
    assert!(w.chronicle.iter().any(|c| c.text.starts_with("Greta Carver and ") && c.text.ends_with(" Vance were married.")));
    // Kin do not marry however fond; nor do the married.
    let (mut w, t) = tiny_world();
    w.persons[t.cai.ix()].birth_day -= 10 * DAYS_PER_YEAR as i32;
    w.persons[t.dara.ix()].birth_day -= 12 * DAYS_PER_YEAR as i32;
    w.persons[t.cai.ix()].befriend(t.dara, 1.0);
    w.persons[t.dara.ix()].befriend(t.cai, 1.0);
    w.persons[t.ada.ix()].befriend(t.finn, 1.0);
    w.persons[t.finn.ix()].befriend(t.ada, 1.0);
    for _ in 0..300 { w.slot += SLOTS_PER_DAY; family::courtship(&mut w); }
    assert_eq!(w.person(t.cai).spouse, None);
    assert_eq!(w.person(t.ada).spouse, Some(t.bram));
}

#[test]
fn the_grown_leave_home_when_they_can_keep_themselves_and_there_is_a_house() {
    let (mut w, t) = tiny_world();
    w.persons[t.cai.ix()].birth_day -= 8 * DAYS_PER_YEAR as i32; // twenty now
    w.persons[t.dara.ix()].birth_day = w.day() - DAYS_PER_YEAR as i32; // and his sister a babe, so she stays a child throughout
    w.persons[t.cai.ix()].prefs[Pref::Autonomy] = 0.9;
    w.persons[t.cai.ix()].prefs[Pref::Family] = 0.3;
    for _ in 0..100 { w.slot += SLOTS_PER_DAY; family::leaving_home(&mut w); }
    assert_eq!(w.person(t.cai).household, w.person(t.ada).household, "no income, nowhere to go");
    let day = w.day();
    w.persons[t.cai.ix()].job = Some(Job { firm: t.farm, wage: 9.0, role: Role::Farmer, since_day: day, owner: false });
    w.firms[t.farm.ix()].workers.push(t.cai);
    w.persons[t.cai.ix()].income.avg = 9.0;
    for _ in 0..100 { w.slot += SLOTS_PER_DAY; family::leaving_home(&mut w); }
    assert_eq!(w.person(t.cai).household, w.person(t.ada).household, "an income, but no empty house");
    add_home(&mut w, Owner::State(TOWN));
    let before = coin(&w);
    for _ in 0..100 { w.slot += SLOTS_PER_DAY; family::leaving_home(&mut w); }
    check(&w);
    let new = w.person(t.cai).household;
    assert_ne!(new, w.person(t.ada).household);
    assert_eq!(w.household(new).members, vec![t.cai]);
    assert!((w.household(new).money - 220.0 / 6.0).abs() < 0.01, "half an adult's share of the purse: {}", w.household(new).money);
    assert!((coin(&w) - before).abs() < 1e-3);
    assert!(w.person(t.cai).diary.iter().any(|d| d.kind == "home"));
    // Coming of age is marked on the day.
    let (mut w, t) = tiny_world();
    w.persons[t.dara.ix()].birth_day = w.day() - 16 * DAYS_PER_YEAR as i32;
    family::coming_of_age(&mut w);
    assert!(w.person(t.dara).diary.iter().any(|d| d.text == "I came of age today."));
    assert!(w.person(t.cai).diary.is_empty());
}

// ------------------------------------------------------------ migration

#[test]
fn the_miserable_with_little_to_hold_them_leave() {
    let (mut w, t) = tiny_world();
    let (birch, ashdown) = (w.person(t.edith).household, w.person(t.ada).household);
    // Everyone is wretched; the Birches rent and own nothing, the Ashdowns have a farm and two houses.
    for p in w.persons.iter_mut() { p.wellbeing.total = -0.7; p.prefs[Pref::Continuity] = 0.3; }
    for h in [1usize, 2, 3] { if HouseholdId(h as u32) != birch { for m in w.households[h].members.clone() { w.persons[m.ix()].wellbeing.total = 0.2; } } }
    w.settings.immigration = false;
    let before = coin(&w);
    for _ in 0..(MISERY_DAYS_FOR_TEST - 1) { w.slot += SLOTS_PER_DAY; migration::emigration(&mut w); }
    assert!(w.demo.leaving.is_empty(), "not before they have borne it for some time");
    for _ in 0..80 { w.slot += SLOTS_PER_DAY; migration::emigration(&mut w); }
    check(&w);
    assert!(!w.person(t.edith).present && !w.person(t.finn).present, "the Birches have gone");
    assert!(w.household(birch).members.is_empty());
    assert!(w.person(t.ada).present && !w.demo.is_leaving(ashdown), "the Ashdowns' property holds them");
    assert!(w.person(t.hugo).present, "and those who are not wretched stay");
    assert!((w.demo.coin_out - 40.0).abs() < 1e-3);
    assert!((coin(&w) + w.demo.coin_out - before).abs() < 1e-3);
    assert!(w.person(t.ada).diary.iter().any(|d| d.text == "My friend Edith has left town for good."));
    assert!(w.chronicle.iter().any(|c| c.text == "The Birch household left Testford for good (emigrated)."));
}
const MISERY_DAYS_FOR_TEST: u32 = migration::MISERY_DAYS as u32;

#[test]
fn the_player_decides_for_the_person_they_live_as() {
    let (mut w, t) = tiny_world();
    for p in [t.edith, t.finn] { w.persons[p.ix()].wellbeing.total = -0.9; w.persons[p.ix()].prefs[Pref::Continuity] = 0.2; }
    w.polity.player.possessing = Some(t.edith);
    w.polity.player.overrides.emigrate = Some(false);
    for _ in 0..200 { w.slot += SLOTS_PER_DAY; migration::emigration(&mut w); }
    assert!(w.person(t.edith).present && w.demo.leaving.is_empty(), "told to stay, they stay");
    // Told to go, they go at once, however content.
    let (mut w, t) = tiny_world();
    w.polity.player.possessing = Some(t.greta);
    w.polity.player.overrides.emigrate = Some(true);
    w.slot += SLOTS_PER_DAY;
    migration::emigration(&mut w);
    assert!(w.demo.is_leaving(w.person(t.greta).household));
    assert_eq!(w.polity.player.overrides.emigrate, None);
    for _ in 0..3 { w.slot += SLOTS_PER_DAY; migration::emigration(&mut w); }
    assert!(!w.person(t.greta).present);
    assert_eq!(w.polity.player.possessing, None);
    check(&w);
}

#[test]
fn outsiders_come_when_the_town_looks_better_than_the_road() {
    let (mut w, t) = tiny_world();
    let bleak = town_appeal(&w);
    assert!(bleak < 0.0, "no work and no house to be had: {bleak}");
    for _ in 0..60 { w.slot += SLOTS_PER_DAY; migration::immigration(&mut w); }
    assert_eq!(w.demo.arrivals, 0);
    // Work, houses and good wages.
    w.firms[t.farm.ix()].vacancies = 2;
    for _ in 0..3 { add_home(&mut w, Owner::State(TOWN)); }
    w.stats.median_wage = 11.0;
    assert!(town_appeal(&w) > bleak + 0.9);
    w.stats.mean_wellbeing = 0.3;
    let good = town_appeal(&w);
    w.stats.mean_wellbeing = -0.5;
    assert!(town_appeal(&w) < good - 0.7, "a town of unhappy people draws fewer");
    w.stats.mean_wellbeing = 0.3;
    let (before, people) = (coin(&w), w.persons.len());
    for _ in 0..60 { w.slot += SLOTS_PER_DAY; migration::immigration(&mut w); }
    check(&w);
    assert!(w.demo.arrivals > 0, "someone came");
    assert_eq!(w.persons.len() - people, w.demo.arrivals as usize);
    assert!(w.demo.coin_in > 0.0);
    assert!((coin(&w) - before - w.demo.coin_in).abs() < 1e-2, "the only new coin is what they carried in");
    let gate = w.map.gates[0];
    for p in &w.persons[people..] {
        assert!(p.immigrant && p.present && p.loc == gate);
        assert!(!w.household(p.household).name.is_empty());
        if let (Some(m), true) = (p.mother, p.father.is_some()) { assert!(w.person(m).children.contains(&p.id)); }
    }
    assert!(w.chronicle.iter().any(|c| c.text.contains("hoping to settle")));
    // With the gates closed to settlers by the world's settings, nobody comes.
    let arrivals = w.demo.arrivals;
    w.settings.immigration = false;
    for _ in 0..60 { w.slot += SLOTS_PER_DAY; migration::immigration(&mut w); }
    assert_eq!(w.demo.arrivals, arrivals);
}

// ------------------------------------------------------------ the whole

#[test]
fn a_town_of_ninety_holds_roughly_steady_over_five_years() {
    let years = 5;
    let (mut births, mut deaths, mut first, mut last, mut weddings) = (0u32, 0u32, 0usize, 0usize, 0usize);
    let seeds = [3u64, 11, 29, 47, 83, 101, 137, 211, 307, 401, 503, 601];
    for seed in seeds {
        let (mut w, _t) = ninety(seed);
        let start = w.present().len();
        assert!((80..=100).contains(&start), "{start}");
        let before = coin(&w);
        live(&mut w, years * DAYS_PER_YEAR);
        let end = w.present().len();
        assert!((coin(&w) + w.demo.coin_out - w.demo.coin_in - before).abs() < 0.5, "coin is neither made nor lost");
        assert!(end as f32 > start as f32 * 0.8 && (end as f32) < start as f32 * 1.2, "seed {seed}: {start} became {end}");
        assert_eq!(end as i64, start as i64 + w.demo.births as i64 - w.demo.deaths as i64);
        births += w.demo.births;
        deaths += w.demo.deaths;
        first += start;
        last += end;
        weddings += w.chronicle.iter().filter(|c| c.text.ends_with("were married.")).count();
    }
    let n = seeds.len() as f32;
    let days = (years * DAYS_PER_YEAR) as f32;
    let (per_death, per_birth) = (days * n / deaths as f32, days * n / births as f32);
    println!("ninety: {} -> {} people; a death every {per_death:.1} days, a birth every {per_birth:.1}; {weddings} weddings", first as f32 / n, last as f32 / n);
    assert!((7.0..=16.0).contains(&per_death), "a death every {per_death} days");
    assert!((6.0..=18.0).contains(&per_birth), "a birth every {per_birth} days");
}

#[test]
fn births_and_deaths_answer_to_conditions() {
    let run = |medicine: f32, cost: f32, sick: bool| {
        let (mut births, mut deaths) = (0, 0);
        for seed in [5u64, 17, 23, 61] {
            let (mut w, _t) = ninety(seed);
            w.tech[Field::Medicine] = medicine;
            w.stats.cost_of_living = cost;
            for _ in 0..84 {
                if sick { for p in w.persons.iter_mut() { p.needs.health = p.needs.health.min(45.0); } }
                live(&mut w, 1);
            }
            births += w.demo.births;
            deaths += w.demo.deaths;
        }
        (births, deaths)
    };
    let (births, deaths) = run(1.0, 5.0, false);
    let (_, with_medicine) = run(2.0, 5.0, false);
    let (_, in_sickness) = run(1.0, 5.0, true);
    let (in_dearth, _) = run(1.0, 9.5, false);
    assert!(with_medicine < deaths, "better medicine, fewer deaths: {with_medicine} against {deaths}");
    assert!(in_sickness > deaths * 2, "poor health, more deaths: {in_sickness} against {deaths}");
    assert!(in_dearth * 2 < births, "when a wage barely feeds its earner, fewer children: {in_dearth} against {births}");
}

#[test]
fn the_same_seed_lives_the_same_history() {
    let run = || {
        let (mut w, t) = ninety(9);
        w.settings.immigration = true;
        w.firms[t.farm.ix()].vacancies = 1;
        for _ in 0..2 { add_home(&mut w, Owner::State(TOWN)); }
        live(&mut w, 100);
        w
    };
    let (a, b) = (run(), run());
    assert_eq!(a.persons.len(), b.persons.len());
    assert_eq!(a.present(), b.present());
    let lines = |w: &World| w.chronicle.iter().map(|c| c.text.clone()).collect::<Vec<_>>();
    assert_eq!(lines(&a), lines(&b));
    assert!(!lines(&a).is_empty());
    assert_eq!(serde_json::to_string(&a.demo).unwrap(), serde_json::to_string(&b.demo).unwrap());
    // The state survives a save and a load.
    let back: World = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(back.demo.births, a.demo.births);
    assert_eq!(back.demo.low_days, a.demo.low_days);
}

/// What the economy and daily life would do over a year, crudely: the grown
/// find work, the old stop, and everyone makes a friend of about their own age.
fn stand_in_for_work_and_friendship(w: &mut World, t: &Tiny) {
    let day = w.day();
    // Someone builds when there is no house to be had.
    if vacant_homes(w).is_empty() { add_home(w, Owner::State(TOWN)); }
    // `economy::move_in`: a household sent to an empty house takes up residence there.
    for h in 0..w.households.len() {
        if w.households[h].members.is_empty() || w.households[h].home.is_some() { continue; }
        if let Some(home) = vacant_homes(w).first().copied() {
            w.households[h].home = Some(home);
            if w.place(home).owner != Owner::Household(HouseholdId(h as u32)) { w.places[home.ix()].tenant = Some(HouseholdId(h as u32)); }
        }
    }
    for i in 0..w.persons.len() {
        if !w.persons[i].present { continue; }
        let (id, age, hh) = (w.persons[i].id, w.persons[i].age_on(day), w.persons[i].household);
        if (ADULT_AGE..ELDER_AGE).contains(&age) && w.persons[i].job.is_none() {
            w.persons[i].job = Some(Job { firm: t.farm, wage: 9.0, role: Role::Farmer, since_day: day, owner: false });
            w.persons[i].income.avg = 9.0;
            w.firms[t.farm.ix()].workers.push(id);
        }
        if age < 6.0 { continue; }
        let peers: Vec<PersonId> = w.persons.iter().filter(|b| b.present && b.household != hh && (b.age_on(day) < ADULT_AGE) == (age < ADULT_AGE) && (b.age_on(day) - age).abs() < if age < ADULT_AGE { 4.0 } else { 12.0 }).map(|b| b.id).collect();
        if peers.is_empty() || w.persons[i].friends.len() >= 6 { continue; }
        let b = peers[w.rng.index(peers.len())];
        let aff = w.rng.range(0.3, 0.9);
        if w.persons[i].affinity(b) < aff {
            let (d1, d2) = (aff - w.persons[i].affinity(b), aff - w.persons[b.ix()].affinity(id));
            w.persons[i].befriend(b, d1);
            w.persons[b.ix()].befriend(id, d2);
        }
    }
}

/// Not a pass-or-fail check: prints how a town left to itself changes over forty years.
/// `cargo test long_run -- --ignored --nocapture`
#[test]
#[ignore]
fn long_run() {
    for seed in [3u64, 11, 29, 47] {
        let (mut w, _t) = ninety(seed);
        let mut line = format!("seed {seed}: {}", w.present().len());
        for _ in 0..8 {
            for _ in 0..5 {
                live(&mut w, DAYS_PER_YEAR);
                stand_in_for_work_and_friendship(&mut w, &_t);
            }
            let day = w.day();
            let kids = w.persons.iter().filter(|p| p.present && p.age_on(day) < ADULT_AGE).count();
            let old = w.persons.iter().filter(|p| p.present && p.age_on(day) >= ELDER_AGE).count();
            line += &format!(" -> {} ({kids} young, {old} old)", w.present().len());
            let women: Vec<&Person> = w.persons.iter().filter(|p| p.present && p.sex == Sex::Female && (17.0..43.0).contains(&p.age_on(day))).collect();
            let wives = women.iter().filter(|p| p.spouse.is_some()).count();
            let wanting = women.iter().filter(|p| p.spouse.is_some_and(|s| child_worth(&w, p.id, s).is_some())).count();
            let single_men = w.persons.iter().filter(|p| p.present && p.sex == Sex::Male && p.spouse.is_none() && (16.0..50.0).contains(&p.age_on(day))).count();
            let weddings = w.chronicle.iter().filter(|c| c.text.ends_with("were married.")).count();
            line += &format!(" [{} women 17-42, {wives} wives, {wanting} wanting, {single_men} single men, {weddings} weddings, {} vacant]", women.len(), vacant_homes(&w).len());
        }
        let m = pref_means(&w);
        println!("{line}; births {} deaths {}; family {:.2}", w.demo.births, w.demo.deaths, m[Pref::Family as usize]);
    }
}
