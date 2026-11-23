//! The clock: what happens in what order.
use crate::types::*;
use crate::{assess, demography, economy, enforce, gov, law, life, research, stats};

fn start_of_day(w: &mut World) {
    let fork = w.is_fork();
    w.forecast.forks_today = 0;
    stats::refresh(w);
    demography::start_of_day(w);
    law::start_of_day(w);
    enforce::start_of_day(w);
    assess::start_of_day(w);
    economy::start_of_day(w);
    research::start_of_day(w);
    if !fork { gov::start_of_day(w); }
    life::start_of_day(w);
}

fn end_of_day(w: &mut World) {
    let fork = w.is_fork();
    research::end_of_day(w); // before payroll clears the day's work records
    economy::end_of_day(w);
    law::end_of_day(w);
    life::end_of_day(w);
    if !fork { gov::end_of_day(w); }
    stats::end_of_day(w);
}

/// Advance the world by one slot (15 minutes).
pub fn step(w: &mut World) {
    let day = w.day();
    if w.day_begun != day {
        w.day_begun = day;
        start_of_day(w);
    }
    law::on_slot(w);
    let mut i = 0;
    while i < w.persons.len() {
        if w.persons[i].present { life::update_person(w, PersonId(i as u32)); }
        i += 1;
    }
    economy::on_slot(w);
    enforce::on_slot(w);
    if !w.is_fork() { gov::on_slot(w); }
    if w.slot_of_day() == SLOTS_PER_DAY - 1 { end_of_day(w); }
    w.slot += 1;
}

/// Advance by whole days, as fast as possible.
pub fn run_days(w: &mut World, days: u32) {
    for _ in 0..days * SLOTS_PER_DAY { step(w); }
}
