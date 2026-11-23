//! Making people: random draws for strangers, inheritance for children, and
//! the slow shaping of the young by those around them.
use super::names;
use crate::types::*;

/// Spread of heritable values among people drawn at random.
pub const GENE_SD: f32 = 0.2;
/// Mutation around the mean of two parents. Averaging halves the variance of
/// the parents' values; this puts back about what was lost, so the spread of
/// genes in the town neither collapses nor grows.
pub const MUTATION_SD: f32 = 0.13;
/// Mutation when only the mother is known: wider, standing in for the unknown father.
pub const LONE_MUTATION_SD: f32 = 0.2;
/// Chance in how parents' ways are passed on.
pub const UPBRINGING_SD: f32 = 0.1;
/// Weekly pull of each influence on the part of a child's preferences that is
/// not inherited. A childhood is 64 weeks, so the household replaces about
/// half of a child's upbringing over it; ten years of school about a quarter.
pub const HOME_PULL: f32 = 0.012;
pub const PEER_PULL: f32 = 0.006;
pub const SCHOOL_PULL: f32 = 0.008;

#[inline]
fn clamp01(x: f32) -> f32 { x.clamp(0.0, 1.0) }

pub fn new_household(w: &mut World, name: &str) -> HouseholdId {
    let id = HouseholdId(w.households.len() as u32);
    w.households.push(Household {
        id, name: name.to_string(), members: Vec::new(), home: None, money: 0.0, stock: [0.0; N_GOODS], arrears: 0.0,
        wealth: 0.0, wealth_rank: id.0 + 1, wealth_pct: 0.0, econ: Default::default(),
    });
    id
}

/// Where someone with no home to be in is put.
pub(super) fn common_ground(w: &World) -> PlaceId {
    if w.map.plaza.ix() < w.places.len() { w.map.plaza } else { PlaceId(0) }
}

pub fn new_person(w: &mut World, household: HouseholdId, sex: Option<Sex>, age_years: f32, surname: &str) -> PersonId {
    let household = if household.ix() < w.households.len() { household } else { new_household(w, surname) };
    let sex = sex.unwrap_or_else(|| if w.rng.chance(0.5) { Sex::Female } else { Sex::Male });
    let name = names::given(w, sex, surname);
    let age = if age_years.is_finite() { age_years.clamp(0.0, 110.0) } else { 0.0 };
    let birth_day = w.day() - (age * DAYS_PER_YEAR as f32).round() as i32;
    let loc = w.households[household.ix()].home.filter(|h| h.ix() < w.places.len()).unwrap_or_else(|| common_ground(w));
    let id = PersonId(w.persons.len() as u32);
    let mut p = Person::new(id, &name, surname, sex, birth_day, household, loc);
    for i in 0..N_PREFS {
        let g = clamp01(w.rng.gauss(0.5, GENE_SD));
        p.genes.0[i] = g;
        p.prefs.0[i] = clamp01(g + w.rng.gauss(0.0, 0.08));
    }
    p.needs = Needs {
        hunger: w.rng.range(10.0, 40.0),
        energy: w.rng.range(70.0, 100.0),
        social: w.rng.range(50.0, 80.0),
        fun: w.rng.range(50.0, 80.0),
        health: w.rng.gauss(85.0 - (age - 45.0).max(0.0) * 0.6, 8.0).clamp(30.0, 100.0),
    };
    p.education = if age < 6.0 { 0.0 } else { clamp01(w.rng.gauss(((age - 6.0) * 0.055).min(0.55), 0.15)) };
    p.skill = clamp01((age - ADULT_AGE) / 25.0);
    p.trust = clamp01(0.45 + (p.prefs[Pref::Conformity] - 0.5) * 0.4);
    p.until = w.slot;
    w.persons.push(p);
    w.households[household.ix()].members.push(id);
    id
}

pub fn inherit(w: &mut World, mother: PersonId, father: Option<PersonId>) -> (Prefs, Prefs) {
    let h = w.settings.heritability.clamp(0.0, 1.0);
    let of = |w: &World, id: PersonId| w.persons.get(id.ix()).map(|p| (p.genes, p.prefs));
    let (mg, mp) = of(w, mother).unwrap_or_default();
    let dad = father.and_then(|f| of(w, f));
    let (mut genes, mut prefs) = (Prefs::default(), Prefs::default());
    for i in 0..N_PREFS {
        let (gene_mid, ways_mid, sd) = match dad {
            Some((fg, fp)) => (0.5 * (mg.0[i] + fg.0[i]), 0.5 * (mp.0[i] + fp.0[i]), MUTATION_SD),
            None => (mg.0[i], mp.0[i], LONE_MUTATION_SD),
        };
        let g = clamp01(gene_mid + w.rng.gauss(0.0, sd));
        let upbringing = clamp01(ways_mid + w.rng.gauss(0.0, UPBRINGING_SD));
        genes.0[i] = g;
        prefs.0[i] = clamp01(h * g + (1.0 - h) * upbringing);
    }
    (genes, prefs)
}

/// Accumulates a weighted mean of preferences.
#[derive(Clone, Copy)]
struct Mean { sum: [f32; N_PREFS], weight: f32 }
impl Mean {
    fn new() -> Mean { Mean { sum: [0.0; N_PREFS], weight: 0.0 } }
    fn add(&mut self, p: &Prefs, weight: f32) {
        for i in 0..N_PREFS { self.sum[i] += p.0[i] * weight; }
        self.weight += weight;
    }
    fn get(&self) -> Option<[f32; N_PREFS]> {
        if self.weight <= 0.0 { return None; }
        let mut out = self.sum;
        for v in out.iter_mut() { *v /= self.weight; }
        Some(out)
    }
}

/// Weekly: each child's expressed preferences drift a little toward those of
/// the adults they live with, of their friends, and - while enrolled - of the
/// people who teach at the school.
///
/// Only the part that is not inherited moves: `prefs = h * genes + (1 - h) *
/// upbringing`, and it is `upbringing` that is pulled. So the heritable share
/// stays what `Settings::heritability` says it is, and the rest belongs to
/// whoever raises and teaches the child.
pub(super) fn upbringing(w: &mut World) {
    let h = w.settings.heritability.clamp(0.0, 1.0);
    if h >= 0.999 { return; }
    let day = w.day();
    let mut staff = Mean::new();
    for f in &w.firms {
        if f.kind != FirmKind::School || !f.open { continue; }
        for t in &f.workers {
            if let Some(t) = w.persons.get(t.ix()) { if t.present { staff.add(&t.prefs, 1.0); } }
        }
    }
    let staff = staff.get();
    let mut changed: Vec<(usize, Prefs)> = Vec::new();
    for (i, c) in w.persons.iter().enumerate() {
        if !c.present || c.age_on(day) >= ADULT_AGE { continue; }
        let mut home = Mean::new();
        for m in &w.households[c.household.ix()].members {
            let a = &w.persons[m.ix()];
            if a.present && a.age_on(day) >= ADULT_AGE { home.add(&a.prefs, 1.0); }
        }
        let mut peers = Mean::new();
        for f in &c.friends {
            if let Some(q) = w.persons.get(f.id.ix()) { if q.present && f.affinity > 0.0 { peers.add(&q.prefs, f.affinity); } }
        }
        let pulls = [(home.get(), HOME_PULL), (peers.get(), PEER_PULL), (if c.enrolled { staff } else { None }, SCHOOL_PULL)];
        let mut out = c.prefs;
        for k in 0..N_PREFS {
            let g = c.genes.0[k];
            let mut u = clamp01((c.prefs.0[k] - h * g) / (1.0 - h));
            let was = u;
            for (toward, rate) in pulls.iter() {
                if let Some(t) = toward { u += rate * (t[k] - was); }
            }
            out.0[k] = clamp01(h * g + (1.0 - h) * clamp01(u));
        }
        if out != c.prefs { changed.push((i, out)); }
    }
    for (i, prefs) in changed { w.persons[i].prefs = prefs; }
}

/// Mean of each expressed preference among those of the people in town for
/// whom `who` holds. With nobody to count, every mean reads 0.5.
pub fn pref_means_of(w: &World, who: impl Fn(&Person) -> bool) -> [f32; N_PREFS] {
    let mut m = Mean::new();
    for p in &w.persons { if p.present && who(p) { m.add(&p.prefs, 1.0); } }
    m.get().unwrap_or([0.5; N_PREFS])
}

/// Mean of each expressed preference across the adults in town, indexed by
/// `Pref as usize` (what `Stats::pref_mean` records).
pub fn pref_means(w: &World) -> [f32; N_PREFS] {
    let day = w.day();
    pref_means_of(w, |p| p.age_on(day) >= ADULT_AGE)
}

/// Mean of each heritable value across everyone in town, children included.
/// Nobody in the town can see this; it is for the player's charts.
pub fn gene_means(w: &World) -> [f32; N_PREFS] {
    let mut m = Mean::new();
    for p in &w.persons { if p.present { m.add(&p.genes, 1.0); } }
    m.get().unwrap_or([0.5; N_PREFS])
}
