// Small shared helpers. All randomness goes through the seeded generator so a
// town can be replayed from its seed.

export function rand(S) {
  let t = (S.rngState = (S.rngState + 0x6d2b79f5) | 0);
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
export const rnd = (S, a, b) => a + rand(S) * (b - a);
export const rint = (S, a, b) => Math.floor(rnd(S, a, b + 1));
export const chance = (S, p) => rand(S) < p;
export const pick = (S, arr) => arr[Math.floor(rand(S) * arr.length)];
export function gauss(S, mean = 0, sd = 1) {
  const u = Math.max(1e-9, rand(S));
  const v = rand(S);
  return mean + sd * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}
export function shuffle(S, arr) {
  for (let i = arr.length - 1; i > 0; i--) {
    const j = Math.floor(rand(S) * (i + 1));
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}
export const clamp = (x, lo, hi) => (x < lo ? lo : x > hi ? hi : x);
export const clamp01 = (x) => clamp(x, 0, 1);
export const lerp = (a, b, t) => a + (b - a) * t;
export const sum = (arr, f = (x) => x) => arr.reduce((s, x) => s + f(x), 0);
export const mean = (arr, f = (x) => x) => (arr.length ? sum(arr, f) / arr.length : 0);
export function median(arr) {
  if (!arr.length) return 0;
  const s = [...arr].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}
export function gini(values) {
  const v = values.map((x) => Math.max(0, x)).sort((a, b) => a - b);
  const n = v.length;
  const total = sum(v);
  if (!n || total <= 0) return 0;
  let acc = 0;
  for (let i = 0; i < n; i++) acc += (2 * (i + 1) - n - 1) * v[i];
  return acc / (n * total);
}
export const round1 = (x) => Math.round(x * 10) / 10;
export const round2 = (x) => Math.round(x * 100) / 100;

// Time. One tick is one minute of town time.
export const DAY = 1440;
export const WEEK = 7;
export const DAYS_PER_YEAR = 14;
export const WEEKDAYS = ['Moonday', 'Tillday', 'Midweek', 'Thatchday', 'Fairday', 'Marketday', 'Restday'];
export const minuteOfDay = (S) => S.tick % DAY;
export const hourOf = (S) => (S.tick % DAY) / 60;
export const weekdayOf = (S) => S.day % WEEK;
export function fmtTime(tick) {
  const m = ((tick % DAY) + DAY) % DAY;
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
}
export const fmtCoins = (x) => (Math.abs(x) >= 100 ? Math.round(x).toString() : (Math.round(x * 10) / 10).toString());
