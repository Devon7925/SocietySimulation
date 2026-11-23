//! What one slot does to a body: hunger, rest, company, fun, health. Rates
//! are the prototype's (given there per ten minutes) scaled to a 15-minute slot.
use crate::types::*;

pub(super) const HUNGER_AWAKE: f32 = 1.08;
pub(super) const HUNGER_ASLEEP: f32 = 0.48;
/// What a meal takes off hunger.
pub(super) const MEAL: f32 = 52.0;
/// Schooling gained per slot at lessons (about a tenth of the way to fully schooled each year).
pub(super) const LESSON: f32 = 0.00017;

/// Is this activity company?
#[inline]
pub(super) fn company(a: Activity) -> bool {
    matches!(a, Activity::Tavern | Activity::Visit | Activity::Chapel | Activity::Play | Activity::Protest | Activity::School | Activity::Host | Activity::Rebel | Activity::Attend)
}
/// Is this activity fun?
#[inline]
pub(super) fn merry(a: Activity) -> bool {
    matches!(a, Activity::Tavern | Activity::Play | Activity::Stroll | Activity::Visit | Activity::Host | Activity::Garden)
}
/// Is this activity toil (time that is not the person's own)?
#[inline]
pub(super) fn toil(a: Activity) -> bool {
    matches!(a, Activity::Work | Activity::Patrol | Activity::Corvee | Activity::Jailed | Activity::Attend)
}

/// Advance one person's needs by one slot. Called for everyone, every slot.
#[inline]
pub(super) fn tick(p: &mut Person, homeless: bool, day: i32, pedagogy: f32) {
    let act = p.activity;
    let asleep = act == Activity::Sleep;
    let labour = matches!(act, Activity::Work | Activity::Corvee | Activity::Patrol);
    let small = day - p.birth_day < 12 * DAYS_PER_YEAR as i32;
    let n = &mut p.needs;

    let appetite = if asleep { HUNGER_ASLEEP } else { HUNGER_AWAKE };
    n.hunger = (n.hunger + if small { appetite * 0.8 } else { appetite }).min(100.0);
    // Prisoners are fed a thin ration (the watch's larder is not modelled).
    if act == Activity::Jailed && n.hunger > 60.0 { n.hunger = 60.0; }

    let rest = if asleep { if homeless { 2.25 } else { 3.15 } } else if labour { -1.08 } else if act == Activity::Travel { -0.9 } else { -0.75 };
    n.energy = (n.energy + rest).clamp(0.0, 100.0);

    let social = if company(act) { 3.9 } else if labour { 0.225 } else if asleep { -0.075 } else { -0.6 };
    n.social = (n.social + social).clamp(0.0, 100.0);

    let fun = if merry(act) { 3.6 } else if labour { -0.63 } else if asleep { 0.0 } else { -0.33 };
    n.fun = (n.fun + fun).clamp(0.0, 100.0);

    if n.hunger > 92.0 { n.health -= 0.195; }
    else if n.hunger < 65.0 && n.energy > 25.0 && n.health < 100.0 { n.health += 0.03; }
    if homeless && asleep { n.health -= 0.03; }
    // Health reaching nothing is death, which is demography's to carry out.
    n.health = n.health.clamp(0.0, 100.0);

    if act == Activity::School { p.education = (p.education + LESSON * pedagogy).min(1.0); }

    let m = &mut p.mind;
    m.sums[0] += n.hunger;
    m.sums[1] += n.energy;
    m.sums[2] += n.social;
    m.sums[3] += n.fun;
    m.ticks = m.ticks.saturating_add(1);
    if toil(act) { m.toil = m.toil.saturating_add(1); }
}
