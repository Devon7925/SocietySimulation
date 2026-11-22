// Money flowing between people and the state: income, taxes, evasion, audits,
// transfers and the mint.
import { evalNum, evalBool } from './expr.js';
import { clamp01, chance } from './util.js';
import { agent, household, hhOf, adultsOf, membersOf, note, chronicle, householdWealth, propertiesOf, isAdult, firm } from './state.js';
import { scopeFor, addImpact, decideViolate, detectProb, punish, agentScope } from './legal.js';

const BASES = { wage: ['income', 'wage'], profit: ['income', 'profit'], rent: ['income', 'rent_income'] };

function entries(S, bases) {
  const t = S._c.idx.tax;
  let out = null;
  for (const b of bases) if (t[b]) out = out ? out.concat(t[b]) : t[b];
  return out;
}

function collect(S, a, e, tax) {
  if (tax <= 0) return 0;
  S.gov.treasury += tax; S.gov.d.revenue += tax;
  e.law.stats.revenue += tax;
  if (a) { addImpact(a, e.law, -tax); a.taxPaid += tax; }
  return tax;
}

/**
 * Pay a person. Income taxes are withheld here unless the person has decided to
 * hide this income, in which case what they owe accumulates until an audit.
 * Returns the net amount that reached the household.
 */
export function pay(S, a, amount, kind) {
  if (amount <= 0) return 0;
  const h = hhOf(S, a);
  a.inc[kind] = (a.inc[kind] || 0) + amount;
  let tax = 0;
  const list = BASES[kind] ? entries(S, BASES[kind]) : null;
  if (list) {
    const sc = scopeFor(S, a, { amount });
    for (const e of list) {
      if (!evalBool(e.p.applies_to, sc, true)) continue;
      const t = amount * clamp01(evalNum(e.p.rate, sc));
      if (t <= 0) continue;
      if (a.evade[e.key]) { a.owed[e.key] = (a.owed[e.key] || 0) + t; e.law.d.viol++; }
      else { tax += collect(S, a, e, t); e.law.d.refrain++; }
    }
  }
  tax = Math.min(tax, amount);
  h.money += amount - tax;
  return amount - tax;
}

/** Marginal rate a person expects on another coin of wages (for work decisions). */
export function expectedWageTax(S, a, wage) {
  const list = entries(S, BASES.wage);
  if (!list) return 0;
  const sc = scopeFor(S, a, { amount: wage, income: Math.max(a.incAvg, wage), wage });
  let r = 0;
  for (const e of list) if (!a.evade[e.key] && evalBool(e.p.applies_to, sc, true)) r += clamp01(evalNum(e.p.rate, sc));
  return Math.min(0.98, r);
}

/** Sales tax due on a sale by firm f. An off-the-books seller accumulates what they owe. */
export function saleTax(S, f, good, value) {
  const list = S._c.idx.tax.sale;
  if (!list || !f || value <= 0) return 0;
  const mgr = f.manager ? agent(S, f.manager) : null;
  const base = mgr ? agentScope(S, mgr) : {};
  let tax = 0;
  for (const e of list) {
    if (e.p.good !== 'any' && e.p.good !== good) continue;
    const sc = Object.assign({}, base, { good, amount: value, firm_type: f.type, firm_owner: f.owner });
    if (!evalBool(e.p.applies_to, sc, true)) continue;
    const t = value * clamp01(evalNum(e.p.rate, sc));
    if (f.offBooks && mgr && f.owner !== 'state') { mgr.owed[e.key] = (mgr.owed[e.key] || 0) + t; e.law.d.viol++; }
    else { tax += collect(S, mgr, e, t); e.law.d.refrain++; }
  }
  return tax;
}
export function saleTaxRate(S, f, good) {
  const list = S._c.idx.tax.sale;
  if (!list) return 0;
  if (f.offBooks) return 0;
  let r = 0;
  for (const e of list) if (e.p.good === 'any' || e.p.good === good) r += clamp01(evalNum(e.p.rate, { good, amount: 1, firm_type: f.type, firm_owner: f.owner }));
  return Math.min(0.95, r);
}

/** Import / export duty rate for a good. */
export function tariff(S, base, good) {
  const list = S._c.idx.tax[base];
  if (!list) return 0;
  let r = 0;
  for (const e of list) if (e.p.good === 'any' || e.p.good === good) r += Math.max(0, evalNum(e.p.rate, { good, amount: 1 }));
  return Math.min(5, r);
}
export function collectTariff(S, base, good, value) {
  const list = S._c.idx.tax[base];
  if (!list) return 0;
  let tax = 0;
  for (const e of list) if (e.p.good === 'any' || e.p.good === good) tax += collect(S, null, e, value * Math.max(0, evalNum(e.p.rate, { good, amount: value })));
  return tax;
}

export function inheritanceTax(S, a, estate) {
  const list = S._c.idx.tax.inheritance;
  if (!list || estate <= 0) return 0;
  const sc = scopeFor(S, a, { amount: estate });
  let tax = 0;
  for (const e of list) if (evalBool(e.p.applies_to, sc, true)) tax += collect(S, null, e, estate * clamp01(evalNum(e.p.rate, sc)));
  return Math.min(tax, estate);
}

/**
 * Weekly reckoning (Moonday morning): audits of last week's hidden income,
 * each taxpayer's decision whether to hide next week's, and the taxes that
 * fall due weekly - wealth, property and head taxes.
 */
export function weeklyFiscal(S) {
  const idx = S._c.idx;
  const all = [];
  for (const b in idx.tax) for (const e of idx.tax[b]) all.push(e);
  const byKey = new Map(all.map((e) => [e.key, e]));
  const pPaper = detectProb(S, 'paper', 0, 0);

  for (const a of S.agents) {
    // Audit what was hidden.
    for (const key of Object.keys(a.owed)) {
      const owed = a.owed[key];
      delete a.owed[key];
      const e = byKey.get(key);
      if (!e || owed < 0.05) continue;
      e.law.stats.violations++;
      if (chance(S, pPaper)) {
        const h = hhOf(S, a);
        const back = Math.min(Math.max(0, h.money), owed);
        h.money -= back; collect(S, a, e, back);
        punish(S, a, e.law, { fine: String(owed * e.p.evasion_fine_multiple), jail_days: e.p.evasion_jail_days }, `hiding ${owed.toFixed(1)} coins of tax`, {});
      } else {
        e.law.stats.evaded += owed;
        addImpact(a, e.law, 0);
      }
    }
    if (!isAdult(a)) continue;
    // Decide whether to hide income this coming week.
    a.evade = {};
    const weekly = Math.max(0, a.incAvg) * 7;
    if (weekly <= 0) continue;
    for (const base of ['income', 'wage', 'profit', 'rent_income']) {
      for (const e of idx.tax[base] || []) {
        const share = base === 'income' ? 1 : base === 'wage' ? (a.job && !a.job.owner ? 1 : 0) : base === 'profit' ? (a.job && a.job.owner ? 1 : 0) : 0.3;
        const sc = scopeFor(S, a, { amount: weekly });
        if (!share || !evalBool(e.p.applies_to, sc, true)) continue;
        const due = weekly * share * clamp01(evalNum(e.p.rate, sc));
        if (due < 0.3) continue;
        // Wages paid by the state or a large employer leave a paper trail.
        const f = a.job ? firm(S, a.job.firm) : null;
        const trail = f && !a.job.owner ? (f.owner === 'state' ? 3 : 1.6) : 1;
        if (decideViolate(S, a, e.law, { gain: due, fine: due * (1 + e.p.evasion_fine_multiple), pen: { jail_days: e.p.evasion_jail_days }, pDetect: Math.min(0.95, pPaper * trail) })) a.evade[e.key] = true;
      }
    }
  }
  // Sales taxes: sellers decide whether to keep a second set of books.
  for (const f of S.firms) {
    f.offBooks = false;
    if (!idx.tax.sale || f.owner === 'state' || !f.manager) continue;
    const mgr = agent(S, f.manager);
    if (!mgr) continue;
    for (const e of idx.tax.sale) {
      if (e.p.good !== 'any' && e.p.good !== (f.type === 'tavern' ? 'ale' : f.good)) continue;
      const due = f.avgSales * f.price * 7 * clamp01(evalNum(e.p.rate, { good: f.good, amount: 1, firm_type: f.type, firm_owner: f.owner }));
      if (due > 0.5 && decideViolate(S, mgr, e.law, { gain: due * (f.owner === 'coop' ? 0.4 : 1), fine: due * (1 + e.p.evasion_fine_multiple), pen: { jail_days: e.p.evasion_jail_days }, pDetect: pPaper })) f.offBooks = true;
    }
  }

  // Wealth, property and head taxes.
  const levy = (h, payer, e, due, what) => {
    if (due <= 0.01 || !payer) return;
    const hide = decideViolate(S, payer, e.law, { gain: due, fine: due * (1 + e.p.evasion_fine_multiple), pen: { jail_days: e.p.evasion_jail_days }, pDetect: Math.min(0.95, pPaper * (what === 'property' ? 2.5 : 1)) });
    if (hide) {
      e.law.d.viol++; e.law.stats.violations++;
      if (chance(S, pPaper)) {
        const back = Math.min(Math.max(0, h.money), due);
        h.money -= back; collect(S, payer, e, back);
        punish(S, payer, e.law, { fine: String(due * e.p.evasion_fine_multiple), jail_days: e.p.evasion_jail_days }, `dodging the ${what} tax`, {});
      } else e.law.stats.evaded += due;
      return;
    }
    e.law.d.refrain++;
    const paid = Math.min(Math.max(0, h.money), due);
    h.money -= paid; collect(S, payer, e, paid);
    if (paid < due - 0.5) note(S, payer, `Could not pay the full ${what} tax - ${(due - paid).toFixed(1)} coins short.`, 'money');
  };
  for (const h of S.households) {
    const adults = adultsOf(S, h);
    const payer = adults.sort((p, q) => q.age - p.age)[0];
    if (!payer) continue;
    for (const e of idx.tax.wealth || []) {
      for (const a of adults) {
        const sc = scopeFor(S, a, {});
        if (evalBool(e.p.applies_to, sc, true)) levy(h, a, e, sc.wealth * clamp01(evalNum(e.p.rate, sc)), 'wealth');
      }
    }
    for (const e of idx.tax.property || []) {
      const sc = scopeFor(S, payer, {});
      if (!evalBool(e.p.applies_to, sc, true)) continue;
      const value = propertiesOf(S, h).reduce((s, b) => s + b.value, 0);
      levy(h, payer, e, value * clamp01(evalNum(e.p.rate, Object.assign(sc, { amount: value }))), 'property');
    }
    for (const e of idx.tax.head || []) {
      let due = 0;
      for (const m of membersOf(S, h)) { const sc = scopeFor(S, m, {}); if (evalBool(e.p.applies_to, sc, true)) due += Math.max(0, evalNum(e.p.rate, sc)); }
      levy(h, payer, e, due, 'head');
    }
  }
}

/** Daily transfers (and weekly ones on Moonday), paid pro rata if the treasury is short. */
export function payTransfers(S, weekly) {
  const due = [];
  let total = 0;
  for (const e of S._c.idx.transfer) {
    if ((e.p.period === 'week') !== weekly) continue;
    for (const a of S.agents) {
      const sc = scopeFor(S, a, {});
      if (!evalBool(e.p.applies_to, sc, false)) continue;
      const amt = Math.max(0, evalNum(e.p.amount, sc));
      if (amt > 0) { due.push([a, e, amt]); total += amt; }
    }
  }
  if (!due.length) return;
  const scale = total > 0 ? Math.min(1, Math.max(0, S.gov.treasury) / total) : 1;
  for (const [a, e, amt] of due) {
    const got = amt * scale;
    if (got > 0) {
      S.gov.treasury -= got; S.gov.d.spending += got;
      e.law.stats.paid += got;
      addImpact(a, e.law, got);
      const who = isAdult(a) ? a : adultsOf(S, hhOf(S, a))[0] || a;
      pay(S, who, got, 'transfer');
    }
    if (scale < 0.98) { a.trust = clamp01(a.trust - 0.015); a.mood -= 2; }
  }
  if (scale < 0.98) {
    S.gov.shortfall += total * (1 - scale);
    if (!S._c.day.shortNoted) { S._c.day.shortNoted = true; chronicle(S, 'economy', `The treasury could only pay ${Math.round(scale * 100)}% of the benefits it owes.`); }
  }
}

export function runMint(S, weekly) {
  for (const e of S._c.idx.mint) {
    if ((e.p.period === 'week') !== weekly) continue;
    S.gov.treasury += e.p.amount;
    S.gov.minted = (S.gov.minted || 0) + e.p.amount;
  }
}

export { household };
