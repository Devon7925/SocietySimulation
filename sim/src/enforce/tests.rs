//! Tests of the enforcement chain on the tiny world. The other modules are
//! stubs here, so each test builds what it needs by hand: a Watch firm, its
//! staff, an active statute, and people standing where the test wants them.
use super::*;
use crate::decide;
use crate::polity::*;
use crate::rng::Rng;
use crate::testkit::*;

fn enact(w: &mut World, mut s: Statute) -> StatuteId {
    let id = StatuteId(w.statutes.len() as u32 + 1);
    s.id = id;
    s.jur = TOWN;
    s.status = StatuteStatus::Active;
    s.enacted_day = Some(w.day());
    s.passed_by = "decree".into();
    w.statutes.push(s);
    id
}

fn unit_def(json: &str) -> UnitDef { serde_json::from_str(json).unwrap() }

fn watch_firm(w: &mut World, t: &Tiny) -> FirmId {
    let id = FirmId(w.firms.len() as u32);
    w.firms.push(Firm {
        id, kind: FirmKind::Watch, name: "Watch".into(), place: t.watchhouse, owner: Owner::State(TOWN), jur: TOWN, manager: None, workers: Vec::new(), open: true, suspended: false, illicit: None,
        cash: 0.0, stock: 0.0, input: 0.0, price: 0.0, sell_price: 0.0, wage_offer: 10.0, vacancies: 0, capacity: 3,
        today: Ledger::default(), yesterday: Ledger::default(), avg_sales: 0.0, avg_profit: 0.0, effort_now: 0.0, econ: Default::default(),
    });
    id
}

fn employ_at(w: &mut World, p: PersonId, firm: FirmId, wage: f32, role: Role) {
    if let Some(j) = w.person(p).job.clone() { w.firm_mut(j.firm).workers.retain(|x| *x != p); }
    let day = w.day();
    w.person_mut(p).job = Some(Job { firm, wage, role, since_day: day, owner: false });
    w.firm_mut(firm).workers.push(p);
}

fn patrol(w: &mut World, p: PersonId, place: PlaceId) {
    let q = w.person_mut(p);
    q.loc = place;
    q.activity = Activity::Patrol;
    q.travel = None;
}

fn stand(w: &mut World, p: PersonId, place: PlaceId) {
    let q = w.person_mut(p);
    q.loc = place;
    q.activity = Activity::Idle;
    q.travel = None;
}

fn prefs(w: &mut World, p: PersonId, set: &[(Pref, f32)]) { for (k, v) in set { w.person_mut(p).prefs[*k] = *v; } }

fn offence(w: &World, offender: PersonId, sid: StatuteId, place: PlaceId, trail: Trail) -> Offence {
    let Clause::Conduct(c) = &w.statute(sid).unwrap().clauses[0] else { panic!("not a conduct clause") };
    let fine: f32 = c.sanction.fine.parse().unwrap_or(0.0);
    Offence { offender, statute: sid, clause: 0, place, trail, gain: 3.0, stake: 3.0, what: "buying ale".into(), sanction: c.sanction.clone(), fine, victim: None }
}

fn treasury(w: &World) -> f32 { w.polity.jur(TOWN).treasury }
fn purse(w: &World, p: PersonId) -> f32 { w.hh_of(p).money }
fn stats(w: &World, sid: StatuteId) -> &StatuteStats { &w.statute(sid).unwrap().stats }

/// The tiny world with a ban on buying ale (fine as given), a town watch backed
/// by a hand-built firm, and Finn serving in it at `wage`.
fn town(fine: f32, unit: &str, wage: f32) -> (World, Tiny, StatuteId, FirmId) {
    let (mut w, t) = tiny_world();
    let sid = enact(&mut w, ban(Action::Buy, "good == \"ale\"", fine));
    let firm = watch_firm(&mut w, &t);
    apply_unit_with_firm(&mut w, TOWN, &unit_def(unit), None, Some(firm));
    employ_at(&mut w, t.finn, firm, wage, Role::Enforcer);
    // Finn has no particular feeling for strangers, so tests turn on what they set.
    prefs(&mut w, t.finn, &[(Pref::Community, 0.0)]);
    (w, t, sid, firm)
}

const PLAIN: &str = r#"{"name":"Watch","staff":3}"#;

#[test]
fn nobody_to_enforce_and_nobody_to_tell() {
    // No watch at all.
    let (mut w, t) = tiny_world();
    let sid = enact(&mut w, ban(Action::Buy, "good == \"ale\"", 10.0));
    stand(&mut w, t.greta, t.market);
    stand(&mut w, t.ida, t.market);
    w.person_mut(t.ida).set_opinion(sid, -1.0);
    assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) < 0.02);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    assert!(!report_offence(&mut w, off));
    assert_eq!(w.person(t.greta).convictions, 0);
    assert_eq!(purse(&w, t.greta), 180.0);

    // A watch exists but is elsewhere, and the only witness is a friend who thinks the rule wrong.
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    patrol(&mut w, t.finn, t.plaza);
    stand(&mut w, t.greta, t.market);
    stand(&mut w, t.ida, t.market);
    w.person_mut(t.ida).set_opinion(sid, -1.0);
    assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) < 0.02);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    assert!(!report_offence(&mut w, off));
    for _ in 0..8 { w.slot += 1; on_slot(&mut w); }
    assert_eq!(w.person(t.greta).convictions, 0);
    assert_eq!(stats(&w, sid).punished, 0);
    assert_eq!(stats(&w, sid).reported, 0);
    assert!(w.enforce.pending.is_empty());
    assert_eq!(treasury(&w), 200.0);
}

#[test]
fn a_willing_witness_brings_the_watch() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    patrol(&mut w, t.finn, t.plaza);
    stand(&mut w, t.greta, t.market);
    stand(&mut w, t.joss, t.market);
    w.person_mut(t.joss).set_opinion(sid, 1.0);
    w.person_mut(t.joss).trust = 0.9;
    prefs(&mut w, t.joss, &[(Pref::Conformity, 0.9)]);
    assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) > 0.5);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    // Not caught in the act: the act goes ahead, the reckoning comes after.
    assert!(!report_offence(&mut w, off));
    assert_eq!(stats(&w, sid).reported, 1);
    assert_eq!(w.enforce.pending.len(), 1);
    w.slot += 1;
    on_slot(&mut w);
    assert!(w.enforce.pending.is_empty());
    assert_eq!(w.person(t.greta).convictions, 1);
    assert_eq!(purse(&w, t.greta), 170.0);
    assert_eq!(treasury(&w), 210.0);
    // Witnesses who approve of the rule felt the disorder of seeing it broken.
    assert!(w.person(t.joss).felt.disorder > 0.0);
}

#[test]
fn an_enforcer_on_the_spot_who_approves_stops_and_fines() {
    for seed in 0..10 {
        let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
        w.rng = Rng::new(seed);
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.greta, t.market);
        assert!(on_duty_at(&w, t.market).contains(&t.finn));
        assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) > 0.8);
        let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
        assert!(report_offence(&mut w, off), "the offender is stopped in the act");
        assert_eq!(w.person(t.greta).convictions, 1);
        assert_eq!(purse(&w, t.greta), 170.0);
        assert_eq!(treasury(&w), 210.0, "the fine reaches the treasury");
        assert_eq!(stats(&w, sid).punished, 1);
        assert_eq!(stats(&w, sid).fines, 10.0);
        assert!(w.person(t.greta).felt.punished >= 10.0);
        assert_eq!(w.person(t.finn).legal.cases_week, 1);
        assert!(w.person(t.greta).diary.iter().any(|d| d.text.contains("fined 10.0 coins")));
    }
}

#[test]
fn an_enforcer_who_despises_the_statute_lets_it_pass() {
    for seed in 0..10 {
        let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
        w.rng = Rng::new(seed);
        w.person_mut(t.finn).set_opinion(sid, -1.0);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.greta, t.market);
        assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) < 0.02);
        let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
        assert!(!report_offence(&mut w, off));
        assert_eq!(stats(&w, sid).ignored, 1);
        assert_eq!(stats(&w, sid).punished, 0);
        assert_eq!(w.person(t.greta).convictions, 0);
        assert_eq!(treasury(&w), 200.0);
    }
}

#[test]
fn an_enforcer_lets_close_kin_pass_but_not_a_stranger() {
    for seed in 0..10 {
        let (mut w, t, sid, firm) = town(10.0, PLAIN, 10.0);
        w.rng = Rng::new(seed);
        employ_at(&mut w, t.bram, firm, 10.0, Role::Enforcer);
        prefs(&mut w, t.bram, &[(Pref::Community, 0.0)]);
        w.person_mut(t.bram).set_opinion(sid, 0.6);
        patrol(&mut w, t.bram, t.market);
        // His wife.
        stand(&mut w, t.ada, t.market);
        let off = offence(&w, t.ada, sid, t.market, Trail::Seen);
        assert!(!report_offence(&mut w, off));
        assert_eq!(stats(&w, sid).ignored, 1);
        assert_eq!(w.person(t.ada).convictions, 0);
        // Someone he does not know.
        stand(&mut w, t.greta, t.market);
        let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
        assert!(report_offence(&mut w, off));
        assert_eq!(w.person(t.greta).convictions, 1);
    }
}

#[test]
fn a_bribe_is_taken_by_the_underpaid_and_unscrupulous() {
    for seed in 0..10 {
        let (mut w, t, sid, _) = town(40.0, PLAIN, 3.0);
        w.rng = Rng::new(seed);
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        prefs(&mut w, t.finn, &[(Pref::Wealth, 1.0), (Pref::Conformity, 0.05), (Pref::Fairness, 0.05)]);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.ada, t.market);
        let (ada0, finn0) = (purse(&w, t.ada), purse(&w, t.finn));
        let off = offence(&w, t.ada, sid, t.market, Trail::Seen);
        assert!(!report_offence(&mut w, off), "bought off: the act goes ahead");
        assert_eq!(stats(&w, sid).bribes, 1);
        assert_eq!(stats(&w, sid).punished, 0);
        assert_eq!(w.person(t.ada).convictions, 0);
        assert_eq!(w.person(t.finn).legal.bribes_taken, 1);
        let bribe = ada0 - purse(&w, t.ada);
        assert!(bribe > 1.0 && bribe < 40.0);
        assert_eq!(purse(&w, t.finn), finn0 + bribe, "the coins went to the enforcer's household");
        assert_eq!(treasury(&w), 200.0);
    }
}

#[test]
fn a_bribe_is_refused_by_the_scrupulous() {
    for seed in 0..10 {
        let (mut w, t, sid, _) = town(40.0, PLAIN, 12.0);
        w.rng = Rng::new(seed);
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        w.person_mut(t.finn).trust = 1.0;
        prefs(&mut w, t.finn, &[(Pref::Wealth, 0.1), (Pref::Conformity, 1.0), (Pref::Fairness, 1.0)]);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.ada, t.market);
        let finn0 = purse(&w, t.finn);
        let off = offence(&w, t.ada, sid, t.market, Trail::Seen);
        assert!(report_offence(&mut w, off));
        assert_eq!(stats(&w, sid).bribes, 0);
        assert_eq!(w.person(t.ada).convictions, 1);
        assert_eq!(purse(&w, t.ada), 180.0);
        assert_eq!(purse(&w, t.finn), finn0);
        assert_eq!(treasury(&w), 240.0);
        assert!(w.person(t.finn).diary.iter().any(|d| d.text.contains("would not take it")));
    }
}

#[test]
fn a_jury_that_rejects_the_statute_acquits() {
    for (view, convicted) in [(-0.9, false), (0.5, true)] {
        for seed in 0..10 {
            let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
            w.rng = Rng::new(seed);
            w.polity.jur_mut(TOWN).court = CourtDef::Jury { size: 5, pool: "is_adult".into() };
            for p in w.present_adults() { w.person_mut(p).set_opinion(sid, view); }
            w.person_mut(t.finn).set_opinion(sid, 1.0);
            patrol(&mut w, t.finn, t.market);
            stand(&mut w, t.greta, t.market);
            let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
            // The enforcer stepped in either way; what differs is the verdict.
            assert!(report_offence(&mut w, off));
            assert_eq!(w.person(t.greta).convictions, convicted as u32);
            assert_eq!(stats(&w, sid).punished, convicted as u32);
            assert_eq!(w.enforce.today.acquitted, !convicted as u32);
            assert_eq!(purse(&w, t.greta), if convicted { 170.0 } else { 180.0 });
            let jurors = w.persons.iter().filter(|p| p.diary.iter().any(|d| d.text.starts_with("Sat on the jury"))).count();
            assert_eq!(jurors, 5);
        }
    }
}

#[test]
fn a_judge_decides_by_their_own_view_and_an_empty_bench_hears_nothing() {
    let case = |holder: Option<Actor>, view: f32| {
        let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
        w.polity.offices.push(Office {
            id: OfficeId(1), jur: TOWN,
            def: OfficeDef { name: "Magistrate".into(), powers: vec![Power::Judge], selection: Selection::Fixed, term_days: 0, salary: 0.0, eligibility: "is_adult".into(), holder: String::new() },
            holder: holder.map(|h| if h == Actor::Player { h } else { Actor::Person(t.joss) }), since_day: 0, term_ends: None, abolished: false, created_by: None,
        });
        w.polity.jur_mut(TOWN).court = CourtDef::Judge { office: "Magistrate".into() };
        w.person_mut(t.joss).set_opinion(sid, view);
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.greta, t.market);
        let est = detection_chance(&w, t.greta, sid, t.market, Trail::Seen);
        let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
        report_offence(&mut w, off);
        (w.person(t.greta).convictions, est)
    };
    let (n, est) = case(Some(Actor::Person(PersonId(0))), 0.3);
    assert!(n == 1 && est > 0.8);
    let (n, est) = case(Some(Actor::Person(PersonId(0))), -0.9);
    assert!(n == 0 && est < 0.02, "a judge who rejects the statute will not convict under it");
    let (n, _) = case(Some(Actor::Player), 0.0);
    assert_eq!(n, 1);
    let (n, est) = case(None, 0.0);
    assert!(n == 0 && est == 0.0);
}

#[test]
fn collective_fines_fall_on_the_whole_household() {
    // Alone, Ada answers with her half of the purse and serves the rest.
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    let single = Sanction { fine: "150".into(), ..Default::default() };
    punish(&mut w, t.ada, sid, &single, 150.0, "hoarding", None);
    assert_eq!(purse(&w, t.ada), 110.0);
    assert_eq!(treasury(&w), 310.0);
    assert!(w.person(t.ada).jailed_until.is_some());

    // Collectively, the household pays it all, and her husband feels it too.
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    let collective = Sanction { fine: "150".into(), collective: true, ..Default::default() };
    punish(&mut w, t.ada, sid, &collective, 150.0, "hoarding", None);
    assert_eq!(purse(&w, t.ada), 70.0);
    assert_eq!(treasury(&w), 350.0);
    assert!(w.person(t.ada).jailed_until.is_none());
    assert!(w.person(t.bram).felt.punished > 30.0);
    assert!(w.person(t.cai).felt.punished > 0.0);
}

#[test]
fn unpaid_fines_become_jail_and_time_served_ends() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    let s = Sanction { fine: "100".into(), ..Default::default() };
    punish(&mut w, t.edith, sid, &s, 100.0, "selling without leave", None);
    // Her share of a purse of 40 is 20; the other 80 is nine days at 9 coins a day.
    assert_eq!(purse(&w, t.edith), 20.0);
    assert_eq!(treasury(&w), 220.0);
    let until = w.person(t.edith).jailed_until.expect("jailed for the unpaid part");
    assert_eq!(until, w.slot + 9 * SLOTS_PER_DAY);
    assert!(w.enforce.prisoners.contains(&t.edith));
    assert!(w.person(t.edith).diary.iter().any(|d| d.text.contains("jailed for 9 days")));
    // Her husband takes it hard; a stranger does not.
    assert!(w.person(t.finn).felt.punished > 0.0);
    assert_eq!(w.person(t.greta).felt.punished, 0.0);
    // Held and fed until the day comes round.
    w.person_mut(t.edith).needs.hunger = 80.0;
    w.slot = SLOTS_PER_DAY;
    start_of_day(&mut w);
    assert!(w.person(t.edith).jailed_until.is_some());
    assert!(w.person(t.edith).needs.hunger <= 30.0);
    w.slot = until;
    on_slot(&mut w);
    assert!(w.person(t.edith).jailed_until.is_none());
    assert!(w.enforce.prisoners.is_empty());
    // A pardon works at any time.
    punish(&mut w, t.edith, sid, &Sanction { jail_days: 2.0, ..Default::default() }, 0.0, "idling", None);
    assert!(w.person(t.edith).jailed_until.is_some());
    release(&mut w, t.edith, "pardoned");
    assert!(w.person(t.edith).jailed_until.is_none());
}

const QUOTA: &str = r#"{"name":"Watch","staff":3,"quota_per_week":5,"quota_penalty":12}"#;

fn a_day_on_patrol(w: &mut World, t: &Tiny) {
    patrol(w, t.finn, t.plaza);
    for p in [t.greta, t.hugo, t.joss] { stand(w, p, t.plaza); }
    for _ in 0..SLOTS_PER_DAY { w.slot += 1; on_slot(w); }
}

#[test]
fn a_quota_makes_the_unscrupulous_charge_the_innocent() {
    let (mut w, t, sid, _) = town(5.0, QUOTA, 10.0);
    prefs(&mut w, t.finn, &[(Pref::Wealth, 0.95), (Pref::Conformity, 0.02), (Pref::Fairness, 0.02)]);
    a_day_on_patrol(&mut w, &t);
    assert!(w.enforce.today.false_charges >= 1, "short of his number, he finds someone to charge");
    assert_eq!(w.enforce.today.offences, 0, "nobody actually did anything");
    assert!(w.person(t.finn).legal.false_charges >= 1);
    let settled = stats(&w, sid).punished + stats(&w, sid).bribes;
    assert!(settled >= 1, "each charge ended in a conviction or a payment to make it go away");
    // He picks people he has no tie to: Hugo is his friend, and is left alone unless nobody else is in reach.
    let charged: Vec<PersonId> = [t.greta, t.hugo, t.joss].into_iter().filter(|p| w.person(*p).legal.last_charge.is_some()).collect();
    assert!(!charged.is_empty() && charged[0] != t.hugo);
    // Those charged for nothing know it, and say so.
    assert!(charged.iter().any(|p| w.person(*p).diary.iter().any(|d| d.text.contains("though I had done nothing") || d.text.contains("to forget it"))));
}

#[test]
fn a_quota_does_not_move_the_scrupulous() {
    for seed in 0..5 {
        let (mut w, t, sid, _) = town(5.0, QUOTA, 10.0);
        w.rng = Rng::new(seed);
        w.person_mut(t.finn).trust = 0.9;
        prefs(&mut w, t.finn, &[(Pref::Wealth, 0.3), (Pref::Conformity, 0.95), (Pref::Fairness, 0.95)]);
        a_day_on_patrol(&mut w, &t);
        assert_eq!(w.enforce.today.false_charges, 0);
        assert_eq!(stats(&w, sid).punished, 0);
        for p in [t.greta, t.hugo, t.joss] { assert_eq!(w.person(p).convictions, 0); }
    }
}

#[test]
fn without_pay_by_results_nobody_is_charged_for_nothing() {
    let (mut w, t, sid, _) = town(5.0, PLAIN, 10.0);
    prefs(&mut w, t.finn, &[(Pref::Wealth, 0.95), (Pref::Conformity, 0.02), (Pref::Fairness, 0.02)]);
    a_day_on_patrol(&mut w, &t);
    assert_eq!(w.enforce.today.false_charges, 0);
    assert_eq!(stats(&w, sid).punished, 0);
}

#[test]
fn bounties_and_fine_shares_are_settled_in_the_morning_and_quotas_weekly() {
    let (mut w, t, sid, _) = town(10.0, r#"{"name":"Watch","staff":3,"fine_share":0.5,"bounty":3,"quota_per_week":4,"quota_penalty":2}"#, 10.0);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    patrol(&mut w, t.finn, t.market);
    stand(&mut w, t.greta, t.market);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    assert!(report_offence(&mut w, off));
    assert_eq!(treasury(&w), 210.0);
    assert_eq!(w.person(t.finn).legal.bonus_due, 8.0);
    let before = purse(&w, t.finn);
    w.slot = SLOTS_PER_DAY;
    start_of_day(&mut w);
    assert_eq!(purse(&w, t.finn), before + 8.0);
    assert_eq!(treasury(&w), 202.0);
    assert_eq!(w.person(t.finn).legal.bonus_due, 0.0);
    assert_eq!(w.person(t.finn).legal.cases_week, 1);
    // The week turns: one case against a quota of four, three short at 2 coins each.
    w.slot = 7 * SLOTS_PER_DAY;
    let before = purse(&w, t.finn);
    start_of_day(&mut w);
    assert_eq!(purse(&w, t.finn), before - 6.0);
    assert_eq!(treasury(&w), 208.0);
    assert_eq!(w.person(t.finn).legal.cases_week, 0);
}

#[test]
fn informers_are_paid_from_the_fine() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"informer_reward","share":0.5,"flat":2}"#).unwrap(), None);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    patrol(&mut w, t.finn, t.plaza);
    stand(&mut w, t.greta, t.market);
    // Joss has no view of the statute and no special sense of duty; the reward is what moves him.
    stand(&mut w, t.joss, t.market);
    prefs(&mut w, t.joss, &[(Pref::Wealth, 0.9), (Pref::Community, 0.0)]);
    let before = purse(&w, t.joss);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    assert!(!report_offence(&mut w, off));
    assert_eq!(stats(&w, sid).reported, 1);
    w.slot += 1;
    on_slot(&mut w);
    assert_eq!(w.person(t.greta).convictions, 1);
    assert_eq!(purse(&w, t.joss), before + 7.0);
    assert_eq!(treasury(&w), 203.0);
    assert!(w.person(t.greta).felt.watched >= 1.0, "to be informed on is to be watched");
}

#[test]
fn a_search_finds_what_was_done_behind_closed_doors() {
    for with_search in [false, true] {
        let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
        w.tech[Field::Surveillance] = 2.0;
        if with_search { apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"search","who":"true","per_week":7}"#).unwrap(), None); }
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        let home = w.home_of(t.greta).unwrap();
        let est = detection_chance(&w, t.greta, sid, home, Trail::Private);
        assert_eq!(est > 0.5, with_search);
        let off = offence(&w, t.greta, sid, home, Trail::Private);
        assert!(!report_offence(&mut w, off));
        assert_eq!(w.enforce.latent.len(), 1);
        w.slot = SLOTS_PER_DAY;
        start_of_day(&mut w);
        assert_eq!(w.person(t.greta).convictions, with_search as u32);
        assert_eq!(w.enforce.latent.len(), !with_search as usize);
        assert_eq!(w.enforce.today.searches > 0, with_search);
        // Everyone whose home was gone through feels it, guilty or not.
        assert_eq!(w.person(t.greta).felt.watched >= 2.0, with_search);
        assert_eq!(w.person(t.hugo).felt.watched >= 2.0, with_search);
        // The enforcer's own house is left to someone else, and there is nobody else.
        assert_eq!(w.person(t.edith).felt.watched, 0.0);
    }
}

#[test]
fn a_checkpoint_checks_each_arrival_once() {
    let (mut w, t, _, _) = town(10.0, PLAIN, 10.0);
    let curfew = enact(&mut w, ban(Action::BeOutside, "true", 4.0));
    apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"checkpoint","places":"type == \"gate\""}"#).unwrap(), None);
    let gate = w.map.gates[0];
    w.person_mut(t.finn).set_opinion(curfew, 1.0);
    stand(&mut w, t.hugo, gate);
    // Nobody is manning it yet.
    w.slot += 1;
    on_slot(&mut w);
    assert_eq!(w.person(t.hugo).felt.watched, 0.0);
    patrol(&mut w, t.finn, gate);
    w.slot += 1;
    on_slot(&mut w);
    assert_eq!(w.person(t.hugo).felt.watched, 1.0);
    assert_eq!(w.person(t.hugo).convictions, 1);
    assert_eq!(w.enforce.today.checks, 1);
    for _ in 0..6 { w.slot += 1; on_slot(&mut w); }
    assert_eq!(w.person(t.hugo).convictions, 1, "not charged again for standing where he stood");
    assert_eq!(w.enforce.today.checks, 1);
    // Leaving and coming back means being checked again, but not charged twice in a day.
    stand(&mut w, t.hugo, t.plaza);
    w.slot += 1;
    on_slot(&mut w);
    stand(&mut w, t.hugo, gate);
    w.slot += 1;
    on_slot(&mut w);
    assert_eq!(w.enforce.today.checks, 2);
    assert_eq!(w.person(t.hugo).convictions, 1);
    // The enforcer does not check himself.
    assert_eq!(w.person(t.finn).felt.watched, 0.0);
}

#[test]
fn a_watchpost_is_felt_and_tells() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    w.tech[Field::Surveillance] = 2.0;
    apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"watchpost","places":"type == \"market\""}"#).unwrap(), None);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    patrol(&mut w, t.finn, t.plaza);
    stand(&mut w, t.greta, t.market);
    w.slot += 1;
    on_slot(&mut w);
    assert!(w.person(t.greta).felt.watched > 0.0);
    // Nobody else is at the market, but the post sees.
    assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Seen) > 0.8);
    let mut caught = 0;
    for seed in 0..40 {
        let mut v = w.clone();
        v.rng = Rng::new(seed);
        let off = offence(&v, t.greta, sid, t.market, Trail::Seen);
        assert!(!report_offence(&mut v, off));
        v.slot += 1;
        on_slot(&mut v);
        assert_eq!(stats(&v, sid).reported, v.person(t.greta).convictions);
        caught += v.person(t.greta).convictions;
    }
    assert!(caught >= 32, "the post notices most of what happens in front of it ({caught} of 40)");
}

#[test]
fn paper_trails_need_clerks_and_registers_help() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    assert_eq!(paper_chance(&w, TOWN, Some(Register::Trade)), 0.0);
    assert_eq!(detection_chance(&w, t.greta, sid, t.market, Trail::Paper), 0.0);
    // The economy stub founds nothing, so a bare office is set up; the farm it pointed at is untouched.
    set_clerks(&mut w, TOWN, 2, 9.0);
    let (_, office) = w.enforce.clerks[0];
    assert_eq!(w.firm(office).kind, FirmKind::Office);
    assert_eq!(w.firm(office).capacity, 2);
    assert_eq!(w.firm(t.farm).kind, FirmKind::Farm);
    assert_eq!(w.firm(t.farm).capacity, 6);
    assert_eq!(paper_chance(&w, TOWN, None), 0.0, "an office with nobody in it finds nothing");
    employ_at(&mut w, t.joss, office, 9.0, Role::Clerk);
    w.person_mut(t.joss).set_opinion(sid, 1.0);
    let plain = paper_chance(&w, TOWN, Some(Register::Trade));
    assert!(plain > 0.05 && plain < 0.2);
    assert_eq!(clerks_of(&w, SOUTH), vec![t.joss]);
    apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"registry","of":"trade"}"#).unwrap(), None);
    let kept = paper_chance(&w, TOWN, Some(Register::Trade));
    assert!(kept > plain + 0.2);
    assert_eq!(paper_chance(&w, TOWN, Some(Register::Births)), plain, "a register of trade says nothing about births");
    assert_eq!(register_of(&w, sid, 0), Some(Register::Trade));
    w.tech[Field::Administration] = 2.0;
    assert!(paper_chance(&w, TOWN, Some(Register::Trade)) > kept);
    assert!(detection_chance(&w, t.greta, sid, t.market, Trail::Paper) > 0.3);
    // Switching the register off again.
    apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"registry","of":"trade","on":false}"#).unwrap(), None);
    assert_eq!(w.polity.instruments.len(), 1);
    assert!(paper_chance(&w, TOWN, Some(Register::Trade)) < kept);
    // In the real chain, some paper offences are found and acted on, and nobody sees them done.
    let mut found = 0;
    for seed in 0..40 {
        let mut v = w.clone();
        v.rng = Rng::new(seed);
        patrol(&mut v, t.finn, t.plaza);
        let off = offence(&v, t.greta, sid, t.market, Trail::Paper);
        assert!(!report_offence(&mut v, off));
        v.slot += 1;
        on_slot(&mut v);
        found += v.person(t.greta).convictions;
    }
    assert!(found > 2 && found < 30);
}

#[test]
fn units_are_founded_replaced_and_disbanded() {
    let (mut w, t) = tiny_world();
    apply_unit(&mut w, TOWN, &unit_def(r#"{"name":"Night Watch","staff":4,"wage":8,"shift":"night"}"#), Some(StatuteId(3)));
    assert_eq!(w.polity.units.len(), 1);
    let fid = w.polity.units[0].firm;
    assert_ne!(fid, t.farm);
    assert_eq!(w.firm(fid).kind, FirmKind::Watch);
    assert_eq!((w.firm(fid).capacity, w.firm(fid).wage_offer), (4, 8.0));
    assert_eq!(w.firm(t.farm).capacity, 6);
    employ_at(&mut w, t.finn, fid, 8.0, Role::Enforcer);
    assert_eq!(unit_of(&w, t.finn), Some(UnitId(0)));
    assert_eq!(enforcers_of(&w, TOWN), vec![t.finn]);
    assert!(enforcers_of(&w, NORTH).is_empty(), "a town unit is not on a ward's payroll");
    // Replaced by name: same firm, new terms, and those serving are paid on the new scale.
    apply_unit(&mut w, SOUTH, &unit_def(r#"{"name":"night watch","staff":2,"wage":11,"armed":false}"#), None);
    assert_eq!(w.polity.units.len(), 1);
    assert_eq!(w.polity.units[0].firm, fid);
    assert_eq!((w.firm(fid).capacity, w.firm(fid).wage_offer), (2, 11.0));
    assert_eq!(w.person(t.finn).job.as_ref().unwrap().wage, 11.0);
    assert_eq!(enforcers_of(&w, SOUTH), vec![t.finn]);
    // Its writ now runs only in the South.
    patrol(&mut w, t.finn, t.plaza);
    assert!(on_duty_at(&w, t.plaza).is_empty());
    let shop = w.firm(t.workshop).place;
    patrol(&mut w, t.finn, shop);
    assert_eq!(on_duty_at(&w, shop), vec![t.finn]);
    assert!(eligible_for(&w, fid, t.finn) && !eligible_for(&w, fid, t.cai));
    assert!(disband(&mut w, "Night Watch"));
    assert!(w.polity.units[0].disbanded && !w.firm(fid).open);
    assert!(on_duty_at(&w, shop).is_empty() && enforcers_of(&w, TOWN).is_empty());
    assert!(!disband(&mut w, "Night Watch"));
}

#[test]
fn postings_follow_shift_posts_and_orders() {
    let (mut w, t, _, firm) = town(10.0, r#"{"name":"Watch","staff":3,"posts":"type == \"gate\" || type == \"plaza\""}"#, 10.0);
    employ_at(&mut w, t.bram, firm, 10.0, Role::Enforcer);
    // 06:00: nobody is on yet.
    assert_eq!(post_for(&mut w, t.finn), None);
    // 09:00: the first of a split unit is on the day shift, the second is not.
    w.slot = 9 * SLOTS_PER_HOUR;
    let gate = w.map.gates[0];
    let post = post_for(&mut w, t.finn).expect("on shift");
    assert!(post == gate || post == t.plaza);
    assert_eq!(post_for(&mut w, t.finn), Some(post), "a post holds for a while");
    assert_eq!(post_for(&mut w, t.bram), None);
    assert_eq!(post_for(&mut w, t.greta), None, "not an enforcer");
    // 17:00 the next day (the second member rests on the first day of the week): the other half.
    w.slot = SLOTS_PER_DAY + 17 * SLOTS_PER_HOUR;
    assert_eq!(post_for(&mut w, t.finn), None);
    assert!(post_for(&mut w, t.bram).is_some());
    // An order from the commander outweighs the rest.
    assert!(direct(&mut w, "Watch", gate, 2.0));
    let mut at_gate = 0;
    for k in 0..20 {
        w.slot = SLOTS_PER_DAY + 9 * SLOTS_PER_HOUR + k;
        w.person_mut(t.finn).legal.post_until = 0;
        if post_for(&mut w, t.finn) == Some(gate) { at_gate += 1; }
    }
    assert!(at_gate >= 17);
    // Each member has a day of rest; the first takes the customary one.
    w.slot = 6 * SLOTS_PER_DAY + 9 * SLOTS_PER_HOUR;
    assert_eq!(post_for(&mut w, t.finn), None);
    // With no posts named, the watch keeps to places the public goes.
    let (mut w, t, _, _) = town(10.0, PLAIN, 10.0);
    for k in 0..20 {
        w.slot = 9 * SLOTS_PER_HOUR + k;
        w.person_mut(t.finn).legal.post_until = 0;
        let post = post_for(&mut w, t.finn).unwrap();
        assert!(w.place(post).kind.public());
    }
}

#[test]
fn force_counts_arms_and_their_quality() {
    let (mut w, t, _, _) = town(10.0, PLAIN, 10.0);
    let armed = force_of(&w, t.finn);
    let plain = force_of(&w, t.greta);
    assert!(armed > 1.5 * plain && plain > 0.0);
    w.tech[Field::Arms] = 2.0;
    assert!((force_of(&w, t.finn) - 2.0 * armed).abs() < 1e-4);
    apply_unit(&mut w, TOWN, &unit_def(r#"{"name":"Watch","staff":3,"armed":false}"#), None);
    let unarmed = force_of(&w, t.finn);
    assert!(unarmed < armed && unarmed > plain);
    w.person_mut(t.finn).jailed_until = Some(w.slot + 10);
    assert_eq!(force_of(&w, t.finn), 0.0);
}

#[test]
fn supervision_makes_the_indifferent_act_and_inspectors_catch_bribes() {
    // An enforcer with no view of the statute and no drive lets things pass when nobody is looking...
    let acts = |unit: &str, inspected: bool| {
        let mut n = 0;
        for seed in 0..20 {
            let (mut w, t, sid, _) = town(10.0, unit, 10.0);
            w.rng = Rng::new(seed);
            if inspected {
                apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"inspectors","count":2}"#).unwrap(), None);
                let (_, office) = w.enforce.inspectors[0];
                employ_at(&mut w, t.joss, office, 10.0, Role::Clerk);
                employ_at(&mut w, t.hugo, office, 10.0, Role::Clerk);
            }
            prefs(&mut w, t.finn, &[(Pref::Diligence, 0.3)]);
            patrol(&mut w, t.finn, t.market);
            stand(&mut w, t.greta, t.market);
            let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
            if report_offence(&mut w, off) { n += 1; }
        }
        n
    };
    assert_eq!(acts(PLAIN, false), 0);
    // ...and acts when a commander or inspectors might notice.
    assert_eq!(acts(r#"{"name":"Watch","staff":3,"commander":"Sovereign"}"#, false), 20);
    assert_eq!(acts(PLAIN, true), 20);

    // Inspectors make a bribe a risk; with enough of them even the greedy think twice.
    let mut taken = 0;
    for seed in 0..20 {
        let (mut w, t, sid, _) = town(40.0, PLAIN, 3.0);
        w.rng = Rng::new(seed);
        apply_instrument(&mut w, TOWN, &serde_json::from_str(r#"{"kind":"inspectors","count":2}"#).unwrap(), None);
        let (_, office) = w.enforce.inspectors[0];
        employ_at(&mut w, t.joss, office, 10.0, Role::Clerk);
        employ_at(&mut w, t.hugo, office, 10.0, Role::Clerk);
        assert_eq!(inspectors_of(&w, TOWN).len(), 2);
        w.person_mut(t.finn).set_opinion(sid, 1.0);
        prefs(&mut w, t.finn, &[(Pref::Wealth, 1.0), (Pref::Conformity, 0.05), (Pref::Fairness, 0.05), (Pref::Risk, 0.5)]);
        patrol(&mut w, t.finn, t.market);
        stand(&mut w, t.ada, t.market);
        let off = offence(&w, t.ada, sid, t.market, Trail::Seen);
        report_offence(&mut w, off);
        taken += stats(&w, sid).bribes;
    }
    assert_eq!(taken, 0);
}

#[test]
fn propensities_agree_with_the_decisions_they_forecast() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    let mut sure = 0;
    for k in 0..300 {
        let mut r = Rng::new(1000 + k);
        for p in [t.joss, t.finn] {
            for i in 0..N_PREFS { w.person_mut(p).prefs.0[i] = r.f32(); }
            w.person_mut(p).trust = r.f32();
            w.person_mut(p).set_opinion(sid, r.range(-1.0, 1.0));
        }
        let (reward, severity, incentive, p_sup) = (r.range(0.0, 12.0), r.range(0.0, 60.0), r.range(0.0, 12.0), r.range(0.0, 0.4));
        let pr = detect::report_propensity(&w, t.joss, t.greta, sid, reward, severity);
        let told = decide::would_report(&mut w, t.joss, t.greta, sid, reward, severity);
        if pr >= 1.0 { assert!(told); sure += 1; }
        if pr <= 0.0 { assert!(!told); sure += 1; }
        let pe = detect::enforce_propensity(&w, t.finn, t.greta, sid, incentive, p_sup, 40.0, severity);
        let acted = decide::would_enforce(&mut w, t.finn, t.greta, sid, incentive, p_sup, 40.0, severity);
        if pe >= 1.0 { assert!(acted); sure += 1; }
        if pe <= 0.0 { assert!(!acted); sure += 1; }
    }
    assert!(sure > 200);
}

#[test]
fn exile_execution_and_marks_are_carried_out_and_grieved() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    for p in [t.ida, t.joss] { w.person_mut(p).set_opinion(sid, -0.8); }
    let (ida0, greta0) = (w.person(t.ida).trust, w.person(t.greta).trust);
    let s = Sanction { jail_days: 4.0, add_status: "branded".into(), sterilize: true, ..Default::default() };
    punish(&mut w, t.hugo, sid, &s, 0.0, "speaking against the Sovereign", None);
    let ix = w.status_ix("branded").unwrap();
    assert!(w.person(t.hugo).has_status(ix) && w.person(t.hugo).sterile);
    assert_eq!(w.person(t.hugo).jailed_until, Some(w.slot + 4 * SLOTS_PER_DAY));
    // His wife, who thinks the rule wrong, trusts the government less; a stranger's trust is unmoved.
    assert!(w.person(t.ida).trust < ida0 - 0.02);
    assert_eq!(w.person(t.greta).trust, greta0);
    assert!(w.person(t.ida).felt.punished > 0.0);
    assert!(w.person(t.ida).diary.iter().any(|d| d.kind == "family" && d.text.contains("jailed for 4 days")));
    assert!(w.chronicle.iter().any(|c| c.text.contains("Hugo Dunmore") && c.persons == vec![t.hugo]));
    assert_eq!(stats(&w, sid).punished, 1);
    // Death is graver still.
    let before = w.person(t.ida).trust;
    punish(&mut w, t.hugo, sid, &Sanction { execute: true, ..Default::default() }, 0.0, "treason", None);
    assert!(w.person(t.ida).trust < before - 0.1);
    assert!(w.person(t.ida).diary.iter().any(|d| d.kind == "grief" && d.text.contains("put to death")));
}

#[test]
fn state_survives_a_round_trip_and_a_quiet_slot_changes_nothing() {
    let (mut w, t, sid, _) = town(10.0, PLAIN, 10.0);
    w.person_mut(t.finn).set_opinion(sid, 1.0);
    stand(&mut w, t.greta, t.market);
    stand(&mut w, t.joss, t.market);
    w.person_mut(t.joss).set_opinion(sid, 1.0);
    prefs(&mut w, t.joss, &[(Pref::Conformity, 0.9)]);
    let off = offence(&w, t.greta, sid, t.market, Trail::Seen);
    report_offence(&mut w, off);
    assert_eq!(w.enforce.pending.len(), 1);
    let json = serde_json::to_string(&w).unwrap();
    let back: World = serde_json::from_str(&json).unwrap();
    assert_eq!(back.enforce.pending.len(), 1);
    assert_eq!(back.enforce.pending[0].offence.trail, Trail::Seen);
    // Nobody is on duty, so the report waits; nothing else moves.
    let rng = serde_json::to_string(&w.rng).unwrap();
    for _ in 0..3 { w.slot += 1; on_slot(&mut w); }
    assert_eq!(w.enforce.pending.len(), 1);
    assert_eq!(serde_json::to_string(&w.rng).unwrap(), rng);
    // A report nobody comes for lapses after a day.
    w.slot += SLOTS_PER_DAY + 1;
    on_slot(&mut w);
    assert!(w.enforce.pending.is_empty());
    assert_eq!(w.person(t.greta).convictions, 0);
}
