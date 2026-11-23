//! Getting about. A person is always at a place; a walk takes
//! `World::travel_slots` slots, during which they stand in the street of the
//! ward they are bound for.
use super::{acts, flag, voice};
use crate::law::{self, Verdict};
use crate::statute::{Act, Action};
use crate::types::*;

/// Has something already claimed this person's next slots (or removed them)?
#[inline]
pub(super) fn busy(w: &World, pid: PersonId) -> bool {
    let p = &w.persons[pid.ix()];
    !p.present || p.travel.is_some() || p.until > w.slot
}
/// As `busy`, or locked up: the planner has nothing more to decide for them now.
#[inline]
pub(super) fn taken(w: &World, pid: PersonId) -> bool {
    busy(w, pid) || w.persons[pid.ix()].jailed_until.is_some()
}

/// A plan with its describing line (left blank inside a forecast, where nobody reads it).
pub(super) fn plan(w: &World, activity: Activity, slots: u32, doing: &str, arg: i64) -> Plan {
    Plan { activity, slots, doing: if w.is_fork() { String::new() } else { doing.to_string() }, arg }
}

/// Remember that a rule stood in this person's way, for their opinion of it
/// and for what they say about their day.
pub(super) fn resent(w: &mut World, pid: PersonId, sid: StatuteId, coins: f32) {
    let daily = crate::decide::daily_scale(w, pid);
    let (slot, day) = (w.slot, w.day());
    let total = w.persons[pid.ix()].wellbeing.total;
    let m = &mut w.persons[pid.ix()].mind;
    m.barred = Some((sid, slot));
    let hit = coins.max(0.0) / daily;
    match m.lived.iter_mut().find(|l| l.sid == sid) {
        Some(l) => l.hit -= hit,
        None => m.lived.push(super::Lived { sid, since: day, wb_at: total, direct: 0.0, hit: -hit, known: false }),
    }
}

/// Start `plan` where the person stands.
pub(super) fn begin(w: &mut World, pid: PersonId, plan: Plan) {
    let slot = w.slot;
    let p = &mut w.persons[pid.ix()];
    if p.activity == Activity::Protest && plan.activity != Activity::Protest { p.civic.protesting = false; }
    p.travel = None;
    p.next = None;
    p.activity = plan.activity;
    p.until = slot + plan.slots.max(1);
    p.doing = plan.doing;
    acts::on_start(w, pid, plan.activity, plan.arg);
}

/// Walk to `to` and begin `plan` there, asking nobody's leave.
pub(super) fn walk(w: &mut World, pid: PersonId, to: PlaceId, plan: Plan) {
    let slot = w.slot;
    let (from, moving) = { let p = &w.persons[pid.ix()]; (p.loc, p.travel.is_some()) };
    if !moving && from == to { begin(w, pid, plan); return; }
    let n = w.travel_slots(from, to).max(1);
    let street = w.street_of(w.places[to.ix()].ward);
    let doing = if w.is_fork() { String::new() } else { voice::walking(w, pid, to, plan.activity) };
    let p = &mut w.persons[pid.ix()];
    if p.activity == Activity::Protest { p.civic.protesting = false; }
    p.travel = Some(Travel { from, to, depart: slot, arrive: slot + n });
    p.loc = street;
    p.activity = Activity::Travel;
    p.until = slot + n;
    p.next = Some(plan);
    p.doing = doing;
}

/// Set out for `to` of one's own accord. `value` is what the errand is worth
/// to the person in coins: it is what they weigh against a rule that keeps
/// them indoors. False if they did not go.
pub(super) fn set_out(w: &mut World, pid: PersonId, to: PlaceId, plan: Plan, value: f32) -> bool {
    let (from, moving, held) = { let p = &w.persons[pid.ix()]; (p.loc, p.travel.is_some(), p.jailed_until.is_some()) };
    if !moving && from == to { begin(w, pid, plan); return true; }
    let compelled = held || matches!(plan.activity, Activity::Jailed | Activity::Leaving);
    if !compelled && !moving && !w.places[from.ix()].kind.outdoor() {
        let street = w.street_of(w.places[from.ix()].ward);
        match law::attempt(w, pid, &Act::new(Action::BeOutside, street, value)) {
            Verdict::Lawful | Verdict::Violated(_) => {}
            Verdict::Refrained(sid) => {
                resent(w, pid, sid, value);
                if !w.is_fork() {
                    let first = !w.persons[pid.ix()].mind.has(flag::BARRED_NOTED);
                    let title = w.statute(sid).map(|s| s.title.clone()).unwrap_or_default();
                    w.persons[pid.ix()].thought = "I would go, but we are not allowed out now.".to_string();
                    if first {
                        w.persons[pid.ix()].mind.set(flag::BARRED_NOTED);
                        w.note(pid, "law", format!("Stayed in when I meant to go out. \"{title}\" forbids being abroad at this hour."));
                    }
                }
                return false;
            }
            Verdict::Stopped(_) => return false,
        }
        if taken(w, pid) { return false; }
    }
    walk(w, pid, to, plan);
    true
}

/// The walk is over: stand in the place and begin what was planned.
pub(super) fn arrive(w: &mut World, pid: PersonId) {
    let p = &mut w.persons[pid.ix()];
    let Some(t) = p.travel.take() else { return };
    p.loc = t.to;
    let plan = p.next.take().unwrap_or(Plan { activity: Activity::Idle, slots: 1, doing: String::new(), arg: 0 });
    begin(w, pid, plan);
}
