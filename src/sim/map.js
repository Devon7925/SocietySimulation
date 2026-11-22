// Town layout and pathfinding.
import { rand, rnd, pick, shuffle } from './util.js';
import { FARM_NAMES, TAVERN_NAMES } from './names.js';

export const T = { GRASS: 0, ROAD: 1, WATER: 2, TREE: 3, FIELD: 4, BUILDING: 5, PLAZA: 6, PARK: 7 };
export const MAP_W = 76;
export const MAP_H = 54;

export const BUILDING_SIZE = {
  townhall: [7, 5], market: [8, 6], tavern: [6, 4], chapel: [4, 6], school: [6, 4], clinic: [5, 4],
  guardhouse: [5, 4], workshop: [5, 4], brewery: [5, 4], lumberyard: [5, 4], farm: [5, 4], home: [4, 3], park: [9, 7],
};
export const BASE_VALUE = {
  home: 140, farm: 420, lumberyard: 320, workshop: 300, brewery: 300, tavern: 280, clinic: 240, school: 220,
  townhall: 600, market: 400, guardhouse: 250, chapel: 200, park: 0,
};

const idx = (x, y) => y * MAP_W + x;
const inB = (x, y) => x >= 0 && y >= 0 && x < MAP_W && y < MAP_H;

function hline(tiles, y, x0, x1, t = T.ROAD) { for (let x = x0; x <= x1; x++) tiles[idx(x, y)] = t; }
function vline(tiles, x, y0, y1, t = T.ROAD) { for (let y = y0; y <= y1; y++) tiles[idx(x, y)] = t; }

function rectFree(tiles, x, y, w, h, margin = 1) {
  for (let yy = y - margin; yy < y + h + margin; yy++) {
    for (let xx = x - margin; xx < x + w + margin; xx++) {
      if (!inB(xx, yy)) return false;
      const t = tiles[idx(xx, yy)];
      const inside = xx >= x && xx < x + w && yy >= y && yy < y + h;
      if (inside ? t !== T.GRASS : t === T.BUILDING || t === T.PARK || t === T.WATER) return false;
    }
  }
  return x >= 1 && y >= 1 && x + w < MAP_W - 1 && y + h < MAP_H - 1;
}

/** Find a lot of the given size that fronts a road. `score(cx, cy)` ranks candidates. */
export function findLot(S, tiles, w, h, score) {
  let best = null;
  let bestScore = -Infinity;
  for (let ry = 0; ry < MAP_H; ry++) {
    for (let rx = 0; rx < MAP_W; rx++) {
      if (tiles[idx(rx, ry)] !== T.ROAD) continue;
      const options = [
        [rx - (w >> 1), ry - h], [rx - (w >> 1), ry + 1], [rx - w, ry - (h >> 1)], [rx + 1, ry - (h >> 1)],
      ];
      for (const [x, y] of options) {
        if (!rectFree(tiles, x, y, w, h)) continue;
        const sc = score(x + w / 2, y + h / 2) + rand(S) * 2;
        if (sc > bestScore) { bestScore = sc; best = { x, y, w, h, door: { x: rx, y: ry } }; }
      }
    }
  }
  return best;
}

export function stampBuilding(tiles, b) {
  const t = b.type === 'park' ? T.PARK : T.BUILDING;
  for (let y = b.y; y < b.y + b.h; y++) for (let x = b.x; x < b.x + b.w; x++) tiles[idx(x, y)] = t;
}

function addField(tiles, b) {
  const sizes = [[11, 7], [9, 6], [8, 5], [6, 4]];
  for (const [fw, fh] of sizes) {
    const cands = [
      [b.x - 3, b.y - fh - 1], [b.x - 3, b.y + b.h + 1], [b.x - fw - 1, b.y - 2], [b.x + b.w + 1, b.y - 2],
      [b.x - fw - 1, b.y - fh], [b.x + b.w + 1, b.y - fh], [b.x - fw - 1, b.y + b.h], [b.x + b.w + 1, b.y + b.h],
    ];
    for (const [fx, fy] of cands) {
      let ok = fx >= 1 && fy >= 1 && fx + fw < MAP_W - 1 && fy + fh < MAP_H - 1;
      for (let y = fy; ok && y < fy + fh; y++) for (let x = fx; x < fx + fw; x++) if (tiles[idx(x, y)] !== T.GRASS) { ok = false; break; }
      if (!ok) continue;
      for (let y = fy; y < fy + fh; y++) for (let x = fx; x < fx + fw; x++) tiles[idx(x, y)] = T.FIELD;
      return { x: fx, y: fy, w: fw, h: fh };
    }
  }
  return null;
}

export function generateMap(S) {
  const tiles = new Array(MAP_W * MAP_H).fill(T.GRASS);
  const cx = 38, cy = 27;
  hline(tiles, cy, 0, MAP_W - 1);
  vline(tiles, cx, 5, MAP_H - 1);
  hline(tiles, 16, 16, 60); hline(tiles, 38, 16, 60);
  vline(tiles, 16, 16, 38); vline(tiles, 60, 16, 38);
  vline(tiles, 27, 16, 38); vline(tiles, 49, 16, 38);
  const plaza = { x: 34, y: 24, w: 9, h: 7 };
  for (let y = plaza.y; y < plaza.y + plaza.h; y++) for (let x = plaza.x; x < plaza.x + plaza.w; x++) tiles[idx(x, y)] = T.PLAZA;
  // The lots still need to "see" the plaza edge as road frontage.
  const frontage = tiles.slice();
  for (let i = 0; i < frontage.length; i++) if (frontage[i] === T.PLAZA) frontage[i] = T.ROAD;

  const buildings = [];
  let nextB = 1;
  const dist = (x, y) => Math.hypot(x - cx, (y - cy) * 1.2);
  const place = (type, score, name) => {
    const [w, h] = BUILDING_SIZE[type];
    const flip = type === 'home' && rand(S) < 0.35;
    const lot = findLot(S, frontage, flip ? w + 1 : w, flip ? h + 1 : h, score);
    if (!lot) return null;
    const b = { id: `b${nextB++}`, type, name: name || type, x: lot.x, y: lot.y, w: lot.w, h: lot.h, door: lot.door, owner: 'none', ownerHh: null, firmId: null, tenantHh: null, rent: 0, value: BASE_VALUE[type] };
    stampBuilding(frontage, b);
    stampBuilding(tiles, b);
    buildings.push(b);
    return b;
  };

  // Farms on the outskirts first so they get room for fields.
  const farmNames = shuffle(S, [...FARM_NAMES]);
  const farmSpots = [[7, 22], [8, 33], [69, 22], [68, 33], [33, 48]];
  farmSpots.forEach(([fx, fy], i) => {
    const b = place('farm', (x, y) => -Math.hypot(x - fx, y - fy), `${farmNames[i]} Farm`);
    if (b) {
      b.field = addField(tiles, b);
      if (b.field) for (let y = b.field.y; y < b.field.y + b.field.h; y++) for (let x = b.field.x; x < b.field.x + b.field.w; x++) frontage[idx(x, y)] = T.FIELD;
    }
  });
  place('lumberyard', (x, y) => -Math.hypot(x - 38, y - 9), 'Northwood Lumberyard');
  place('townhall', (x, y) => -dist(x, y) - (y > cy ? 6 : 0), 'Town Hall');
  place('market', (x, y) => -dist(x, y), 'Market Square');
  place('tavern', (x, y) => -dist(x, y), pick(S, TAVERN_NAMES));
  place('chapel', (x, y) => -dist(x, y) * 0.8, 'Chapel of the Hearth');
  place('guardhouse', (x, y) => -dist(x, y) * 0.8, 'Watch House');
  place('park', (x, y) => -Math.abs(dist(x, y) - 9), 'Willow Green');
  place('clinic', (x, y) => -Math.abs(dist(x, y) - 10), "Healer's House");
  place('school', (x, y) => -Math.abs(dist(x, y) - 11), 'Schoolhouse');
  place('workshop', (x, y) => -Math.abs(dist(x, y) - 14) - (x > cx ? 0 : 3), "Wainwright's Workshop");
  place('workshop', (x, y) => -Math.abs(dist(x, y) - 14) - (x < cx ? 0 : 3), 'Anvil & Loom');
  place('brewery', (x, y) => -Math.abs(dist(x, y) - 15), 'Hearthstone Brewery');

  // Park pond.
  const park = buildings.find((b) => b.type === 'park');
  if (park) {
    for (let y = park.y + 2; y < park.y + 5; y++) for (let x = park.x + 3; x < park.x + 6; x++) tiles[idx(x, y)] = T.WATER;
  }

  // Forest in the north, scattered trees elsewhere.
  for (let y = 0; y < MAP_H; y++) {
    for (let x = 0; x < MAP_W; x++) {
      if (tiles[idx(x, y)] !== T.GRASS) continue;
      let nearRoad = false;
      for (let dy = -1; dy <= 1 && !nearRoad; dy++) for (let dx = -1; dx <= 1; dx++) if (inB(x + dx, y + dy) && tiles[idx(x + dx, y + dy)] === T.ROAD) { nearRoad = true; break; }
      const edge = x < 3 || x > MAP_W - 4 || y > MAP_H - 3;
      const p = y < 11 ? (nearRoad ? 0.12 : 0.62) : edge ? 0.3 : dist(x, y) > 30 ? 0.06 : 0;
      if (rand(S) < p) tiles[idx(x, y)] = T.TREE;
    }
  }
  const forage = [];
  for (let y = 5; y < 13; y++) for (let x = 30; x < 47; x++) if (tiles[idx(x, y)] === T.GRASS && Math.abs(x - cx) <= 2) forage.push(idx(x, y));
  if (!forage.length) forage.push(idx(cx, 6));

  const patrol = [];
  for (let y = 14; y <= 40; y += 2) for (let x = 14; x <= 62; x += 2) if (tiles[idx(x, y)] === T.ROAD || tiles[idx(x, y)] === T.PLAZA) patrol.push(idx(x, y));

  S.map = { w: MAP_W, h: MAP_H, tiles, frontage, plaza, exits: [{ x: 0, y: cy }, { x: MAP_W - 1, y: cy }, { x: cx, y: MAP_H - 1 }], forage, patrol, nextB };
  S.buildings = buildings;
  return { place };
}

/** Add a new house to a living town (construction). Returns the building or null. */
export function addHome(S, name) {
  const M = S.map;
  const lot = findLot(S, M.frontage, 4, 3, (x, y) => -Math.hypot(x - 38, y - 27));
  if (!lot) return null;
  const b = { id: `b${M.nextB++}`, type: 'home', name, x: lot.x, y: lot.y, w: lot.w, h: lot.h, door: lot.door, owner: 'none', ownerHh: null, firmId: null, tenantHh: null, rent: 0, value: BASE_VALUE.home };
  stampBuilding(M.frontage, b);
  stampBuilding(M.tiles, b);
  S.buildings.push(b);
  if (S._c) { S._c.paths.clear(); S._c.bById = null; }
  return b;
}

// ---------------------------------------------------------------- pathfinding

const COST = [1.8, 1, Infinity, Infinity, 3, Infinity, 1, 1.3];
const NB = [[1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1], [1, 1, 1.42], [1, -1, 1.42], [-1, 1, 1.42], [-1, -1, 1.42]];
const g = new Float32Array(MAP_W * MAP_H);
const came = new Int32Array(MAP_W * MAP_H);
const stamp = new Int32Array(MAP_W * MAP_H);
let stampId = 0;

class Heap {
  constructor() { this.n = []; this.f = []; }
  push(node, f) {
    const { n, f: fs } = this;
    let i = n.length;
    n.push(node); fs.push(f);
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (fs[p] <= fs[i]) break;
      [n[p], n[i]] = [n[i], n[p]]; [fs[p], fs[i]] = [fs[i], fs[p]];
      i = p;
    }
  }
  pop() {
    const { n, f: fs } = this;
    const top = n[0];
    const ln = n.pop(); const lf = fs.pop();
    if (n.length) {
      n[0] = ln; fs[0] = lf;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1, r = l + 1;
        let m = i;
        if (l < n.length && fs[l] < fs[m]) m = l;
        if (r < n.length && fs[r] < fs[m]) m = r;
        if (m === i) break;
        [n[m], n[i]] = [n[i], n[m]]; [fs[m], fs[i]] = [fs[i], fs[m]];
        i = m;
      }
    }
    return top;
  }
  get size() { return this.n.length; }
}

/** A* over the tile grid. Returns an array of tile indices from start to goal. */
export function findPath(S, sx, sy, gx, gy) {
  sx = Math.round(sx); sy = Math.round(sy); gx = Math.round(gx); gy = Math.round(gy);
  const tiles = S.map.tiles;
  const start = idx(sx, sy), goal = idx(gx, gy);
  if (start === goal) return [start];
  const key = start * 8192 + goal;
  const cache = S._c.paths;
  const hit = cache.get(key);
  if (hit) return hit;
  stampId++;
  const heap = new Heap();
  g[start] = 0; stamp[start] = stampId; came[start] = -1;
  heap.push(start, 0);
  let found = false;
  while (heap.size) {
    const cur = heap.pop();
    if (cur === goal) { found = true; break; }
    const cxx = cur % MAP_W, cyy = (cur / MAP_W) | 0;
    for (const [dx, dy, dc] of NB) {
      const nx = cxx + dx, ny = cyy + dy;
      if (!inB(nx, ny)) continue;
      const ni = idx(nx, ny);
      let c = COST[tiles[ni]];
      if (c === Infinity) { if (ni !== goal) continue; c = 1; }
      if (dx && dy && (COST[tiles[idx(cxx + dx, cyy)]] === Infinity || COST[tiles[idx(cxx, cyy + dy)]] === Infinity)) continue;
      const ng = g[cur] + c * dc;
      if (stamp[ni] !== stampId || ng < g[ni]) {
        g[ni] = ng; stamp[ni] = stampId; came[ni] = cur;
        heap.push(ni, ng + Math.hypot(nx - gx, ny - gy));
      }
    }
  }
  let path;
  if (found) {
    path = [];
    for (let c = goal; c !== -1; c = came[c]) path.push(c);
    path.reverse();
  } else {
    path = [start, goal]; // unreachable: cut straight across rather than strand the walker
  }
  if (cache.size > 6000) cache.clear();
  cache.set(key, path);
  return path;
}

export const tileXY = (i) => [i % MAP_W, (i / MAP_W) | 0];
export const tileIdx = idx;

/** A resting spot inside a building (or area) for a given occupant. */
export function slotIn(b, seed) {
  const iw = Math.max(1, b.w - 1), ih = Math.max(1, b.h - 1);
  const k = Math.abs(seed * 2654435761) % (iw * ih * 4);
  const sx = (k % (iw * 2)) / 2, sy = Math.floor(k / (iw * 2)) / 2;
  return { x: b.x + 0.25 + Math.min(sx, iw - 0.5), y: b.y + 0.25 + Math.min(sy, ih - 0.5) };
}
export function randomSpot(S, area) {
  return { x: Math.round(rnd(S, area.x, area.x + area.w - 1)), y: Math.round(rnd(S, area.y, area.y + area.h - 1)) };
}
