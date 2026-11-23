//! What expressions in statutes can see.
//!
//! Three scopes, each a superset of the town scope:
//!  * person scope - "who": a person plus the town and the moment;
//!  * act scope    - "when": a person doing something somewhere (person scope plus the act and its place);
//!  * place scope  - a building or piece of land, its owner and its occupants.
//!
//! Deliberately absent: a person's preferences and genes. The state can only
//! see what is on the record - age, kin, wealth, work, schooling, convictions,
//! legal statuses - never what someone privately wants.
use crate::expr::{eval_bool, eval_num, Scope, Value};
use crate::statute::{Act, Wish};
use crate::types::*;

pub const TOWN_VARS: [&str; 18] = ["hour", "weekday", "day", "year", "is_night", "is_restday", "population", "treasury", "median_wage", "median_wealth", "median_income", "price_food", "price_wood", "price_wares", "price_ale", "enforcers", "unemployment", "town"];
pub const PERSON_VARS: [&str; 41] = [
    "id", "name", "surname", "age", "sex", "is_child", "is_adult", "is_elder", "married", "children", "household_size", "money", "wealth", "wealth_rank",
    "wealth_percentile", "income", "wage", "employed", "job", "workplace", "employer_type", "is_owner", "is_landlord", "owns_home", "properties", "homeless",
    "education", "health", "convictions", "is_enforcer", "is_official", "immigrant", "in_school", "ward", "office", "jailed", "sterile", "parents_alive", "siblings", "born_day", "spouse",
];
pub const PERSON_FUNCS: [&str; 5] = ["has", "holds", "member", "score", "tested"];
pub const ACT_VARS: [&str; 16] = ["good", "price", "quantity", "place", "place_ward", "place_owner", "hours_today", "act_wage", "property_type", "property_value", "properties_owned", "worker", "firm_type", "firm_owner", "amount", "subject"];
pub const PLACE_VARS: [&str; 13] = ["id", "name", "type", "value", "owner_type", "owner", "workers", "residents", "is_rented", "is_business", "is_home", "ward", "rent"];

fn town_var(w: &World, name: &str) -> Option<Value> {
    Some(match name {
        "hour" => w.hour().into(),
        "weekday" => w.weekday().into(),
        "day" => (w.day() + 1).into(),
        "year" => (w.day() / DAYS_PER_YEAR as i32 + 1).into(),
        "is_night" => w.is_night().into(),
        "is_restday" => w.is_rest_day().into(),
        "population" => w.stats.population.into(),
        "treasury" => w.polity.jurisdictions.get(w.polity.town.ix()).map(|j| j.treasury).unwrap_or(0.0).into(),
        "median_wage" => w.stats.median_wage.into(),
        "median_wealth" => w.stats.median_wealth.into(),
        "median_income" => w.stats.median_income.into(),
        "price_food" => w.stats.price[Good::Food as usize].into(),
        "price_wood" => w.stats.price[Good::Wood as usize].into(),
        "price_wares" => w.stats.price[Good::Wares as usize].into(),
        "price_ale" => w.stats.price[Good::Ale as usize].into(),
        "enforcers" => w.stats.enforcers.into(),
        "unemployment" => w.stats.unemployment.into(),
        "town" => w.town_name.as_str().into(),
        _ => return None,
    })
}

fn person_var(w: &World, pid: PersonId, name: &str) -> Option<Value> {
    let p = w.person(pid);
    let h = w.household(p.household);
    let day = w.day();
    let age = p.age_on(day);
    let adults = w.adults_of(h.id).len().max(1) as f32;
    Some(match name {
        "id" => p.id.0.into(),
        "name" => p.name.as_str().into(),
        "surname" => p.surname.as_str().into(),
        "age" => (age.floor()).into(),
        "sex" => (if p.sex == Sex::Female { "female" } else { "male" }).into(),
        "is_child" => (age < ADULT_AGE).into(),
        "is_adult" => (age >= ADULT_AGE).into(),
        "is_elder" => (age >= ELDER_AGE).into(),
        "married" => p.spouse.is_some().into(),
        "spouse" => p.spouse.map(|s| Value::from(s.0)).unwrap_or(Value::Null),
        "children" => p.children.iter().filter(|c| { let c = w.person(**c); c.present && c.household == p.household && c.age_on(day) < ADULT_AGE }).count().into(),
        "household_size" => w.members(h.id).len().into(),
        "money" => (h.money / adults).into(),
        "wealth" => (h.wealth / adults).into(),
        "wealth_rank" => h.wealth_rank.into(),
        "wealth_percentile" => h.wealth_pct.into(),
        "income" => p.income.avg.into(),
        "wage" => p.job.as_ref().map(|j| j.wage).unwrap_or(0.0).into(),
        "employed" => p.job.is_some().into(),
        "job" => p.job.as_ref().map(|j| j.role.name()).unwrap_or("none").into(),
        "workplace" => p.job.as_ref().map(|j| w.place(w.firm(j.firm).place).kind.name()).unwrap_or("none").into(),
        "employer_type" => p.job.as_ref().map(|j| w.firm(j.firm).owner.kind_name()).unwrap_or("none").into(),
        "is_owner" => w.places.iter().any(|pl| pl.owner == Owner::Household(h.id) && pl.firm.is_some()).into(),
        "is_landlord" => w.places.iter().any(|pl| pl.owner == Owner::Household(h.id) && pl.kind == PlaceKind::Home && Some(pl.id) != h.home).into(),
        "owns_home" => h.home.map(|hm| w.place(hm).owner == Owner::Household(h.id)).unwrap_or(false).into(),
        "properties" => w.places.iter().filter(|pl| pl.owner == Owner::Household(h.id)).count().into(),
        "homeless" => h.home.is_none().into(),
        "education" => p.education.into(),
        "health" => p.needs.health.into(),
        "convictions" => p.convictions.into(),
        "is_enforcer" => matches!(p.job.as_ref().map(|j| j.role), Some(Role::Enforcer)).into(),
        "is_official" => (matches!(p.job.as_ref().map(|j| j.role), Some(Role::Enforcer | Role::Clerk | Role::Laborer | Role::Scholar)) || !w.polity.offices_of(crate::polity::Actor::Person(pid)).is_empty()).into(),
        "immigrant" => p.immigrant.into(),
        "in_school" => p.enrolled.into(),
        "ward" => h.home.map(|hm| w.polity.jur(w.place(hm).ward).name.as_str()).unwrap_or("").into(),
        "office" => w.polity.offices_of(crate::polity::Actor::Person(pid)).first().map(|o| w.polity.office(*o).def.name.as_str()).unwrap_or("").into(),
        "jailed" => p.jailed_until.is_some().into(),
        "sterile" => p.sterile.into(),
        "parents_alive" => [p.mother, p.father].iter().flatten().filter(|m| w.person(**m).alive).count().into(),
        "siblings" => p.mother.map(|m| w.person(m).children.len().saturating_sub(1)).unwrap_or(0).into(),
        "born_day" => p.birth_day.into(),
        _ => return None,
    })
}

fn person_call(w: &World, pid: PersonId, name: &str, args: &[Value]) -> Option<Value> {
    let arg = args.first().map(|v| v.as_str()).unwrap_or("");
    match name {
        "has" => Some(w.status_ix(arg).map(|i| w.person(pid).has_status(i)).unwrap_or(false).into()),
        "holds" => Some(w.polity.offices.iter().any(|o| !o.abolished && o.holder == Some(crate::polity::Actor::Person(pid)) && o.def.name.eq_ignore_ascii_case(arg)).into()),
        // An untested person has no score: `score("x") < 0.3` is false for them, as is `score("x") > 0.3`.
        "score" => Some(w.assessment_names.iter().position(|n| n.eq_ignore_ascii_case(arg)).and_then(|i| w.person(pid).score(i)).map(Value::from).unwrap_or(Value::Null)),
        "tested" => Some(w.assessment_names.iter().position(|n| n.eq_ignore_ascii_case(arg)).and_then(|i| w.person(pid).score(i)).is_some().into()),
        "member" => Some(w.polity.bodies.iter().any(|b| !b.abolished && b.members.contains(&crate::polity::Actor::Person(pid)) && b.def.name.eq_ignore_ascii_case(arg)).into()),
        _ => None,
    }
}

/// A person, the town and the moment.
pub struct PersonScope<'a> {
    pub w: &'a World,
    pub pid: PersonId,
    /// Optional extra numbers by name (e.g. `amount` for a tax base).
    pub extra: &'a [(&'a str, f32)],
}
impl<'a> PersonScope<'a> {
    pub fn new(w: &'a World, pid: PersonId) -> Self { PersonScope { w, pid, extra: &[] } }
}
impl Scope for PersonScope<'_> {
    fn get(&self, path: &[String]) -> Value {
        let n = path[0].as_str();
        if path.len() == 1 {
            if let Some((_, v)) = self.extra.iter().find(|(k, _)| *k == n) { return (*v).into(); }
            return person_var(self.w, self.pid, n).or_else(|| town_var(self.w, n)).unwrap_or(Value::Null);
        }
        Value::Null
    }
    fn call(&self, name: &str, args: &[Value]) -> Option<Value> { person_call(self.w, self.pid, name, args) }
}

/// A person doing something somewhere.
pub struct ActScope<'a> {
    pub w: &'a World,
    pub pid: PersonId,
    pub act: &'a Act,
}
impl Scope for ActScope<'_> {
    fn get(&self, path: &[String]) -> Value {
        let (w, act) = (self.w, self.act);
        let n = path[0].as_str();
        if path.len() == 2 && (n == "worker" || n == "subject") {
            return act.other.and_then(|o| person_var(w, o, &path[1])).unwrap_or(Value::Null);
        }
        if path.len() > 1 { return Value::Null; }
        let pl = w.place(act.place);
        match n {
            "good" => act.good.map(|g| Value::from(g.name())).unwrap_or(Value::Null),
            "price" => act.price.into(),
            "quantity" => act.quantity.into(),
            "amount" => (act.price * act.quantity).into(),
            "place" => pl.kind.name().into(),
            "place_ward" => w.polity.jur(pl.ward).name.as_str().into(),
            "place_owner" => pl.owner.kind_name().into(),
            "hours_today" => act.hours_today.into(),
            "act_wage" => act.wage.into(),
            "property_type" => act.property.map(|p| Value::from(w.place(p).kind.name())).unwrap_or(Value::Null),
            "property_value" => act.property.map(|p| Value::from(w.place(p).value)).unwrap_or(Value::Null),
            "properties_owned" => act.properties_owned.into(),
            "firm_type" => act.firm.map(|f| Value::from(w.firm(f).kind.name())).unwrap_or(Value::Null),
            "firm_owner" => act.firm.map(|f| Value::from(w.firm(f).owner.kind_name())).unwrap_or(Value::Null),
            _ => person_var(w, self.pid, n).or_else(|| town_var(w, n)).unwrap_or(Value::Null),
        }
    }
    fn call(&self, name: &str, args: &[Value]) -> Option<Value> { person_call(self.w, self.pid, name, args) }
}

/// A remembered act replayed against a (possibly hypothetical) rule: the
/// person as they are now, the act as it was then.
pub struct WishScope<'a> {
    pub w: &'a World,
    pub pid: PersonId,
    pub wish: &'a Wish,
}
impl Scope for WishScope<'_> {
    fn get(&self, path: &[String]) -> Value {
        if path.len() > 1 { return Value::Null; }
        let (w, x) = (self.w, self.wish);
        match path[0].as_str() {
            "good" => x.good.map(|g| Value::from(g.name())).unwrap_or(Value::Null),
            "price" => x.price.into(),
            "quantity" => x.quantity.into(),
            "amount" => (x.price * x.quantity).into(),
            "place" => x.place_kind.name().into(),
            "place_ward" => w.polity.jur(x.ward).name.as_str().into(),
            "hours_today" => x.hours_today.into(),
            "act_wage" => x.wage.into(),
            "hour" => x.hour.into(),
            "weekday" => x.weekday.into(),
            "is_night" => (!(5.0..22.0).contains(&x.hour)).into(),
            "is_restday" => (x.weekday == REST_DAY).into(),
            n => person_var(w, self.pid, n).or_else(|| town_var(w, n)).unwrap_or(Value::Null),
        }
    }
    fn call(&self, name: &str, args: &[Value]) -> Option<Value> { person_call(self.w, self.pid, name, args) }
}

/// A building or piece of land.
pub struct PlaceScope<'a> {
    pub w: &'a World,
    pub place: PlaceId,
}
impl Scope for PlaceScope<'_> {
    fn get(&self, path: &[String]) -> Value {
        let w = self.w;
        let pl = w.place(self.place);
        let n = path[0].as_str();
        if n == "owner" {
            let head = match pl.owner { Owner::Household(h) => w.head_of(h), _ => None };
            return match (head, path.get(1)) {
                (Some(h), Some(field)) => person_var(w, h, field).unwrap_or(Value::Null),
                (Some(h), None) => h.0.into(),
                _ => Value::Null,
            };
        }
        if path.len() > 1 { return Value::Null; }
        match n {
            "id" => pl.id.0.into(),
            "name" => pl.name.as_str().into(),
            "type" => pl.kind.name().into(),
            "value" => pl.value.into(),
            "rent" => pl.rent.into(),
            "owner_type" => pl.owner.kind_name().into(),
            "workers" => pl.firm.map(|f| w.firm(f).workers.len()).unwrap_or(0).into(),
            "residents" => w.households.iter().filter(|h| h.home == Some(pl.id)).map(|h| w.members(h.id).len()).sum::<usize>().into(),
            "is_rented" => pl.tenant.is_some().into(),
            "is_business" => pl.firm.map(|f| !w.firm(f).kind.is_org()).unwrap_or(false).into(),
            "is_home" => (pl.kind == PlaceKind::Home).into(),
            "ward" => w.polity.jur(pl.ward).name.as_str().into(),
            _ => town_var(w, n).unwrap_or(Value::Null),
        }
    }
}

/// The town and the moment only (conditions of event rules), plus an optional subject person as `subject.*`.
pub struct TownScope<'a> {
    pub w: &'a World,
    pub subject: Option<PersonId>,
}
impl Scope for TownScope<'_> {
    fn get(&self, path: &[String]) -> Value {
        if path.len() == 2 && path[0] == "subject" {
            return self.subject.and_then(|s| person_var(self.w, s, &path[1])).unwrap_or(Value::Null);
        }
        if path.len() > 1 { return Value::Null; }
        town_var(self.w, &path[0]).unwrap_or(Value::Null)
    }
    fn call(&self, name: &str, args: &[Value]) -> Option<Value> { self.subject.and_then(|s| person_call(self.w, s, name, args)) }
}

// Convenience evaluators.
pub fn person_bool(w: &World, e: &str, pid: PersonId, default: bool) -> bool { eval_bool(e, &PersonScope::new(w, pid), default) }
pub fn person_num(w: &World, e: &str, pid: PersonId, default: f32) -> f32 { eval_num(e, &PersonScope::new(w, pid), default) }
pub fn person_num_with(w: &World, e: &str, pid: PersonId, extra: &[(&str, f32)], default: f32) -> f32 { eval_num(e, &PersonScope { w, pid, extra }, default) }
pub fn act_bool(w: &World, e: &str, pid: PersonId, act: &Act, default: bool) -> bool { eval_bool(e, &ActScope { w, pid, act }, default) }
pub fn act_num(w: &World, e: &str, pid: PersonId, act: &Act, default: f32) -> f32 { eval_num(e, &ActScope { w, pid, act }, default) }
pub fn place_bool(w: &World, e: &str, place: PlaceId, default: bool) -> bool { eval_bool(e, &PlaceScope { w, place }, default) }
pub fn town_bool(w: &World, e: &str, subject: Option<PersonId>, default: bool) -> bool { eval_bool(e, &TownScope { w, subject }, default) }
pub fn town_num(w: &World, e: &str, subject: Option<PersonId>, default: f32) -> f32 { eval_num(e, &TownScope { w, subject }, default) }
pub fn wish_bool(w: &World, e: &str, pid: PersonId, wish: &Wish, default: bool) -> bool { eval_bool(e, &WishScope { w, pid, wish }, default) }

/// People matching a person-scope filter among those present.
pub fn select_people(w: &World, filter: &str) -> Vec<PersonId> {
    w.persons.iter().filter(|p| p.present && person_bool(w, filter, p.id, false)).map(|p| p.id).collect()
}
