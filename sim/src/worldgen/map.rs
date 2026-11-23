//! The town plan: roads, lots, buildings, fields and trees, the pseudo-places
//! (plaza, streets, forest, gates), and how long it takes to walk between
//! any two places.
use super::names::{FARM_NAMES, TAVERN_NAMES};
use crate::types::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const MAP_W: i32 = 76;
pub const MAP_H: i32 = 54;
/// The crossing of the two main roads, in the middle of the plaza.
const CX: i32 = 38;
const CY: i32 = 27;
/// Homes the founders build (a couple more than there are households).
pub const N_HOMES: usize = 31;
/// Road tiles a person walks in one slot.
pub const TILES_PER_SLOT: u32 = 13;
/// Walking costs are kept in hundredths of a road tile so that paths are exact integers.
const ROAD_COST: u32 = 100;

/// Footprint of a building of each kind, in tiles.
pub fn size_of(kind: PlaceKind) -> (i32, i32) {
    match kind {
        PlaceKind::Townhall => (7, 5),
        PlaceKind::Market => (8, 6),
        PlaceKind::Tavern | PlaceKind::School | PlaceKind::Academy => (6, 4),
        PlaceKind::Chapel => (4, 6),
        PlaceKind::Park => (9, 7),
        PlaceKind::Home => (4, 3),
        PlaceKind::Plaza => (9, 7),
        PlaceKind::Street | PlaceKind::Gate => (1, 1),
        PlaceKind::Forest => (MAP_W, 11),
        PlaceKind::Clinic | PlaceKind::Watchhouse | PlaceKind::Workshop | PlaceKind::Brewery | PlaceKind::Lumberyard | PlaceKind::Farm => (5, 4),
    }
}

/// What a building of each kind is worth when the town is founded.
pub fn base_value(kind: PlaceKind) -> f32 {
    match kind {
        PlaceKind::Home => 140.0,
        PlaceKind::Farm => 420.0,
        PlaceKind::Lumberyard => 320.0,
        PlaceKind::Workshop | PlaceKind::Brewery => 300.0,
        PlaceKind::Tavern => 280.0,
        PlaceKind::Clinic => 240.0,
        PlaceKind::School => 220.0,
        PlaceKind::Academy => 260.0,
        PlaceKind::Townhall => 600.0,
        PlaceKind::Market => 400.0,
        PlaceKind::Watchhouse => 250.0,
        PlaceKind::Chapel => 200.0,
        PlaceKind::Plaza | PlaceKind::Park | PlaceKind::Forest | PlaceKind::Street | PlaceKind::Gate => 0.0,
    }
}

// ------------------------------------------------------------------ tiles

#[inline]
fn in_bounds(m: &Map, x: i32, y: i32) -> bool { x >= 0 && y >= 0 && x < m.w && y < m.h }
#[inline]
fn at(m: &Map, x: i32, y: i32) -> u8 { m.tiles[(y * m.w + x) as usize] }
#[inline]
fn set(m: &mut Map, x: i32, y: i32, t: u8) {
    if in_bounds(m, x, y) { let i = (y * m.w + x) as usize; m.tiles[i] = t; }
}
fn stamp(m: &mut Map, r: Rect, t: u8) {
    for y in r.y..r.y + r.h { for x in r.x..r.x + r.w { set(m, x, y, t); } }
}
fn hline(m: &mut Map, y: i32, x0: i32, x1: i32) { for x in x0..=x1 { set(m, x, y, tile::ROAD); } }
fn vline(m: &mut Map, x: i32, y0: i32, y1: i32) { for y in y0..=y1 { set(m, x, y, tile::ROAD); } }
fn overlaps(a: &Rect, b: &Rect) -> bool { a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h }

/// How a tile counts when looking for somewhere to build: the plaza's edge is
/// as good a frontage as a road, and trees can be cleared.
#[inline]
fn frontage(t: u8) -> u8 {
    match t { tile::PLAZA => tile::ROAD, tile::TREE => tile::GRASS, t => t }
}

/// Places that take up ground (as opposed to the streets, gates, forest and plaza, which are areas).
fn solid(kind: PlaceKind) -> bool { !matches!(kind, PlaceKind::Street | PlaceKind::Gate | PlaceKind::Forest | PlaceKind::Plaza) }

/// Is `r` open ground, clear of the map edge, with a tile's breathing space
/// from any building, park or water - by the tiles and by the places' own footprints?
fn rect_free(m: &Map, places: &[Place], r: Rect) -> bool {
    if !(r.x >= 1 && r.y >= 1 && r.x + r.w < m.w - 1 && r.y + r.h < m.h - 1) { return false; }
    for y in r.y - 1..=r.y + r.h {
        for x in r.x - 1..=r.x + r.w {
            let t = at(m, x, y);
            let inside = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            if inside {
                if frontage(t) != tile::GRASS { return false; }
            } else if matches!(t, tile::BUILDING | tile::PARK | tile::WATER) {
                return false;
            }
        }
    }
    let grown = Rect { x: r.x - 1, y: r.y - 1, w: r.w + 2, h: r.h + 2 };
    !places.iter().any(|p| (solid(p.kind) && overlaps(&p.rect, &grown)) || p.field.is_some_and(|f| overlaps(&f, &r)))
}

/// A site for a building and the road tile at its door.
pub(super) struct Lot {
    pub rect: Rect,
    pub door: (i32, i32),
}

/// Find a free lot of the given size that fronts a road. `score(cx, cy)`
/// ranks candidates by the centre of the lot; a little chance breaks ties.
/// Works on any living world: it reads only the tiles and the places.
pub(super) fn find_lot(w: &mut World, lw: i32, lh: i32, score: impl Fn(f32, f32) -> f32) -> Option<Lot> {
    let World { map, places, rng, .. } = w;
    if map.w <= 0 || map.h <= 0 || map.tiles.len() != (map.w * map.h) as usize { return None; }
    let mut best = None;
    let mut best_score = f32::NEG_INFINITY;
    for ry in 0..map.h {
        for rx in 0..map.w {
            if frontage(at(map, rx, ry)) != tile::ROAD { continue; }
            let options = [(rx - (lw >> 1), ry - lh), (rx - (lw >> 1), ry + 1), (rx - lw, ry - (lh >> 1)), (rx + 1, ry - (lh >> 1))];
            for (x, y) in options {
                let rect = Rect { x, y, w: lw, h: lh };
                if !rect_free(map, places, rect) { continue; }
                let sc = score(x as f32 + lw as f32 / 2.0, y as f32 + lh as f32 / 2.0) + rng.f32() * 2.0;
                if sc > best_score { best_score = sc; best = Some(Lot { rect, door: (rx, ry) }); }
            }
        }
    }
    best
}

/// Mark a new building's footprint on the tiles.
pub(super) fn stamp_building(m: &mut Map, r: Rect) { stamp(m, r, tile::BUILDING); }

// ------------------------------------------------------------------ wards

/// The middle of town: the centre of the plaza (or of the map, if there is no plaza).
pub(super) fn centre(w: &World) -> (f32, f32) {
    match w.places.get(w.map.plaza.ix()) {
        Some(p) if p.kind == PlaceKind::Plaza => (p.rect.x as f32 + p.rect.w as f32 / 2.0, p.rect.y as f32 + p.rect.h as f32 / 2.0),
        _ => (w.map.w as f32 / 2.0, w.map.h as f32 / 2.0),
    }
}

/// The ward a point lies in. The founders' four wards are the quarters north,
/// east, south and west of the plaza, divided along the diagonals through it.
/// A town divided some other way gives a new site the ward of its nearest neighbour.
pub(super) fn ward_at(w: &World, x: f32, y: f32) -> JurId {
    let wards = &w.polity.wards;
    if wards.len() == 4 {
        let (cx, cy) = centre(w);
        let (dx, dy) = (x - cx, y - cy);
        let i = if dy.abs() >= dx.abs() { if dy <= 0.0 { 0 } else { 2 } } else if dx > 0.0 { 1 } else { 3 };
        return wards[i];
    }
    let mut best: Option<(f32, JurId)> = None;
    for p in w.places.iter().filter(|p| solid(p.kind)) {
        let (px, py) = (p.rect.x as f32 + p.rect.w as f32 / 2.0, p.rect.y as f32 + p.rect.h as f32 / 2.0);
        let d = (px - x) * (px - x) + (py - y) * (py - y);
        if best.is_none_or(|(bd, _)| d < bd) { best = Some((d, p.ward)); }
    }
    best.map(|(_, ward)| ward).or_else(|| wards.first().copied()).unwrap_or(w.polity.town)
}

fn rect_ward(w: &World, r: &Rect) -> JurId { ward_at(w, r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0) }

/// Add a place to the world. It starts unowned, with no firm.
pub(super) fn push_place(w: &mut World, kind: PlaceKind, name: &str, rect: Rect, door: (i32, i32), ward: JurId) -> PlaceId {
    let id = PlaceId(w.places.len() as u32);
    w.places.push(Place { id, kind, name: name.to_string(), ward, rect, door, owner: Owner::None, tenant: None, rent: 0.0, value: base_value(kind), firm: None, field: None });
    id
}

// ------------------------------------------------------------- generation

/// Distance from the plaza, stretched a little north-south so the town grows wider than tall.
fn dist(x: f32, y: f32) -> f32 {
    let (dx, dy) = (x - CX as f32, (y - CY as f32) * 1.2);
    (dx * dx + dy * dy).sqrt()
}
fn gap(x: f32, y: f32, px: f32, py: f32) -> f32 { ((x - px) * (x - px) + (y - py) * (y - py)).sqrt() }

/// Put up a building on the best free lot by `score`.
fn build(w: &mut World, kind: PlaceKind, name: &str, roomy: bool, score: impl Fn(f32, f32) -> f32) -> Option<PlaceId> {
    let (bw, bh) = size_of(kind);
    let (bw, bh) = if roomy { (bw + 1, bh + 1) } else { (bw, bh) };
    let lot = find_lot(w, bw, bh, score)?;
    stamp(&mut w.map, lot.rect, if kind == PlaceKind::Park { tile::PARK } else { tile::BUILDING });
    let ward = rect_ward(w, &lot.rect);
    Some(push_place(w, kind, name, lot.rect, lot.door, ward))
}

/// Lay out a field beside a farmhouse, as large as the ground allows: first
/// in the usual spots above, below and beside the house, then anywhere close by.
fn add_field(m: &mut Map, b: Rect) -> Option<Rect> {
    let fits = |m: &Map, f: &Rect| {
        f.x >= 1 && f.y >= 1 && f.x + f.w < m.w - 1 && f.y + f.h < m.h - 1 && (f.y..f.y + f.h).all(|y| (f.x..f.x + f.w).all(|x| at(m, x, y) == tile::GRASS))
    };
    for (fw, fh) in [(11, 7), (9, 6), (8, 5), (6, 4)] {
        let usual = [
            (b.x - 3, b.y - fh - 1), (b.x - 3, b.y + b.h + 1), (b.x - fw - 1, b.y - 2), (b.x + b.w + 1, b.y - 2),
            (b.x - fw - 1, b.y - fh), (b.x + b.w + 1, b.y - fh), (b.x - fw - 1, b.y + b.h), (b.x + b.w + 1, b.y + b.h),
        ];
        let mut found = usual.into_iter().map(|(x, y)| Rect { x, y, w: fw, h: fh }).find(|f| fits(m, f));
        if found.is_none() {
            // Nearest clear ground within a few tiles of the house, leaving a tile's gap around it.
            let yard = Rect { x: b.x - 1, y: b.y - 1, w: b.w + 2, h: b.h + 2 };
            let mut best = i32::MAX;
            for y in b.y - fh - 4..=b.y + b.h + 4 {
                for x in b.x - fw - 4..=b.x + b.w + 4 {
                    let f = Rect { x, y, w: fw, h: fh };
                    let away = (2 * x + fw - 2 * b.x - b.w).abs() + (2 * y + fh - 2 * b.y - b.h).abs();
                    if away < best && !overlaps(&f, &yard) && fits(m, &f) { best = away; found = Some(f); }
                }
            }
        }
        if let Some(f) = found {
            stamp(m, f, tile::FIELD);
            return Some(f);
        }
    }
    None
}

/// Draw the town: roads and plaza, the pseudo-places, the farms and their
/// fields, the public buildings and businesses, the park and its pond, the
/// forest, and the homes. Expects the polity (and so the wards) to exist.
pub(super) fn lay_out(w: &mut World) {
    let town = w.polity.town;
    w.map.w = MAP_W;
    w.map.h = MAP_H;
    w.map.tiles = vec![tile::GRASS; (MAP_W * MAP_H) as usize];
    {
        let m = &mut w.map;
        hline(m, CY, 0, MAP_W - 1);
        vline(m, CX, 5, MAP_H - 1);
        // The ring of streets around the centre, with a back lane beyond it to the north and to the south.
        for y in [11, 16, 38, 43] { hline(m, y, 16, 60); }
        for x in [16, 60] { vline(m, x, 11, 43); }
        for x in [27, 49] { vline(m, x, 16, 38); }
    }

    // The plaza comes first: the wards are reckoned from it.
    let (pw, ph) = size_of(PlaceKind::Plaza);
    let plaza_rect = Rect { x: CX - pw / 2, y: CY - ph / 2, w: pw, h: ph };
    stamp(&mut w.map, plaza_rect, tile::PLAZA);
    let north = w.polity.wards.first().copied().unwrap_or(town);
    let plaza = push_place(w, PlaceKind::Plaza, "The Plaza", plaza_rect, (CX, CY), north);
    w.map.plaza = plaza;
    w.places[plaza.ix()].owner = Owner::State(town);

    // One street per ward, standing for everywhere out of doors in it.
    let wards = w.polity.wards.clone();
    let corners = [(CX, 16), (60, CY), (CX, 38), (16, CY)];
    w.map.streets = wards.iter().enumerate().map(|(i, &ward)| {
        let (x, y) = corners[i % corners.len()];
        let name = format!("{} streets", w.polity.jur(ward).name);
        push_place(w, PlaceKind::Street, &name, Rect { x, y, w: 1, h: 1 }, (x, y), ward)
    }).collect();

    let (fw, fh) = size_of(PlaceKind::Forest);
    let forest_rect = Rect { x: 0, y: 0, w: fw, h: fh };
    let ward = rect_ward(w, &forest_rect);
    w.map.forest = push_place(w, PlaceKind::Forest, "The Northwood", forest_rect, (CX, 5), ward);

    w.map.gates = [("West Gate", (0, CY)), ("East Gate", (MAP_W - 1, CY)), ("South Gate", (CX, MAP_H - 1))].into_iter().map(|(name, (x, y))| {
        let rect = Rect { x, y, w: 1, h: 1 };
        let ward = rect_ward(w, &rect);
        push_place(w, PlaceKind::Gate, name, rect, (x, y), ward)
    }).collect();

    // Farms on the outskirts first so they get room for fields.
    let mut farm_names = FARM_NAMES.to_vec();
    w.rng.shuffle(&mut farm_names);
    let spots = [(7.0, 22.0), (8.0, 33.0), (69.0, 22.0), (68.0, 33.0), (33.0, 48.0)];
    for (i, (fx, fy)) in spots.into_iter().enumerate() {
        let name = format!("{} Farm", farm_names[i % farm_names.len()]);
        if let Some(id) = build(w, PlaceKind::Farm, &name, false, |x, y| -gap(x, y, fx, fy)) {
            let rect = w.places[id.ix()].rect;
            w.places[id.ix()].field = add_field(&mut w.map, rect);
        }
    }
    let (cx, cy) = (CX as f32, CY as f32);
    build(w, PlaceKind::Lumberyard, "Northwood Lumberyard", false, |x, y| -gap(x, y, cx, 9.0));
    let townhall = build(w, PlaceKind::Townhall, "Town Hall", false, |x, y| -dist(x, y) - if y > cy { 6.0 } else { 0.0 });
    let market = build(w, PlaceKind::Market, "Market Square", false, |x, y| -dist(x, y));
    let tavern = TAVERN_NAMES[w.rng.index(TAVERN_NAMES.len())];
    build(w, PlaceKind::Tavern, tavern, false, |x, y| -dist(x, y));
    let chapel = build(w, PlaceKind::Chapel, "Chapel of the Hearth", false, |x, y| -dist(x, y) * 0.8);
    let watchhouse = build(w, PlaceKind::Watchhouse, "Watch House", false, |x, y| -dist(x, y) * 0.8);
    let park = build(w, PlaceKind::Park, "Willow Green", false, |x, y| -(dist(x, y) - 9.0).abs());
    build(w, PlaceKind::Clinic, "Healer's House", false, |x, y| -(dist(x, y) - 10.0).abs());
    build(w, PlaceKind::School, "Schoolhouse", false, |x, y| -(dist(x, y) - 11.0).abs());
    build(w, PlaceKind::Workshop, "Wainwright's Workshop", false, |x, y| -(dist(x, y) - 14.0).abs() - if x > cx { 0.0 } else { 3.0 });
    build(w, PlaceKind::Workshop, "Anvil & Loom", false, |x, y| -(dist(x, y) - 14.0).abs() - if x < cx { 0.0 } else { 3.0 });
    build(w, PlaceKind::Brewery, "Hearthstone Brewery", false, |x, y| -(dist(x, y) - 15.0).abs());
    let academy = build(w, PlaceKind::Academy, "The Academy", false, |x, y| -(dist(x, y) - 12.0).abs());
    for id in [townhall, market, chapel, watchhouse, park, academy].into_iter().flatten() { w.places[id.ix()].owner = Owner::State(town); }
    w.map.market = market.unwrap_or(plaza);

    // The park's pond.
    if let Some(park) = park {
        let r = w.places[park.ix()].rect;
        stamp(&mut w.map, Rect { x: r.x + 3, y: r.y + 2, w: 3, h: 3 }, tile::WATER);
    }

    // Forest in the north, scattered trees towards the edges.
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            if at(&w.map, x, y) != tile::GRASS { continue; }
            let near_road = (-1..=1).any(|dy| (-1..=1).any(|dx| in_bounds(&w.map, x + dx, y + dy) && at(&w.map, x + dx, y + dy) == tile::ROAD));
            let edge = !(3..=MAP_W - 4).contains(&x) || y > MAP_H - 3;
            let p = if y < 11 { if near_road { 0.12 } else { 0.62 } } else if edge { 0.3 } else if dist(x as f32, y as f32) > 30.0 { 0.06 } else { 0.0 };
            if w.rng.f32() < p { set(&mut w.map, x, y, tile::TREE); }
        }
    }

    // Homes, in a loose ring around the centre. Some have a little more room.
    for _ in 0..N_HOMES {
        let roomy = w.rng.chance(0.35);
        build(w, PlaceKind::Home, "House", roomy, |x, y| -(dist(x, y) - 13.0).abs());
    }
}

// ----------------------------------------------------------------- walking

/// Cost of stepping onto a tile, in hundredths of a road tile; None if it cannot be walked on.
fn step_cost(t: u8) -> Option<u32> {
    match t {
        tile::ROAD | tile::PLAZA => Some(ROAD_COST),
        tile::PARK => Some(130),
        tile::GRASS => Some(180),
        tile::FIELD => Some(300),
        _ => None,
    }
}

const NEIGHBOURS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Cheapest walking cost from `from` to every tile (`u32::MAX` where there is
/// no way). Roads are quickest, then parkland, grass, and fields; water,
/// trees and buildings must be walked around; corners cannot be cut. A step
/// costs the mean of the two tiles it joins, so a walk costs the same both ways.
fn flood(m: &Map, from: (i32, i32)) -> Vec<u32> {
    let mut cost = vec![u32::MAX; m.tiles.len()];
    if !in_bounds(m, from.0, from.1) || m.tiles.len() != (m.w * m.h) as usize { return cost; }
    let start = (from.1 * m.w + from.0) as usize;
    cost[start] = 0;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, start as u32)));
    while let Some(Reverse((c, i))) = heap.pop() {
        if c > cost[i as usize] { continue; }
        let (x, y) = (i as i32 % m.w, i as i32 / m.w);
        for (dx, dy) in NEIGHBOURS {
            let (nx, ny) = (x + dx, y + dy);
            if !in_bounds(m, nx, ny) { continue; }
            let Some(step) = step_cost(at(m, nx, ny)) else { continue };
            let diagonal = dx != 0 && dy != 0;
            if diagonal && (step_cost(at(m, nx, y)).is_none() || step_cost(at(m, x, ny)).is_none()) { continue; }
            let pair = step_cost(at(m, x, y)).unwrap_or(ROAD_COST) + step;
            let nc = c + if diagonal { pair * 71 / 100 } else { pair / 2 };
            let ni = (ny * m.w + nx) as usize;
            if nc < cost[ni] { cost[ni] = nc; heap.push(Reverse((nc, ni as u32))); }
        }
    }
    cost
}

/// Cost of reaching `door` given a flood from somewhere else. A door that is
/// itself blocked can still be stepped onto from a neighbouring tile.
fn door_cost(m: &Map, cost: &[u32], door: (i32, i32)) -> Option<u32> {
    if !in_bounds(m, door.0, door.1) { return None; }
    let direct = cost[(door.1 * m.w + door.0) as usize];
    if direct != u32::MAX { return Some(direct); }
    NEIGHBOURS.iter().filter(|(dx, dy)| in_bounds(m, door.0 + dx, door.1 + dy))
        .map(|(dx, dy)| cost[((door.1 + dy) * m.w + door.0 + dx) as usize]).filter(|c| *c != u32::MAX).min().map(|c| c + ROAD_COST)
}

/// Walking cost between two tiles, in hundredths of a road tile; None if there is no way.
pub fn path_cost(m: &Map, from: (i32, i32), to: (i32, i32)) -> Option<u32> { door_cost(m, &flood(m, from), to) }

/// Slots to walk a path of the given cost: at least one, at most 255.
/// Where there is no path at all, guess from the straight distance over rough ground.
fn slots_for(cost: Option<u32>, from: (i32, i32), to: (i32, i32)) -> u8 {
    let cost = cost.unwrap_or_else(|| (((from.0 - to.0).abs() + (from.1 - to.1).abs()) as u32) * 180);
    let per_slot = ROAD_COST * TILES_PER_SLOT;
    cost.div_ceil(per_slot).clamp(1, 255) as u8
}

/// Recompute `Map::travel` for every pair of places from the tiles.
pub(super) fn rebuild_travel(w: &mut World) {
    let n = w.places.len();
    let mut travel = vec![0u8; n * n];
    for a in 0..n {
        let from = w.places[a].door;
        let cost = flood(&w.map, from);
        for b in 0..n {
            if a != b { let to = w.places[b].door; travel[a * n + b] = slots_for(door_cost(&w.map, &cost, to), from, to); }
        }
    }
    w.map.travel = travel;
    w.map.travel_n = n;
}

/// The last place in `w.places` has just been built: give it a row and a
/// column in `Map::travel`. Does nothing if the table does not cover every earlier place.
pub(super) fn extend_travel(w: &mut World) {
    let n = w.map.travel_n;
    if w.places.len() != n + 1 || w.map.travel.len() != n * n { return; }
    let m = n + 1;
    let from = w.places[n].door;
    let cost = flood(&w.map, from);
    let mut travel = vec![0u8; m * m];
    for a in 0..n {
        travel[a * m..a * m + n].copy_from_slice(&w.map.travel[a * n..a * n + n]);
        let to = w.places[a].door;
        let s = slots_for(door_cost(&w.map, &cost, to), from, to);
        travel[n * m + a] = s;
        travel[a * m + n] = s;
    }
    w.map.travel = travel;
    w.map.travel_n = m;
}
