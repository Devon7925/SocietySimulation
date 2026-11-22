//! Deterministic random numbers. All randomness in the simulation flows
//! through `World::rng` so that a world can be replayed from its seed and so
//! that a fork of the world diverges only because of the change being tested.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    s: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { s: seed ^ 0x9E37_79B9_7F4A_7C15 };
        r.next_u64();
        r
    }
    /// splitmix64
    pub fn next_u64(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u64 << 24) as f32)
    }
    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f32() * (hi - lo)
    }
    /// Uniform integer in [lo, hi] inclusive.
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % ((hi - lo + 1) as u64)) as i32
    }
    pub fn index(&mut self, len: usize) -> usize {
        if len == 0 { 0 } else { (self.next_u64() % len as u64) as usize }
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
    pub fn gauss(&mut self, mean: f32, sd: f32) -> f32 {
        let u = self.f32().max(1e-7);
        let v = self.f32();
        mean + sd * (-2.0 * u.ln()).sqrt() * (2.0 * std::f32::consts::PI * v).cos()
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() { None } else { Some(&items[self.index(items.len())]) }
    }
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
    /// An independent stream, for work that must not disturb the main sequence.
    pub fn split(&mut self) -> Rng {
        Rng::new(self.next_u64())
    }
}
