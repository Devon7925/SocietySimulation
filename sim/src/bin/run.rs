//! Headless runner: `cargo run --release --bin run -- [seed] [days]`.
//! Prints one line of town statistics per day; used for calibration.
use society::types::*;
use society::{engine, worldgen};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let days: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let every: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut w = worldgen::generate(seed);
    println!("{} - seed {seed}: {} people, {} households, {} firms, {} places", w.town_name, w.present().len(), w.households.len(), w.firms.len(), w.places.len());
    let t = Instant::now();
    println!("{:>4} {:>4} {:>6} {:>6} {:>6} {:>5} {:>5} {:>6} {:>6} {:>6} {:>6} {:>7} {:>5} {:>4} {:>4} {:>4}", "day", "pop", "well", "trust", "wage", "unemp", "gini", "food", "wood", "wares", "ale", "treas", "money", "off", "pun", "home");
    for d in 0..days {
        for _ in 0..SLOTS_PER_DAY { engine::step(&mut w); }
        if (d + 1) % every == 0 {
            if let Some(s) = w.history.last() {
                println!("{:>4} {:>4} {:>6.3} {:>6.2} {:>6.2} {:>5.2} {:>5.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>7.0} {:>5.0} {:>4} {:>4} {:>4}", s.day + 1, s.population, s.mean_wellbeing, s.trust, s.median_wage, s.unemployment, s.gini, s.price[0], s.price[1], s.price[2], s.price[3], s.treasury, s.money_supply, s.offences, s.punished, s.homeless);
            }
        }
    }
    let el = t.elapsed().as_secs_f64();
    println!("{days} days in {:.2}s ({:.2} ms/day)", el, el * 1000.0 / days.max(1) as f64);
    for c in w.chronicle.iter().rev().take(15).rev() { println!("  [day {}] {}", c.slot / SLOTS_PER_DAY + 1, c.text); }
}
