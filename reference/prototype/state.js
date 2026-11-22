// Accessors, caches and logging shared by every system.
import { DAY, fmtTime } from './util.js';

export const FIRM = {
  farm: { good: 'food', input: null, inPer: 0, A: 1.2, cap: 7, job: 'farmer', hours: [7, 17] },
  lumberyard: { good: 'wood', input: null, inPer: 0, A: 1.05, cap: 6, job: 'lumberjack', hours: [7, 17] },
  workshop: { good: 'goods', input: 'wood', inPer: 1, A: 0.62, cap: 6, job: 'crafter', hours: [7, 17] },
  brewery: { good: 'ale', input: 'food', inPer: 0.8, A: 0.9, cap: 5, job: 'brewer', hours: [7, 17] },
  tavern: { good: 'drink', input: 'ale', inPer: 0.5, A: 0, cap: 4, job: 'tavernkeeper', hours: [14, 23], service: true },
  clinic: { good: 'healthcare', input: null, A: 0, cap: 3, job: 'healer', hours: [8, 17], service: true },
  school: { good: 'education', input: null, A: 0, cap: 3, job: 'teacher', hours: [8, 15], service: true },
  watch: { good: null, cap: 30, job: 'guard', hours: [6, 18], org: true },
  office: { good: null, cap: 6, job: 'clerk', hours: [8, 17], org: true },
  works: { good: null, cap: 40, job: 'laborer', hours: [7, 16], org: true },
};
export const BASE_PRICE = { food: 2, wood: 2, goods: 6.5, ale: 3, drink: 2.2, healthcare: 7, education: 0.8 };
export const IMPORT_MULT = 1.9;
export const EXPORT_MULT = 0.5;

export function initCache(S) {
  Object.defineProperty(S, '_c', { value: { paths: new Map(), aById: null, hById: null, bById: null, fById: null, scopes: new Map(), idx: null, day: {} }, enumerable: false, writable: true, configurable: true });
}
function byId(S, key, list) {
  let m = S._c[key];
  if (!m) { m = new Map(); for (const x of list) m.set(x.id, x); S._c[key] = m; }
  return m;
}
export const agent = (S, id) => byId(S, 'aById', S.agents).get(id);
export const household = (S, id) => byId(S, 'hById', S.households).get(id);
export const building = (S, id) => byId(S, 'bById', S.buildings).get(id);
export const firm = (S, id) => byId(S, 'fById', S.firms).get(id);
export const hhOf = (S, a) => household(S, a.hh);
export const invalidate = (S, ...keys) => { for (const k of keys) S._c[k] = null; S._c.scopes.clear(); };

export const isAdult = (a) => a.age >= 16;
export const isElder = (a) => a.age >= 60;
export const adultsOf = (S, h) => h.members.map((id) => agent(S, id)).filter((a) => a && isAdult(a));
export const membersOf = (S, h) => h.members.map((id) => agent(S, id)).filter(Boolean);
export const fullName = (a) => `${a.name} ${a.surname}`;

/** A line in a person's own diary. */
export function note(S, a, text, kind = '') {
  a.log.push({ t: S.tick, text, kind });
  if (a.log.length > 90) a.log.splice(0, a.log.length - 90);
}
/** A line in the town chronicle. */
export function chronicle(S, kind, text, extra = {}) {
  S.events.push({ t: S.tick, day: S.day, kind, text, ...extra });
  if (S.events.length > 400) S.events.splice(0, S.events.length - 400);
}
export const stamp = (S) => `Day ${S.day + 1}, ${fmtTime(S.tick)}`;
export const todayTick = (S, hour) => S.day * DAY + Math.round(hour * 60);

export function householdWealth(S, h) {
  let w = h.money;
  for (const b of S.buildings) if (b.owner === 'private' && b.ownerHh === h.id) w += b.value;
  return w;
}
export function propertiesOf(S, h) { return S.buildings.filter((b) => b.owner === 'private' && b.ownerHh === h.id); }
