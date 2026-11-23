//! Tests of daily life on the tiny world, driving only this module (every
//! other module is a stub here: nothing can be bought, no rule forbids anything).
use super::*;
use crate::forecast::{self, Forecast};
use crate::statute::{Statute, StatuteStatus};
use crate::testkit::{tiny_world, Tiny};

/// One slot of the day as the engine would run it, life only.
fn step(w: &mut World) {
    if w.slot_of_day() == 0 { start_of_day(w); }
    for i in 0..w.persons.len() { update_person(w, PersonId(i as u32)); }
    if w.slot_of_day() == SLOTS_PER_DAY - 1 { end_of_day(w); }
    w.slot += 1;
}
fn at_hour(hour: f32) -> (World, Tiny) {
    let (mut w, t) = tiny_world();
    w.slot = (hour * SLOTS_PER_HOUR as f32) as u32;
    for p in w.persons.iter_mut() { p.until = w.slot; }
    (w, t)
}
fn home(w: &World, p: PersonId) -> PlaceId { w.home_of(p).unwrap() }
fn hold(w: &mut World, p: PersonId, a: Activity) {
    let p = w.person_mut(p);
    p.activity = a;
    p.until = u32::MAX;
}
fn close(a: f32, b: f32) -> bool { (a - b).abs() < 1e-3 }

// ------------------------------------------------------------------- needs

#[test]
fn needs_move_as_specified_over_a_day() {
    let (mut w, t) = at_hour(6.0);
    // Greta sits idle and awake, Hugo sleeps at home, Ida works.
    hold(&mut w, t.greta, Activity::Idle);
    hold(&mut w, t.hugo, Activity::Sleep);
    hold(&mut w, t.ida, Activity::Work);
    for p in [t.greta, t.hugo, t.ida] { let n = &mut w.person_mut(p).needs; n.hunger = 0.0; n.energy = 50.0; n.social = 50.0; n.fun = 50.0; n.health = 80.0; }
    let before = w.person(t.greta).needs;
    for _ in 0..20 { for p in [t.greta, t.hugo, t.ida] { update_person(&mut w, p); } w.slot += 1; }
    let (g, h, i) = (w.person(t.greta).needs, w.person(t.hugo).needs, w.person(t.ida).needs);
    assert!(close(g.hunger, 20.0 * 1.08), "awake hunger {}", g.hunger);
    assert!(close(g.energy, before.energy - 20.0 * 0.75));
    assert!(close(g.social, before.social - 20.0 * 0.6));
    assert!(close(g.fun, before.fun - 20.0 * 0.33));
    assert!(close(g.health, 80.0 + 20.0 * 0.03), "the fed and rested mend: {}", g.health);
    assert!(close(h.hunger, 20.0 * 0.48), "asleep hunger {}", h.hunger);
    assert!(close(h.energy, 100.0), "sleep restores: {}", h.energy);
    assert!(close(i.energy, 50.0 - 20.0 * 1.08) && close(i.fun, 50.0 - 20.0 * 0.63) && close(i.social, 50.0 + 20.0 * 0.225));
    // Over the rest of the day the idler grows hungry to the limit and, starving, sickens.
    for _ in 0..96 { update_person(&mut w, t.greta); w.slot += 1; }
    let g = w.person(t.greta).needs;
    assert_eq!(g.hunger, 100.0);
    assert!(g.health < 80.0 && g.energy == 0.0);
    // A child's appetite is smaller; lessons teach.
    hold(&mut w, t.dara, Activity::School);
    let (h0, e0) = (w.person(t.dara).needs.hunger, w.person(t.dara).education);
    update_person(&mut w, t.dara);
    assert!(close(w.person(t.dara).needs.hunger, h0 + 1.08 * 0.8));
    let e1 = w.person(t.dara).education;
    assert!(e1 > e0);
    w.tech[Field::Pedagogy] = 2.0;
    update_person(&mut w, t.dara);
    assert!(((w.person(t.dara).education - e1) / (e1 - e0) - 2.0).abs() < 0.01, "better teaching teaches faster");
}

#[test]
fn sleeping_rough_rests_less_and_wears_on_health() {
    let (mut w, t) = at_hour(1.0);
    let hh = w.person(t.joss).household;
    hold(&mut w, t.greta, Activity::Sleep);
    for p in [t.greta, t.joss] { let n = &mut w.person_mut(p).needs; n.energy = 10.0; n.health = 90.0; n.hunger = 70.0; }
    w.household_mut(hh).home = None;
    hold(&mut w, t.joss, Activity::Sleep);
    for _ in 0..10 { update_person(&mut w, t.greta); update_person(&mut w, t.joss); w.slot += 1; }
    assert!(close(w.person(t.greta).needs.energy, 10.0 + 31.5));
    assert!(close(w.person(t.joss).needs.energy, 10.0 + 22.5));
    assert!(w.person(t.joss).needs.health < w.person(t.greta).needs.health);
}

// ---------------------------------------------------------------- movement

#[test]
fn a_traveller_arrives_after_the_right_number_of_slots_and_starts_the_plan() {
    let (mut w, t) = at_hour(10.0);
    let from = w.person(t.greta).loc;
    let n = w.map.travel_n;
    w.map.travel[from.ix() * n + t.market.ix()] = 3;
    let start = w.slot;
    go(&mut w, t.greta, t.market, Plan { activity: Activity::Stroll, slots: 4, doing: "Looking at the stalls".into(), arg: 0 });
    {
        let p = w.person(t.greta);
        let tr = p.travel.expect("walking");
        assert_eq!((tr.from, tr.to, tr.depart, tr.arrive), (from, t.market, start, start + 3));
        assert_eq!(p.activity, Activity::Travel);
        assert_eq!(p.loc, w.street_of(w.place(t.market).ward), "on the street of the ward they are bound for");
        assert!(p.doing.starts_with("Walking to"));
        assert_eq!(p.next.as_ref().unwrap().activity, Activity::Stroll);
    }
    for k in 1..3 {
        w.slot = start + k;
        update_person(&mut w, t.greta);
        assert!(w.person(t.greta).travel.is_some() && w.person(t.greta).activity == Activity::Travel, "still walking at +{k}");
    }
    w.slot = start + 3;
    update_person(&mut w, t.greta);
    let p = w.person(t.greta);
    assert!(p.travel.is_none() && p.next.is_none());
    assert_eq!((p.loc, p.activity, p.until), (t.market, Activity::Stroll, start + 3 + 4));
    assert_eq!(p.doing, "Looking at the stalls");
    // Already there: the plan starts at once.
    go(&mut w, t.greta, t.market, Plan { activity: Activity::Idle, slots: 2, doing: "Waiting".into(), arg: 0 });
    let p = w.person(t.greta);
    assert!(p.travel.is_none());
    assert_eq!((p.activity, p.until), (Activity::Idle, start + 3 + 2));
}

#[test]
fn someone_leaving_walks_to_a_gate() {
    let (mut w, t) = at_hour(10.0);
    w.person_mut(t.greta).activity = Activity::Leaving;
    update_person(&mut w, t.greta);
    let gate = w.map.gates[0];
    assert_eq!(w.person(t.greta).travel.map(|x| x.to), Some(gate));
    w.slot += 1;
    update_person(&mut w, t.greta);
    assert_eq!((w.person(t.greta).loc, w.person(t.greta).activity), (gate, Activity::Leaving));
}

#[test]
fn the_jailed_stay_put_until_released() {
    let (mut w, t) = at_hour(10.0);
    let until = w.slot + 12;
    { let p = w.person_mut(t.finn); p.loc = t.watchhouse; p.jailed_until = Some(until); p.needs.hunger = 75.0; }
    for _ in 0..12 {
        update_person(&mut w, t.finn);
        let p = w.person(t.finn);
        assert_eq!((p.loc, p.activity), (t.watchhouse, Activity::Jailed));
        assert!(p.until > w.slot && p.needs.hunger <= 76.1, "prisoners are fed a ration");
        w.slot += 1;
    }
    update_person(&mut w, t.finn);
    let p = w.person(t.finn);
    assert!(p.jailed_until.is_none() && p.activity != Activity::Jailed);
    assert!(p.until > w.slot || p.travel.is_some());
}

// ----------------------------------------------------------------- planner

#[test]
fn a_hungry_person_with_food_at_home_goes_home_and_eats() {
    let (mut w, t) = at_hour(10.0);
    let hh = w.person(t.hugo).household;
    { let p = w.person_mut(t.hugo); p.loc = t.plaza; p.needs.hunger = 80.0; }
    let food = w.household(hh).stock[Good::Food as usize];
    update_person(&mut w, t.hugo);
    assert_eq!(w.person(t.hugo).travel.map(|x| x.to), Some(home(&w, t.hugo)), "sets out for home");
    w.slot += 1;
    update_person(&mut w, t.hugo);
    let p = w.person(t.hugo);
    assert_eq!((p.loc, p.activity), (home(&w, t.hugo), Activity::Eat));
    assert!(p.needs.hunger < 35.0, "the meal took the edge off: {}", p.needs.hunger);
    assert!(close(w.household(hh).stock[Good::Food as usize], food - 0.5));
    assert_eq!(p.mind.meals, 1);
    assert!(p.doing.contains("at home"));
}

#[test]
fn with_an_empty_larder_and_no_market_the_meal_is_missed_and_noted() {
    let (mut w, t) = at_hour(21.0);
    let hh = w.person(t.greta).household;
    w.household_mut(hh).stock[Good::Food as usize] = 0.0;
    w.person_mut(t.greta).needs.hunger = 85.0;
    update_person(&mut w, t.greta);
    let p = w.person(t.greta);
    assert_eq!(p.mind.missed, 1);
    assert!(p.diary.iter().any(|d| d.kind == "hardship" && d.text.contains("supper")), "{:?}", p.diary);
    assert!(p.until > w.slot);
}

#[test]
fn a_worker_within_shift_hours_goes_to_work_and_adds_effort() {
    let (mut w, t) = at_hour(8.0);
    let place = w.firm(t.workshop).place;
    update_person(&mut w, t.hugo);
    assert_eq!(w.person(t.hugo).travel.map(|x| x.to), Some(place));
    assert!(w.person(t.hugo).mind.lunch > 0.0, "packed something to eat");
    w.slot += 1;
    update_person(&mut w, t.hugo);
    let p = w.person(t.hugo);
    assert_eq!((p.loc, p.activity), (place, Activity::Work));
    assert!(p.work.effort > 0.0 && p.doing.contains("South Workshop"));
    assert!(p.diary.iter().any(|d| d.kind == "work"));
    let (e0, s0) = (w.firm(t.workshop).effort_now, w.person(t.hugo).work.slots_today);
    for _ in 0..4 { w.slot += 1; update_person(&mut w, t.hugo); }
    assert!(close(w.firm(t.workshop).effort_now - e0, 4.0 * w.person(t.hugo).work.effort));
    assert_eq!(w.person(t.hugo).work.slots_today, s0 + 4);
    assert!(w.person(t.hugo).until <= 12 * SLOTS_PER_HOUR, "the morning's work stops for dinner");
    // Nobody works a farm on the rest day.
    let (mut w, t) = at_hour(8.0);
    w.slot += REST_DAY * SLOTS_PER_DAY;
    for p in w.persons.iter_mut() { p.until = w.slot; }
    update_person(&mut w, t.bram);
    assert!(w.person(t.bram).travel.map(|x| x.to) != Some(w.firm(t.farm).place) && w.person(t.bram).activity != Activity::Work);
    assert!(!operates_today(FirmKind::Farm, REST_DAY) && operates_today(FirmKind::Tavern, REST_DAY));
}

#[test]
fn past_bedtime_people_sleep_at_home_and_the_homeless_sleep_rough() {
    let (mut w, t) = at_hour(23.5);
    let hh = w.person(t.joss).household;
    w.household_mut(hh).home = None;
    w.person_mut(t.joss).loc = t.market;
    w.person_mut(t.greta).loc = t.plaza;
    for _ in 0..3 { for p in [t.bram, t.greta, t.joss] { update_person(&mut w, p); } w.slot += 1; }
    let b = w.person(t.bram);
    assert_eq!((b.activity, b.loc), (Activity::Sleep, home(&w, t.bram)));
    assert!(b.until > w.slot + 16, "asleep until morning");
    let g = w.person(t.greta);
    assert_eq!((g.activity, g.loc), (Activity::Sleep, home(&w, t.greta)), "walked home to bed");
    let j = w.person(t.joss);
    assert_eq!(j.activity, Activity::Sleep);
    assert!(w.place(j.loc).kind.outdoor() && j.doing.contains("rough"), "{} at {:?}", j.doing, w.place(j.loc).kind);
}

#[test]
fn parents_decide_each_morning_whether_a_child_goes_to_school() {
    let (mut w, t) = at_hour(0.0);
    let id = PlaceId(w.places.len() as u32);
    let mut pl = w.place(t.townhall).clone();
    pl.id = id;
    pl.kind = PlaceKind::School;
    pl.name = "Test School".into();
    pl.owner = Owner::None;
    w.places.push(pl);
    let fid = FirmId(w.firms.len() as u32);
    let mut f = w.firm(t.tavern_firm).clone();
    f.id = fid;
    f.kind = FirmKind::School;
    f.name = "Test School".into();
    f.place = id;
    f.workers.clear();
    f.manager = None;
    f.price = 0.8;
    f.sell_price = 0.8;
    w.firms.push(f);
    w.places[id.ix()].firm = Some(fid);
    start_of_day(&mut w);
    assert!(w.person(t.cai).enrolled && w.person(t.dara).enrolled, "a household with money and schooled parents sends its children");
    assert!(!w.person(t.ada).enrolled);
    // The same parents with an empty purse do not.
    let hh = w.person(t.cai).household;
    w.household_mut(hh).money = 1.0;
    start_of_day(&mut w);
    assert!(!w.person(t.cai).enrolled && !w.person(t.dara).enrolled);
    // Nor do parents who care nothing for family or the future.
    w.household_mut(hh).money = 220.0;
    for p in [t.ada, t.bram] { let pr = &mut w.person_mut(p).prefs; pr[Pref::Family] = 0.0; pr[Pref::Patience] = 0.0; }
    start_of_day(&mut w);
    assert!(!w.person(t.cai).enrolled);
}

#[test]
fn time_together_builds_friendship_slowly() {
    let (mut w, t) = at_hour(10.0);
    let before = w.person(t.greta).affinity(t.ida);
    let ida_home = home(&w, t.ida);
    hold(&mut w, t.ida, Activity::Home);
    go(&mut w, t.greta, ida_home, Plan { activity: Activity::Visit, slots: 4, doing: String::new(), arg: t.ida.0 as i64 });
    w.slot += 1;
    update_person(&mut w, t.greta);
    let after = w.person(t.greta).affinity(t.ida);
    assert!(after > before && after < before + 0.05);
    assert!(w.person(t.ida).affinity(t.greta) > before);
    assert_eq!(w.person(t.ida).activity, Activity::Host);
    assert_eq!(w.person(t.ida).until, w.person(t.greta).until);
    assert!(w.person(t.greta).diary.iter().any(|d| d.text.contains("Ida")));
}

// --------------------------------------------------------------- wellbeing

/// The change in total wellbeing `change` makes for `p` when `p` holds `pref` at `level`.
fn swing(pref: Pref, level: f32, who: fn(&Tiny) -> PersonId, change: fn(&mut World, &Tiny)) -> f32 {
    let (mut w, t) = at_hour(10.0);
    let p = who(&t);
    w.person_mut(p).prefs[pref] = level;
    let before = wellbeing_now(&w, p).total;
    change(&mut w, &t);
    wellbeing_now(&w, p).total - before
}

#[test]
fn wellbeing_rises_with_food_money_and_friends_in_proportion_to_the_preference() {
    fn greta(t: &Tiny) -> PersonId { t.greta }
    fn edith(t: &Tiny) -> PersonId { t.edith }
    fn fed(w: &mut World, t: &Tiny) {
        w.person_mut(t.greta).needs.hunger = 5.0;
        let h = w.person(t.greta).household;
        w.household_mut(h).stock[Good::Food as usize] = 12.0;
    }
    fn rich(w: &mut World, t: &Tiny) { let h = w.person(t.edith).household; w.household_mut(h).money += 300.0; }
    fn glad_friend(w: &mut World, t: &Tiny) { w.person_mut(t.ida).wellbeing.own = 0.9; }
    let cases: [(Pref, fn(&Tiny) -> PersonId, fn(&mut World, &Tiny)); 3] = [(Pref::Comfort, greta, fed), (Pref::Wealth, edith, rich), (Pref::Community, greta, glad_friend)];
    for (pref, who, change) in cases {
        let (none, some, much) = (swing(pref, 0.0, who, change), swing(pref, 0.3, who, change), swing(pref, 1.0, who, change));
        assert!(none.abs() < 1e-6, "{pref:?}: no weight, no difference ({none})");
        assert!(some > 0.0 && much > some * 2.0, "{pref:?}: {some} then {much}");
    }
    // Components stay within bounds, and `own` leaves out family and community.
    let (mut w, t) = at_hour(10.0);
    for p in w.persons.iter_mut() { p.wellbeing.own = -1.0; }
    let a = wellbeing_now(&w, t.ada);
    assert!(a.parts.iter().all(|x| (-1.0..=1.0).contains(x)), "{:?}", a.parts);
    assert!(a.parts[Pref::Family as usize] < -0.9 && a.parts[Pref::Community as usize] < -0.9);
    for p in w.persons.iter_mut() { p.wellbeing.own = 1.0; }
    let b = wellbeing_now(&w, t.ada);
    assert!(close(a.own, b.own), "own is self-regarding");
    assert!(b.total > a.total);
}

#[test]
fn only_those_who_care_for_the_community_feel_the_neighbours_misery() {
    let measure = |community: f32| {
        let (mut w, t) = at_hour(10.0);
        w.person_mut(t.greta).prefs[Pref::Community] = community;
        let before = wellbeing_now(&w, t.greta).total;
        for p in w.persons.iter_mut() { if p.id != t.greta { p.wellbeing.own = -0.9; } }
        wellbeing_now(&w, t.greta).total - before
    };
    assert!(measure(0.0).abs() < 1e-6, "indifferent");
    assert!(measure(1.0) < -0.05, "not indifferent: {}", measure(1.0));
}

#[test]
fn being_constrained_lowers_autonomy_more_for_those_who_prize_it() {
    fn hugo(t: &Tiny) -> PersonId { t.hugo }
    fn forbidden(w: &mut World, t: &Tiny) { w.person_mut(t.hugo).felt.constrained = 9.0; }
    let (low, high) = (swing(Pref::Autonomy, 0.2, hugo, forbidden), swing(Pref::Autonomy, 1.0, hugo, forbidden));
    assert!(low < 0.0 && high < low * 2.0, "{low} {high}");
    assert!(swing(Pref::Autonomy, 0.0, hugo, forbidden).abs() < 1e-6);
    let (mut w, t) = at_hour(10.0);
    let free = wellbeing_now(&w, t.hugo).parts[Pref::Autonomy as usize];
    w.person_mut(t.hugo).felt.watched = 3.0;
    let watched = wellbeing_now(&w, t.hugo).parts[Pref::Autonomy as usize];
    forbidden(&mut w, &t);
    let both = wellbeing_now(&w, t.hugo).parts[Pref::Autonomy as usize];
    assert!(free > watched && watched > both && both >= -1.0);
}

#[test]
fn the_night_folds_the_day_into_wellbeing_and_clears_what_was_felt() {
    let (mut w, t) = at_hour(23.75);
    end_of_day(&mut w);
    let first = w.person(t.hugo).wellbeing;
    assert!(w.person(t.hugo).mind.seen && close(w.person(t.hugo).mind.base, first.total));
    // A day of being punished and robbed shows, but only in part: components settle over about a week.
    w.slot += SLOTS_PER_DAY;
    { let f = &mut w.person_mut(t.hugo).felt; f.punished = 30.0; f.victimized = 20.0; f.constrained = 12.0; }
    let raw = wellbeing_now(&w, t.hugo);
    end_of_day(&mut w);
    let p = w.person(t.hugo);
    let (sec, aut) = (Pref::Security as usize, Pref::Autonomy as usize);
    assert!(p.wellbeing.parts[sec] < first.parts[sec] && p.wellbeing.parts[sec] > raw.parts[sec]);
    assert!(p.wellbeing.parts[aut] < first.parts[aut] && p.wellbeing.parts[aut] > raw.parts[aut]);
    assert!(p.wellbeing.total < first.total);
    assert_eq!((p.felt.punished, p.felt.victimized, p.felt.constrained), (0.0, 0.0, 0.0));
    assert_eq!((p.mind.ticks, p.mind.toil), (0, 0));
    // The household feels it too, a night later, through family.
    let ida_before = w.person(t.ida).wellbeing.parts[Pref::Family as usize];
    w.slot += SLOTS_PER_DAY;
    end_of_day(&mut w);
    assert!(w.person(t.ida).wellbeing.parts[Pref::Family as usize] < ida_before);
}

#[test]
fn trust_follows_fortune_and_falls_hardest_under_rejected_rules() {
    let run = |opinion: f32| {
        let (mut w, t) = at_hour(23.75);
        let mut s = crate::testkit::ban(crate::statute::Action::Consume, "good == \"ale\"", 5.0);
        s.id = StatuteId(1);
        s.status = StatuteStatus::Active;
        s.enacted_day = Some(0);
        s.passed_by = "founding charter".into();
        w.statutes.push(s);
        end_of_day(&mut w);
        w.slot += SLOTS_PER_DAY;
        let calm = w.person(t.finn).trust;
        w.person_mut(t.hugo).set_opinion(StatuteId(1), opinion);
        w.person_mut(t.hugo).felt.punished = 20.0;
        w.statutes[0].stats.punished = 1;
        end_of_day(&mut w);
        (w.person(t.hugo).trust, w.person(t.finn).trust, calm)
    };
    let (rejecting, bystander, calm) = run(-0.9);
    let (accepting, _, _) = run(0.9);
    assert!(rejecting < accepting, "{rejecting} {accepting}");
    assert!(accepting < bystander, "any punishment costs some trust");
    assert!((bystander - calm).abs() < 0.02, "a quiet day leaves trust about where it was");
    // Faring worse than they used to wears trust down; faring better builds it.
    let fare = |hunger: f32, money: f32| {
        let (mut w, t) = at_hour(23.75);
        end_of_day(&mut w);
        let hh = w.person(t.greta).household;
        w.household_mut(hh).money = money;
        for _ in 0..6 {
            w.slot += SLOTS_PER_DAY;
            { let p = w.person_mut(t.greta); p.needs.hunger = hunger; p.mind.ticks = 0; }
            end_of_day(&mut w);
        }
        w.person(t.greta).trust
    };
    let (worse, steady, better) = (fare(95.0, 5.0), fare(25.0, 180.0), fare(5.0, 2000.0));
    assert!(worse < steady - 0.02 && better > steady + 0.01, "{worse} {steady} {better}");
}

// ---------------------------------------------------------------- opinions

#[test]
fn opinions_follow_the_sign_of_a_stored_forecast() {
    let (mut w, t) = at_hour(10.0);
    w.settings.forecast_noise = 0.0;
    let sid = StatuteId(9);
    let mut delta = vec![0.0; w.persons.len()];
    delta[t.ada.ix()] = 0.08;
    delta[t.bram.ix()] = -0.08;
    delta[t.cai.ix()] = 0.5;
    delta[t.greta.ix()] = 0.3;
    forecast::store(&mut w, sid, Forecast { delta, ..Default::default() });
    form_opinions(&mut w, sid);
    let (a, b, g) = (w.person(t.ada).opinion(sid).unwrap(), w.person(t.bram).opinion(sid).unwrap(), w.person(t.greta).opinion(sid).unwrap());
    assert!(a > 0.05 && b < -0.05 && g > a && g <= 1.0, "{a} {b} {g}");
    assert_eq!(w.person(t.hugo).opinion(sid), Some(0.0), "unaffected, unmoved");
    assert!(w.person(t.cai).opinion(sid).is_none(), "children hold no view of the law");
    // The patient weigh a future gain more heavily.
    w.person_mut(t.ada).prefs[Pref::Patience] = 1.0;
    w.person_mut(t.bram).prefs[Pref::Patience] = 0.0;
    form_opinions(&mut w, sid);
    assert!(w.person(t.ada).opinion(sid).unwrap() > -w.person(t.bram).opinion(sid).unwrap());
    // With nothing to go on, no view is invented.
    form_opinions(&mut w, StatuteId(77));
    assert!(w.person(t.ada).opinion(StatuteId(77)).is_none());
}

#[test]
fn opinions_of_statutes_in_force_drift_toward_lived_experience() {
    let (mut w, t) = at_hour(23.75);
    w.settings.forecast_noise = 0.0;
    let mut s = Statute::draft("The Dry Law", vec![]);
    s.id = StatuteId(3);
    s.status = StatuteStatus::Active;
    s.enacted_day = Some(0);
    s.passed_by = "decree".into();
    w.statutes.push(s);
    for p in [t.hugo, t.finn, t.greta] { w.person_mut(p).set_opinion(StatuteId(3), 0.6); }
    w.person_mut(t.ada).set_opinion(StatuteId(42), 0.5);
    end_of_day(&mut w);
    assert!(w.person(t.ada).opinion(StatuteId(42)).is_none(), "views of what no longer exists are dropped");
    assert!(w.person(t.ada).opinion(StatuteId(3)).is_some(), "everyone grown comes to a view of what is in force");
    assert!(w.person(t.cai).opinion(StatuteId(3)).is_none());
    // Hugo is stopped by it day after day; Greta's life gets better under it; Finn's does not change.
    for _ in 0..8 {
        w.slot += SLOTS_PER_DAY;
        moves::resent(&mut w, t.hugo, StatuteId(3), 6.0);
        let hh = w.person(t.greta).household;
        w.household_mut(hh).money += 400.0;
        end_of_day(&mut w);
    }
    let (h, f, g) = (w.person(t.hugo).opinion(StatuteId(3)).unwrap(), w.person(t.finn).opinion(StatuteId(3)).unwrap(), w.person(t.greta).opinion(StatuteId(3)).unwrap());
    assert!(h < 0.0, "what it cost him turned him against it: {h}");
    assert!(g > f && f > h, "{g} {f} {h}");
    let mean = { let ids = w.present_adults(); ids.iter().map(|p| w.person(*p).opinion(StatuteId(3)).unwrap()).sum::<f32>() / ids.len() as f32 };
    assert!(close(w.statutes[0].stats.approval, mean));
    // Repealed: the view goes with it.
    w.statutes[0].status = StatuteStatus::Repealed;
    w.slot += SLOTS_PER_DAY;
    end_of_day(&mut w);
    assert!(w.person(t.hugo).opinion(StatuteId(3)).is_none() && w.person(t.hugo).mind.lived.is_empty());
}

// ------------------------------------------------------------- whole days

#[test]
fn over_three_days_nobody_is_left_with_nothing_to_do() {
    let (mut w, t) = tiny_world();
    let money: f32 = w.households.iter().map(|h| h.money).sum();
    let mut seen = std::collections::BTreeSet::new();
    let mut worked = 0;
    for _ in 0..3 * SLOTS_PER_DAY {
        step(&mut w);
        let now = w.slot - 1;
        for p in &w.persons {
            assert!(p.travel.is_some() || p.until > now, "{} has nothing to do at slot {now}: {:?} until {}", p.name, p.activity, p.until);
            assert!(!p.doing.is_empty(), "{} is doing something unnamed ({:?})", p.name, p.activity);
            if let Some(tr) = p.travel { assert!(tr.arrive > now && p.activity == Activity::Travel && p.next.is_some()); }
            assert!((0.0..=100.0).contains(&p.needs.hunger) && (0.0..=100.0).contains(&p.needs.energy));
            seen.insert(format!("{:?}", p.activity));
        }
        if w.person(t.hugo).activity == Activity::Work { worked += 1; }
    }
    for a in ["Sleep", "Eat", "Work", "Travel", "Home"] { assert!(seen.contains(a), "nobody ever did {a}: {seen:?}"); }
    let after: f32 = w.households.iter().map(|h| h.money).sum();
    assert!((money - after).abs() < 0.01, "life neither makes nor destroys coin: {money} -> {after}");
    for p in &w.persons {
        assert!(p.wellbeing.parts.iter().all(|x| x.is_finite() && (-1.0..=1.0).contains(x)), "{}: {:?}", p.name, p.wellbeing.parts);
        assert!((0.0..=1.0).contains(&p.trust) && p.mind.seen);
    }
    // A worker puts in most of a working day on working days.
    assert!(worked >= 3 * 28, "Hugo worked {worked} slots in three days");
    assert_eq!(w.person(t.hugo).work.slots_today, worked);
    assert!(w.person(t.hugo).diary.iter().any(|d| d.kind == "work"));
    assert!(!w.person(t.hugo).thought.is_empty());
}

#[test]
fn the_same_seed_lives_the_same_days() {
    let run = || {
        let (mut w, _) = tiny_world();
        for _ in 0..2 * SLOTS_PER_DAY { step(&mut w); }
        serde_json::to_string(&w.persons).unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn inside_a_forecast_nothing_is_written_down() {
    let (mut w, _t) = tiny_world();
    w.depth = 1;
    for _ in 0..2 * SLOTS_PER_DAY { step(&mut w); }
    for p in &w.persons {
        assert!(p.diary.is_empty() && p.thought.is_empty() && p.doing.is_empty(), "{}: {:?} / {:?} / {:?}", p.name, p.diary, p.thought, p.doing);
        assert!(p.travel.is_some() || p.until >= w.slot);
    }
    // ...and the days themselves go the same way as in the real world.
    let (mut real, _) = tiny_world();
    for _ in 0..2 * SLOTS_PER_DAY { step(&mut real); }
    for (a, b) in w.persons.iter().zip(real.persons.iter()) {
        assert_eq!((a.loc, a.activity, a.until), (b.loc, b.activity, b.until), "{}", a.name);
        assert!(close(a.needs.hunger, b.needs.hunger) && close(a.wellbeing.total, b.wellbeing.total));
    }
}

#[test]
fn the_player_living_as_someone_chooses_their_protest_and_their_leaving() {
    let (mut w, t) = at_hour(10.0);
    w.polity.player.possessing = Some(t.hugo);
    w.polity.player.overrides.protest = Some(true);
    update_person(&mut w, t.hugo);
    assert_eq!(w.person(t.hugo).next.as_ref().map(|n| n.activity), Some(Activity::Protest));
    w.slot += 1;
    update_person(&mut w, t.hugo);
    assert!(w.person(t.hugo).civic.protesting && w.person(t.hugo).loc == t.plaza);
    // Left to himself (the stub government never calls anyone out), another man goes to work instead.
    update_person(&mut w, t.finn);
    assert_ne!(w.person(t.finn).next.as_ref().map(|n| n.activity), Some(Activity::Protest));
    // Told to leave, he makes for the gate; the protest flag does not outlive the protest.
    w.polity.player.overrides.emigrate = Some(true);
    w.slot = w.person(t.hugo).until;
    update_person(&mut w, t.hugo);
    let p = w.person(t.hugo);
    assert!(!p.civic.protesting);
    assert_eq!(p.travel.map(|x| x.to), Some(w.map.gates[0]));
    assert_eq!(p.next.as_ref().map(|n| n.activity), Some(Activity::Leaving));
}

#[test]
fn small_children_are_fed_at_home_and_enforcers_stand_their_post_only_when_posted() {
    let (mut w, t) = at_hour(10.0);
    // Dara, made a toddler, is fed from the larder without leaving the house.
    w.person_mut(t.dara).birth_day = w.day() - 3 * DAYS_PER_YEAR as i32;
    w.person_mut(t.dara).needs.hunger = 70.0;
    let hh = w.person(t.dara).household;
    let food = w.household(hh).stock[Good::Food as usize];
    update_person(&mut w, t.dara);
    let d = w.person(t.dara);
    assert_eq!((d.activity, d.loc), (Activity::Eat, home(&w, t.dara)));
    assert!(d.needs.hunger < 30.0 && w.household(hh).stock[Good::Food as usize] < food);
    // An enforcer with no posting (the stub watch posts nobody) is off duty and does not count as working.
    w.person_mut(t.finn).job.as_mut().unwrap().role = Role::Enforcer;
    update_person(&mut w, t.finn);
    let f = w.person(t.finn);
    assert!(f.activity != Activity::Patrol && f.activity != Activity::Work && f.next.as_ref().map(|n| n.activity) != Some(Activity::Work));
    // Someone walked to a post by another module is on patrol there, and that counts as time worked.
    go(&mut w, t.finn, t.plaza, Plan { activity: Activity::Patrol, slots: 3, doing: "On watch".into(), arg: 0 });
    let before = w.person(t.finn).work.slots_today;
    for _ in 0..3 { w.slot += 1; update_person(&mut w, t.finn); }
    assert_eq!(w.person(t.finn).work.slots_today, before + 3);
}
