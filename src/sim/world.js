// Founding the town: people, households, firms and who owns what.
import { rand, rnd, rint, chance, pick, gauss, shuffle, clamp, clamp01, DAY } from './util.js';
import { generateMap, slotIn } from './map.js';
import { FEMALE, MALE, SURNAMES, TOWN_NAMES } from './names.js';
import { FIRM, BASE_PRICE, initCache, agent, household, building, invalidate } from './state.js';

export function newAgent(S, { sex, age, surname, hh, name }) {
  sex = sex || (chance(S, 0.5) ? 'female' : 'male');
  const tr = () => clamp01(gauss(S, 0.5, 0.2));
  const va = () => clamp(gauss(S, 0, 0.45), -1, 1);
  const a = {
    id: `a${S.nextId++}`, name: name || pick(S, sex === 'female' ? FEMALE : MALE), surname, sex, age, hh,
    t: { conformity: tr(), greed: tr(), risk: tr(), altruism: tr(), diligence: clamp01(gauss(S, 0.6, 0.18)), sociability: tr(), temperance: tr(), thrift: tr() },
    v: { liberty: va(), equality: va(), tradition: va(), order: va(), prosperity: va(), community: va() },
    hunger: rnd(S, 10, 40), energy: rnd(S, 70, 100), social: rnd(S, 50, 80), fun: rnd(S, 50, 80), health: clamp(gauss(S, 85 - Math.max(0, age - 45) * 0.6, 8), 30, 100),
    happy: 60, trust: 0.5, griev: 0,
    job: null, edu: 0, skill: 0, wake: rnd(S, 5.4, 6.4), bed: rnd(S, 21, 22.8),
    x: 0, y: 0, inside: null, path: null, pathI: 0, act: { type: 'idle', until: 0, desc: 'Getting up' }, next: null, dest: null,
    spouse: null, parents: [], kids: [], friends: {},
    inc: { wage: 0, profit: 0, rent: 0, transfer: 0, other: 0 }, incAvg: 0, nonLabor: 0, taxPaid: 0,
    log: [], thought: '',
    opin: {}, impact: {}, impactAvg: {}, acts: [], frust: 0, frustAvg: 0, mood: 0,
    evade: {}, owed: {}, record: 0, jail: null, immigrant: false, born: S.day, enrolled: false,
    worked: 0, drinks: 0, hung: 0, idleDays: 0, sadDays: 0, protest: false, voted: {}, labor: 0, lastMeal: 0, meals: 0,
  };
  a.v.order = clamp(a.v.order + (a.t.conformity - 0.5) * 0.8, -1, 1);
  a.v.liberty = clamp(a.v.liberty + (a.t.risk - 0.5) * 0.5 - (a.t.conformity - 0.5) * 0.4, -1, 1);
  a.v.community = clamp(a.v.community + (a.t.altruism - 0.5) * 0.8, -1, 1);
  a.v.prosperity = clamp(a.v.prosperity + (a.t.greed - 0.5) * 0.7, -1, 1);
  a.v.tradition = clamp(a.v.tradition + (age - 35) * 0.008, -1, 1);
  a.trust = clamp01(0.45 + (a.t.conformity - 0.5) * 0.4 + a.v.order * 0.1);
  a.edu = age < 6 ? 0 : clamp01(gauss(S, Math.min(0.55, (age - 6) * 0.055), 0.15));
  a.skill = clamp01((age - 16) / 25);
  S.agents.push(a);
  invalidate(S, 'aById');
  return a;
}

export function newHousehold(S, surname) {
  const h = { id: `h${S.nextId++}`, name: surname, members: [], home: null, money: 0, inv: { food: 0, goods: 0, ale: 0, wood: 0 }, arrears: 0, spend: 0 };
  S.households.push(h);
  invalidate(S, 'hById');
  return h;
}

export function newFirm(S, type, b, name) {
  const def = FIRM[type];
  const f = {
    id: `f${S.nextId++}`, type, b: b.id, name: name || b.name, owner: def.org ? 'state' : 'private', ownerHh: null, manager: null,
    good: def.good, cash: 0, stock: 0, input: 0, price: BASE_PRICE[def.good] || 0, wageOffer: 9, workers: [], vacancies: 0, cap: def.cap,
    open: true, illicit: false, suspended: false, offBooks: false, capBreak: false,
    d: { sold: 0, rev: 0, prod: 0, wages: 0, inCost: 0, tax: 0, served: 0 }, avgSales: 0, avgProfit: 0, unfilled: 0, lossDays: 0, eff: 0, project: null, progress: 0,
  };
  S.firms.push(f);
  if (!def.org || type !== 'works') b.firmId = f.id;
  invalidate(S, 'fById');
  return f;
}

export function hire(S, f, a, wage, role) {
  if (a.job) leaveJob(S, a);
  a.job = { firm: f.id, wage, role: role || FIRM[f.type].job, since: S.day, owner: false };
  if (!f.workers.includes(a.id)) f.workers.push(a.id);
  a.idleDays = 0;
  S._c.scopes.delete(a.id);
}
export function leaveJob(S, a) {
  if (!a.job) return;
  const f = S.firms.find((x) => x.id === a.job.firm);
  if (f) f.workers = f.workers.filter((id) => id !== a.id);
  a.job = null;
  S._c.scopes.delete(a.id);
}

function makeFamily(S, kind) {
  const surname = S.surnames.pop() || pick(S, SURNAMES);
  const h = newHousehold(S, surname);
  const add = (opts) => { const a = newAgent(S, { ...opts, surname, hh: h.id }); h.members.push(a.id); return a; };
  const couple = (lo, hi) => {
    const age = rnd(S, lo, hi);
    const p1 = add({ sex: 'female', age: clamp(age + rnd(S, -3, 3), 17, 85) });
    const p2 = add({ sex: 'male', age: clamp(age + rnd(S, -3, 4), 17, 85) });
    p1.spouse = p2.id; p2.spouse = p1.id;
    return [p1, p2];
  };
  const kids = (parents, n) => {
    const youngest = Math.min(...parents.map((p) => p.age));
    for (let i = 0; i < n; i++) {
      const age = rnd(S, 0.5, Math.min(15.5, youngest - 18));
      if (age < 0.5) continue;
      const k = add({ age });
      k.parents = parents.map((p) => p.id);
      parents.forEach((p) => p.kids.push(k.id));
    }
  };
  if (kind === 'family') { const ps = couple(24, 46); kids(ps, rint(S, 1, 3)); }
  else if (kind === 'couple') couple(19, 55);
  else if (kind === 'single') add({ age: rnd(S, 18, 50) });
  else if (kind === 'elders') { if (chance(S, 0.6)) couple(61, 76); else add({ age: rnd(S, 61, 80) }); }
  else if (kind === 'parent') { const p = add({ age: rnd(S, 26, 45) }); kids([p], rint(S, 1, 2)); }
  return h;
}

export function createWorld(seed = 1) {
  const S = {
    version: 1, seed, rngState: seed | 0, tick: 6 * 60, day: 0, nextId: 1, townName: '',
    map: null, buildings: [], firms: [], households: [], agents: [], graves: 0,
    gov: null, laws: [], events: [], history: [], nextLaw: 1,
    stat: { medianWage: 9, medianWealth: 100, medianIncome: 9, unemployment: 0, happy: 60, approval: 0.2, gini: 0.3, crimeToday: 0 },
    trader: { bought: 0, sold: 0 }, births: 0, deaths: 0, arrivals: 0, departures: 0,
  };
  initCache(S);
  for (let i = 0; i < 10; i++) rand(S);
  S.townName = pick(S, TOWN_NAMES);
  S.surnames = shuffle(S, [...SURNAMES]);
  const { place } = generateMap(S);
  const center = (x, y) => -Math.abs(Math.hypot(x - 38, (y - 27) * 1.2) - 13);
  for (let i = 0; i < 31; i++) place('home', center, 'House');

  // Firms.
  for (const b of S.buildings) if (FIRM[b.type]) newFirm(S, b.type, b);
  const guardhouse = S.buildings.find((b) => b.type === 'guardhouse');
  const townhall = S.buildings.find((b) => b.type === 'townhall');
  const watch = newFirm(S, 'watch', guardhouse, 'Town Watch');
  const office = newFirm(S, 'office', townhall, "Steward's Office");
  for (const b of [guardhouse, townhall, ...S.buildings.filter((x) => ['market', 'chapel', 'park'].includes(x.type))]) b.owner = 'state';

  S.gov = {
    treasury: 260, system: 'Lordship', decision: 'decree', franchise: 'is_adult', threshold: 0.5, citizenRepeal: false,
    guardsTarget: 0, guardWage: 10, clerksTarget: 0, clerkWage: 10, watch: watch.id, office: office.id,
    amenity: 0, votes: [], unrest: { share: 0, streak: 0, last: 0 }, mint: [], publicJobs: [],
    d: { revenue: 0, spending: 0, fines: 0 }, shortfall: 0, uprisings: 0,
  };

  // Households.
  const kinds = [];
  const mix = { family: 14, couple: 4, single: 5, elders: 4, parent: 2 };
  for (const [k, n] of Object.entries(mix)) for (let i = 0; i < n; i++) kinds.push(k);
  shuffle(S, kinds);
  // Make sure the first households can run the firms.
  kinds.sort((a, b) => (a === 'elders') - (b === 'elders'));
  const hhs = kinds.map((k) => makeFamily(S, k));
  const homes = S.buildings.filter((b) => b.type === 'home');
  hhs.forEach((h, i) => { const b = homes[i]; if (b) { h.home = b.id; b.name = `${h.name} House`; } });

  const workers = (h) => h.members.map((id) => agent(S, id)).filter((a) => a.age >= 17 && a.age < 62);
  const firms = S.firms.filter((f) => !FIRM[f.type].org);
  const order = ['clinic', 'school', 'farm', 'farm', 'farm', 'farm', 'farm', 'lumberyard', 'workshop', 'workshop', 'brewery', 'tavern'];
  const ownerHhs = [];
  let hi = 0;
  for (const type of order) {
    const f = firms.find((x) => x.type === type && !x.ownerHh);
    while (hi < hhs.length && !workers(hhs[hi]).length) hi++;
    const h = hhs[hi++];
    if (!f || !h) continue;
    const b = building(S, f.b);
    f.ownerHh = h.id; b.owner = 'private'; b.ownerHh = h.id;
    const boss = workers(h).sort((p, q) => q.age - p.age)[0];
    f.manager = boss.id;
    hire(S, f, boss, 0);
    boss.job.owner = true;
    boss.skill = clamp01(boss.skill + 0.3);
    if (type === 'clinic' || type === 'school') boss.edu = clamp01(0.7 + rand(S) * 0.25);
    boss.v.prosperity = clamp(boss.v.prosperity + 0.3, -1, 1);
    boss.v.equality = clamp(boss.v.equality - 0.25, -1, 1);
    h.money = rnd(S, 160, 300);
    f.cash = 90;
    ownerHhs.push(h);
    if (type === 'tavern' || type === 'brewery') b.name = f.name;
    else if (type === 'farm') f.name = b.name;
    else if (type !== 'lumberyard') { f.name = b.name = type === 'workshop' ? `${h.name} Workshop` : type === 'clinic' ? `${h.name} Clinic` : `${h.name} School`; }
  }
  // Homes: two landlord families hold the rented stock.
  const landlords = shuffle(S, ownerHhs.slice(2)).slice(0, 2);
  hhs.forEach((h, i) => {
    const b = building(S, h.home);
    if (!b) return;
    const owns = ownerHhs.includes(h) || rand(S) < 0.58;
    b.owner = 'private';
    if (owns) b.ownerHh = h.id;
    else { b.ownerHh = landlords[i % 2].id; b.tenantHh = h.id; b.rent = 1.6; }
    if (!h.money) h.money = rnd(S, 25, 90);
    h.inv.food = h.members.length * rnd(S, 2, 3.5);
    h.inv.goods = h.members.length * rnd(S, 0.6, 1.2);
  });
  homes.slice(hhs.length).forEach((b, i) => { b.owner = 'private'; b.ownerHh = landlords[i % 2].id; b.rent = 1.6; b.name = 'Vacant House'; });

  // Jobs for everyone else.
  const staff = { farm: 3, lumberyard: 3, workshop: 3, brewery: 2, tavern: 2, clinic: 1, school: 1 };
  const pool = shuffle(S, S.agents.filter((a) => a.age >= 16 && a.age < 62 && !a.job));
  for (const f of firms) {
    const need = staff[f.type] || 0;
    for (let i = 0; i < need && pool.length; i++) {
      let k = pool.length - 1;
      if (f.type === 'clinic' || f.type === 'school') k = pool.reduce((best, a, j) => (a.edu > pool[best].edu ? j : best), 0);
      const [a] = pool.splice(k, 1);
      hire(S, f, a, clamp(gauss(S, 9, 0.8), 7, 11));
    }
    f.stock = f.type === 'farm' ? 30 : f.type === 'tavern' ? 0 : 12;
    f.input = FIRM[f.type].input ? 14 : 0;
    f.avgSales = f.type === 'farm' ? 18 : 8;
  }
  S.unassigned = pool.map((a) => a.id); // guards and clerks are hired from here by the founding laws

  // Friendships among neighbours and workmates give everyone someone to visit.
  for (const a of S.agents) {
    if (a.age < 6) continue;
    const peers = S.agents.filter((b) => b !== a && b.hh !== a.hh && Math.abs(b.age - a.age) < (a.age < 16 ? 4 : 18) && (b.age < 16) === (a.age < 16));
    for (let i = 0; i < 3 && peers.length; i++) {
      const b = pick(S, peers);
      const aff = rnd(S, 0.3, 0.8);
      a.friends[b.id] = Math.max(a.friends[b.id] || 0, aff);
      b.friends[a.id] = Math.max(b.friends[a.id] || 0, aff);
    }
  }
  for (const a of S.agents) placeAtHome(S, a);
  delete S.surnames;
  return S;
}

export function placeAtHome(S, a) {
  const h = household(S, a.hh);
  const b = h && h.home ? building(S, h.home) : null;
  if (b) { const p = slotIn(b, parseInt(a.id.slice(1), 10)); a.x = p.x; a.y = p.y; a.inside = b.id; }
  else { a.x = S.map.plaza.x + rnd(S, 0, 8); a.y = S.map.plaza.y + rnd(S, 0, 6); a.inside = null; }
  a.path = null;
  a.act = { type: 'idle', until: S.tick + rint(S, 1, 20), desc: 'At home' };
}

export { DAY };
