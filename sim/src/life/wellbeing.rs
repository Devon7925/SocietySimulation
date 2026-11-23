//! Wellbeing: ten components, each about -1..=1, weighted by the person's own
//! preferences into the one number they are trying to raise. Also trust in
//! the government, which follows from how they fare and how they are treated.
//!
//! See the table in docs/DESIGN.md §People. Every component is computed from
//! the person's circumstances; the preferences only weight them.
use super::flag;
use crate::decide;
use crate::polity::Actor;
use crate::types::*;

/// Nightly smoothing: components settle over about a week.
const SMOOTH: f32 = 1.0 / 6.0;
/// "How they used to fare" is an average over about a month.
const BASELINE: f32 = 1.0 / 28.0;

/// x / (1 + x): a cost that bites less the more of it there already is.
#[inline]
fn sat(x: f32) -> f32 { let x = x.max(0.0); x / (1.0 + x) }

/// What everyone in town shares: the things a person's wellbeing depends on
/// that are not about them in particular.
pub(super) struct Town {
    /// Mean self-regarding wellbeing of everyone present.
    pub mean_own: f32,
    pub gini: f32,
    /// Offences per head, lately.
    pub offence: f32,
    /// How much the rules have changed in the last fortnight.
    pub churn: f32,
    pub cost_of_living: f32,
}

pub(super) fn town(w: &World) -> Town {
    let (mut sum, mut n) = (0.0f32, 0u32);
    for p in &w.persons { if p.present { sum += p.wellbeing.own; n += 1; } }
    let today = w.day();
    let mut churn = 0.0;
    for s in &w.statutes {
        if s.passed_by.contains("founding") { continue; }
        for d in [s.enacted_day, s.repealed_day].into_iter().flatten() {
            let age = today - d;
            if (0..14).contains(&age) { churn += (1.0 - age as f32 / 14.0) * if s.is_constitutional() { 2.0 } else { 1.0 }; }
        }
    }
    Town {
        mean_own: if n > 0 { sum / n as f32 } else { 0.0 },
        gini: w.stats.gini,
        offence: w.stats.offences as f32 / (w.stats.population.max(n).max(1)) as f32,
        churn,
        cost_of_living: w.stats.cost_of_living.max(1.0),
    }
}

/// Value of the property each household owns.
fn property(w: &World) -> Vec<f32> {
    let mut v = vec![0.0; w.households.len()];
    for pl in &w.places { if let Owner::Household(h) = pl.owner { if let Some(x) = v.get_mut(h.ix()) { *x += pl.value; } } }
    v
}

/// The ten components for one person, from the given needs (the instant's, or the day's average).
pub(super) fn parts(w: &World, pid: PersonId, needs: &Needs, toil: f32, t: &Town, prop: Option<&[f32]>) -> [f32; N_WANTS] {
    let p = &w.persons[pid.ix()];
    let h = &w.households[p.household.ix()];
    let day = w.day();
    let daily = decide::daily_scale(w, pid);
    let mut out = [0.0f32; N_WANTS];

    // Who shares the household.
    let (mut n, mut adults) = (0.0f32, 0.0f32);
    let (mut fam, mut fam_w) = (0.0f32, 0.0f32);
    for &m in &h.members {
        let o = &w.persons[m.ix()];
        if !o.present { continue; }
        n += 1.0;
        if o.age_on(day) >= ADULT_AGE { adults += 1.0; }
        if m != pid { fam += o.wellbeing.own; fam_w += 1.0; }
    }
    let n = n.max(1.0);

    // comfort: hunger, a stocked larder, rest, health, a home, wares and firewood.
    let fed = (1.0 - needs.hunger / 40.0).clamp(-1.0, 1.0);
    let larder = (h.stock[Good::Food as usize] / (n * 0.85 * 3.0)).min(1.0) * 2.0 - 1.0;
    let rested = ((needs.energy - 40.0) / 50.0).clamp(-1.0, 1.0);
    let hale = ((needs.health - 60.0) / 40.0).clamp(-1.0, 1.0);
    let housed = if h.home.is_some() { 0.5 } else { -1.0 };
    let wares = (h.stock[Good::Wares as usize] / (n * 0.45)).min(1.0) * 2.0 - 1.0;
    let fire = (h.stock[Good::Wood as usize] / (n * 0.25)).min(1.0) * 2.0 - 1.0;
    out[Pref::Comfort as usize] = 0.30 * fed + 0.12 * larder + 0.15 * rested + 0.20 * hale + 0.13 * housed + 0.06 * wares + 0.04 * fire;

    // leisure: fun, company, hours of one's own.
    let free = (1.0 - toil / 32.0).clamp(-1.0, 1.0);
    out[Pref::Leisure as usize] = 0.4 * (needs.fun - 50.0) / 50.0 + 0.3 * (needs.social - 50.0) / 50.0 + 0.3 * free;

    // wealth: what the household holds per adult, against two months' living. Saturating:
    // a coin means more to those with few.
    let owned = match prop { Some(v) => v.get(h.id.ix()).copied().unwrap_or(0.0), None => w.places.iter().filter(|pl| pl.owner == Owner::Household(h.id)).map(|pl| pl.value).sum() };
    let x = ((h.money + owned).max(0.0) / adults.max(1.0)) / (60.0 * t.cost_of_living);
    out[Pref::Wealth as usize] = 1.0 - 2.0 / (1.0 + 3.0 * x);

    // family: how the household and close kin are doing.
    for k in [p.spouse, p.mother, p.father].into_iter().flatten().chain(p.children.iter().copied()) {
        let o = &w.persons[k.ix()];
        if o.present && o.household != p.household { fam += 0.6 * o.wellbeing.own; fam_w += 0.6; }
    }
    out[Pref::Family as usize] = if fam_w > 0.0 { (fam / fam_w).clamp(-1.0, 1.0) } else { 0.0 };

    // community: how friends (by closeness) and the town at large are doing.
    let (mut fr, mut fr_w) = (0.0f32, 0.0f32);
    for f in &p.friends {
        let o = &w.persons[f.id.ix()];
        if o.present && f.affinity > 0.0 { fr += f.affinity * o.wellbeing.own; fr_w += f.affinity; }
    }
    out[Pref::Community as usize] = (if fr_w > 0.0 { 0.6 * fr / fr_w + 0.4 * t.mean_own } else { t.mean_own }).clamp(-1.0, 1.0);

    // autonomy: what rules stopped them doing or made them do; being watched.
    let f = &p.felt;
    out[Pref::Autonomy as usize] = (0.5 - 1.5 * sat(f.constrained / daily + 0.15 * f.watched.max(0.0))).clamp(-1.0, 1.0);

    // fairness: inequality in the town; being singled out.
    out[Pref::Fairness as usize] = (0.6 - 1.6 * t.gini - sat(f.slighted / daily)).clamp(-1.0, 1.0);

    // security: crime suffered, disorder seen, punishment, and how lawless the town is.
    out[Pref::Security as usize] = (0.6 - 1.2 * sat(f.victimized / daily) - 0.8 * sat(f.punished / daily) - (0.12 * f.disorder).clamp(-0.2, 0.6) - (3.0 * t.offence).min(0.6)).clamp(-1.0, 1.0);

    // continuity: how much the rules, and their own circumstances, have lately changed.
    out[Pref::Continuity as usize] = (0.5 - 0.3 * t.churn - p.mind.upheaval).clamp(-1.0, 1.0);

    // esteem: honours, office, standing among neighbours, a clean name.
    let me = Actor::Person(pid);
    let office = w.polity.offices.iter().any(|o| !o.abolished && o.holder == Some(me)) || w.polity.bodies.iter().any(|b| !b.abolished && b.members.contains(&me));
    let adult = p.age_on(day) >= ADULT_AGE;
    let place = match &p.job { Some(j) if j.owner => 0.15, Some(_) => 0.05, None if adult && p.age_on(day) < ELDER_AGE => -0.15, None => 0.0 };
    let friends: f32 = p.friends.iter().map(|f| f.affinity).sum();
    out[Pref::Esteem as usize] = (0.4 * (h.wealth_pct - 50.0) / 50.0 + 0.3 * sat(friends / 2.0) + if office { 0.35 } else { 0.0 } + 0.5 * sat(f.honored) + place - 0.15 * p.convictions.min(4) as f32).clamp(-1.0, 1.0);

    out
}

/// Weigh the components by what this person cares about.
pub(super) fn assemble(prefs: &Prefs, parts: [f32; N_WANTS]) -> Wellbeing {
    let (mut own, mut own_w, mut total, mut total_w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..N_WANTS {
        let wt = prefs.0[k].max(0.0);
        total += wt * parts[k];
        total_w += wt;
        if k != Pref::Family as usize && k != Pref::Community as usize { own += wt * parts[k]; own_w += wt; }
    }
    Wellbeing { parts, own: if own_w > 1e-6 { own / own_w } else { 0.0 }, total: if total_w > 1e-6 { total / total_w } else { 0.0 } }
}

pub(super) fn now(w: &World, pid: PersonId) -> Wellbeing {
    let Some(p) = w.persons.get(pid.ix()) else { return Wellbeing::default() };
    let t = town(w);
    assemble(&p.prefs, parts(w, pid, &p.needs, p.mind.toil as f32, &t, None))
}

/// Night: fold the day into everyone's wellbeing and trust, and clear `felt`.
pub(super) fn nightly(w: &mut World) {
    let day = w.day();
    let n = w.persons.len();

    // What changed in each person's own circumstances today.
    for i in 0..n {
        if !w.persons[i].present { continue; }
        let size = { let h = &w.households[w.persons[i].household.ix()]; h.members.iter().filter(|m| w.persons[m.ix()].present).count().min(255) as u8 };
        let home = w.households[w.persons[i].household.ix()].home;
        let p = &mut w.persons[i];
        let job = p.job.as_ref().map(|j| j.firm);
        let m = &mut p.mind;
        let mut up = 0.0;
        if m.seen {
            if job != m.snap_job { up += if job.is_none() { 0.5 } else { 0.3 }; }
            if home != m.snap_home { up += if home.is_none() { 0.8 } else { 0.4 }; }
            if p.household != m.snap_household || size != m.snap_size { up += 0.3; }
            let rel = (p.income.avg - m.income_base).abs() / m.income_base.max(4.0);
            up += 0.1 * rel.min(1.0);
            m.income_base += 0.1 * (p.income.avg - m.income_base);
        } else {
            m.income_base = p.income.avg;
        }
        m.upheaval = (m.upheaval * 0.85 + up).min(1.5);
        m.snap_job = job;
        m.snap_home = home;
        m.snap_household = p.household;
        m.snap_size = size;
    }

    // Which rules people were punished under today, and what was decided by a vote.
    let mut punishing: Vec<StatuteId> = Vec::new();
    let mut voted_on = false;
    for s in &w.statutes {
        let seen = w.life.punished_seen.iter().find(|(id, _)| *id == s.id).map(|x| x.1).unwrap_or(0);
        if s.stats.punished > seen { punishing.push(s.id); }
        let by_vote = !s.passed_by.is_empty() && !s.passed_by.contains("decree") && !s.passed_by.contains("uprising") && !s.passed_by.contains("founding");
        if by_vote && (s.enacted_day == Some(day) || s.repealed_day == Some(day)) { voted_on = true; }
    }
    w.life.punished_seen.clear();
    for k in 0..w.statutes.len() { let s = &w.statutes[k]; if s.stats.punished > 0 { let e = (s.id, s.stats.punished); w.life.punished_seen.push(e); } }
    let shortfall = w.polity.jurisdictions.iter().any(|j| j.shortfall_today > 0.01);

    // Everyone's day, taken from the same evening's picture of everyone else.
    let prop = property(w);
    // Newcomers (and everyone, the first night) have no standing wellbeing for
    // their kin and neighbours to go by yet: give them their self-regarding part first.
    if w.persons.iter().any(|p| p.present && !p.mind.seen) {
        let t = town(w);
        for i in 0..n {
            if !w.persons[i].present || w.persons[i].mind.seen { continue; }
            let own = { let p = &w.persons[i]; assemble(&p.prefs, parts(w, PersonId(i as u32), &p.needs, p.mind.toil as f32, &t, Some(&prop))).own };
            w.persons[i].wellbeing.own = own;
        }
    }
    let t = town(w);
    let mut today: Vec<[f32; N_WANTS]> = Vec::with_capacity(n);
    for i in 0..n {
        let p = &w.persons[i];
        if !p.present { today.push([0.0; N_WANTS]); continue; }
        let mut needs = p.needs;
        if p.mind.ticks > 0 {
            let k = p.mind.ticks as f32;
            needs.hunger = p.mind.sums[0] / k;
            needs.energy = p.mind.sums[1] / k;
            needs.social = p.mind.sums[2] / k;
            needs.fun = p.mind.sums[3] / k;
        }
        today.push(parts(w, PersonId(i as u32), &needs, p.mind.toil as f32, &t, Some(&prop)));
    }

    for i in 0..n {
        if !w.persons[i].present { continue; }
        let pid = PersonId(i as u32);
        let daily = decide::daily_scale(w, pid);
        let me = Actor::Person(pid);
        let sits = w.polity.offices.iter().any(|o| !o.abolished && o.holder == Some(me)) || w.polity.bodies.iter().any(|b| !b.abolished && b.members.contains(&me));
        let public_pay_owed = { let p = &w.persons[i]; p.work.unpaid_days > 0 && p.job.as_ref().and_then(|j| w.firms.get(j.firm.ix())).map(|f| f.kind.is_org() || matches!(f.owner, Owner::State(_))).unwrap_or(false) };
        let reject = if punishing.is_empty() { 0.3 } else {
            let p = &w.persons[i];
            punishing.iter().map(|s| (-p.opinion(*s).unwrap_or(0.0)).max(0.0)).sum::<f32>() / punishing.len() as f32
        };
        let p = &mut w.persons[i];

        // wellbeing
        let mut parts = p.wellbeing.parts;
        if p.mind.seen { for k in 0..N_WANTS { parts[k] += SMOOTH * (today[i][k] - parts[k]); } } else { parts = today[i]; }
        p.wellbeing = assemble(&p.prefs, parts);
        let total = p.wellbeing.total;
        if !p.mind.seen { p.mind.base = total; p.mind.seen = true; }

        // trust: drifts toward how they are faring against how they used to...
        let faring = total - p.mind.base;
        let target = (0.5 + 5.0 * faring + 0.2 * total).clamp(0.0, 1.0);
        let mut trust = p.trust + 0.08 * (target - p.trust);
        // ...falls when they are punished, the more so under rules they reject...
        if p.felt.punished > 0.0 { trust -= sat(p.felt.punished / daily) * (0.02 + 0.12 * reject); }
        // ...and when what the government promised does not arrive...
        let got = p.income.transfer;
        if shortfall && p.mind.transfer_base > 0.2 && got < 0.9 * p.mind.transfer_base { trust -= 0.05 * (1.0 - got / p.mind.transfer_base); }
        if public_pay_owed { trust -= 0.03; }
        // ...and rises when things are decided by a process they have a part in.
        let cast_a_vote = p.mind.day == day && p.mind.has(flag::VOTED);
        if cast_a_vote || (sits && voted_on) { trust += 0.02; }
        p.trust = trust.clamp(0.0, 1.0);

        let m = &mut p.mind;
        m.base += BASELINE * (total - m.base);
        m.transfer_base += 0.2 * (got - m.transfer_base);
        m.sums = [0.0; 4];
        m.ticks = 0;
        m.toil = 0;
        p.felt = Felt::default();
    }
}

/// Weekly: friendships not kept up fade a little, and the faintest are forgotten.
pub(super) fn tend_friendships(w: &mut World) {
    for p in w.persons.iter_mut() {
        if !p.present || p.friends.is_empty() { continue; }
        for f in p.friends.iter_mut() { f.affinity *= 0.99; }
        if p.friends.len() > 16 {
            p.friends.retain(|f| f.affinity >= 0.03);
            while p.friends.len() > 16 {
                let mut weakest = 0;
                for k in 1..p.friends.len() { if p.friends[k].affinity < p.friends[weakest].affinity { weakest = k; } }
                p.friends.remove(weakest);
            }
        }
    }
}
