// Firms, the market, wages, rents and the labour market. Prices and wages are
// never set by rule here: each firm nudges its own price toward what clears
// its stock, and hires while one more pair of hands pays for itself.
import { evalNum, evalBool } from './expr.js';
import { clamp, clamp01, mean, median, rnd, chance, pick, gauss, weekdayOf } from './util.js';
import { FIRM, BASE_PRICE, IMPORT_MULT, EXPORT_MULT, agent, household, building, firm, hhOf, adultsOf, membersOf, note, chronicle, fullName, isAdult, invalidate, propertiesOf } from './state.js';
import { hire, leaveJob } from './world.js';
import { attempt, scopeFor, agentScope, priceLimit, drawSubsidy, subsidyFor, decideViolate, detectProb, punish, addImpact, requirementsFor, setOwner } from './legal.js';
import { pay, saleTax, saleTaxRate, tariff, collectTariff, expectedWageTax } from './fiscal.js';
import { addHome } from './map.js';

const HOURS = 9;
export const lawGood = (f) => (f.type === 'tavern' ? 'ale' : f.good);

function entryByKey(S, key) {
  if (!key) return null;
  const [id, i] = key.split(':');
  const law = S.laws.find((l) => l.id === id);
  return law && law.status === 'active' ? { law, p: law.provisions[+i], key } : null;
}

// ------------------------------------------------------------------ market

export const importPrice = (S, good) => BASE_PRICE[good] * IMPORT_MULT * (S.trader.mult?.[good] || 1) * (1 + tariff(S, 'import', good));
export const exportPrice = (S, good) => BASE_PRICE[good] * EXPORT_MULT * (S.trader.mult?.[good] || 1) * Math.max(0, 1 - tariff(S, 'export', good));

export function marketOffers(S, good, exclude) {
  const list = [];
  for (const f of S.firms) {
    if (f.good !== good || f === exclude || !f.open || f.suspended || f.stock < 0.05) continue;
    list.push({ f, price: f.sellPrice || f.price });
  }
  list.push({ trader: true, price: importPrice(S, good) });
  list.sort((p, q) => p.price - q.price);
  return list;
}
export function marketPrice(S, good) {
  const o = marketOffers(S, good);
  return o[0].price;
}

function settleSale(S, f, good, n, gross) {
  const tax = saleTax(S, f, good, gross);
  f.cash += gross - tax; f.stock -= n;
  f.d.sold += n; f.d.rev += gross; f.d.tax += tax;
  // A firm selling unlawfully risks every sale being the one that is noticed.
  if ((f.illicit || f.capBreak) && f.manager) {
    const e = entryByKey(S, f.violKey);
    const mgr = agent(S, f.manager);
    const mk = S._c.marketB || (S._c.marketB = S.buildings.find((b) => b.type === 'market'));
    if (e && mgr && !mgr.jail && chance(S, detectProb(S, 'trade', mk.x, mk.y) * 0.25)) {
      const pen = e.p.penalty || {};
      if (pen.confiscate) { f.stock = 0; }
      punish(S, mgr, e.law, pen, `selling ${good} unlawfully`, {});
    }
  }
}

/** A person buys from the market, cheapest seller first. Returns the quantity obtained. */
export function buyGood(S, a, good, qty, wtp, place = 'market') {
  if (qty <= 0) return 0;
  const h = hhOf(S, a);
  const offers = marketOffers(S, good);
  if (offers[0].price > wtp) return 0;
  const r = attempt(S, a, 'buy', { good, price: offers[0].price, quantity: qty, place }, Math.max(0.5, (wtp - offers[0].price) * qty), { kind: 'trade' });
  if (r === 'refrain' || r === 'caught') return 0;
  const sub = subsidyFor(S, a, good);
  let got = 0;
  for (const o of offers) {
    if (qty < 0.05 || o.price > wtp) break;
    const net = o.price * (1 - (sub ? sub.fr : 0));
    const afford = net > 0 ? Math.max(0, h.money) / net : qty;
    const n = Math.min(qty, o.trader ? qty : o.f.stock, afford);
    if (n < 0.05) break;
    const gross = n * o.price;
    const covered = drawSubsidy(S, a, good, gross);
    h.money -= gross - covered; h.spend += gross - covered;
    if (o.trader) { const duty = collectTariff(S, 'import', good, gross / (1 + tariff(S, 'import', good))); S.trader.sold += gross - duty; }
    else settleSale(S, o.f, good, n, gross);
    h.inv[good] = (h.inv[good] || 0) + n;
    qty -= n; got += n;
  }
  return got;
}

function firmBuy(S, f, good, qty) {
  const mgr = agent(S, f.manager);
  if (!mgr || qty <= 0) return 0;
  const offers = marketOffers(S, good, f);
  const r = attempt(S, mgr, 'buy', { good, price: offers[0].price, quantity: qty, place: 'market' }, Math.max(2, f.avgProfit), { kind: 'trade', mustComply: f.owner === 'state', silent: true });
  if (r === 'refrain' || r === 'caught') return 0;
  let got = 0;
  for (const o of offers) {
    if (qty < 0.05) break;
    const n = Math.min(qty, o.trader ? qty : o.f.stock, Math.max(0, f.cash + 15) / o.price);
    if (n < 0.05) break;
    const gross = n * o.price;
    f.cash -= gross; f.d.inCost += gross;
    if (o.trader) { const duty = collectTariff(S, 'import', good, gross / (1 + tariff(S, 'import', good))); S.trader.sold += gross - duty; }
    else settleSale(S, o.f, good, n, gross);
    f.input += n; qty -= n; got += n;
  }
  return got;
}

/** Pay for a service (a drink, a treatment, a day of school). Returns true if served. */
export function buyService(S, a, f, target, o = {}) {
  const h = hhOf(S, a);
  const price = f.sellPrice || f.price;
  const sub = subsidyFor(S, a, target);
  const net = price * (1 - (sub ? sub.fr : 0));
  if (h.money < net) return false;
  const covered = drawSubsidy(S, a, target, price);
  h.money -= price - covered; h.spend += price - covered;
  const tax = o.taxGood ? saleTax(S, f, o.taxGood, price) : 0;
  f.cash += price - tax; f.d.rev += price; f.d.tax += tax; f.d.served++; f.d.sold++;
  return true;
}

// ------------------------------------------------------------------- firms

export function staffPresent(S, f) {
  let n = 0;
  for (const id of f.workers) { const w = agent(S, id); if (w && w.act.type === 'work' && w.inside === f.b) n++; }
  return n;
}

function ensureManager(S, f) {
  let m = f.manager ? agent(S, f.manager) : null;
  if (m && m.job && m.job.firm === f.id && !m.dead) return m;
  m = null;
  if (f.owner === 'private') {
    const oh = household(S, f.ownerHh);
    const cand = oh ? adultsOf(S, oh).filter((p) => p.age < 72 && (!p.job || p.job.firm === f.id || !p.job.owner)).sort((p, q) => q.skill - p.skill)[0] : null;
    if (cand) {
      if (!cand.job || cand.job.firm !== f.id) { leaveJob(S, cand); cand.job = { firm: f.id, wage: 0, role: FIRM[f.type].job, since: S.day, owner: true }; f.workers.push(cand.id); S._c.scopes.delete(cand.id); note(S, cand, `Took over the running of ${f.name}.`, 'work'); }
      cand.job.owner = true; cand.job.wage = 0;
      m = cand;
    }
  }
  if (!m) {
    const ws = f.workers.map((id) => agent(S, id)).filter(Boolean).sort((p, q) => q.skill + q.edu - p.skill - p.edu);
    m = ws[0] || null;
    if (m && f.owner === 'state') m.job.wage = Math.max(m.job.wage, S.stat.medianWage * 1.2);
  }
  f.manager = m ? m.id : null;
  return m;
}

function layOff(S, f, why) {
  const ws = f.workers.map((id) => agent(S, id)).filter((w) => w && !w.job.owner && w.id !== f.manager).sort((p, q) => q.job.since - p.job.since);
  const w = ws[0];
  if (!w) return false;
  leaveJob(S, w);
  w.mood -= 8;
  note(S, w, `Let go from ${f.name}${why ? ` - ${why}` : ''}.`, 'work');
  return true;
}

function closeFirm(S, f, why) {
  for (const id of [...f.workers]) { const w = agent(S, id); if (w && !w.job.owner) { leaveJob(S, w); note(S, w, `${f.name} has shut. I am out of work.`, 'work'); } }
  f.open = false; f.closedDay = S.day; f.vacancies = 0;
  chronicle(S, 'economy', `${f.name} has closed${why ? `: ${why}` : ''}.`, { firm: f.id });
}

function planOrg(S, f) {
  const g = S.gov;
  const target = f.type === 'watch' ? g.guardsTarget : f.type === 'office' ? g.clerksTarget : f.cap;
  const wage = f.type === 'works' ? f.wageOffer : g.guardWage;
  f.wageOffer = wage; f.open = true; f.sellPrice = 0;
  for (const id of f.workers) { const w = agent(S, id); if (w) w.job.wage = wage; }
  while (f.workers.length > target) { const w = agent(S, f.workers[f.workers.length - 1]); if (!w) { f.workers.pop(); continue; } leaveJob(S, w); note(S, w, `Dismissed from the ${f.name}.`, 'work'); }
  f.vacancies = g.treasury > wage ? Math.max(0, target - f.workers.length) : 0;
  f.d = { sold: 0, rev: 0, prod: 0, wages: 0, inCost: 0, tax: 0, served: 0 };
}

function planFirm(S, f) {
  const def = FIRM[f.type];
  if (def.org) return planOrg(S, f);
  const b = building(S, f.b);
  const d = f.d;
  const profit = d.rev - d.wages - d.inCost - d.tax;
  const operated = d.prod > 0 || d.sold > 0 || d.wages > 0;
  f.avgProfit = 0.75 * f.avgProfit + 0.25 * profit;
  f.avgSales = 0.7 * f.avgSales + 0.3 * d.sold;
  f.lossDays = operated && profit < -0.5 ? f.lossDays + 1 : Math.max(0, f.lossDays - 1);
  f.last = { ...d, profit };
  f.d = { sold: 0, rev: 0, prod: 0, wages: 0, inCost: 0, tax: 0, served: 0 };

  if (!f.open) {
    // A shuttered private firm reopens if its owners can stake it again.
    const oh = f.owner === 'private' ? household(S, f.ownerHh) : null;
    if (f.owner !== 'private' || (oh && oh.money > 60 && S.day - (f.closedDay || 0) >= 5)) {
      if (oh) { oh.money -= 40; f.cash = Math.max(f.cash, 0) + 40; }
      f.open = true; f.lossDays = 0;
      chronicle(S, 'economy', `${f.name} has reopened.`, { firm: f.id });
    } else return;
  }
  const mgr = ensureManager(S, f);
  if (!mgr) { f.vacancies = f.owner === 'private' ? 0 : 1; f.suspended = true; return; }
  const good = lawGood(f);

  // --- Is it lawful to keep operating? The manager decides whether to carry on regardless.
  f.illicit = false; f.suspended = !!mgr.jail && f.workers.length <= 1;
  if (GOODSET.has(good)) {
    const stake = f.owner === 'state' ? 0 : Math.max(4, f.avgProfit * (f.owner === 'coop' ? 1 / Math.max(1, f.workers.length) : 1) + S.stat.medianWage) * 3;
    const ctx = { good, price: f.price, quantity: Math.max(1, f.avgSales), place: f.type, firm_type: f.type, firm_owner: f.owner };
    for (const action of f.type === 'tavern' ? ['sell'] : ['produce', 'sell']) {
      const list = S._c.idx.prohibit[action];
      if (!list) continue;
      const hit = list.find((e) => evalBool(e.p.condition, scopeFor(S, mgr, ctx), false));
      if (!hit) continue;
      const r = attempt(S, mgr, action, ctx, stake, { kind: 'site', x: b.x, y: b.y, mustComply: f.owner === 'state', silent: true, why: `${action === 'produce' ? 'making' : 'selling'} ${good} at ${f.name}` });
      if (r === 'violate') { f.illicit = true; f.violKey = hit.key; }
      else {
        f.suspended = true;
        if (r === 'caught' && hit.p.penalty.confiscate) { f.stock = 0; if (f.type === 'tavern') f.input = 0; }
      }
    }
  }
  if (f.suspended) {
    f.vacancies = 0;
    f.idleDays = (f.idleDays || 0) + 1;
    if (f.idleDays === 3) { while (layOff(S, f, 'the work is forbidden now')) { /* all */ } }
    return;
  }
  f.idleDays = 0;

  const n = f.workers.length;
  const ws = f.workers.map((id) => agent(S, id)).filter(Boolean);
  const effAvg = clamp(mean(ws, (w) => w.eff || 0.8) || 0.8, 0.3, 1.4);
  const ref = ws.find((w) => !w.job.owner) || mgr;
  const wsub = subsidyFor(S, ref, 'wages');
  const wcost = 1 - (wsub ? wsub.fr : 0);

  if (def.service) {
    // --- Service firms price by how busy they are and staff to demand.
    const perStaff = f.type === 'tavern' ? 22 : f.type === 'clinic' ? 7 : 14;
    const demand = f.type === 'school' ? S.agents.filter((k) => k.enrolled).length : f.avgSales;
    const util = demand / Math.max(1, n * perStaff);
    if (util > 0.75) f.price *= 1.035; else if (util < 0.3) f.price *= 0.96;
    const alePx = f.type === 'tavern' ? marketPrice(S, 'ale') : 0;
    const floor = f.type === 'tavern' ? alePx * 0.5 * 1.15 : BASE_PRICE[f.good] * 0.2;
    f.price = clamp(f.price, floor, BASE_PRICE[f.good] * 6);
    const want = clamp(Math.ceil(demand / (perStaff * 0.8)), 1, f.cap);
    const revPerStaff = (f.last.rev - f.last.inCost) / Math.max(1, n);
    f.vacancies = n < want && (f.avgProfit > -1 || n === 0) && revPerStaff > f.wageOffer * wcost * 0.8 ? 1 : 0;
    if (n > want + (f.type === 'tavern' ? 0 : 0) && n > 1) { f.lowDays = (f.lowDays || 0) + 1; if (f.lowDays >= 3) { layOff(S, f, 'not enough custom'); f.lowDays = 0; } } else f.lowDays = 0;
    if (f.type === 'tavern') {
      f.input *= 0.99;
      const need = Math.max(2, f.avgSales * 0.5);
      if (f.input < need * 1.3) firmBuy(S, f, 'ale', need * 2.2 - f.input);
    }
  } else {
    // --- Producers.
    const target = Math.max(3, f.avgSales * 2);
    if (f.stock < 0.35 * target) f.price *= f.stock < 0.05 * target ? 1.06 : 1.03;
    else if (f.stock > 1.5 * target) f.price *= 0.95;
    const uc = f.last.prod > 0.5 ? (f.last.wages + f.last.inCost) / f.last.prod : 0;
    const floor = uc * (f.stock > 4 * target ? 0.35 : 0.65);
    f.price = clamp(f.price, Math.max(BASE_PRICE[good] * 0.25, floor), BASE_PRICE[good] * IMPORT_MULT * 1.7);
  }

  // --- Legal price limits. Over a ceiling, the manager either complies or sells on the sly.
  f.sellPrice = f.price; f.capBreak = false;
  const target = f.type === 'tavern' ? 'ale' : f.good;
  const lim = priceLimit(S, target, scopeFor(S, mgr, { good: target, price: f.price, quantity: 1, firm_type: f.type, firm_owner: f.owner }));
  if (lim) {
    const k = f.type === 'tavern' ? 0.6 : 1; // a tavern measure is about half a unit of ale
    const max = lim.max * k, min = lim.min * k;
    if (f.price > max) {
      const gain = (f.price - max) * Math.max(1, f.avgSales) * (f.owner === 'coop' ? 0.5 : 1);
      const pen = lim.e.p.penalty;
      const mk = S.buildings.find((x) => x.type === 'market');
      if (f.owner !== 'state' && decideViolate(S, mgr, lim.e.law, { gain, fine: Math.max(0, evalNum(pen.fine, scopeFor(S, mgr, { price: f.price, good: target }))), pen, pDetect: Math.min(0.9, detectProb(S, 'trade', mk.x, mk.y) * 3) })) {
        f.capBreak = true; f.violKey = lim.e.key; lim.e.law.d.viol++; lim.e.law.stats.violations++;
      } else { f.sellPrice = max; lim.e.law.d.refrain++; }
    } else if (f.price < min) { f.sellPrice = min; lim.e.law.d.refrain++; }
  }

  if (!def.service) {
    const target2 = Math.max(3, f.avgSales * 2);
    const inPx = def.input ? marketPrice(S, def.input) : 0;
    const pNet = f.sellPrice * (1 - saleTaxRate(S, f, good)) - inPx * def.inPer;
    const out = (L) => def.A * Math.pow(Math.max(0, L) * effAvg, 0.7) * HOURS;
    const glut = f.stock > 3 * target2;
    const mrpNext = (out(n + 1) - out(n)) * pNet * (glut ? 0.3 : 1);
    const mrpLast = n > 0 ? (out(n) - out(n - 1)) * pNet * (glut ? 0.6 : 1) : Infinity;
    f.mrp = mrpNext;
    if (f.owner === 'coop') {
      const avgNow = (out(n) * pNet) / Math.max(1, n), avgNext = (out(n + 1) * pNet) / (n + 1);
      const solidarity = mean(ws, (w) => w.v.community);
      f.vacancies = n < f.cap && avgNext >= Math.max(avgNow * 0.9, S.stat.medianWage * (1.2 - 0.3 * solidarity)) ? 1 : 0;
      f.wageOffer = Math.max(1, avgNext);
    } else {
      f.vacancies = n < f.cap && mrpNext > f.wageOffer * wcost * 1.05 ? 1 : 0;
      if (f.vacancies) { if (++f.unfilled >= 2) f.wageOffer = Math.min((mrpNext * 0.97) / wcost, f.wageOffer * 1.04); }
      else { f.unfilled = 0; if (mrpNext < f.wageOffer * wcost * 0.9) f.wageOffer = Math.max(1, f.wageOffer * 0.985); }
      const junior = ws.filter((w) => !w.job.owner && w.id !== f.manager).sort((p, q) => q.job.since - p.job.since)[0];
      if (junior && (mrpLast < junior.job.wage * wcost * 0.8 || (f.cash < -5 && f.lossDays > 2))) {
        f.lowDays = (f.lowDays || 0) + 1;
        if (f.lowDays >= 3) { layOff(S, f, 'not enough work to cover the wage'); f.lowDays = 0; }
      } else f.lowDays = 0;
    }
    // Inputs for the day.
    if (def.input) {
      const need = out(f.workers.length) * def.inPer;
      if (f.input < need * 1.2) firmBuy(S, f, def.input, need * 2 + 1 - f.input);
    }
    // Sell a glut to the travelling trader rather than let it rot.
    if (f.stock > 3.5 * target2) {
      const q = f.stock - 2.5 * target2;
      const ep = exportPrice(S, good);
      const lawful = attempt(S, mgr, 'sell', { good, price: ep, quantity: q, place: 'market', firm_type: f.type, firm_owner: f.owner }, 1, { silent: true, mustComply: true }) === 'ok';
      if (ep > 0 && lawful) {
        const gross = q * BASE_PRICE[good] * EXPORT_MULT * (S.trader.mult?.[good] || 1);
        collectTariff(S, 'export', good, gross);
        f.cash += q * ep; f.stock -= q; f.d.rev += q * ep; S.trader.bought += q * ep;
      }
    }
    if (good === 'food') f.stock *= 0.97; else if (good === 'ale') f.stock *= 0.995;
  }

  // --- Wages: raises to keep people, cuts when the firm is bleeding, and the legal floor.
  if (f.owner !== 'coop') {
    for (const w of ws) {
      if (w.job.owner) continue;
      if (w.job.wage < f.wageOffer * 0.97 && f.avgProfit > 0) w.job.wage += (f.wageOffer - w.job.wage) * 0.25;
      if (f.lossDays >= 4 && f.owner !== 'state') w.job.wage *= 0.97;
      const wl = priceLimit(S, 'wage', scopeFor(S, w, { price: w.job.wage }));
      w.job.under = false;
      if (wl && w.job.wage < wl.min) {
        const gap = wl.min - w.job.wage;
        const pen = wl.e.p.penalty;
        if (f.owner !== 'state' && decideViolate(S, mgr, wl.e.law, { gain: gap * 2, fine: Math.max(0, evalNum(pen.fine, scopeFor(S, mgr, { price: w.job.wage }))), pen, pDetect: detectProb(S, 'paper', 0, 0) })) {
          w.job.under = true; wl.e.law.d.viol++; wl.e.law.stats.violations++;
          if (chance(S, detectProb(S, 'paper', 0, 0) / 5)) { punish(S, mgr, wl.e.law, pen, `paying ${fullName(w)} below the legal wage`, {}); w.job.wage = wl.min; w.job.under = false; }
        } else { w.job.wage = wl.min; wl.e.law.d.refrain++; addImpact(w, wl.e.law, gap); }
      } else if (wl && w.job.wage > wl.max) { addImpact(w, wl.e.law, -(w.job.wage - wl.max)); w.job.wage = wl.max; wl.e.law.d.refrain++; }
      // Is employing this person lawful at all?
      const r = S._c.idx.prohibit.employ ? attempt(S, mgr, 'employ', { worker: workerView(S, w), wage: w.job.wage, place: f.type }, Math.max(1, (f.mrp || w.job.wage) - w.job.wage + 2), { kind: 'site', mustComply: f.owner === 'state', silent: true, why: `employing ${fullName(w)}` }) : 'ok';
      if (r === 'refrain' || r === 'caught') { leaveJob(S, w); note(S, w, `${f.name} can no longer lawfully employ me.`, 'work'); if (w.lastLaw) addImpact(w, S.laws.find((l) => l.id === mgr.lastLaw) || { id: 'x' }, -w.job?.wage || -5); }
    }
  }

  // --- Cash: owners take profits out and put money in when the till is empty.
  const dailyCost = Math.max(8, f.last.wages + f.last.inCost);
  const reserve = dailyCost * 4 + 15;
  if (f.owner === 'private') {
    const oh = household(S, f.ownerHh);
    if (f.cash > reserve) pay(S, mgr.job.owner ? mgr : (adultsOf(S, oh)[0] || mgr), (f.cash - reserve) * 0.6, 'profit'), f.cash -= (f.cash - reserve) * 0.6;
    else if (f.cash < 0 && oh) { const put = Math.min(-f.cash, Math.max(0, oh.money) * 0.5); oh.money -= put; f.cash += put; }
    if (f.cash < -10 && f.lossDays > 5) closeFirm(S, f, 'it could not pay its way');
  } else if (f.owner === 'state') {
    if (f.cash > reserve) { const s = f.cash - reserve; f.cash -= s; S.gov.treasury += s; S.gov.d.revenue += s; }
    else if (f.cash < 0) { const put = Math.min(-f.cash, Math.max(0, S.gov.treasury)); S.gov.treasury -= put; S.gov.d.spending += put; f.cash += put; }
    if (f.cash < -40) closeFirm(S, f, 'the treasury could not keep it running');
  } else if (f.cash < -30 && f.lossDays > 6) closeFirm(S, f, 'the co-operative ran out of money');
}

const GOODSET = new Set(['food', 'wood', 'goods', 'ale']);
const workerView = (S, w) => { const s = agentScope(S, w); return { id: s.id, age: s.age, sex: s.sex, immigrant: s.immigrant, is_child: s.is_child, is_elder: s.is_elder, married: s.married, education: s.education, criminal_record: s.criminal_record, children: s.children, name: s.name, surname: s.surname }; };

export function planFirms(S) {
  S._c.marketB = null;
  const m = S.trader.mult || (S.trader.mult = { food: 1, wood: 1, goods: 1, ale: 1 });
  if (weekdayOf(S) === 0) for (const g in m) m[g] = clamp(m[g] * rnd(S, 0.95, 1.05), 0.8, 1.25);
  for (const f of [...S.firms]) planFirm(S, f);
  refreshPrices(S);
}
export function refreshPrices(S) {
  const p = {};
  for (const g of GOODSET) p[g] = marketPrice(S, g);
  const tv = S.firms.find((f) => f.type === 'tavern'), cl = S.firms.find((f) => f.type === 'clinic'), sc = S.firms.find((f) => f.type === 'school');
  p.drink = tv ? tv.sellPrice || tv.price : 0; p.healthcare = cl ? cl.sellPrice || cl.price : 0; p.education = sc ? sc.sellPrice || sc.price : 0;
  S.stat.price = p;
}

/** Every minute: turn the effort of the workers on site into goods. */
export function produceTick(S) {
  for (const f of S.firms) {
    if (f.eff <= 0) continue;
    const def = FIRM[f.type];
    if (def.A > 0 && !f.suspended) {
      let out = (def.A * Math.pow(f.eff, 0.7)) / 60;
      if (def.input) { const need = out * def.inPer; if (f.input < need) out *= f.input / Math.max(need, 1e-9); f.input -= out * def.inPer; }
      f.stock += out; f.d.prod += out;
    } else if (f.type === 'works' && f.project) {
      if (f.project.kind === 'amenities') S.gov.amenity = Math.min(100, S.gov.amenity + f.eff * 0.0009);
      else if (f.project.kind === 'housing') f.progress += f.eff / 60;
    }
    f.eff = 0;
  }
}

/** End of day: wages. A firm that cannot pay loses its people; so does a state that cannot. */
export function payroll(S) {
  for (const f of S.firms) {
    const def = FIRM[f.type];
    const ws = f.workers.map((id) => agent(S, id)).filter(Boolean);
    const isState = def.org || f.owner === 'state';
    let pool = 0, mins = 0;
    if (f.owner === 'coop') { pool = Math.max(0, f.cash - 12 - (f.last ? f.last.inCost * 2 : 0)); mins = ws.reduce((s, w) => s + w.worked, 0); }
    for (const w of ws) {
      const m = w.worked; w.worked = 0;
      if (!m) continue;
      w.skill = clamp01(w.skill + 0.0025);
      if (w.job.owner) continue;
      if (f.owner === 'coop') {
        const share = mins > 0 ? (pool * m) / mins : 0;
        f.cash -= share; f.d.wages += share;
        w.job.wage = w.job.wage * 0.7 + share * 0.3;
        pay(S, w, share, 'wage');
        continue;
      }
      const due = w.job.wage * Math.min(1, m / 480);
      let amt;
      if (isState) {
        const fromFirm = def.org ? 0 : Math.min(due, Math.max(0, f.cash));
        const fromTreasury = Math.min(due - fromFirm, Math.max(0, S.gov.treasury));
        f.cash -= fromFirm; S.gov.treasury -= fromTreasury; S.gov.d.spending += fromTreasury;
        amt = fromFirm + fromTreasury;
      } else {
        amt = Math.min(due, Math.max(0, f.cash + 10));
        f.cash -= amt;
        f.cash += drawSubsidy(S, w, 'wages', amt);
      }
      f.d.wages += amt;
      pay(S, w, amt, 'wage');
      if (amt < due * 0.85) {
        w.unpaid = (w.unpaid || 0) + 1; w.mood -= 6;
        if (isState) w.trust = clamp01(w.trust - 0.05);
        if (w.unpaid >= 2) { note(S, w, `${f.name} has not paid me in full for days. I quit.`, 'work'); leaveJob(S, w); if (def.org && f.type === 'watch') chronicle(S, 'politics', `${fullName(w)} left the watch over unpaid wages.`, { agents: [w.id] }); }
        else note(S, w, `${f.name} could only pay ${amt.toFixed(1)} of the ${due.toFixed(1)} coins I earned today.`, 'money');
      } else w.unpaid = 0;
    }
  }
}

// ------------------------------------------------------------------ housing

export function collectRents(S) {
  const rents = [];
  for (const b of S.buildings) {
    if (b.type !== 'home' || !b.tenantHh) continue;
    const h = household(S, b.tenantHh);
    if (!h || h.home !== b.id) { b.tenantHh = null; continue; }
    const head = adultsOf(S, h).sort((p, q) => q.age - p.age)[0];
    if (!head) continue;
    let rent = b.rent;
    const oh = b.owner === 'private' ? household(S, b.ownerHh) : null;
    const lord = oh ? adultsOf(S, oh).sort((p, q) => q.age - p.age)[0] : null;
    const lim = priceLimit(S, 'rent', scopeFor(S, head, { price: rent }));
    if (lim && rent > lim.max) {
      const pen = lim.e.p.penalty;
      if (lord && decideViolate(S, lord, lim.e.law, { gain: (rent - lim.max) * 7, fine: Math.max(0, evalNum(pen.fine, scopeFor(S, lord, { price: rent }))), pen, pDetect: detectProb(S, 'paper', 0, 0) })) {
        lim.e.law.d.viol++; lim.e.law.stats.violations++;
        if (chance(S, detectProb(S, 'paper', 0, 0) / 7)) { punish(S, lord, lim.e.law, pen, 'charging more rent than the law allows', {}); rent = lim.max; b.rent = lim.max; }
      } else { addImpact(head, lim.e.law, rent - lim.max); rent = lim.max; lim.e.law.d.refrain++; }
    } else if (lim && rent < lim.min) rent = lim.min;
    rents.push(rent);
    const sub = subsidyFor(S, head, 'rent');
    const net = rent * (1 - (sub ? sub.fr : 0));
    if (h.money >= net) {
      const covered = drawSubsidy(S, head, 'rent', rent);
      h.money -= rent - covered; h.spend += rent - covered;
      if (lord) pay(S, lord, rent, 'rent'); else { S.gov.treasury += rent; S.gov.d.revenue += rent; }
      h.arrears = Math.max(0, h.arrears - 0.5);
    } else {
      h.arrears++;
      if (h.arrears === 3) note(S, head, 'We are behind on the rent. If this goes on we will be put out.', 'money');
      if (h.arrears > 7) {
        h.home = null; h.arrears = 0; b.tenantHh = null; b.name = 'Vacant House';
        for (const m of membersOf(S, h)) { m.mood -= 25; note(S, m, 'We have been evicted. We have nowhere to sleep.', 'grief'); }
        chronicle(S, 'economy', `The ${h.name} household was evicted for unpaid rent.`, { agents: h.members.slice(0, 1) });
        S._c.scopes.clear();
      }
    }
  }
  if (rents.length) S.stat.medianRent = median(rents);
}

/** Weekly: landlords reprice, the homeless look for rooms, and those who can afford to, build. */
export function weeklyHousing(S) {
  const homes = S.buildings.filter((b) => b.type === 'home');
  const vacant = () => homes.filter((b) => !S.households.some((h) => h.home === b.id));
  const seekers = S.households.filter((h) => !h.home && h.members.length);
  for (const b of homes) {
    if (b.owner !== 'private' && b.owner !== 'state') continue;
    const empty = !S.households.some((h) => h.home === b.id);
    if (empty) b.rent = Math.max(0.3, (b.rent || 1.6) * 0.92);
    else if (b.tenantHh && b.owner === 'private' && (seekers.length || vacant().length === 0)) b.rent *= 1.03;
  }
  for (const h of seekers.sort((p, q) => q.money - p.money)) {
    const inc = adultsOf(S, h).reduce((s, a) => s + a.incAvg, 0);
    const opts = vacant().filter((b) => b.owner === 'state' || b.owner === 'private').sort((p, q) => (p.rent || 0) - (q.rent || 0));
    const b = opts[0];
    if (!b) break;
    const own = b.owner === 'private' && b.ownerHh === h.id;
    if (!own && (b.rent || 0) > Math.max(0.6, inc * 0.45) && h.money < (b.rent || 0) * 10) continue;
    moveIn(S, h, b);
  }
  // Building: households with savings and no home of their own, or landlords who see unmet demand.
  const still = S.households.filter((h) => !h.home && h.members.length);
  const crowded = vacant().length === 0;
  const woodPx = marketPrice(S, 'wood'), goodsPx = marketPrice(S, 'goods');
  const cost = 12 * woodPx + 5 * goodsPx + 60;
  if (still.length || crowded) {
    const builders = [...still.filter((h) => h.money > cost * 1.1), ...S.households.filter((h) => h.home && h.money > cost * 2.2 && propertiesOf(S, h).length > 0 && (still.length || crowded))].slice(0, 1);
    for (const h of builders) {
      const head = adultsOf(S, h)[0];
      if (!head) continue;
      const count = propertiesOf(S, h).length + 1;
      if (attempt(S, head, 'own_property', { property_type: 'home', property_value: 140, properties_owned: count }, 20, { kind: 'site' }) !== 'ok') continue;
      const b = addHome(S, `${h.name} House`);
      if (!b) break;
      h.money -= cost;
      sellToBuilder(S, 'wood', 12); sellToBuilder(S, 'goods', 5);
      b.owner = 'private'; b.ownerHh = h.id; b.rent = S.stat.medianRent || 1.6;
      if (!h.home) moveIn(S, h, b); else b.name = 'Vacant House';
      chronicle(S, 'economy', `The ${h.name} household built a new house.`, { building: b.id });
    }
  }
  // Public housing crews finish homes when enough labour and materials have gone in.
  for (const f of S.firms) {
    if (f.type !== 'works' || f.project?.kind !== 'housing' || f.progress < 90) continue;
    if (S.gov.treasury < cost - 60) continue;
    const b = addHome(S, 'Public House');
    if (!b) continue;
    f.progress -= 90;
    S.gov.treasury -= cost - 60; S.gov.d.spending += cost - 60;
    sellToBuilder(S, 'wood', 12); sellToBuilder(S, 'goods', 5);
    b.owner = 'state'; b.rent = Math.max(0.3, (S.stat.medianRent || 1.6) * 0.5);
    chronicle(S, 'economy', 'The public works crew finished a new house.', { building: b.id });
    const h = S.households.filter((x) => !x.home && x.members.length)[0];
    if (h) moveIn(S, h, b);
  }
}
function sellToBuilder(S, good, qty) {
  for (const o of marketOffers(S, good)) {
    if (qty <= 0) break;
    if (o.trader) { S.trader.sold += qty * o.price; break; }
    const n = Math.min(qty, o.f.stock);
    settleSale(S, o.f, good, n, n * o.price);
    qty -= n;
  }
}
export function moveIn(S, h, b) {
  h.home = b.id; h.arrears = 0;
  const own = b.owner === 'private' && b.ownerHh === h.id;
  b.tenantHh = own ? null : h.id;
  b.name = `${h.name} House`;
  for (const m of membersOf(S, h)) note(S, m, own ? 'We have a house of our own.' : `We have taken a house at ${(b.rent || 0).toFixed(1)} coins a day.`, 'home');
  S._c.scopes.clear();
}

// ------------------------------------------------------------ labour market

/** The lowest take-home wage for which this person will work. */
export function reservationWage(S, a) {
  const h = hhOf(S, a);
  const adults = Math.max(1, adultsOf(S, h).length);
  const col = S.stat.costOfLiving || 5;
  const cushion = Math.max(0, h.money) / adults / (col * 20);
  let r = col * (0.3 + 0.9 * (1 - a.t.diligence)) * (1 + 0.5 * Math.min(4, cushion)) * (1 + (0.9 * Math.max(0, a.nonLabor)) / col);
  if (a.age >= 60) r *= 1.5;
  if (a.mustWork) r *= 0.25;
  return r;
}

export function qualifies(S, f, a) {
  if (f.type === 'clinic') return a.edu > 0.55;
  if (f.type === 'school') return a.edu > 0.45;
  if (f.type === 'watch') return a.age >= 18 && a.age < 58 && a.health > 50;
  if (f.type === 'office') return a.edu > 0.35 && a.age >= 18;
  if (f.type === 'works') return a.age >= 16 && evalBool(f.project?.applies_to, scopeFor(S, a, {}), true);
  if (a.age < 16) return f.type === 'farm' || f.type === 'workshop';
  return true;
}

/** Best open position for a job-seeker: { f, wage, net } or null. */
export function bestVacancy(S, a, minNet = 0) {
  let best = null;
  for (const f of S.firms) {
    if (!f.vacancies || !f.open || f.suspended || (a.job && a.job.firm === f.id) || !qualifies(S, f, a)) continue;
    let wage = f.wageOffer * (a.age < 16 ? 0.5 : 1);
    const net = wage * (1 - expectedWageTax(S, a, wage));
    const score = net * (1 + (((parseInt(a.id.slice(1), 10) * 31 + parseInt(f.id.slice(1), 10) * 17) % 20) - 10) / 100);
    if (net >= minNet && (!best || score > best.score)) best = { f, wage, net, score };
  }
  return best;
}

/** A person asks for the job in person. Returns true if hired. */
export function applyFor(S, a, f) {
  if (!f.vacancies || !f.open || f.suspended) return false;
  const mgr = f.manager ? agent(S, f.manager) : null;
  let wage = f.wageOffer * (a.age < 16 ? 0.5 : 1);
  const wl = priceLimit(S, 'wage', scopeFor(S, a, { price: wage }));
  if (wl && wage < wl.min) {
    if ((f.mrp || 0) >= wl.min * 1.02 || FIRM[f.type].org) wage = wl.min; else return false;
  }
  if (wl && wage > wl.max) wage = wl.max;
  if (mgr && S._c.idx.prohibit.employ) {
    const r = attempt(S, mgr, 'employ', { worker: workerView(S, a), wage, place: f.type }, Math.max(1, (f.mrp || wage) - wage + 2), { kind: 'site', mustComply: f.owner === 'state', silent: true, why: `hiring ${fullName(a)}` });
    if (r === 'refrain' || r === 'caught') { note(S, a, `${f.name} turned me away: the law forbids them to hire me.`, 'work'); return false; }
  }
  hire(S, f, a, wage);
  f.vacancies--; f.unfilled = 0;
  note(S, a, `Hired at ${f.name} as a ${a.job.role} for ${wage.toFixed(1)} coins a day.`, 'work');
  return true;
}

/** Each morning the employed ask themselves whether the job is still worth it. */
export function reviewJobs(S) {
  for (const a of S.agents) {
    a.mustWork = false;
    if (isAdult(a) && a.age < 65) {
      const e = requirementsFor(S, a, 'work');
      if (e) {
        // The idle decide whether to obey the summons to work; shirkers risk being noticed.
        const idle = !a.job;
        const defy = idle && decideViolate(S, a, e.law, { gain: 4 + (1 - a.t.diligence) * 6, fine: Math.max(0, evalNum(e.p.penalty.fine, scopeFor(S, a, {}))), pen: e.p.penalty, pDetect: detectProb(S, 'public', a.x, a.y) * 0.5 });
        a.mustWork = !defy;
        if (idle && defy) { e.law.d.viol++; e.law.stats.violations++; if (chance(S, detectProb(S, 'public', a.x, a.y) * 0.3)) punish(S, a, e.law, e.p.penalty, 'idleness', {}); }
        else if (idle) { e.law.d.refrain++; }
      }
    }
    if (!a.job || a.job.owner) continue;
    const f = firm(S, a.job.firm);
    if (!f) { a.job = null; continue; }
    const net = a.job.wage * (1 - expectedWageTax(S, a, a.job.wage));
    const res = reservationWage(S, a);
    if (a.age >= 16 && net < res * 0.7 && S.day - a.job.since > 2) {
      note(S, a, a.nonLabor > net ? `Gave up my place at ${f.name}. We get by without the wage.` : `Left ${f.name}: ${net.toFixed(1)} coins a day is not worth the labour.`, 'work');
      leaveJob(S, a);
      continue;
    }
    if (a.age >= 16 && chance(S, 0.12)) {
      const v = bestVacancy(S, a, net * 1.18);
      if (v && applyFor(S, a, v.f)) continue;
    }
    if (a.age >= 62 && chance(S, 0.2)) {
      const h = hhOf(S, a);
      if (a.nonLabor > (S.stat.costOfLiving || 5) * 0.9 || h.money / Math.max(1, adultsOf(S, h).length) > 260) { note(S, a, `Retired from ${f.name}.`, 'work'); leaveJob(S, a); }
    }
  }
}

/** Parents decide each morning whether each child goes to school. */
export function decideSchooling(S) {
  const sf = S.firms.find((f) => f.type === 'school');
  const open = sf && sf.open && !sf.suspended && sf.workers.length > 0;
  const cap = sf ? sf.workers.length * 14 : 0;
  let enrolled = 0;
  for (const k of S.agents) {
    if (k.age < 6 || k.age >= 16) { k.enrolled = false; continue; }
    const h = hhOf(S, k);
    const parents = adultsOf(S, h);
    const head = parents[0];
    if (!open || !head || enrolled >= cap) { k.enrolled = false; continue; }
    const price = sf.sellPrice || sf.price;
    const sub = subsidyFor(S, head, 'education');
    const net = price * (1 - (sub ? sub.fr : 0));
    const worth = 0.5 + 2.2 * mean(parents, (p) => p.edu) + mean(parents, (p) => p.v.prosperity) * 0.4 + Math.min(1.5, h.money / 150);
    let go = h.money > net * 5 + 3 && worth > net;
    const e = requirementsFor(S, k, 'attend_school');
    if (e) {
      const sc = scopeFor(S, head, {});
      if (!go && h.money >= net) {
        const defy = decideViolate(S, head, e.law, { gain: (net - worth) * 6 + (k.job ? k.job.wage * 6 : 0) + 1, fine: Math.max(0, evalNum(e.p.penalty.fine, sc)), pen: e.p.penalty, pDetect: detectProb(S, 'site', 0, 0) });
        if (defy) { e.law.d.viol++; e.law.stats.violations++; if (chance(S, detectProb(S, 'site', 0, 0) / 6)) punish(S, head, e.law, e.p.penalty, `keeping ${k.name} out of school`, { sc }); }
        else { go = true; e.law.d.refrain++; addImpact(head, e.law, -Math.max(0, net - worth)); }
      } else if (go) e.law.d.refrain++;
    }
    k.enrolled = go;
    if (go) { enrolled++; if (k.job) leaveJob(S, k); }
  }
}

/** Daily wear on household goods and the hearth. */
export function householdUpkeep(S) {
  for (const h of S.households) {
    const n = h.members.length;
    h.inv.goods = Math.max(0, h.inv.goods - n * 0.11);
    h.inv.wood = Math.max(0, (h.inv.wood || 0) - n * 0.05);
    h.inv.food *= 0.985;
    h.spendAvg = (h.spendAvg || 0) * 0.8 + h.spend * 0.2;
    h.spend = 0;
  }
}

export { planFirm, layOff, closeFirm, gauss, pick, invalidate, setOwner };
