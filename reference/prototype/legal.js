// The legal system: indexing active laws, deciding whether a person obeys,
// catching and punishing those who do not, enacting, repealing and voting.
import { evalNum, evalBool } from './expr.js';
import { validateLaw } from './lawSpec.js';
import { chance, clamp, clamp01, rnd, hourOf, weekdayOf, pick, mean } from './util.js';
import { FIRM, agent, household, building, firm, hhOf, adultsOf, membersOf, note, chronicle, fullName, householdWealth, propertiesOf, invalidate, isAdult } from './state.js';
import { newFirm, leaveJob } from './world.js';
import { jailAgent } from './behavior.js';
import { die, exileAgent } from './society.js';
import { staticImpact } from './society.js';

// ------------------------------------------------------------------ scopes

export function agentScope(S, a) {
  let sc = S._c.scopes.get(a.id);
  if (sc) return sc;
  const h = hhOf(S, a);
  const adults = Math.max(1, adultsOf(S, h).length);
  const props = propertiesOf(S, h);
  const f = a.job ? firm(S, a.job.firm) : null;
  const home = h.home ? building(S, h.home) : null;
  const role = a.job ? a.job.role : 'none';
  sc = {
    id: a.id, name: a.name, surname: a.surname, age: Math.floor(a.age), sex: a.sex, is_child: a.age < 16, is_adult: a.age >= 16, is_elder: a.age >= 60,
    married: !!a.spouse, children: a.kids.filter((k) => { const c = agent(S, k); return c && c.age < 16 && c.hh === a.hh; }).length, household_size: h.members.length,
    money: h.money / adults, wealth: householdWealth(S, h) / adults, wealth_rank: h.rank || 1, wealth_percentile: h.pct ?? 50,
    income: a.incAvg, wage: a.job ? a.job.wage : 0, employed: !!a.job, job: role,
    workplace: f ? (f.type === 'watch' ? 'guardhouse' : f.type === 'office' ? 'townhall' : f.type === 'works' ? 'plaza' : f.type) : 'none', employer_type: f ? f.owner : 'none',
    is_owner: props.some((b) => FIRM[b.type]), is_landlord: props.some((b) => b.type === 'home' && b.id !== h.home), owns_home: !!home && home.owner === 'private' && home.ownerHh === h.id,
    properties: props.length, homeless: !home, education: a.edu, health: a.health, happiness: a.happy, criminal_record: a.record,
    is_guard: role === 'guard', is_official: role === 'guard' || role === 'clerk' || role === 'laborer', immigrant: !!a.immigrant, in_school: !!a.enrolled,
  };
  S._c.scopes.set(a.id, sc);
  return sc;
}

export function envScope(S) {
  const c = S._c;
  if (c.envTick === S.tick && c.env) return c.env;
  const hr = hourOf(S);
  const px = S.stat.price || {};
  c.envTick = S.tick;
  c.env = {
    hour: hr, weekday: weekdayOf(S), day: S.day + 1, is_night: hr >= 22 || hr < 5, is_restday: weekdayOf(S) === 6, population: S.agents.length, treasury: S.gov.treasury,
    median_wage: S.stat.medianWage, median_wealth: S.stat.medianWealth, median_income: S.stat.medianIncome,
    price_food: px.food ?? 2, price_wood: px.wood ?? 2, price_goods: px.goods ?? 6, price_ale: px.ale ?? 3, guards: guardsOnStaff(S).length,
  };
  return c.env;
}
export const scopeFor = (S, a, ctx) => Object.assign({}, agentScope(S, a), envScope(S), ctx);

export function propertyScope(S, b) {
  const f = b.firmId ? firm(S, b.firmId) : null;
  const oh = b.owner === 'private' && b.ownerHh ? household(S, b.ownerHh) : null;
  const head = oh ? adultsOf(S, oh).sort((p, q) => q.age - p.age)[0] : null;
  const res = S.households.filter((h) => h.home === b.id);
  return Object.assign({}, envScope(S), {
    id: b.id, name: b.name, type: b.type, value: b.value, owner_type: f && f.owner === 'coop' ? 'coop' : b.owner, owner: head ? agentScope(S, head) : null,
    workers: f ? f.workers.length : 0, residents: res.reduce((n, h) => n + h.members.length, 0), is_rented: !!b.tenantHh, is_business: !!FIRM[b.type] && !FIRM[b.type].org, is_home: b.type === 'home',
  });
}

// ------------------------------------------------------------------- index

/** Rebuild the lookup tables from the active statute book. Call after any change. */
export function rebuildIndex(S) {
  const idx = { prohibit: {}, tax: {}, transfer: [], subsidy: {}, price: {}, require: {}, mint: [], works: [] };
  const g = S.gov;
  Object.assign(g, { system: 'Lordship', decision: 'decree', franchise: 'is_adult', threshold: 0.5, citizenRepeal: false, guardsTarget: 0, clerksTarget: 0, guardWage: 10 });
  if (g.forced) Object.assign(g, g.forced);
  const push = (o, k, e) => (o[k] || (o[k] = [])).push(e);
  for (const law of S.laws) {
    if (law.status !== 'active') continue;
    law.provisions.forEach((p, i) => {
      const e = { law, p, key: `${law.id}:${i}` };
      switch (p.type) {
        case 'prohibit': push(idx.prohibit, p.action, e); break;
        case 'tax': push(idx.tax, p.base, e); break;
        case 'transfer': idx.transfer.push(e); break;
        case 'subsidy': push(idx.subsidy, p.target, e); break;
        case 'price_control': push(idx.price, p.target, e); break;
        case 'require': push(idx.require, p.behavior, e); break;
        case 'mint': if (p.period !== 'once') idx.mint.push(e); break;
        case 'public_employment': idx.works.push(e); break;
        case 'government':
          if (!g.forced || law.enacted > g.forced.since) Object.assign(g, { system: p.system_name, decision: p.decision, franchise: p.franchise, threshold: p.threshold, citizenRepeal: p.citizen_repeal });
          break;
        case 'enforcement':
          if (p.guards >= 0) g.guardsTarget = p.guards;
          if (p.guard_wage >= 0) g.guardWage = p.guard_wage;
          if (p.clerks >= 0) g.clerksTarget = p.clerks;
          break;
      }
    });
  }
  S._c.idx = idx;
  // Public works crews follow their provisions.
  const wanted = new Map(idx.works.map((e) => [e.key, e]));
  for (const f of S.firms.filter((x) => x.type === 'works')) {
    const e = wanted.get(f.project?.key);
    if (!e) {
      for (const id of [...f.workers]) { const a = agent(S, id); if (a) { leaveJob(S, a); note(S, a, `The ${f.name} crew was disbanded.`, 'work'); } }
      S.firms = S.firms.filter((x) => x !== f);
      invalidate(S, 'fById');
    } else { f.cap = e.p.jobs; f.wageOffer = e.p.wage; wanted.delete(f.project.key); }
  }
  for (const [key, e] of wanted) {
    const site = S.buildings.find((b) => b.type === 'park') || S.buildings.find((b) => b.type === 'townhall');
    const f = newFirm(S, 'works', site, e.p.label || 'Public Works');
    f.project = { key, kind: e.p.project, law: e.law.id, applies_to: e.p.applies_to };
    f.cap = e.p.jobs; f.wageOffer = e.p.wage;
  }
  S._c.scopes.clear();
}

export const guardsOnStaff = (S) => { const f = firm(S, S.gov.watch); return f ? f.workers.map((id) => agent(S, id)).filter(Boolean) : []; };
export function guardsOnDuty(S) {
  const c = S._c;
  if (c.dutyTick !== undefined && S.tick - c.dutyTick < 5 && c.duty) return c.duty;
  c.dutyTick = S.tick;
  c.duty = guardsOnStaff(S).filter((g) => !g.jail && (g.act.type === 'work' || (g.act.type === 'walk' && g.next && g.next.type === 'work')));
  return c.duty;
}

const KIND = { buy: 'trade', sell: 'trade', consume: 'trade', produce: 'site', work: 'site', employ: 'site', be_outside: 'public', visit: 'public', protest: 'public', own_property: 'site', emigrate: 'gate', immigrate: 'gate', have_child: 'record', marry: 'record', steal: 'public', forage: 'public' };

/** Chance that an offence of a given kind at (x, y) is noticed by the authorities. */
export function detectProb(S, kind, x, y) {
  const duty = guardsOnDuty(S);
  let near = 0;
  for (const g of duty) if (Math.abs(g.x - x) + Math.abs(g.y - y) < 14) near++;
  const clerks = (firm(S, S.gov.office)?.workers.length) || 0;
  switch (kind) {
    case 'public': return Math.min(0.92, 0.015 + 0.3 * near + 0.01 * duty.length);
    case 'trade': return Math.min(0.5, 0.004 + 0.07 * near + 0.004 * duty.length);
    case 'paper': return Math.min(0.65, 0.015 + 0.02 * guardsOnStaff(S).length + 0.14 * clerks);
    case 'site': return Math.min(0.8, 0.02 + 0.05 * guardsOnStaff(S).length);
    case 'gate': return Math.min(0.8, 0.02 + 0.07 * duty.length);
    case 'record': return Math.min(0.95, 0.1 + 0.1 * guardsOnStaff(S).length + 0.25 * clerks);
  }
  return 0.05;
}

export function addImpact(a, law, coins) { a.impact[law.id] = (a.impact[law.id] || 0) + coins; }

/**
 * The compliance decision. A person breaks a law when what they stand to gain
 * outweighs the punishment they expect plus the weight of their own conscience.
 * Conscience is stronger in conformist people, under a government they trust,
 * for laws they agree with, and for laws their neighbours actually keep.
 */
export function decideViolate(S, a, law, { gain, fine = 0, pen = {}, pDetect = 0.1, extra = 0 }) {
  if (gain <= 0) return false;
  const daily = Math.max(4, a.incAvg || S.stat.medianWage || 8);
  const agree = a.opin[law.id] ?? 0;
  const flouted = law.violEMA || 0;
  const duty = a.t.conformity * (0.3 + 0.7 * a.trust) * (1 + 0.6 * agree) * (1 - 0.6 * flouted) * (law.passedBy === 'vote' ? 1.2 : 1);
  const moral = Math.max(0, duty) * (daily * 0.9 + gain * 0.8);
  const severity = fine + extra + (pen.jail_days || 0) * daily * 1.5 + (pen.exile ? daily * 40 : 0) + (pen.execute ? daily * 300 : 0);
  const fear = pDetect * severity * (1.7 - 1.3 * a.t.risk);
  return gain * rnd(S, 0.75, 1.25) > moral + fear;
}

function recordAct(S, a, action, ctx, value) {
  if (value <= 0) return;
  const acts = a.acts;
  acts.push({ a: action, c: { hour: hourOf(S), weekday: weekdayOf(S), ...ctx }, v: value, d: S.day });
  if (acts.length > 40) acts.shift();
}

/**
 * A person is about to do something. Returns:
 *   'ok'      - no law forbids it
 *   'refrain' - forbidden, and they choose to obey
 *   'violate' - forbidden, they do it anyway and get away with it
 *   'caught'  - forbidden, they tried, and were caught (the act does not happen)
 */
export function attempt(S, a, action, ctx, value, o = {}) {
  if (!o.silent) recordAct(S, a, action, ctx, value);
  const list = S._c.idx.prohibit[action];
  if (!list) return 'ok';
  const sc = scopeFor(S, a, ctx);
  let hit = null;
  for (const e of list) if (evalBool(e.p.condition, sc, false)) { hit = e; break; }
  if (!hit) return 'ok';
  const { law, p } = hit;
  const pd = clamp01(detectProb(S, o.kind || KIND[action], o.x ?? a.x, o.y ?? a.y) * (o.detectMult ?? 1));
  const fine = Math.max(0, evalNum(p.penalty.fine, sc));
  const extra = p.penalty.confiscate ? (ctx.price || 0) * (ctx.quantity || 0) : 0;
  if (o.mustComply || !decideViolate(S, a, law, { gain: value, fine, pen: p.penalty, pDetect: pd, extra })) {
    law.stats.refrained++; law.d.refrain++;
    a.frust += Math.min(value, 12);
    addImpact(a, law, -Math.min(value, 12) * 0.6);
    a.lastLaw = law.id;
    return 'refrain';
  }
  law.stats.violations++; law.d.viol++;
  S.stat.crimeToday++;
  a.lastLaw = law.id;
  if (chance(S, pd)) {
    const r = punish(S, a, law, p.penalty, o.why || describe(action, ctx), { sc });
    return r === 'bribed' ? 'violate' : 'caught';
  }
  return 'violate';
}

function describe(action, ctx) {
  switch (action) {
    case 'buy': return `buying ${ctx.good}`;
    case 'sell': return `selling ${ctx.good}`;
    case 'consume': return `drinking ${ctx.good}`;
    case 'produce': return `making ${ctx.good}`;
    case 'work': return 'working when forbidden';
    case 'employ': return 'unlawful hiring';
    case 'be_outside': return 'being out of doors';
    case 'visit': return `going to the ${ctx.place}`;
    case 'own_property': return 'unlawful ownership';
    case 'steal': return 'theft';
    case 'forage': return 'foraging';
    case 'have_child': return 'having a child';
    default: return action.replace('_', ' ');
  }
}

/** Carry out a penalty on someone who has been caught. */
export function punish(S, a, law, pen, why, o = {}) {
  const sc = o.sc || scopeFor(S, a, {});
  const h = hhOf(S, a);
  const fine = Math.max(0, evalNum(pen.fine, sc)) + (o.extraFine || 0);
  const daily = Math.max(4, a.incAvg || S.stat.medianWage);
  const severity = fine + (pen.jail_days || 0) * daily + (pen.exile || pen.execute ? 300 : 0);
  // A guard who is greedy, unscrupulous and badly paid may look the other way for a price.
  const duty = guardsOnDuty(S).filter((g) => g.id !== a.id);
  const g = duty.length ? pick(S, duty) : null;
  if (g && severity > 0 && !o.noBribe) {
    const underpaid = clamp(1.4 - g.job.wage / Math.max(1, S.stat.medianWage), 0.25, 1.3);
    const corrupt = g.t.greed * (1 - g.t.conformity) * underpaid * (1.2 - g.trust * 0.4);
    const bribe = Math.max(2, Math.min(severity * 0.4, h.money * 0.6));
    if (h.money >= bribe && chance(S, corrupt * 0.9) && chance(S, 0.4 + a.t.risk * 0.6)) {
      h.money -= bribe; hhOf(S, g).money += bribe; g.inc.other += bribe;
      law.stats.bribes = (law.stats.bribes || 0) + 1; S.gov.d.bribes = (S.gov.d.bribes || 0) + 1;
      addImpact(a, law, -bribe);
      a.trust = clamp01(a.trust - 0.03);
      note(S, a, `Caught ${why} under "${law.title}" - but ${fullName(g)} of the watch took ${bribe.toFixed(1)} coins to forget it.`, 'crime');
      note(S, g, `Took ${bribe.toFixed(1)} coins from ${fullName(a)} to overlook ${why}.`, 'crime');
      return 'bribed';
    }
  }
  a.record++;
  law.stats.punished++;
  const paid = Math.min(Math.max(0, h.money), fine);
  h.money -= paid; S.gov.treasury += paid; S.gov.d.revenue += paid; S.gov.d.fines += paid; law.stats.fines += paid;
  let jail = (pen.jail_days || 0) + (fine > paid + 0.5 ? Math.min(10, Math.ceil((fine - paid) / 6)) : 0);
  jail = Math.min(60, jail);
  addImpact(a, law, -(paid + jail * daily));
  const disagrees = (a.opin[law.id] ?? 0) < 0;
  a.trust = clamp01(a.trust - 0.03 - (disagrees ? 0.07 : 0));
  a.mood -= 6 + jail * 2;
  const kin = [a.spouse, ...a.parents, ...a.kids].map((id) => id && agent(S, id)).filter(Boolean);
  if (pen.execute) {
    chronicle(S, 'crime', `${fullName(a)} was executed under "${law.title}" for ${why}.`, { agents: [a.id] });
    for (const k of kin) { k.trust = clamp01(k.trust - 0.35); k.mood -= 40; addImpact(k, law, -40); note(S, k, `${fullName(a)} was executed. I will not forget it.`, 'grief'); }
    for (const id of Object.keys(a.friends)) { const fr = agent(S, id); if (fr) { fr.trust = clamp01(fr.trust - 0.1); fr.mood -= 12; } }
    die(S, a, 'executed');
    return 'executed';
  }
  if (pen.exile) {
    chronicle(S, 'crime', `${fullName(a)} was banished under "${law.title}" for ${why}.`, { agents: [a.id] });
    for (const k of kin) { k.trust = clamp01(k.trust - 0.15); k.mood -= 20; addImpact(k, law, -15); }
    exileAgent(S, a, `Banished for ${why}.`);
    return 'exiled';
  }
  const parts = [];
  if (paid > 0) parts.push(`fined ${paid.toFixed(1)} coins`);
  if (jail > 0) parts.push(`jailed for ${jail} day${jail > 1 ? 's' : ''}`);
  note(S, a, `Caught ${why} under "${law.title}": ${parts.join(' and ') || 'let off with a warning'}.`, 'crime');
  if (jail >= 3 || fine >= 30) chronicle(S, 'crime', `${fullName(a)} was ${parts.join(' and ')} for ${why}.`, { agents: [a.id] });
  for (const k of kin) if (jail > 0) { k.mood -= 4; k.trust = clamp01(k.trust - (disagrees ? 0.02 : 0)); }
  if (jail > 0) jailAgent(S, a, jail);
  return 'punished';
}

// ----------------------------------------------------- enacting & repealing

export function newLawRecord(S, compiled, text) {
  const { law, errors } = validateLaw(compiled);
  if (errors.length) throw new Error(`Invalid law: ${errors.join('; ')}`);
  return Object.assign(law, {
    id: `L${S.nextLaw++}`, text: text || law.summary, status: 'draft', proposed: S.day, enacted: null, repealed: null, passedBy: null,
    stats: { revenue: 0, paid: 0, violations: 0, refrained: 0, punished: 0, fines: 0, evaded: 0, bribes: 0 }, d: { viol: 0, refrain: 0 }, violEMA: 0, approval: 0, baseline: null, vote: null,
  });
}

/** The player proposes a law. Under decree it takes effect now; under a vote it goes to the people. */
export function proposeLaw(S, compiled, text, o = {}) {
  const law = newLawRecord(S, compiled, text);
  S.laws.push(law);
  for (const a of S.agents) if (isAdult(a)) a.opin[law.id] = opinionOnProposal(S, a, law);
  if (S.gov.decision === 'decree' || o.force) { enactLaw(S, law, 'decree'); return law; }
  law.status = 'voting';
  callVote(S, law, 'enact');
  return law;
}

export function opinionOnProposal(S, a, law) {
  let ideo = 0;
  for (const k in a.v) ideo += a.v[k] * (law.ideology[k] || 0);
  const self = staticImpact(S, a, law, true) / Math.max(3, a.incAvg + 2);
  return clamp(0.55 * clamp(ideo / 1.2, -1, 1) + 0.8 * Math.tanh(self * 2.5), -1, 1);
}

export function callVote(S, law, kind) {
  const day = hourOf(S) < 11 ? S.day : S.day + 1;
  const vote = { law: law.id, kind, day, yes: 0, no: 0, done: false };
  S.gov.votes.push(vote);
  law.vote = vote;
  chronicle(S, 'politics', `A vote is called for Day ${day + 1}: ${kind === 'enact' ? 'shall' : 'shall the town repeal'} "${law.title}"${kind === 'enact' ? ' become law?' : '?'}`, { law: law.id });
  return vote;
}

export function canVote(S, a) { return isAdult(a) && evalBool(S.gov.franchise, scopeFor(S, a, {}), false); }

export function tallyVotes(S) {
  for (const v of S.gov.votes) {
    if (v.done || v.day > S.day) continue;
    v.done = true;
    const law = S.laws.find((l) => l.id === v.law);
    if (!law) continue;
    const cast = v.yes + v.no;
    const passed = cast > 0 && v.yes / cast > S.gov.threshold;
    if (v.kind === 'enact') {
      if (law.status !== 'voting') continue;
      if (passed) { chronicle(S, 'politics', `"${law.title}" passed, ${v.yes} to ${v.no}.`, { law: law.id }); enactLaw(S, law, 'vote'); }
      else { law.status = 'rejected'; chronicle(S, 'politics', `"${law.title}" was voted down, ${v.no} to ${v.yes}.`, { law: law.id }); }
    } else if (law.status === 'active') {
      if (passed) { chronicle(S, 'politics', `The town voted ${v.yes} to ${v.no} to repeal "${law.title}".`, { law: law.id }); repealLaw(S, law.id, 'vote'); }
      else chronicle(S, 'politics', `The vote to repeal "${law.title}" failed, ${v.no} to ${v.yes}.`, { law: law.id });
    }
  }
  S.gov.votes = S.gov.votes.filter((v) => !v.done || S.day - v.day < 3);
}

export function enactLaw(S, law, by) {
  law.status = 'active';
  law.enacted = S.day;
  law.passedBy = by;
  law.baseline = { day: S.day, ...snapshot(S) };
  for (const a of S.agents) a.happyAt = a.happyAt || {}, a.happyAt[law.id] = a.happy;
  rebuildIndex(S);
  chronicle(S, 'law', `"${law.title}" is now law${by === 'vote' ? ' by vote of the people' : by === 'decree' ? ' by decree' : ''}.`, { law: law.id });
  for (const p of law.provisions) {
    if (p.type === 'property_transfer') executeTransfer(S, law, p);
    else if (p.type === 'mint' && p.period === 'once') { S.gov.treasury += p.amount; chronicle(S, 'economy', `${Math.round(p.amount)} new coins were struck for the treasury.`); }
    else if (p.type === 'declaration' && p.rename_town) { chronicle(S, 'law', `${S.townName} shall henceforth be known as ${p.rename_town}.`); S.townName = p.rename_town; }
    else if (p.type === 'repeal') for (const id of p.law_ids) if (id !== law.id) repealLaw(S, id, 'law');
  }
  rebuildIndex(S);
  return law;
}

export function repealLaw(S, id, how = 'decree') {
  const law = S.laws.find((l) => l.id === id);
  if (!law || law.status !== 'active') return false;
  law.status = 'repealed';
  law.repealed = S.day;
  rebuildIndex(S);
  if (how !== 'vote') chronicle(S, 'law', `"${law.title}" has been repealed${how === 'uprising' ? ' by the crowd' : ''}.`, { law: law.id });
  return true;
}

/** The player asks to repeal. Under a vote regime the people decide. */
export function requestRepeal(S, id) {
  const law = S.laws.find((l) => l.id === id);
  if (!law || law.status !== 'active') return null;
  if (S.gov.decision === 'decree') { repealLaw(S, id); return 'repealed'; }
  if (law.vote && !law.vote.done) return 'voting';
  callVote(S, law, 'repeal');
  return 'voting';
}

export function snapshot(S) {
  const s = S.stat;
  return { pop: S.agents.length, happy: s.happy, approval: s.approval, medianWage: s.medianWage, medianWealth: s.medianWealth, unemployment: s.unemployment, gini: s.gini, treasury: S.gov.treasury, food: s.price?.food ?? 2, goods: s.price?.goods ?? 6, crime: s.crimeAvg || 0 };
}

// ---------------------------------------------------------------- property

/** Hand a building to a new owner. `to` is { owner: 'state' | 'coop' } or { owner: 'private', hh }. */
export function setOwner(S, b, to) {
  const f = b.firmId ? firm(S, b.firmId) : null;
  const res = S.households.find((h) => h.home === b.id);
  b.owner = to.owner === 'coop' ? 'private' : to.owner;
  b.ownerHh = to.owner === 'private' ? to.hh : null;
  if (to.owner === 'coop') b.owner = 'coop';
  if (b.type === 'home') {
    if (res && !(to.owner === 'private' && to.hh === res.id)) { b.tenantHh = res.id; if (!b.rent) b.rent = S.stat.medianRent || 1.6; }
    else b.tenantHh = null;
    b.name = res ? `${res.name} House` : 'Vacant House';
  }
  if (f && !FIRM[f.type].org) {
    const oldMgr = f.manager ? agent(S, f.manager) : null;
    f.owner = to.owner; f.ownerHh = to.owner === 'private' ? to.hh : null;
    f.illicit = false; f.capBreak = false; f.offBooks = false;
    const wage = Math.max(f.wageOffer, S.stat.medianWage);
    if (to.owner === 'private') {
      const nh = household(S, to.hh);
      let boss = adultsOf(S, nh).filter((p) => p.age < 68 && !p.jail && (!p.job || !p.job.owner)).sort((p, q) => q.skill - p.skill)[0];
      if (oldMgr && oldMgr.hh === to.hh) boss = oldMgr;
      if (oldMgr && oldMgr !== boss && oldMgr.job) { oldMgr.job.owner = false; oldMgr.job.wage = wage; }
      if (boss) {
        if (!boss.job || boss.job.firm !== f.id) { leaveJob(S, boss); boss.job = { firm: f.id, wage: 0, role: FIRM[f.type].job, since: S.day, owner: true }; f.workers.push(boss.id); }
        boss.job.owner = true; boss.job.wage = 0;
        f.manager = boss.id;
      }
    } else if (oldMgr && oldMgr.job) {
      oldMgr.job.owner = false;
      oldMgr.job.wage = to.owner === 'state' ? wage * 1.25 : wage;
    }
    f.open = true;
  }
  S._c.scopes.clear();
}

function executeTransfer(S, law, p) {
  const picked = S.buildings.filter((b) => b.type !== 'park' && evalBool(p.select, propertyScope(S, b), false));
  if (!picked.length) { chronicle(S, 'law', `"${law.title}": no property matched, so nothing changed hands.`, { law: law.id }); return; }
  const guards = guardsOnStaff(S).length;
  let moved = 0, resisted = 0;
  for (const b of picked) {
    const oh = b.owner === 'private' && b.ownerHh ? household(S, b.ownerHh) : null;
    const f = b.firmId ? firm(S, b.firmId) : null;
    // Work out the recipient.
    let to = null;
    if (p.to === 'state') to = { owner: 'state' };
    else if (p.to === 'agent') { const t = agent(S, p.to_agent_id); if (t) to = { owner: 'private', hh: t.hh }; }
    else if (p.to === 'occupants') {
      if (f && !FIRM[f.type].org) to = { owner: 'coop' };
      else { const res = S.households.find((h) => h.home === b.id); if (res) to = { owner: 'private', hh: res.id }; }
    } else if (p.to === 'landless') {
      const res = S.households.find((h) => h.home === b.id);
      const landless = S.households.filter((h) => !propertiesOf(S, h).length && adultsOf(S, h).length);
      let nh = res && landless.includes(res) ? res : null;
      if (!nh && f) nh = landless.find((h) => h.members.some((id) => f.workers.includes(id)));
      if (!nh) nh = landless.sort((x, y) => x.money - y.money)[0];
      if (nh) to = { owner: 'private', hh: nh.id };
    } else if (p.to === 'auction') {
      const bidders = S.households.filter((h) => h !== oh && h.money > b.value * 0.5 && adultsOf(S, h).length).sort((x, y) => y.money - x.money);
      if (bidders[0]) { const price = Math.min(bidders[0].money * 0.8, b.value * 1.1); bidders[0].money -= price; S.gov.treasury += price; S.gov.d.revenue += price; to = { owner: 'private', hh: bidders[0].id }; }
    }
    if (!to) continue;
    if (to.owner === b.owner && (to.hh || null) === (b.ownerHh || null)) continue;
    // The losing owner may refuse; without guards the state cannot make them.
    if (oh) {
      const head = adultsOf(S, oh).sort((x, y) => y.age - x.age)[0];
      const comp = Math.min(S.gov.treasury, b.value * p.compensation);
      const loss = b.value - comp;
      if (head && loss > 0) {
        const defy = guards === 0 || decideViolate(S, head, law, { gain: loss, pen: { jail_days: 5 }, pDetect: Math.min(0.95, 0.3 + 0.2 * guards) });
        if (defy && guards === 0) {
          resisted++;
          note(S, head, `They say ${b.name} is no longer mine under "${law.title}". There is no one to make me give it up.`, 'law');
          for (const m of adultsOf(S, oh)) m.trust = clamp01(m.trust - 0.1);
          continue;
        }
        if (defy) { head.record++; jailAgent(S, head, 3); note(S, head, `Refused to hand over ${b.name}. The watch took it anyway and locked me up.`, 'crime'); law.stats.violations++; law.stats.punished++; }
      }
      S.gov.treasury -= comp; S.gov.d.spending += comp; oh.money += comp;
      for (const m of adultsOf(S, oh)) {
        addImpact(m, law, -loss / 3);
        m.trust = clamp01(m.trust - Math.min(0.3, (loss / Math.max(60, householdWealth(S, oh) + loss)) * 0.5));
        m.mood -= Math.min(30, loss / 10);
        note(S, m, comp >= b.value * 0.95 ? `The state bought ${b.name} from us for ${comp.toFixed(0)} coins under "${law.title}".` : `${b.name} was taken from us under "${law.title}"${comp > 0 ? ` for only ${comp.toFixed(0)} coins` : ''}.`, 'law');
      }
    }
    setOwner(S, b, to);
    moved++;
    if (to.owner === 'private') {
      for (const m of adultsOf(S, household(S, to.hh))) { addImpact(m, law, b.value / 6); m.mood += 10; note(S, m, `${b.name} is ours now, under "${law.title}".`, 'law'); }
    } else if (to.owner === 'coop' && f) {
      for (const id of f.workers) { const w = agent(S, id); if (w) { addImpact(w, law, b.value / f.workers.length / 6); note(S, w, `${f.name} now belongs to those of us who work it.`, 'law'); } }
    }
  }
  const dest = { state: 'the state', agent: 'a new owner', occupants: 'their occupants', auction: 'the highest bidders', landless: 'the landless' }[p.to];
  chronicle(S, 'law', `"${law.title}": ${moved} propert${moved === 1 ? 'y' : 'ies'} passed to ${dest}${resisted ? `; ${resisted} owner${resisted > 1 ? 's' : ''} refused and no guard could compel them` : ''}.`, { law: law.id });
}

// ------------------------------------------------- price limits & subsidies

/** Legal bounds on a price for this person/transaction: { min, max, e } (e is the binding entry). */
export function priceLimit(S, target, sc) {
  const list = S._c.idx.price[target];
  if (!list) return null;
  let min = -Infinity, max = Infinity, e = null;
  for (const x of list) {
    if (!evalBool(x.p.applies_to, sc, true)) continue;
    if (x.p.min !== '') { const v = evalNum(x.p.min, sc, NaN); if (v > min) { min = v; e = x; } }
    if (x.p.max !== '') { const v = evalNum(x.p.max, sc, NaN); if (v < max) { max = v; e = x; } }
  }
  return e ? { min, max, e } : null;
}

/** Fraction of a purchase the treasury covers for this buyer. */
export function subsidyFor(S, a, target) {
  const list = S._c.idx.subsidy[target];
  if (!list) return null;
  const sc = scopeFor(S, a, {});
  let best = 0, e = null;
  for (const x of list) {
    if (!evalBool(x.p.applies_to, sc, true)) continue;
    const fr = clamp01(evalNum(x.p.fraction, sc));
    if (fr > best) { best = fr; e = x; }
  }
  return e ? { fr: best, e } : null;
}

/** Take a subsidy out of the treasury if it can afford it. Returns the amount covered. */
export function drawSubsidy(S, a, target, gross) {
  const s = subsidyFor(S, a, target);
  if (!s) return 0;
  const amt = Math.min(S.gov.treasury, gross * s.fr);
  if (amt <= 0) return 0;
  S.gov.treasury -= amt; S.gov.d.spending += amt;
  s.e.law.stats.paid += amt;
  addImpact(a, s.e.law, amt);
  return amt;
}

export function requirementsFor(S, a, behavior) {
  const list = S._c.idx.require[behavior];
  if (!list) return null;
  const sc = scopeFor(S, a, {});
  for (const e of list) if (evalBool(e.p.applies_to, sc, false)) return e;
  return null;
}

export { mean };
