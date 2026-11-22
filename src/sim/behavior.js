// What each person does with their day. Nobody follows a script: every time an
// activity ends, the person looks at the hour, their needs, their purse and
// the law, and picks what to do next.
import { evalBool, evalNum } from './expr.js';
import { rand, rnd, rint, chance, pick, clamp, clamp01, hourOf, weekdayOf, mean, DAY } from './util.js';
import { findPath, slotIn, MAP_W, tileXY } from './map.js';
import { FIRM, BASE_PRICE, agent, household, building, firm, hhOf, adultsOf, membersOf, note, chronicle, fullName, isAdult } from './state.js';
import { attempt, scopeFor, decideViolate, detectProb, punish, requirementsFor, canVote, opinionOnProposal, addImpact } from './legal.js';
import { buyGood, buyService, marketPrice, importPrice, bestVacancy, applyFor, reservationWage, staffPresent } from './economy.js';
import { expectedWageTax } from './fiscal.js';
import { removeAgent } from './society.js';

const FOREVER = 1e12;
const num = (id) => parseInt(id.slice(1), 10);
const byType = (S, type) => (S._c.day[`b_${type}`] ||= S.buildings.find((b) => b.type === type));
const firmOfType = (S, type) => S.firms.find((f) => f.type === type);
const homeOf = (S, a) => { const h = hhOf(S, a); return h && h.home ? building(S, h.home) : null; };
const inWindow = (hr, start, end) => (start <= end ? hr >= start && hr < end : hr >= start || hr < end);
const hoursUntil = (hr, target) => ((target - hr + 24) % 24);
const mealName = (hr) => (hr < 10 ? 'Breakfast' : hr < 15.5 ? 'Dinner' : 'Supper');
const mealtime = (hr) => (hr >= 5 && hr < 8.5) || (hr >= 11.7 && hr < 14) || (hr >= 17.3 && hr < 20.5);
const marketOpen = (hr) => hr >= 7 && hr < 19;

// ---------------------------------------------------------------- movement

function settle(S, a, b, then) {
  if (b) { a.inside = b.id; const p = then.spot || slotIn(b, num(a.id) + (then.type === 'sleep' ? 0 : then.type.length)); a.x = p.x; a.y = p.y; }
  else if (then.spot) { a.x = then.spot.x; a.y = then.spot.y; }
  a.path = null; a.next = null; a.dest = null;
  a.act = then;
  then.until = S.tick + Math.max(1, Math.round(then.dur || 30));
  onStart(S, a);
}

/** Walk to a building ({ b }) or a spot ({ x, y, label }) and then begin `then`. */
export function goTo(S, a, target, then) {
  const b = target.b || null;
  if (b && a.inside === b.id) { settle(S, a, b, then); return true; }
  let sx, sy;
  if (a.inside) { const cur = building(S, a.inside); sx = cur.door.x; sy = cur.door.y; } else { sx = Math.round(a.x); sy = Math.round(a.y); }
  const tx = b ? b.door.x : Math.round(target.x), ty = b ? b.door.y : Math.round(target.y);
  a.inside = null; a.x = sx; a.y = sy;
  a.path = findPath(S, sx, sy, tx, ty); a.pathI = 0;
  a.dest = b ? { b: b.id } : { x: tx, y: ty };
  a.next = then;
  a.act = { type: 'walk', until: FOREVER, desc: then.walk || `Walking to ${target.label || b.name}` };
  if (a.path.length <= 1) arrive(S, a);
  return true;
}
function arrive(S, a) {
  const d = a.dest;
  settle(S, a, d && d.b ? building(S, d.b) : null, a.next || { type: 'idle', dur: 10, desc: 'Pausing' });
}
function advance(S, a) {
  let step = a.age < 10 ? 0.7 : a.age > 66 ? 0.72 : 0.95;
  const p = a.path;
  while (step > 0 && a.pathI < p.length - 1) {
    const [nx, ny] = tileXY(p[a.pathI + 1]);
    const dx = nx - a.x, dy = ny - a.y;
    const d = Math.hypot(dx, dy);
    if (d <= step) { a.x = nx; a.y = ny; a.pathI++; step -= d; }
    else { a.x += (dx / d) * step; a.y += (dy / d) * step; step = 0; }
  }
  if (a.pathI >= p.length - 1) arrive(S, a);
}
function start(S, a, then) { settle(S, a, a.inside ? building(S, a.inside) : null, then); return true; }

/** Set out for somewhere, unless the law keeps this person indoors and they obey. */
function travel(S, a, target, then, value = 2) {
  if (target.b && a.inside === target.b.id) return goTo(S, a, target, then);
  if (S._c.idx.prohibit.be_outside) {
    const r = attempt(S, a, 'be_outside', { place: 'street' }, value, { kind: 'public' });
    if (r === 'refrain') { a.thought = 'I would go, but we are not allowed out now.'; return false; }
    if (r === 'caught') return !!a.jail || !!a.leaving || a.dead;
  }
  return goTo(S, a, target, then);
}

function placeTarget(S, a, place) {
  if (place === 'plaza') { const p = S.map.plaza; return { x: p.x + rint(S, 1, p.w - 2), y: p.y + rint(S, 1, p.h - 2), label: 'the plaza' }; }
  if (place === 'forest') { const [x, y] = tileXY(pick(S, S.map.forage)); return { x, y, label: 'the forest' }; }
  if (place === 'home') { const b = homeOf(S, a); return b ? { b } : null; }
  if (place === 'street') return null;
  const b = byType(S, place);
  return b ? { b } : null;
}
function parkSpot(S) {
  const b = byType(S, 'park');
  if (!b) return placeTarget(S, null, 'plaza');
  for (let i = 0; i < 8; i++) { const x = b.x + rint(S, 0, b.w - 1), y = b.y + rint(S, 0, b.h - 1); if (!(x >= b.x + 3 && x < b.x + 6 && y >= b.y + 2 && y < b.y + 5)) return { b, spot: { x, y } }; }
  return { b, spot: { x: b.x, y: b.y } };
}

// ----------------------------------------------------------------- the day

export function shiftOf(S, a, f) {
  const [s, e] = FIRM[f.type].hours;
  if (f.type === 'watch') return num(a.id) % 2 ? [14.5, 24] : [6, 15.5];
  return [s, e];
}
const operatesToday = (f, wd) => wd !== 6 || f.type === 'tavern' || f.type === 'watch';

function awake(a, hr) {
  let wake = a.wake, bed = a.bed;
  if (a.job && a.job.late) { wake = 8.5; bed = 0.8; }
  if (a.age < 12) bed = Math.min(bed, 20.5);
  return inWindow(hr, wake, bed);
}

export function updateAgent(S, a) {
  if (((S.tick + num(a.id)) % 10) === 0) tickNeeds(S, a);
  if (a.path) { advance(S, a); return; }
  const act = a.act;
  if (act.type === 'work') {
    const f = act.f ? firm(S, act.f) : null;
    if (f) { f.eff += a.eff || 0.6; a.worked++; }
  }
  if (S.tick >= act.until) {
    onEnd(S, a);
    if (a.dead || a.gone) return;
    if (a.jail) { if (S.day >= a.jail.until && hourOf(S) >= 8) release(S, a); else { a.act = { type: 'jailed', until: S.tick + 60, desc: 'Locked in the watch house' }; return; } }
    chooseActivity(S, a);
    if (!a.path && a.act.until <= S.tick) a.act = { type: 'idle', until: S.tick + rint(S, 15, 30), desc: a.act.desc || 'Idling' };
  }
}

function tickNeeds(S, a) {
  const t = a.act.type;
  const asleep = t === 'sleep';
  const h = hhOf(S, a);
  a.hunger = Math.min(100, a.hunger + (asleep ? 0.32 : 0.72) * (a.age < 12 ? 0.8 : 1));
  a.energy = clamp(a.energy + (asleep ? (h.home ? 2.1 : 1.5) : t === 'work' || t === 'corvee' ? -0.72 : t === 'walk' ? -0.6 : -0.5), 0, 100);
  const company = t === 'tavern' || t === 'visit' || t === 'chapel' || t === 'play' || t === 'protest' || t === 'school' || t === 'host';
  a.social = clamp(a.social + (company ? 2.6 : t === 'work' ? 0.15 : asleep ? -0.05 : -0.4), 0, 100);
  const merry = t === 'tavern' || t === 'play' || t === 'stroll' || t === 'visit' || t === 'host' || t === 'garden';
  a.fun = clamp(a.fun + (merry ? 2.4 : t === 'work' || t === 'corvee' ? -0.42 : asleep ? 0 : -0.22), 0, 100);
  if (a.hunger > 92) a.health -= 0.13;
  else if (a.hunger < 65 && a.energy > 25 && a.health < 100) a.health += 0.02;
  if (!h.home && asleep) a.health -= 0.02;
  if (t === 'school') a.edu = Math.min(1, a.edu + 0.00023);
  if (a.health < 0) a.health = 0;
}

function onStart(S, a) {
  const act = a.act;
  const h = hhOf(S, a);
  switch (act.type) {
    case 'eat': {
      if (act.src === 'home') {
        if (h.inv.food < act.q) { act.type = 'idle'; act.desc = 'Finding the larder bare'; act.until = S.tick + 5; a.thought = 'There is nothing to eat at home.'; return; }
        h.inv.food -= act.q;
      }
      a.hunger = Math.max(0, a.hunger - 52); a.meals++;
      a.lastMeal = S.tick;
      break;
    }
    case 'shop': doShopping(S, a, h); if (h.shopper === a.id) h.shopper = null; break;
    case 'work': startWork(S, a); break;
    case 'tavern': atTavern(S, a, h); break;
    case 'treatment': {
      const f = firm(S, act.f);
      if (f && staffPresent(S, f) > 0 && buyService(S, a, f, 'healthcare')) { a.health = Math.min(100, a.health + 30); note(S, a, `Saw the healer at ${f.name} (${(f.sellPrice || f.price).toFixed(1)} coins).`, 'health'); }
      else { act.desc = 'Turned away at the clinic'; act.until = S.tick + 5; note(S, a, f && staffPresent(S, f) > 0 ? 'Could not afford the healer.' : 'No healer was in.', 'health'); }
      break;
    }
    case 'apply': {
      const f = firm(S, act.f);
      if (!f || !applyFor(S, a, f)) { a.thought = 'No luck finding work today.'; a.mood -= 2; }
      break;
    }
    case 'vote': castVotes(S, a); break;
    case 'school': {
      const sf = firmOfType(S, 'school');
      const payer = adultsOf(S, h)[0];
      if (a.schoolDay !== S.day) {
        a.schoolDay = S.day;
        if (!sf || !payer || !buyService(S, payer, sf, 'education')) { a.enrolled = false; act.until = S.tick + 5; act.desc = 'Sent home from school'; if (payer) note(S, payer, `Could not pay ${a.name}'s school fee today.`, 'money'); }
      }
      break;
    }
    case 'steal': doSteal(S, a, h); break;
    case 'forage': {
      const r = attempt(S, a, 'forage', { place: 'forest', good: 'food' }, 3 + a.hunger / 20, { kind: 'public' });
      if (r === 'refrain' || r === 'caught') { act.until = S.tick + 5; act.fail = true; a.thought = 'Gathering in the forest is forbidden.'; }
      break;
    }
    case 'visit': {
      const fr = agent(S, act.friend);
      if (fr && fr.inside === a.inside && fr.act.type !== 'sleep') {
        a.friends[fr.id] = Math.min(1, (a.friends[fr.id] || 0.2) + 0.03); fr.friends[a.id] = Math.min(1, (fr.friends[a.id] || 0.2) + 0.03);
        fr.social = Math.min(100, fr.social + 12); a.social = Math.min(100, a.social + 8);
        if (fr.act.type === 'home' || fr.act.type === 'idle' || fr.act.type === 'garden') { fr.act = { type: 'host', until: act.until, desc: `Hosting ${a.name}` }; }
        act.desc = `Visiting ${fullName(fr)}`;
      } else { act.desc = fr ? `Knocking at ${fr.name}'s door - nobody home` : 'Nobody home'; act.until = S.tick + 6; }
      break;
    }
    case 'chapel': case 'stroll': case 'play': mingle(S, a); break;
    case 'protest': a.protest = true; mingle(S, a); break;
    case 'gone': removeAgent(S, a, act.why || 'left'); break;
    case 'homeale': {
      const r = attempt(S, a, 'consume', { good: 'ale', price: 0, quantity: 0.5, place: 'home' }, 2 + (1 - a.t.temperance) * 4, { kind: 'trade', detectMult: 0.3 });
      if ((r === 'ok' || r === 'violate') && h.inv.ale >= 0.5) { h.inv.ale -= 0.5; a.fun = Math.min(100, a.fun + 14); a.drinks++; }
      else act.desc = 'Resting at home';
      break;
    }
  }
}

function onEnd(S, a) {
  const act = a.act;
  if (act.type === 'forage' && !act.fail) {
    const got = rnd(S, 0.4, 1.1);
    hhOf(S, a).inv.food += got;
    note(S, a, `Gathered roots and berries in the forest (${got.toFixed(1)} food).`, 'hardship');
  } else if (act.type === 'beg') {
    let got = 0;
    for (let i = 0; i < 4; i++) { const d = pick(S, S.agents); const dh = hhOf(S, d); if (d !== a && isAdult(d) && dh.money > 40 && chance(S, d.t.altruism * 0.5)) { const g = rnd(S, 0.5, 2); dh.money -= g; got += g; } }
    hhOf(S, a).money += got; a.inc.other += got;
    note(S, a, got > 0 ? `Begged in the plaza. Kind souls gave ${got.toFixed(1)} coins.` : 'Begged in the plaza. Nobody gave.', 'hardship');
  } else if (act.type === 'corvee') {
    a.labor = (a.labor || 0) + (act.dur || 0);
    S.gov.amenity = Math.min(100, S.gov.amenity + 0.05);
  } else if (act.type === 'tavern') {
    if (a.drinks >= 4) a.hung = 1;
  } else if (act.type === 'protest') a.protest = false;
}

// -------------------------------------------------------------------- work

function startWork(S, a) {
  const f = firm(S, a.act.f);
  if (!f || !a.job) { a.eff = 0; return; }
  const n = f.workers.length;
  const skill = 0.55 + 0.45 * a.skill + (f.type === 'clinic' || f.type === 'school' ? 0.3 : 0.1) * a.edu;
  const ageF = a.age < 16 ? 0.45 : a.age > 60 ? Math.max(0.5, 1 - (a.age - 60) * 0.02) : 1;
  const healthF = 0.5 + 0.5 * (a.health / 100);
  // What a worker puts in depends on their own diligence, on whether the fruit
  // of the work is theirs, on being watched, and on how they feel they are treated.
  const stake = a.job.owner ? 1 : f.owner === 'coop' ? 1 / Math.max(1, n) + 0.15 : 0;
  const mgr = f.manager ? agent(S, f.manager) : null;
  let watch = 0;
  if (!a.job.owner && mgr && mgr.id !== a.id) watch = f.owner === 'private' ? 0.5 + 0.5 * mgr.t.greed : f.owner === 'state' ? 0.7 * mgr.t.diligence * (0.5 + 0.5 * mgr.t.conformity) : 0.45;
  const fair = clamp((a.job.wage - S.stat.medianWage) / Math.max(1, S.stat.medianWage), -0.5, 0.5);
  let effort = 0.4 + 0.45 * a.t.diligence + 0.35 * stake * (0.4 + 0.6 * a.t.greed) + 0.22 * watch * (1 - a.t.diligence) + (a.happy - 50) / 250 + (a.job.owner ? 0 : fair * 0.2) + (f.owner === 'coop' ? 0.1 * a.v.community : 0);
  if (a.hung) effort -= 0.15;
  if (a.energy < 25) effort -= 0.1;
  if (a.hunger > 75) effort -= 0.12;
  if (a.mustWork && a.job.wage < reservationWage(S, a) * 2) effort -= 0.1;
  a.eff = clamp(effort, 0.2, 1.35) * skill * ageF * healthF;
}

const WORK_DESC = { farmer: 'Working the fields at', lumberjack: 'Felling timber for', crafter: 'At the bench in', brewer: 'Brewing at', tavernkeeper: 'Serving at', healer: 'Tending the sick at', teacher: 'Teaching at', guard: 'On patrol', clerk: 'Keeping the ledgers at the', laborer: 'Labouring on the' };

function work(S, a, hr, wd) {
  const f = firm(S, a.job.firm);
  if (!f) { a.job = null; return false; }
  if (!f.open || f.suspended || !operatesToday(f, wd)) return false;
  const [st, en] = shiftOf(S, a, f);
  a.job.late = en > 22;
  if (hr < st - 0.5 || hr >= en - 0.2) return false;
  if (a.noWork && a.noWork > S.tick) return false;
  if (hr >= 12 && hr < 12.8 && st < 11 && en > 14 && a.lunchDay !== S.day) return false; // dinner break
  // Deeply aggrieved people may walk out and join a protest instead.
  if (a.griev > 78 && a.strike === S.day) return false;
  const b = building(S, f.b);
  if (S._c.idx.prohibit.work) {
    const r = attempt(S, a, 'work', { place: b.type, hours_today: a.worked / 60, wage: a.job.wage }, Math.max(1, a.job.wage * 0.3), { kind: 'site' });
    if (r === 'refrain') { a.noWork = S.tick + 120; a.thought = 'The law says I may not work now.'; return false; }
    if (r === 'caught') { a.noWork = S.tick + 120; return !!a.jail; }
  }
  let endHr = en;
  if (st < 11 && en > 14 && hr < 12) endHr = 12;
  const dur = clamp(Math.min(120, (endHr - hr) * 60), 10, 120);
  const role = a.job.role;
  const then = { type: 'work', dur, f: f.id, desc: role === 'guard' ? 'On patrol' : `${WORK_DESC[role] || 'Working at'} ${f.name}`, walk: role === 'guard' ? 'Walking the beat' : `Walking to work at ${f.name}` };
  // Take something to eat along.
  if (a.inside && a.inside === hhOf(S, a).home && a.lunch < 0.5) { const h = hhOf(S, a); if (h.inv.food >= 1.5) { h.inv.food -= 0.5; a.lunch += 0.5; } }
  let target = { b };
  if (role === 'guard') {
    let hot = hr >= 18 ? byType(S, 'tavern') : chance(S, 0.4) ? byType(S, 'market') : null;
    if (S.gov.unrest.last > 2 && chance(S, 0.6)) hot = null, target = placeTarget(S, a, 'plaza');
    else if (hot && chance(S, 0.5)) target = { x: hot.door.x, y: hot.door.y, label: 'the beat' };
    else { const [x, y] = tileXY(pick(S, S.map.patrol)); target = { x, y, label: 'the beat' }; }
    then.dur = rint(S, 12, 30);
    return goTo(S, a, target, then);
  }
  if (f.type === 'farm' && b.field) then.spot = { x: b.field.x + rint(S, 0, b.field.w - 1), y: b.field.y + rint(S, 0, b.field.h - 1) };
  if (f.type === 'works') target = parkSpot(S), then.spot = target.spot;
  return travel(S, a, target, then, a.job.wage);
}

function seekWork(S, a, hr, wd) {
  if (a.sought === S.day || hr < 6.5 || hr > 15 || a.age >= 72) return false;
  a.sought = S.day;
  const res = reservationWage(S, a);
  const v = bestVacancy(S, a, res);
  if (!v) {
    const any = bestVacancy(S, a, 0);
    a.thought = any ? `The only work going pays ${any.net.toFixed(1)} a day. Not worth my while.` : 'There is no work to be had.';
    if (!any) a.idleDays++;
    return false;
  }
  a.thought = `I hear ${v.f.name} is hiring.`;
  return travel(S, a, { b: building(S, v.f.b) }, { type: 'apply', dur: 20, f: v.f.id, desc: `Asking for work at ${v.f.name}` }, v.net * 3);
}

// -------------------------------------------------------------------- food

function eat(S, a, h, hb, hr, urgent) {
  const q = a.age < 12 ? 0.3 : 0.5;
  const atHome = hb && a.inside === hb.id;
  if (a.lunch >= q && !atHome) { a.lunch -= q; return start(S, a, { type: 'eat', dur: 20, q, src: 'lunch', desc: 'Eating a packed meal' }); }
  if (h.inv.food >= q) {
    if (hb) return travel(S, a, { b: hb }, { type: 'eat', dur: 30, q, src: 'home', desc: `${mealName(hr)} at home` }, 8);
    h.inv.food -= q;
    return start(S, a, { type: 'eat', dur: 20, q, src: 'pocket', desc: 'Eating what little we have' });
  }
  if (a.age >= 14 && marketOpen(hr) && h.money >= (S.stat.price?.food || 2) * 0.6) return goShop(S, a, h, 'We are out of food.');
  if (!urgent && a.hunger < 70) return false;
  if (a.age < 10) return false;
  return hardship(S, a, h, hb, hr);
}

function hardship(S, a, h, hb, hr) {
  if (a.hardDay === S.day && a.hardN >= 2) return false;
  if (a.hardDay !== S.day) { a.hardDay = S.day; a.hardN = 0; }
  a.hardN++;
  // First, those who love us.
  const kin = [...a.parents, ...a.kids, ...Object.keys(a.friends).sort((p, q) => a.friends[q] - a.friends[p])].map((id) => agent(S, id)).filter((k) => k && k.hh !== a.hh && isAdult(k));
  for (const k of kin.slice(0, 4)) {
    const kh = hhOf(S, k);
    if (kh.inv.food > kh.members.length * 2 && chance(S, k.t.altruism * 0.7 + (a.friends[k.id] || 0.6) * 0.3)) {
      const g = Math.min(2, kh.inv.food * 0.25);
      kh.inv.food -= g; h.inv.food += g;
      note(S, a, `${fullName(k)} shared ${g.toFixed(1)} food with us. We had nothing.`, 'hardship');
      note(S, k, `Gave ${g.toFixed(1)} food to ${fullName(a)}, who had none.`, 'kindness');
      k.mood += 1;
      return eat(S, a, h, hb, hr, true);
    }
  }
  a.mood -= 3;
  const desperate = a.hunger > 86;
  if (desperate && a.age >= 12 && chance(S, clamp01(1 - a.t.conformity * 0.9 - a.t.altruism * 0.2 + (a.hunger > 95 ? 0.4 : 0)))) {
    a.thought = 'I cannot watch us starve. I will take what we need.';
    return travel(S, a, { b: byType(S, 'market') }, { type: 'steal', dur: 10, good: 'food', q: 2, desc: 'Loitering by the food stalls' }, 10);
  }
  if (hr >= 6 && hr < 18 && chance(S, 0.6)) {
    a.thought = 'Nothing in the larder. Perhaps the forest will feed us.';
    return travel(S, a, placeTarget(S, a, 'forest'), { type: 'forage', dur: rint(S, 100, 160), desc: 'Foraging in the forest' }, 6);
  }
  if (hr >= 8 && hr < 20) {
    a.thought = 'I have no choice but to beg.';
    return travel(S, a, placeTarget(S, a, 'plaza'), { type: 'beg', dur: 90, desc: 'Begging in the plaza' }, 4);
  }
  return false;
}

function doSteal(S, a, h) {
  const act = a.act;
  const good = act.good;
  const px = marketPrice(S, good);
  const victim = S.firms.filter((f) => f.good === good && f.stock >= act.q).sort((p, q) => q.stock - p.stock)[0];
  if (!victim) { act.desc = 'Finding nothing to take'; return; }
  const need = good === 'food' && a.hunger > 80;
  const r = attempt(S, a, 'steal', { good, quantity: act.q, price: px, place: 'market' }, px * act.q * (need ? 5 : 1.3), { kind: 'public', why: `stealing ${good}` });
  if (r === 'refrain') { a.thought = 'I could not bring myself to do it.'; return; }
  if (r === 'caught') return;
  victim.stock -= act.q;
  h.inv[good] += act.q;
  S.stat.theftToday = (S.stat.theftToday || 0) + 1;
  const owner = victim.manager ? agent(S, victim.manager) : null;
  if (owner) { owner.mood -= 3; owner.robbed = S.day; if (chance(S, 0.5)) note(S, owner, `Some ${good} went missing from our stall at the market.`, 'crime'); }
  note(S, a, need ? `Took ${act.q} ${good} from the market without paying. We have to eat.` : `Lifted ${act.q} ${good} from a market stall.`, 'crime');
}

// ---------------------------------------------------------------- shopping

function needsShopping(S, h) {
  const need = h.members.length * 0.85;
  if (h.shopper && h.shopUntil > S.tick) return false;
  if (h.money < 1) return false;
  return h.inv.food < need * 1.5 || (h.inv.goods < h.members.length * 0.35 && h.money > 25) || ((h.inv.wood || 0) < h.members.length * 0.15 && h.money > 15);
}
function goShop(S, a, h, why) {
  h.shopper = a.id; h.shopUntil = S.tick + 150;
  if (why) a.thought = why;
  const ok = travel(S, a, { b: byType(S, 'market') }, { type: 'shop', dur: rint(S, 18, 35), desc: 'Shopping at the market' }, 8);
  if (!ok) h.shopper = null;
  return ok;
}
function doShopping(S, a, h) {
  const ms = membersOf(S, h);
  const n = ms.length;
  const need = ms.reduce((s, m) => s + (m.age < 12 ? 0.6 : 1), 0);
  const px = S.stat.price || BASE_PRICE;
  const before = h.money;
  const got = {};
  let want = need * 3.2 - h.inv.food;
  if (h.money < need * px.food * 3) want = Math.min(want, need * 1.6 - h.inv.food);
  if (want > 0.3) got.food = buyGood(S, a, 'food', want, Math.min(BASE_PRICE.food * 6, Math.max(px.food * 1.3, h.money / Math.max(1, need))));
  const home = h.home ? building(S, h.home) : null;
  const rent = home && home.tenantHh === h.id ? home.rent : 0;
  const disc = () => Math.max(0, h.money - need * px.food * 4 - rent * 4);
  if (h.inv.goods < n * 0.45) {
    const q = Math.min(n * 1.0 - h.inv.goods, (disc() * 0.45) / px.goods);
    if (q > 0.2) got.goods = buyGood(S, a, 'goods', q, px.goods * 1.25);
  }
  if ((h.inv.wood || 0) < n * 0.25) {
    const q = Math.min(n * 0.6, (disc() * 0.2) / px.wood);
    if (q > 0.2) got.wood = buyGood(S, a, 'wood', q, px.wood * 1.25);
  }
  const fond = mean(ms.filter(isAdult), (m) => 1 - m.t.temperance);
  if (fond > 0.42 && h.inv.ale < 0.5) {
    const q = Math.min(1.5, (disc() * 0.14 * fond * 2) / px.ale);
    if (q >= 0.5) got.ale = buyGood(S, a, 'ale', q, px.ale * 1.3);
  }
  const parts = Object.entries(got).filter(([, v]) => v > 0.05).map(([g, v]) => `${v.toFixed(1)} ${g}`);
  const spent = before - h.money;
  if (parts.length) note(S, a, `Bought ${parts.join(', ')} at the market for ${spent.toFixed(1)} coins.`, 'money');
  else if (want > 0.3) { note(S, a, h.money < px.food ? 'Went to market but could not afford food.' : 'Went to market; could not get what we needed.', 'hardship'); a.mood -= 2; }
}

// ------------------------------------------------------------------ leisure

function atTavern(S, a, h) {
  const f = firm(S, a.act.f);
  mingle(S, a);
  if (!f || !f.open || f.suspended || staffPresent(S, f) === 0) { a.act.desc = 'Sitting in the quiet tavern'; return; }
  const want = 1 + Math.round((1 - a.t.temperance) * 2.4 * rand(S));
  let had = 0;
  const price = f.sellPrice || f.price;
  for (let i = 0; i < want; i++) {
    const wtp = 1.2 + (1 - a.t.temperance) * 5 + (100 - a.fun) / 40 - a.t.thrift * 1.2;
    if (f.input < 0.5 || price > wtp || h.money < price + (S.stat.price?.food || 2) * 2) break;
    const ctx = { good: 'ale', price, quantity: 0.5, place: 'tavern' };
    let r = attempt(S, a, 'buy', ctx, wtp - price + 1, { kind: 'trade', silent: true });
    if (r === 'ok' || r === 'violate') r = attempt(S, a, 'consume', ctx, wtp - price + 1, { kind: 'trade' });
    if (r === 'caught') return;
    if (r === 'refrain') { a.thought = 'A drink would be welcome, but it is against the law.'; break; }
    if (!buyService(S, a, f, 'ale', { taxGood: 'ale' })) break;
    f.input -= 0.5; a.drinks++; had++;
    a.fun = Math.min(100, a.fun + 13); a.social = Math.min(100, a.social + 5);
    if (f.illicit && f.manager && chance(S, detectProb(S, 'trade', a.x, a.y) * 0.25)) {
      const mgr = agent(S, f.manager); const law = S.laws.find((l) => l.id === (f.violKey || '').split(':')[0]);
      if (mgr && law) punish(S, mgr, law, law.provisions[+f.violKey.split(':')[1]].penalty, 'selling ale unlawfully', {});
    }
  }
  if (had) note(S, a, `${had} drink${had > 1 ? 's' : ''} at ${f.name} (${(had * price).toFixed(1)} coins).`, 'leisure');
  else if (f.input < 0.5) a.act.desc = `At ${f.name} - the barrels are dry`;
}

function mingle(S, a) {
  if (a.age < 6) return;
  let best = null;
  for (let i = 0; i < 6; i++) {
    const o = pick(S, S.agents);
    if (o === a || o.hh === a.hh || (o.age < 16) !== (a.age < 16)) continue;
    if (a.inside ? o.inside === a.inside : !o.inside && Math.abs(o.x - a.x) + Math.abs(o.y - a.y) < 7) { best = o; break; }
  }
  if (!best) return;
  const d = 0.035 * (0.5 + a.t.sociability);
  a.friends[best.id] = Math.min(1, (a.friends[best.id] || 0.1) + d);
  best.friends[a.id] = Math.min(1, (best.friends[a.id] || 0.1) + d);
  a.social = Math.min(100, a.social + 4);
  a.met = best.id;
}

function wantsProtest(S, a, hr) {
  if (!isAdult(a) || a.griev < 50 || hr < 9 || hr > 16.5) return false;
  if (a.protestDay === S.day) return a.protestGo;
  a.protestDay = S.day;
  // People join when their anger and nerve, plus the sight of others already out, cross their own threshold.
  const bold = a.t.risk * 0.6 + (a.griev - 50) / 60 + S.gov.unrest.share * 2.5 + (a.v.liberty > 0 ? a.v.liberty * 0.15 : 0);
  a.protestGo = bold > 0.72 + rnd(S, -0.1, 0.1);
  if (a.protestGo && a.griev > 78) a.strike = S.day;
  return a.protestGo;
}
function protest(S, a, hr) {
  if (!wantsProtest(S, a, hr)) return false;
  const r = attempt(S, a, 'protest', { place: 'plaza' }, 4 + (a.griev - 50) / 4, { kind: 'public' });
  if (r === 'refrain') { a.protestGo = false; a.thought = 'I would be in the plaza shouting, if it were not forbidden.'; return false; }
  if (r === 'caught') { a.protestGo = false; return !!a.jail; }
  const top = S.laws.filter((l) => l.status === 'active').sort((p, q) => (a.opin[p.id] ?? 0) - (a.opin[q.id] ?? 0))[0];
  a.thought = top && (a.opin[top.id] ?? 0) < -0.2 ? `Enough. "${top.title}" has to go.` : 'Things cannot go on like this.';
  if (a.protestNote !== S.day) { a.protestNote = S.day; note(S, a, top && (a.opin[top.id] ?? 0) < -0.2 ? `Joined the protest in the plaza against "${top.title}".` : 'Joined the protest in the plaza.', 'politics'); }
  return travel(S, a, placeTarget(S, a, 'plaza'), { type: 'protest', dur: rint(S, 90, 170), desc: 'Protesting in the plaza', cause: top ? top.id : null }, 8);
}

function castVotes(S, a) {
  for (const v of S.gov.votes) {
    if (v.done || v.day !== S.day || a.voted[`${v.law}:${v.kind}`]) continue;
    const law = S.laws.find((l) => l.id === v.law);
    if (!law) continue;
    const op = a.opin[law.id] ?? opinionOnProposal(S, a, law);
    const yes = v.kind === 'enact' ? op > 0 : op < 0;
    if (yes) v.yes++; else v.no++;
    a.voted[`${v.law}:${v.kind}`] = S.day;
    a.trust = clamp01(a.trust + 0.01);
    note(S, a, `Voted ${yes ? 'for' : 'against'} ${v.kind === 'enact' ? '' : 'repealing '}"${law.title}".`, 'politics');
  }
}
function hasBallot(S, a) {
  for (const v of S.gov.votes) if (!v.done && v.day === S.day && !a.voted[`${v.law}:${v.kind}`]) {
    if (a.ballotDay !== S.day) { a.ballotDay = S.day; a.ballotGo = canVote(S, a) && chance(S, 0.45 + 0.25 * a.t.conformity + 0.3 * Math.abs(a.opin[v.law] ?? 0) + 0.1 * a.v.community); }
    return a.ballotGo;
  }
  return false;
}

function mustAttend(S, a, hr) {
  const list = S._c.idx.require.attend;
  if (!list) return false;
  const sc = scopeFor(S, a, {});
  for (const e of list) {
    if (!evalBool(e.p.applies_to, sc, false) || !evalBool(e.p.when, sc, false)) continue;
    if (!a.att || a.att.key !== e.key || a.att.day !== S.day) {
      const defy = decideViolate(S, a, e.law, { gain: 3 + (a.job ? a.job.wage * 0.2 : 0), fine: Math.max(0, evalNum(e.p.penalty.fine, sc)), pen: e.p.penalty, pDetect: detectProb(S, 'public', a.x, a.y) * 0.6 + 0.1 });
      a.att = { key: e.key, day: S.day, ok: !defy };
      if (defy) { e.law.d.viol++; e.law.stats.violations++; if (chance(S, detectProb(S, 'public', a.x, a.y) * 0.4 + 0.05)) { punish(S, a, e.law, e.p.penalty, 'staying away when summoned', { sc }); if (a.jail) return true; } }
      else { e.law.d.refrain++; addImpact(a, e.law, -1.5); }
    }
    if (!a.att.ok) continue;
    const target = placeTarget(S, a, e.p.location) || placeTarget(S, a, 'plaza');
    if (!target) continue;
    return goTo(S, a, target, { type: 'attend', dur: 30, desc: `Attending as required by "${e.law.title}"` });
  }
  return false;
}

function corvee(S, a, hr) {
  if (!isAdult(a) || hr < 7 || hr > 17) return false;
  const e = requirementsFor(S, a, 'public_labor');
  if (!e) return false;
  const owed = e.p.hours_per_week * 60 - (a.labor || 0);
  if (owed <= 0) return false;
  if (a.corveeDay !== S.day) {
    a.corveeDay = S.day;
    a.corveeOk = !decideViolate(S, a, e.law, { gain: 2 + (owed / 60) * 1.2, fine: Math.max(0, evalNum(e.p.penalty.fine, scopeFor(S, a, {}))), pen: e.p.penalty, pDetect: detectProb(S, 'paper', 0, 0) + 0.15 });
  }
  if (!a.corveeOk) return false;
  const t = parkSpot(S);
  addImpact(a, e.law, -1.5);
  return travel(S, a, t, { type: 'corvee', dur: Math.min(120, owed), spot: t.spot, desc: 'Doing compulsory labour on the public works' }, 3);
}

function errands(S, a, h, hr) {
  if (marketOpen(hr) && needsShopping(S, h) && (a.age >= 16 || !adultsOf(S, h).length)) return goShop(S, a, h, 'Time to go to market.');
  if (a.health < 55 && hr >= 8 && hr < 16.5 && a.clinicDay !== S.day) {
    a.clinicDay = S.day;
    const f = firmOfType(S, 'clinic');
    if (f && f.open && !f.suspended && h.money > (f.sellPrice || f.price) + 4) {
      if (S._c.idx.prohibit.visit && attempt(S, a, 'visit', { place: 'clinic' }, 8) !== 'ok') return false;
      a.thought = 'I am not well. I should see the healer.';
      return travel(S, a, { b: building(S, f.b) }, { type: 'treatment', dur: 40, f: f.id, desc: `Seeing the healer at ${f.name}` }, 10);
    }
  }
  return false;
}

function visitOK(S, a, place, value) {
  if (!S._c.idx.prohibit.visit) return true;
  const r = attempt(S, a, 'visit', { place }, value, { kind: 'public' });
  return r === 'ok' || r === 'violate';
}

function leisure(S, a, h, hb, hr, wd) {
  const kid = a.age < 16;
  const dark = hr >= 20.5 || hr < 5.5;
  if (kid && (hr >= 19 || hr < 6.5)) return stayHome(S, a, hb, 'At home with the family', rint(S, 40, 70));
  const opts = [];
  const tavern = firmOfType(S, 'tavern');
  if (!kid && tavern && tavern.open && !tavern.suspended && hr >= 15 && hr < 22.6 && h.money > (tavern.sellPrice || tavern.price) * 2 + 6) {
    opts.push(['tavern', ((100 - a.fun) / 100) * 1.1 + a.t.sociability * 0.8 + (1 - a.t.temperance) * 0.9 - a.t.thrift * 0.4 - (a.tavernDay === S.day ? 1.2 : 0) + (wd >= 5 ? 0.25 : 0)]);
  }
  const friends = Object.keys(a.friends);
  if (friends.length && hr >= 9 && hr < 20.5) opts.push(['visit', (100 - a.social) / 100 + a.t.sociability * 0.5 - (a.visitDay === S.day ? 0.8 : 0)]);
  if (!dark) opts.push([kid ? 'play' : 'stroll', 0.45 + (100 - a.fun) / 160 + (kid ? 0.9 : 0) + S.gov.amenity / 120]);
  if (wd === 6 && hr >= 7.5 && hr < 10.5 && !kid && a.chapelDay !== S.day) opts.push(['chapel', 0.5 + a.v.tradition * 1.6 + a.v.community * 0.3]);
  opts.push(['home', 0.62 + (a.energy < 40 ? 0.8 : 0) + (dark ? 0.5 : 0) + (a.age > 60 ? 0.2 : 0)]);
  // The light-fingered look for their chance.
  if (!kid && a.t.greed > 0.72 && a.t.altruism < 0.3 && a.t.conformity < 0.34 && marketOpen(hr) && a.stealDay !== S.day) opts.push(['pilfer', 0.9 + a.t.risk * 0.5]);
  let best = opts[0], bs = -Infinity;
  for (const o of opts) { const s = o[1] + rand(S) * 0.5; if (s > bs) { bs = s; best = o; } }
  switch (best[0]) {
    case 'tavern': {
      a.tavernDay = S.day;
      if (!visitOK(S, a, 'tavern', 3 + (1 - a.t.temperance) * 3)) break;
      a.thought = pick(S, ['A drink and some company would do me good.', `Off to ${tavern.name}.`, 'I have earned a drink.']);
      if (travel(S, a, { b: building(S, tavern.b) }, { type: 'tavern', dur: rint(S, 60, 140), f: tavern.id, desc: `Drinking at ${tavern.name}` }, 3 + a.t.sociability * 4)) return true;
      break;
    }
    case 'visit': {
      a.visitDay = S.day;
      const pool = friends.map((id) => agent(S, id)).filter((f) => f && !f.jail);
      if (!pool.length) break;
      pool.sort((p, q) => a.friends[q.id] - a.friends[p.id]);
      const fr = pool[Math.min(pool.length - 1, Math.floor(rand(S) * rand(S) * pool.length))];
      const fb = homeOf(S, fr);
      if (!fb || !visitOK(S, a, 'home', 2)) break;
      a.thought = `I should call on ${fr.name}.`;
      if (travel(S, a, { b: fb }, { type: 'visit', dur: rint(S, 50, 110), friend: fr.id, desc: `Visiting ${fullName(fr)}` }, 2 + a.t.sociability * 3)) return true;
      break;
    }
    case 'play': case 'stroll': {
      const inPark = chance(S, 0.5);
      if (!visitOK(S, a, inPark ? 'park' : 'plaza', 2)) break;
      const t = inPark ? parkSpot(S) : placeTarget(S, a, 'plaza');
      a.thought = kid ? 'Out to play!' : pick(S, ['A walk will clear my head.', 'Some air, I think.', 'I will see who is about.']);
      if (travel(S, a, t, { type: best[0], dur: rint(S, 40, 90), spot: t.spot, desc: kid ? (inPark ? 'Playing on the green' : 'Playing in the plaza') : inPark ? 'Strolling on the green' : 'Passing the time in the plaza' }, 2)) return true;
      break;
    }
    case 'chapel': {
      a.chapelDay = S.day;
      const cb = byType(S, 'chapel');
      if (!cb || !visitOK(S, a, 'chapel', 2 + a.v.tradition * 6)) break;
      a.thought = 'Restday. To the chapel.';
      if (travel(S, a, { b: cb }, { type: 'chapel', dur: rint(S, 70, 100), desc: 'At the Restday gathering' }, 2 + a.v.tradition * 5)) return true;
      break;
    }
    case 'pilfer': {
      a.stealDay = S.day;
      if (travel(S, a, { b: byType(S, 'market') }, { type: 'steal', dur: 12, good: chance(S, 0.5) ? 'goods' : 'ale', q: 1, desc: 'Lingering by the stalls' }, 4)) return true;
      break;
    }
  }
  return stayHome(S, a, hb, null, rint(S, 45, 85));
}

function stayHome(S, a, hb, desc, dur) {
  if (!hb) {
    const t = placeTarget(S, a, 'plaza');
    return goTo(S, a, t, { type: 'idle', dur, desc: 'With nowhere to go' });
  }
  const h = hhOf(S, a);
  const hr = hourOf(S);
  let then = { type: 'home', dur, desc: desc || (a.age < 16 ? 'Playing at home' : pick(S, ['Resting at home', 'Doing chores at home', 'Mending and tidying at home', 'At home with the family'])) };
  if (!desc && a.age >= 18 && hr >= 18 && h.inv.ale >= 0.5 && a.t.temperance < 0.55 && a.aleDay !== S.day) { a.aleDay = S.day; then = { type: 'homeale', dur, desc: 'Having a cup of ale at home' }; }
  else if (!desc && a.age >= 50 && hr > 8 && hr < 18 && chance(S, 0.4)) then = { type: 'garden', dur, desc: 'Tending the garden' };
  // Getting home is worth a good deal, even at risk.
  return travel(S, a, { b: hb }, then, 6) || start(S, a, { type: 'idle', dur: 30, desc: 'Waiting where I am' });
}

function sleep(S, a, hb, hr) {
  const wake = a.job && a.job.late ? 8.5 : a.wake;
  const dur = a.energy < 8 && awake(a, hr) ? 120 : Math.max(30, hoursUntil(hr, wake) * 60 + rint(S, 0, 20));
  if (hb) {
    const ok = travel(S, a, { b: hb }, { type: 'sleep', dur, desc: 'Asleep at home', walk: 'Heading home to bed' }, 10);
    if (ok) return true;
    return start(S, a, { type: 'sleep', dur, desc: 'Dozing where I am until I may go home' });
  }
  if (a.inside) return start(S, a, { type: 'sleep', dur, desc: 'Sleeping where I can' });
  const t = parkSpot(S);
  return goTo(S, a, t, { type: 'sleep', dur, spot: t.spot, desc: 'Sleeping rough on the green' });
}

function leave(S, a) {
  const ex = S.map.exits.slice().sort((p, q) => Math.hypot(p.x - a.x, p.y - a.y) - Math.hypot(q.x - a.x, q.y - a.y))[0];
  return goTo(S, a, { x: ex.x, y: ex.y, label: 'the road out of town' }, { type: 'gone', dur: 5, why: a.leaving, desc: 'Leaving town', walk: 'Leaving town for good' });
}

export function chooseActivity(S, a) {
  const hr = hourOf(S), wd = weekdayOf(S);
  const h = hhOf(S, a);
  const hb = h.home ? building(S, h.home) : null;
  a.protest = false;
  if (a.leaving) return leave(S, a);
  if (a.age < 6) return stayHome(S, a, hb, a.age < 2 ? 'In the cradle' : 'Playing at home', 60);
  if (!awake(a, hr) || a.energy < 8) return sleep(S, a, hb, hr);
  if (mustAttend(S, a, hr)) return true;
  if (a.hunger > 78 && eat(S, a, h, hb, hr, true)) return true;
  let onShift = false;
  if (a.job) { const f = firm(S, a.job.firm); if (f && f.open && !f.suspended && operatesToday(f, wd)) { const [st, en] = shiftOf(S, a, f); onShift = hr >= st && hr < en - 0.2; } }
  const dinner = hr >= 12 && hr < 13.2;
  if (dinner && onShift && a.lunchDay !== S.day) { a.lunchDay = S.day; if (a.hunger > 28 && eat(S, a, h, hb, hr, false)) return true; }
  if (!onShift && hasBallot(S, a) && hr >= 8 && hr < 17.6) {
    if (travel(S, a, { b: byType(S, 'townhall') }, { type: 'vote', dur: 12, desc: 'Casting a vote at the Town Hall' }, 3)) return true;
  }
  if (!onShift && a.hunger > (mealtime(hr) ? 36 : 66) && eat(S, a, h, hb, hr, false)) return true;
  if (a.job && work(S, a, hr, wd)) return true;
  if (a.age < 16 && a.enrolled && wd !== 6 && hr >= 7.4 && hr < 13.8) {
    const sf = firmOfType(S, 'school');
    if (sf && sf.open && !sf.suspended) {
      if (a.inside === h.home && a.lunch < 0.3 && h.inv.food >= 1) { h.inv.food -= 0.3; a.lunch += 0.3; }
      if (travel(S, a, { b: building(S, sf.b) }, { type: 'school', dur: Math.max(20, (14 - hr) * 60), desc: `At lessons in ${sf.name}`, walk: 'Walking to school' }, 3)) return true;
    }
  }
  if (a.age >= 14) {
    if (corvee(S, a, hr)) return true;
    if (errands(S, a, h, hr)) return true;
    if (!a.job && a.age >= 16 && seekWork(S, a, hr, wd)) return true;
    if (!a.job && a.age >= 10 && a.age < 16 && !a.enrolled && h.money / h.members.length < 6 && seekWork(S, a, hr, wd)) return true;
    if (protest(S, a, hr)) return true;
  }
  return leisure(S, a, h, hb, hr, wd);
}

// -------------------------------------------------------------------- jail

export function jailAgent(S, a, days) {
  a.jail = { until: S.day + Math.max(1, Math.round(days)), days };
  a.protest = false;
  const b = byType(S, 'guardhouse');
  if (b) goTo(S, a, { b }, { type: 'jailed', dur: 120, desc: 'Locked in the watch house', walk: 'Being marched to the watch house' });
}
function release(S, a) {
  const days = a.jail.days;
  a.jail = null;
  a.hunger = Math.min(a.hunger, 50);
  note(S, a, `Released from the watch house after ${days} day${days > 1 ? 's' : ''}.`, 'crime');
  if (a.job && !a.job.owner && days > 3) {
    const f = firm(S, a.job.firm);
    note(S, a, `${f ? f.name : 'My employer'} did not keep my place.`, 'work');
    if (f) f.workers = f.workers.filter((id) => id !== a.id);
    a.job = null;
  }
}

export { DAY };
