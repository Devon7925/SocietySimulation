//! The JSON command interface used by the browser (through wasm) and by tests.
//!
//! One entry point: [`call_json`] takes `{"cmd": "...", ...}` and returns
//! `{"ok": true, ...}` or `{"ok": false, "error": "..."}`. The world lives in
//! a thread-local, so a page (or a test) drives exactly one town.
use crate::forecast::{self, Change};
use crate::polity::*;
use crate::statute::*;
use crate::types::*;
use crate::{engine, gov, law, life, stats, worldgen};
use serde_json::{json, Value};
use std::cell::RefCell;

thread_local! {
    static WORLD: RefCell<Option<World>> = const { RefCell::new(None) };
}

pub const ACTIVITIES: [Activity; 29] = [
    Activity::Idle, Activity::Travel, Activity::Sleep, Activity::Eat, Activity::Work, Activity::Patrol, Activity::School, Activity::Shop, Activity::Tavern, Activity::Visit,
    Activity::Host, Activity::Stroll, Activity::Play, Activity::Garden, Activity::Home, Activity::Chapel, Activity::Protest, Activity::Vote, Activity::Apply, Activity::Treatment,
    Activity::Forage, Activity::Beg, Activity::Steal, Activity::Corvee, Activity::Attend, Activity::Jailed, Activity::Campaign, Activity::Rebel, Activity::Leaving,
];
fn act_ix(a: Activity) -> usize { ACTIVITIES.iter().position(|x| *x == a).unwrap_or(0) }

fn actor_name(w: &World, a: Option<Actor>) -> String {
    match a { None => "vacant".into(), Some(Actor::Player) => "You".into(), Some(Actor::Person(p)) => w.persons.get(p.ix()).map(|p| p.full_name()).unwrap_or_else(|| "?".into()) }
}
fn actor_json(w: &World, a: Option<Actor>) -> Value {
    match a { None => Value::Null, Some(Actor::Player) => json!({"player": true, "name": "You"}), Some(Actor::Person(p)) => json!({"person": p.0, "name": actor_name(w, a)}) }
}
fn jur_name(w: &World, j: JurId) -> String { w.polity.jurisdictions.get(j.ix()).map(|j| j.name.clone()).unwrap_or_default() }
fn place_name(w: &World, p: PlaceId) -> String { w.places.get(p.ix()).map(|p| p.name.clone()).unwrap_or_default() }
fn person_name(w: &World, p: PersonId) -> String { w.persons.get(p.ix()).map(|p| p.full_name()).unwrap_or_default() }
fn owner_json(w: &World, o: Owner) -> Value {
    match o {
        Owner::None => json!({"kind": "none", "name": "nobody"}),
        Owner::Household(h) => json!({"kind": "private", "household": h.0, "name": format!("the {} household", w.households.get(h.ix()).map(|h| h.name.as_str()).unwrap_or("?"))}),
        Owner::State(j) => json!({"kind": "state", "name": jur_name(w, j)}),
        Owner::Coop(f) => json!({"kind": "coop", "name": format!("its workers ({})", w.firms.get(f.ix()).map(|f| f.name.as_str()).unwrap_or("?"))}),
    }
}

fn static_view(w: &World) -> Value {
    json!({
        "town": w.town_name, "seed": w.seed,
        "map": {"w": w.map.w, "h": w.map.h, "tiles": w.map.tiles},
        "places": w.places.iter().map(|p| json!({"id": p.id.0, "kind": p.kind.name(), "name": p.name, "ward": p.ward.0, "rect": [p.rect.x, p.rect.y, p.rect.w, p.rect.h], "door": [p.door.0, p.door.1], "field": p.field.map(|f| [f.x, f.y, f.w, f.h])})).collect::<Vec<_>>(),
        "wards": w.polity.wards.iter().map(|j| json!({"id": j.0, "name": jur_name(w, *j)})).collect::<Vec<_>>(),
        "activities": ACTIVITIES.iter().map(|a| serde_json::to_value(a).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "prefs": ALL_PREFS.iter().map(|p| p.name()).collect::<Vec<_>>(),
        "goods": ALL_GOODS.iter().map(|g| g.name()).collect::<Vec<_>>(),
        "fields": ALL_FIELDS.iter().map(|f| f.name()).collect::<Vec<_>>(),
        "weekdays": WEEKDAYS, "slots_per_day": SLOTS_PER_DAY, "days_per_year": DAYS_PER_YEAR,
    })
}

/// Everything the renderer needs each slot, compactly: one row per present person.
/// Row: [id, place, activity, from, to, depart, arrive, flags]; `from`/`to` are -1 when not walking.
/// Flags: 1 protesting, 2 rebelling, 4 jailed, 8 child, 16 elder, 32 enforcer, 64 female.
fn frame(w: &World, follow: Option<PersonId>) -> Value {
    let day = w.day();
    let rows: Vec<Value> = w.persons.iter().filter(|p| p.present).map(|p| {
        let age = p.age_on(day);
        let mut flags = 0;
        if p.civic.protesting { flags |= 1; }
        if p.civic.rebelling { flags |= 2; }
        if p.jailed_until.is_some() { flags |= 4; }
        if age < ADULT_AGE { flags |= 8; }
        if age >= ELDER_AGE { flags |= 16; }
        if matches!(p.job.as_ref().map(|j| j.role), Some(Role::Enforcer)) { flags |= 32; }
        if p.sex == Sex::Female { flags |= 64; }
        let (from, to, dep, arr) = match p.travel { Some(t) => (t.from.0 as i64, t.to.0 as i64, t.depart as i64, t.arrive as i64), None => (-1, -1, 0, 0) };
        json!([p.id.0, p.loc.0, act_ix(p.activity), from, to, dep, arr, flags])
    }).collect();
    let s = &w.stats;
    let mut out = json!({
        "slot": w.slot, "day": day, "hour": w.hour(), "weekday": w.weekday(), "town": w.town_name,
        "people": rows,
        "stats": {"population": s.population, "wellbeing": s.mean_wellbeing, "trust": s.trust, "treasury": w.polity.jurisdictions.get(w.polity.town.ix()).map(|j| j.treasury).unwrap_or(0.0), "median_wage": s.median_wage, "unemployment": s.unemployment, "food": s.price[0], "protesters": w.persons.iter().filter(|p| p.present && p.civic.protesting).count()},
        "bills_open": w.polity.bills.iter().filter(|b| !matches!(b.state, BillState::Passed | BillState::Failed { .. })).count(),
        "possessing": w.polity.player.possessing.map(|p| p.0),
        "in_office": !w.polity.offices_of(Actor::Player).is_empty(),
        "chronicle_len": w.chronicle.len(),
        "last_chronicle_slot": w.chronicle.last().map(|c| c.slot).unwrap_or(0),
    });
    if let Some(p) = follow.and_then(|f| w.persons.get(f.ix())) {
        out["follow"] = json!({"id": p.id.0, "present": p.present, "alive": p.alive, "doing": p.doing, "thought": p.thought, "needs": p.needs, "wellbeing": p.wellbeing.total, "place": place_name(w, p.loc), "money": w.households[p.household.ix()].money});
    }
    out
}

fn roster(w: &World) -> Value {
    let day = w.day();
    Value::Array(w.persons.iter().filter(|p| p.present).map(|p| {
        let h = &w.households[p.household.ix()];
        json!({"id": p.id.0, "name": p.full_name(), "age": p.age_on(day).floor(), "sex": p.sex, "job": p.job.as_ref().map(|j| j.role.name()).unwrap_or(if p.age_on(day) < ADULT_AGE { "child" } else { "none" }),
            "household": h.id.0, "ward": h.home.map(|hm| jur_name(w, w.places[hm.ix()].ward)).unwrap_or_default(), "wellbeing": p.wellbeing.total, "trust": p.trust, "wealth": h.wealth, "office": w.polity.offices_of(Actor::Person(p.id)).first().map(|o| w.polity.office(*o).def.name.clone())})
    }).collect())
}

fn person_view(w: &World, id: PersonId) -> Result<Value, String> {
    let p = w.persons.get(id.ix()).ok_or("no such person")?;
    let h = &w.households[p.household.ix()];
    let day = w.day();
    let rel = |o: Option<PersonId>| o.map(|x| json!({"id": x.0, "name": person_name(w, x), "alive": w.persons[x.ix()].alive}));
    let prefs: serde_json::Map<String, Value> = ALL_PREFS.iter().map(|k| (k.name().to_string(), json!(p.prefs[*k]))).collect();
    let parts: serde_json::Map<String, Value> = ALL_PREFS.iter().take(N_WANTS).enumerate().map(|(i, k)| (k.name().to_string(), json!(p.wellbeing.parts[i]))).collect();
    let mut friends: Vec<&Friend> = p.friends.iter().filter(|f| w.persons[f.id.ix()].present).collect();
    friends.sort_by(|a, b| b.affinity.partial_cmp(&a.affinity).unwrap_or(std::cmp::Ordering::Equal));
    let opinions: Vec<Value> = p.opinions.iter().filter_map(|(sid, v)| {
        let title = w.statute(*sid).map(|s| (s.title.clone(), s.is_active())).or_else(|| w.polity.bills.iter().find(|b| b.statute.id == *sid).map(|b| (b.statute.title.clone(), false)))?;
        Some(json!({"statute": sid.0, "title": title.0, "active": title.1, "opinion": v}))
    }).collect();
    Ok(json!({
        "id": p.id.0, "name": p.full_name(), "first_name": p.name, "sex": p.sex, "age": p.age_on(day), "alive": p.alive, "present": p.present,
        "doing": p.doing, "thought": p.thought, "activity": p.activity, "place": {"id": p.loc.0, "name": place_name(w, p.loc)},
        "job": p.job.as_ref().map(|j| json!({"role": j.role.name(), "firm": j.firm.0, "firm_name": w.firms[j.firm.ix()].name, "wage": j.wage, "owner": j.owner, "since_day": j.since_day})),
        "household": {"id": h.id.0, "name": h.name, "money": h.money, "stock": h.stock, "wealth": h.wealth, "rank": h.wealth_rank, "arrears": h.arrears,
            "home": h.home.map(|hm| json!({"id": hm.0, "name": place_name(w, hm), "rent": w.places[hm.ix()].rent, "rented": w.places[hm.ix()].tenant == Some(h.id), "ward": jur_name(w, w.places[hm.ix()].ward)})),
            "members": w.members(h.id).iter().map(|m| json!({"id": m.0, "name": person_name(w, *m), "age": w.age(*m).floor()})).collect::<Vec<_>>()},
        "needs": p.needs, "education": p.education, "skill": p.skill, "prefs": prefs, "wellbeing": {"total": p.wellbeing.total, "own": p.wellbeing.own, "parts": parts},
        "trust": p.trust, "income": p.income, "convictions": p.convictions, "jailed_until": p.jailed_until, "sterile": p.sterile, "immigrant": p.immigrant, "enrolled": p.enrolled,
        "spouse": rel(p.spouse), "mother": rel(p.mother), "father": rel(p.father),
        "children": p.children.iter().map(|c| json!({"id": c.0, "name": person_name(w, *c), "alive": w.persons[c.ix()].alive, "age": w.age(*c).floor()})).collect::<Vec<_>>(),
        "friends": friends.iter().take(8).map(|f| json!({"id": f.id.0, "name": person_name(w, f.id), "affinity": f.affinity})).collect::<Vec<_>>(),
        "statuses": w.status_names.iter().enumerate().filter(|(i, _)| p.has_status(*i)).map(|(_, n)| n.clone()).collect::<Vec<_>>(),
        "offices": w.polity.offices_of(Actor::Person(id)).iter().map(|o| w.polity.office(*o).def.name.clone()).collect::<Vec<_>>(),
        "bodies": w.polity.bodies_of(Actor::Person(id)).iter().map(|b| w.polity.body(*b).def.name.clone()).collect::<Vec<_>>(),
        "opinions": opinions, "protesting": p.civic.protesting, "rebelling": p.civic.rebelling,
        "diary": p.diary.iter().rev().take(80).collect::<Vec<_>>(),
    }))
}

fn place_view(w: &World, id: PlaceId) -> Result<Value, String> {
    let p = w.places.get(id.ix()).ok_or("no such place")?;
    let firm = p.firm.and_then(|f| w.firms.get(f.ix())).map(|f| json!({
        "id": f.id.0, "kind": f.kind.name(), "name": f.name, "open": f.open, "suspended": f.suspended, "illicit": f.illicit.map(|s| s.0), "owner": owner_json(w, f.owner),
        "manager": f.manager.map(|m| json!({"id": m.0, "name": person_name(w, m)})),
        "workers": f.workers.iter().map(|m| json!({"id": m.0, "name": person_name(w, *m), "wage": w.persons[m.ix()].job.as_ref().map(|j| j.wage).unwrap_or(0.0)})).collect::<Vec<_>>(),
        "cash": f.cash, "stock": f.stock, "input": f.input, "price": f.sell_price, "wage_offer": f.wage_offer, "vacancies": f.vacancies, "capacity": f.capacity,
        "yesterday": f.yesterday, "avg_sales": f.avg_sales, "avg_profit": f.avg_profit, "output": f.kind.output().map(|g| g.name()),
    }));
    let residents: Vec<Value> = w.households.iter().filter(|h| h.home == Some(id)).flat_map(|h| w.members(h.id)).map(|m| json!({"id": m.0, "name": person_name(w, m)})).collect();
    Ok(json!({
        "id": p.id.0, "kind": p.kind.name(), "name": p.name, "ward": jur_name(w, p.ward), "owner": owner_json(w, p.owner), "value": p.value, "rent": p.rent, "rented": p.tenant.is_some(),
        "firm": firm, "residents": residents,
        "present": w.people_at(id).iter().map(|m| json!({"id": m.0, "name": person_name(w, *m), "doing": w.persons[m.ix()].doing})).collect::<Vec<_>>(),
    }))
}

fn describe_selection(s: &Selection) -> String {
    match s {
        Selection::Fixed => "held until removed".into(),
        Selection::Election { method, franchise, weight, .. } => format!("elected by {} vote of those for whom `{franchise}`{}", serde_json::to_value(method).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(), if weight == "1" { String::new() } else { format!(", ballots weighted by `{weight}`") }),
        Selection::Lottery { pool } => format!("drawn by lot from those for whom `{pool}`"),
        Selection::Heredity => "inherited by the holder's eldest child".into(),
        Selection::Appointment { appointer } => format!("appointed by {appointer}"),
        Selection::Merit { score } => format!("goes to whoever scores highest on `{score}`"),
        Selection::Cooptation => "chosen by the sitting members".into(),
        Selection::Auction => "sold to the highest bidder".into(),
        Selection::Force => "held by whoever commands the most force".into(),
    }
}
fn describe_procedure(p: &ProcedureDef) -> Vec<String> {
    let mut out = Vec::new();
    if p.proposers.is_empty() { out.push("Nobody here may propose laws; this place is governed from above.".to_string()); return out; }
    let who: Vec<String> = p.proposers.iter().map(|x| match x {
        Proposer::Office { name } => format!("the {name}"),
        Proposer::Body { name } => format!("any member of the {name}"),
        Proposer::Initiative { franchise, signatures } => format!("anyone for whom `{franchise}` with signatures from {:.0}% of them", signatures * 100.0),
    }).collect();
    out.push(format!("May propose: {}.", who.join("; ")));
    for (i, s) in p.steps.iter().enumerate() {
        out.push(format!("Step {}: {}", i + 1, match s {
            Step::Assent { office } => format!("the {office} must assent."),
            Step::Body { name, threshold } => format!("the {name} votes; passes with more than {:.0}%.", threshold * 100.0),
            Step::Referendum { franchise, threshold, turnout, weight } => format!("a vote of everyone for whom `{franchise}`; passes with more than {:.0}%{}{}.", threshold * 100.0, if *turnout > 0.0 { format!(" if at least {:.0}% turn out", turnout * 100.0) } else { String::new() }, if weight == "1" { String::new() } else { format!(", ballots weighted by `{weight}`") }),
        }));
    }
    if let Some(v) = &p.veto {
        out.push(if v.override_body.is_empty() { format!("The {} may veto.", v.office) } else { format!("The {} may veto; the {} can override with {:.0}%.", v.office, v.override_body, v.override_threshold * 100.0) });
    }
    out
}

fn bill_json(w: &World, b: &Bill) -> Value {
    json!({
        "id": b.id.0, "kind": b.kind, "title": b.statute.title, "summary": b.statute.summary, "statute": b.statute.id.0, "target": b.target.map(|t| t.0),
        "sponsor": actor_json(w, Some(b.sponsor)), "jurisdiction": jur_name(w, b.jur), "constitutional": b.constitutional, "step": b.step, "state": b.state,
        "introduced_day": b.introduced_day, "history": b.history, "forecast": b.forecast, "lines": law::describe(w, &b.statute),
        "awaiting_you": matches!(&b.state, BillState::Assent { office, .. } if w.polity.office(*office).holder == Some(w.polity.player_actor())),
        "you_may_vote": w.polity.player.possessing.map(|p| gov::may_vote(w, p, b.id)).unwrap_or(false),
    })
}

fn government_view(w: &World) -> Value {
    let pl = &w.polity;
    let player = pl.player_actor();
    json!({
        "jurisdictions": pl.jurisdictions.iter().map(|j| json!({
            "id": j.id.0, "name": j.name, "parent": j.parent.map(|p| p.0), "treasury": j.treasury, "amenity": j.amenity, "court": j.court,
            "procedure": describe_procedure(&j.procedure), "amend": describe_procedure(&j.amend), "has_legislature": !j.procedure.proposers.is_empty(),
            "you_may_propose": gov::can_propose(w, player, j.id, false), "you_may_amend": gov::can_propose(w, player, j.id, true),
            "you_may_decree": pl.has_power(player, j.id, Power::Decree),
        })).collect::<Vec<_>>(),
        "offices": pl.offices.iter().filter(|o| !o.abolished).map(|o| json!({
            "id": o.id.0, "name": o.def.name, "jurisdiction": jur_name(w, o.jur), "holder": actor_json(w, o.holder), "powers": o.def.powers, "selection": describe_selection(&o.def.selection),
            "term_days": o.def.term_days, "term_ends": o.term_ends, "salary": o.def.salary, "since_day": o.since_day, "eligibility": o.def.eligibility,
        })).collect::<Vec<_>>(),
        "bodies": pl.bodies.iter().filter(|b| !b.abolished).map(|b| json!({
            "id": b.id.0, "name": b.def.name, "jurisdiction": jur_name(w, b.jur), "seats": b.def.seats, "members": b.members.iter().map(|m| actor_json(w, Some(*m))).collect::<Vec<_>>(),
            "selection": describe_selection(&b.def.selection), "term_days": b.def.term_days, "term_ends": b.term_ends, "salary": b.def.salary,
        })).collect::<Vec<_>>(),
        "units": pl.units.iter().filter(|u| !u.disbanded).map(|u| json!({
            "id": u.id.0, "name": u.def.name, "jurisdiction": jur_name(w, u.jur), "def": u.def,
            "staff": w.firms.get(u.firm.ix()).map(|f| f.workers.iter().map(|m| json!({"id": m.0, "name": person_name(w, *m)})).collect::<Vec<_>>()).unwrap_or_default(),
        })).collect::<Vec<_>>(),
        "instruments": pl.instruments.iter().map(|i| json!({"jurisdiction": jur_name(w, i.jur), "def": i.def})).collect::<Vec<_>>(),
        "bills": pl.bills.iter().rev().take(40).map(|b| bill_json(w, b)).collect::<Vec<_>>(),
        "player": {"possessing": pl.player.possessing.map(|p| json!({"id": p.0, "name": person_name(w, p)})), "deposed": pl.player.deposed, "offices": pl.offices_of(Actor::Player).iter().map(|o| pl.office(*o).def.name.clone()).collect::<Vec<_>>(), "overrides": pl.player.overrides},
        "statuses": w.status_names,
        "tech": ALL_FIELDS.iter().map(|f| json!({"field": f.name(), "level": w.tech[*f]})).collect::<Vec<_>>(),
        "settings": w.settings,
    })
}

fn statute_json(w: &World, s: &Statute) -> Value {
    json!({"id": s.id.0, "title": s.title, "summary": s.summary, "text": s.text, "status": s.status, "jurisdiction": jur_name(w, s.jur), "enacted_day": s.enacted_day, "repealed_day": s.repealed_day,
        "passed_by": s.passed_by, "sponsor": actor_json(w, s.sponsor), "stats": s.stats, "lines": law::describe(w, s), "unmodeled": s.unmodeled, "notes": s.notes, "clauses": s.clauses})
}

fn history_view(w: &World) -> Value {
    let h = &w.history;
    let col = |f: &dyn Fn(&Stats) -> f32| -> Vec<f32> { h.iter().map(f).collect() };
    json!({
        "day": h.iter().map(|s| s.day).collect::<Vec<_>>(),
        "population": col(&|s| s.population as f32), "wellbeing": col(&|s| s.mean_wellbeing), "trust": col(&|s| s.trust), "median_wage": col(&|s| s.median_wage), "median_wealth": col(&|s| s.median_wealth),
        "unemployment": col(&|s| s.unemployment), "gini": col(&|s| s.gini), "treasury": col(&|s| s.treasury), "revenue": col(&|s| s.revenue), "spending": col(&|s| s.spending),
        "offences": col(&|s| s.offences as f32), "punished": col(&|s| s.punished as f32), "bribes": col(&|s| s.bribes as f32), "protesters": col(&|s| s.protesters as f32),
        "food": col(&|s| s.price[0]), "wood": col(&|s| s.price[1]), "wares": col(&|s| s.price[2]), "ale": col(&|s| s.price[3]), "money_supply": col(&|s| s.money_supply), "homeless": col(&|s| s.homeless as f32),
        "prefs": ALL_PREFS.iter().map(|p| json!({"name": p.name(), "mean": h.iter().map(|s| s.pref_mean[*p as usize]).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "now": w.stats,
    })
}

/// The snapshot of the town handed to the drafting model with each request.
fn brief(w: &World) -> String {
    let mut o = String::new();
    let s = &w.stats;
    let day = w.day();
    o += &format!("Town: {} - day {} ({}), year {}. Population {} ({} adults).\n", w.town_name, day + 1, WEEKDAYS[w.weekday() as usize], day / DAYS_PER_YEAR as i32 + 1, s.population, s.adults);
    o += &format!("Money: median wage {:.1}/day, median income {:.1}/day, median household wealth {:.0}, cost of living {:.1}/day. Prices: food {:.1}, wood {:.1}, wares {:.1}, ale {:.1}, a cup at the tavern {:.1}, a clinic visit {:.1}, a day of school {:.1}, median rent {:.1}/day. Unemployment {:.0}%.\n", s.median_wage, s.median_income, s.median_wealth, s.cost_of_living, s.price[0], s.price[1], s.price[2], s.price[3], s.drink_price, s.clinic_price, s.school_price, s.median_rent, s.unemployment * 100.0);
    o += "\nJurisdictions:\n";
    for j in &w.polity.jurisdictions {
        o += &format!("- \"{}\"{} treasury {:.0}. {}\n", j.name, j.parent.map(|p| format!(" (ward of {})", jur_name(w, p))).unwrap_or_else(|| " (the town)".into()), j.treasury, describe_procedure(&j.procedure).join(" "));
    }
    o += "\nOffices:\n";
    for of in w.polity.offices.iter().filter(|o| !o.abolished) {
        o += &format!("- \"{}\" in {}: held by {}; powers {:?}; {}; term {} days; salary {:.1}.\n", of.def.name, jur_name(w, of.jur), actor_name(w, of.holder), of.def.powers, describe_selection(&of.def.selection), of.def.term_days, of.def.salary);
    }
    if w.polity.bodies.iter().any(|b| !b.abolished) { o += "\nBodies:\n"; }
    for b in w.polity.bodies.iter().filter(|b| !b.abolished) {
        o += &format!("- \"{}\" in {}: {} seats; {}.\n", b.def.name, jur_name(w, b.jur), b.def.seats, describe_selection(&b.def.selection));
    }
    o += "\nEnforcement units:\n";
    for u in w.polity.units.iter().filter(|u| !u.disbanded) {
        o += &format!("- \"{}\" in {}: {} staff at {:.1}/day; commander \"{}\".\n", u.def.name, jur_name(w, u.jur), u.def.staff, u.def.wage, u.def.commander);
    }
    if !w.status_names.is_empty() { o += &format!("\nLegal statuses in use: {}.\n", w.status_names.join(", ")); }
    o += "\nStatutes in force (id, title, what it does):\n";
    for st in w.statutes.iter().filter(|s| s.is_active()) {
        o += &format!("- {} \"{}\" [{}]: {}\n", st.id.0, st.title, jur_name(w, st.jur), if st.summary.is_empty() { law::describe(w, st).join(" ") } else { st.summary.clone() });
    }
    o += "\nPlaces (id, name, type, ward, owner):\n";
    for p in w.places.iter().filter(|p| !matches!(p.kind, PlaceKind::Street | PlaceKind::Gate | PlaceKind::Forest)) {
        let owner = match p.owner { Owner::None => "nobody".to_string(), Owner::Household(h) => format!("{} household", w.households[h.ix()].name), Owner::State(j) => format!("state ({})", jur_name(w, j)), Owner::Coop(_) => "its workers".into() };
        o += &format!("- {} \"{}\" {} {} - {}\n", p.id.0, p.name, p.kind.name(), jur_name(w, p.ward), owner);
    }
    o += "\nAdults (id, name, age, job, ward, household wealth):\n";
    for p in w.persons.iter().filter(|p| p.present && p.age_on(day) >= ADULT_AGE) {
        let h = &w.households[p.household.ix()];
        o += &format!("- {} {} {} {} {} {:.0}\n", p.id.0, p.full_name(), p.age_on(day).floor(), p.job.as_ref().map(|j| if j.owner { format!("{} (owner)", j.role.name()) } else { j.role.name().to_string() }).unwrap_or_else(|| "none".into()), h.home.map(|hm| jur_name(w, w.places[hm.ix()].ward)).unwrap_or_else(|| "homeless".into()), h.wealth);
    }
    o
}

fn need<'a>(w: &'a mut Option<World>) -> Result<&'a mut World, String> { w.as_mut().ok_or_else(|| "no world yet: send {\"cmd\":\"new\"} first".to_string()) }
fn pid(v: &Value, key: &str) -> Option<PersonId> { v.get(key).and_then(|x| x.as_u64()).map(|x| PersonId(x as u32)) }

fn parse_statute(w: &World, v: &Value) -> Result<Statute, String> {
    let mut s: Statute = serde_json::from_value(v.clone()).map_err(|e| format!("the statute is not in the right shape: {e}"))?;
    s.id = StatuteId(0);
    s.status = StatuteStatus::Draft;
    s.stats = StatuteStats::default();
    s.enacted_day = None;
    s.repealed_day = None;
    s.jur = w.polity.jur_by_name(&s.jurisdiction).ok_or_else(|| format!("there is no jurisdiction called \"{}\"", s.jurisdiction))?;
    Ok(s)
}

fn dispatch(slot: &mut Option<World>, c: &Value) -> Result<Value, String> {
    let cmd = c.get("cmd").and_then(|x| x.as_str()).ok_or("missing \"cmd\"")?;
    match cmd {
        "new" => {
            let seed = c.get("seed").and_then(|x| x.as_u64()).unwrap_or(1);
            let mut w = worldgen::generate(seed);
            if let Some(s) = c.get("settings") { if let Ok(s) = serde_json::from_value::<Settings>(s.clone()) { w.settings = s; } }
            let v = static_view(&w);
            *slot = Some(w);
            Ok(v)
        }
        "manual" => Ok(json!({"manual": law::manual()})),
        "load" => {
            let text = c.get("save").and_then(|x| x.as_str()).ok_or("missing \"save\"")?;
            let mut w: World = serde_json::from_str(text).map_err(|e| format!("could not read the save: {e}"))?;
            law::rebuild(&mut w);
            let v = static_view(&w);
            *slot = Some(w);
            Ok(v)
        }
        _ => {
            let w = need(slot)?;
            match cmd {
                "static" => Ok(static_view(w)),
                "step" => {
                    let n = c.get("slots").and_then(|x| x.as_u64()).unwrap_or(1).min(96 * 400);
                    for _ in 0..n { engine::step(w); }
                    Ok(frame(w, pid(c, "follow")))
                }
                "frame" => Ok(frame(w, pid(c, "follow"))),
                "roster" => Ok(json!({"people": roster(w)})),
                "person" => person_view(w, pid(c, "id").ok_or("missing \"id\"")?),
                "place" => place_view(w, PlaceId(c.get("id").and_then(|x| x.as_u64()).ok_or("missing \"id\"")? as u32)),
                "government" => Ok(government_view(w)),
                "statutes" => Ok(json!({"statutes": w.statutes.iter().rev().map(|s| statute_json(w, s)).collect::<Vec<_>>()})),
                "history" => Ok(history_view(w)),
                "chronicle" => {
                    let since = c.get("since").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                    Ok(json!({"entries": w.chronicle.iter().filter(|e| e.slot >= since).collect::<Vec<_>>()}))
                }
                "brief" => Ok(json!({"brief": brief(w)})),
                "validate" => {
                    let s = match parse_statute(w, c.get("statute").ok_or("missing \"statute\"")?) { Ok(s) => s, Err(e) => return Ok(json!({"errors": [e], "lines": []})) };
                    let errors = law::validate(w, &s);
                    let lines = if errors.is_empty() { law::describe(w, &s) } else { Vec::new() };
                    let player = w.polity.player_actor();
                    Ok(json!({"errors": errors, "lines": lines, "constitutional": s.is_constitutional(), "may_propose": gov::can_propose(w, player, s.jur, s.is_constitutional()), "decree": w.polity.has_power(player, s.jur, Power::Decree), "jurisdiction": jur_name(w, s.jur)}))
                }
                "forecast" => {
                    let s = parse_statute(w, c.get("statute").ok_or("missing \"statute\"")?)?;
                    let errors = law::validate(w, &s);
                    if !errors.is_empty() { return Err(errors.join("; ")); }
                    let f = forecast::forecast_now(w, &Change::Enact(s));
                    Ok(json!({"summary": f.summary}))
                }
                "propose" => {
                    let mut s = parse_statute(w, c.get("statute").ok_or("missing \"statute\"")?)?;
                    if let Some(t) = c.get("text").and_then(|x| x.as_str()) { s.text = t.to_string(); }
                    s.proposed_day = w.day();
                    let sponsor = w.polity.player_actor();
                    s.sponsor = Some(sponsor);
                    w.forecast.forks_today = 0; // the player's own proposals are always forecast
                    let before = w.statutes.len();
                    let bill = gov::introduce(w, s, sponsor, BillKind::Enact, None)?;
                    let enacted = w.statutes.len() > before;
                    Ok(json!({"bill": bill.0, "enacted": enacted, "bill_view": w.polity.bills.iter().find(|b| b.id == bill).map(|b| bill_json(w, b))}))
                }
                "repeal" => {
                    let id = StatuteId(c.get("id").and_then(|x| x.as_u64()).ok_or("missing \"id\"")? as u32);
                    let target = w.statute(id).filter(|s| s.is_active()).cloned().ok_or("no such statute in force")?;
                    let sponsor = w.polity.player_actor();
                    w.forecast.forks_today = 0;
                    let bill = gov::introduce(w, target, sponsor, BillKind::Repeal, Some(id))?;
                    Ok(json!({"bill": bill.0, "repealed": w.statute(id).map(|s| !s.is_active()).unwrap_or(true)}))
                }
                "assent" => {
                    let bill = BillId(c.get("bill").and_then(|x| x.as_u64()).ok_or("missing \"bill\"")? as u32);
                    gov::player_assent(w, bill, c.get("approve").and_then(|x| x.as_bool()).unwrap_or(true))?;
                    Ok(json!({}))
                }
                "possess" => {
                    gov::possess(w, pid(c, "id"))?;
                    Ok(json!({"possessing": w.polity.player.possessing.map(|p| p.0)}))
                }
                "possess_random" => {
                    let adults = w.present_adults();
                    if adults.is_empty() { return Err("there is nobody to become".into()); }
                    let pick = adults[w.rng.index(adults.len())];
                    gov::possess(w, Some(pick))?;
                    Ok(json!({"possessing": pick.0, "name": person_name(w, pick)}))
                }
                "override" => {
                    let o: Overrides = serde_json::from_value(c.get("overrides").cloned().unwrap_or(Value::Null)).map_err(|e| e.to_string())?;
                    w.polity.player.overrides = o;
                    Ok(json!({}))
                }
                "settings" => {
                    if let Some(s) = c.get("set") { w.settings = serde_json::from_value(s.clone()).map_err(|e| e.to_string())?; }
                    Ok(json!({"settings": w.settings}))
                }
                "opinions" => {
                    // How the town divides on one statute or bill, for the UI.
                    let sid = StatuteId(c.get("id").and_then(|x| x.as_u64()).ok_or("missing \"id\"")? as u32);
                    if c.get("refresh").and_then(|x| x.as_bool()).unwrap_or(false) { life::form_opinions(w, sid); }
                    let ops: Vec<Value> = w.present_adults().iter().filter_map(|p| w.person(*p).opinion(sid).map(|o| json!([p.0, o]))).collect();
                    Ok(json!({"opinions": ops}))
                }
                "refresh_stats" => { stats::refresh(w); Ok(json!({})) }
                "save" => Ok(json!({"save": serde_json::to_string(&*w).map_err(|e| e.to_string())?})),
                other => Err(format!("unknown command \"{other}\"")),
            }
        }
    }
}

/// Run one command. Never panics on bad input.
pub fn call_json(input: &str) -> String {
    let parsed: Result<Value, _> = serde_json::from_str(input);
    let out = match parsed {
        Err(e) => Err(format!("bad JSON: {e}")),
        Ok(c) => WORLD.with(|w| dispatch(&mut w.borrow_mut(), &c)),
    };
    match out {
        Ok(mut v) => {
            if !v.is_object() { v = json!({"value": v}); }
            v["ok"] = Value::Bool(true);
            v.to_string()
        }
        Err(e) => json!({"ok": false, "error": e}).to_string(),
    }
}

/// Run a closure against the live world (tests and the headless runner).
pub fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> Option<R> {
    WORLD.with(|w| w.borrow_mut().as_mut().map(f))
}

// ------------------------------------------------------------ wasm exports
//
// A minimal ABI with no generated glue: the host writes UTF-8 JSON into
// memory obtained from `alloc`, calls `call`, and reads the reply from
// `out_ptr` / `out_len`.

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let p = buf.as_mut_ptr();
    std::mem::forget(buf);
    p
}

/// # Safety
/// `ptr` must come from `alloc(len)` with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// # Safety
/// `ptr..ptr+len` must be valid UTF-8 written by the host into memory from `alloc`.
#[no_mangle]
pub unsafe extern "C" fn call(ptr: *const u8, len: usize) {
    let input = std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("{}");
    let reply = call_json(input);
    OUT.with(|o| *o.borrow_mut() = reply.into_bytes());
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 { OUT.with(|o| o.borrow().as_ptr()) }
#[no_mangle]
pub extern "C" fn out_len() -> usize { OUT.with(|o| o.borrow().len()) }
