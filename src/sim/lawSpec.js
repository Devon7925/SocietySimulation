// The law language: what a compiled law looks like, which variables its
// expressions may use, and how to check one. This file is the single source of
// truth shared by the simulation, the validator and the LLM prompt.
import { checkExpr } from './expr.js';

export const GOODS = ['food', 'wood', 'goods', 'ale'];
export const VALUE_DIMS = ['liberty', 'equality', 'tradition', 'order', 'prosperity', 'community'];
export const PLACES = ['home', 'farm', 'lumberyard', 'workshop', 'brewery', 'tavern', 'clinic', 'school', 'market', 'townhall', 'guardhouse', 'chapel', 'plaza', 'park', 'forest', 'street'];
export const JOBS = ['farmer', 'lumberjack', 'crafter', 'brewer', 'tavernkeeper', 'healer', 'teacher', 'guard', 'clerk', 'laborer', 'none'];

export const TAX_BASES = ['income', 'wage', 'profit', 'rent_income', 'wealth', 'property', 'head', 'sale', 'import', 'export', 'inheritance'];
export const SUBSIDY_TARGETS = ['food', 'wood', 'goods', 'ale', 'healthcare', 'education', 'rent', 'wages'];
export const PRICE_TARGETS = ['food', 'wood', 'goods', 'ale', 'healthcare', 'education', 'rent', 'wage'];
export const ACTIONS = ['buy', 'sell', 'produce', 'consume', 'work', 'employ', 'be_outside', 'visit', 'protest', 'own_property', 'emigrate', 'immigrate', 'have_child', 'marry', 'steal', 'forage'];
export const BEHAVIORS = ['attend_school', 'work', 'public_labor', 'attend'];
export const TRANSFER_TARGETS = ['state', 'agent', 'occupants', 'auction', 'landless'];
export const PROJECTS = ['amenities', 'housing', 'none'];

export const AGENT_VARS = ['id', 'name', 'surname', 'age', 'sex', 'is_child', 'is_adult', 'is_elder', 'married', 'children', 'household_size', 'money', 'wealth', 'wealth_rank', 'wealth_percentile', 'income', 'wage', 'employed', 'job', 'workplace', 'employer_type', 'is_owner', 'is_landlord', 'owns_home', 'properties', 'homeless', 'education', 'health', 'happiness', 'criminal_record', 'is_guard', 'is_official', 'immigrant', 'in_school'];
export const ENV_VARS = ['hour', 'weekday', 'day', 'is_night', 'is_restday', 'population', 'treasury', 'median_wage', 'median_wealth', 'median_income', 'price_food', 'price_wood', 'price_goods', 'price_ale', 'guards'];
export const ACTION_VARS = ['good', 'price', 'quantity', 'place', 'hours_today', 'property_type', 'property_value', 'properties_owned', 'worker', 'firm_type', 'firm_owner', 'amount'];
export const PROPERTY_VARS = ['id', 'name', 'type', 'value', 'owner_type', 'owner', 'workers', 'residents', 'is_rented', 'is_business', 'is_home'];

const A = [...AGENT_VARS, ...ENV_VARS];
const ACT = [...A, ...ACTION_VARS];
const PROP = [...PROPERTY_VARS, ...ENV_VARS];

const str = (x, d = '') => (typeof x === 'string' ? x : x == null ? d : String(x));
const numv = (x, d = 0) => (typeof x === 'number' && Number.isFinite(x) ? x : Number.isFinite(parseFloat(x)) ? parseFloat(x) : d);

function normPenalty(p) {
  p = p && typeof p === 'object' ? p : {};
  return {
    fine: str(p.fine, '0') || '0',
    jail_days: Math.max(0, Math.min(60, numv(p.jail_days))),
    confiscate: !!p.confiscate,
    exile: !!p.exile,
    execute: !!p.execute,
  };
}

/**
 * Normalise and check a compiled law. Returns { law, errors }. Errors are
 * written so they can be handed straight back to the model for a second try.
 */
export function validateLaw(raw) {
  const errors = [];
  const law = {
    title: str(raw?.title, 'Untitled law').slice(0, 80),
    summary: str(raw?.summary).slice(0, 600),
    provisions: [],
    ideology: {},
    unmodeled: Array.isArray(raw?.unmodeled) ? raw.unmodeled.map((x) => str(x).slice(0, 300)).filter(Boolean).slice(0, 8) : [],
    notes: str(raw?.interpretation_notes ?? raw?.notes).slice(0, 600),
  };
  for (const k of VALUE_DIMS) law.ideology[k] = Math.max(-1, Math.min(1, numv(raw?.ideology?.[k])));
  const provs = Array.isArray(raw?.provisions) ? raw.provisions : [];
  if (provs.length > 12) errors.push('at most 12 provisions per law');
  provs.slice(0, 12).forEach((p, i) => {
    const at = `provisions[${i}] (${p?.type})`;
    const bad = (m) => errors.push(`${at}: ${m}`);
    const expr = (v, allowed, field, dflt) => {
      const s = v == null || v === '' ? dflt : typeof v === 'number' || typeof v === 'boolean' ? String(v) : str(v);
      const e = checkExpr(s, allowed);
      if (e) bad(`${field}: ${e}`);
      return s;
    };
    const one = (v, list, field) => { if (!list.includes(v)) bad(`${field} must be one of ${list.join(' | ')} (got ${JSON.stringify(v)})`); return v; };
    const label = str(p?.label).slice(0, 60);
    switch (p?.type) {
      case 'tax':
        law.provisions.push({
          type: 'tax', label, base: one(p.base, TAX_BASES, 'base'), rate: expr(p.rate, ACT, 'rate', '0'), applies_to: expr(p.applies_to, ACT, 'applies_to', 'true'),
          good: p.good && p.good !== 'any' ? one(p.good, GOODS, 'good') : 'any',
          evasion_fine_multiple: Math.max(0, Math.min(20, numv(p.evasion_fine_multiple, 2))), evasion_jail_days: Math.max(0, Math.min(60, numv(p.evasion_jail_days))),
        });
        break;
      case 'transfer':
        law.provisions.push({ type: 'transfer', label, amount: expr(p.amount, A, 'amount', '0'), period: one(p.period || 'day', ['day', 'week'], 'period'), applies_to: expr(p.applies_to, A, 'applies_to', 'true') });
        break;
      case 'subsidy':
        law.provisions.push({ type: 'subsidy', label, target: one(p.target, SUBSIDY_TARGETS, 'target'), fraction: expr(p.fraction, A, 'fraction', '1'), applies_to: expr(p.applies_to, A, 'applies_to', 'true') });
        break;
      case 'price_control': {
        const q = { type: 'price_control', label, target: one(p.target, PRICE_TARGETS, 'target'), min: p.min === '' || p.min == null ? '' : expr(p.min, ACT, 'min', ''), max: p.max === '' || p.max == null ? '' : expr(p.max, ACT, 'max', ''), applies_to: expr(p.applies_to, ACT, 'applies_to', 'true'), penalty: normPenalty(p.penalty) };
        if (q.min === '' && q.max === '') bad('price_control needs a min or a max');
        const pe = checkExpr(q.penalty.fine, ACT); if (pe) bad(`penalty.fine: ${pe}`);
        law.provisions.push(q);
        break;
      }
      case 'prohibit': {
        const q = { type: 'prohibit', label, action: one(p.action, ACTIONS, 'action'), condition: expr(p.condition, ACT, 'condition', 'true'), penalty: normPenalty(p.penalty) };
        const pe = checkExpr(q.penalty.fine, ACT); if (pe) bad(`penalty.fine: ${pe}`);
        law.provisions.push(q);
        break;
      }
      case 'require': {
        const q = { type: 'require', label, behavior: one(p.behavior, BEHAVIORS, 'behavior'), applies_to: expr(p.applies_to, A, 'applies_to', 'true'), hours_per_week: Math.max(0, Math.min(60, numv(p.hours_per_week, 4))), location: str(p.location, 'plaza'), when: expr(p.when, A, 'when', 'true'), penalty: normPenalty(p.penalty) };
        if (q.behavior === 'attend') one(q.location, PLACES, 'location');
        const pe = checkExpr(q.penalty.fine, A); if (pe) bad(`penalty.fine: ${pe}`);
        law.provisions.push(q);
        break;
      }
      case 'property_transfer':
        law.provisions.push({ type: 'property_transfer', label, select: expr(p.select, PROP, 'select', 'false'), to: one(p.to, TRANSFER_TARGETS, 'to'), to_agent_id: str(p.to_agent_id), compensation: Math.max(0, Math.min(2, numv(p.compensation))) });
        if (p.to === 'agent' && !/^a\d+$/.test(str(p.to_agent_id))) bad('to_agent_id must be a person id like "a12" when to is "agent"');
        break;
      case 'government':
        law.provisions.push({ type: 'government', label, system_name: str(p.system_name, 'Reformed government').slice(0, 40), decision: one(p.decision, ['decree', 'vote'], 'decision'), franchise: expr(p.franchise, A, 'franchise', 'is_adult'), threshold: Math.max(0.05, Math.min(1, numv(p.threshold, 0.5))), citizen_repeal: !!p.citizen_repeal });
        break;
      case 'enforcement':
        law.provisions.push({ type: 'enforcement', label, guards: Math.round(Math.max(-1, Math.min(30, numv(p.guards, -1)))), guard_wage: Math.max(-1, Math.min(200, numv(p.guard_wage, -1))), clerks: Math.round(Math.max(-1, Math.min(6, numv(p.clerks, -1)))) });
        break;
      case 'public_employment':
        law.provisions.push({ type: 'public_employment', label: label || 'Public works', jobs: Math.round(Math.max(0, Math.min(40, numv(p.jobs)))), wage: Math.max(0, Math.min(200, numv(p.wage))), project: one(p.project || 'amenities', PROJECTS, 'project'), applies_to: expr(p.applies_to, A, 'applies_to', 'is_adult') });
        break;
      case 'mint':
        law.provisions.push({ type: 'mint', label, amount: Math.max(0, Math.min(1e6, numv(p.amount))), period: one(p.period || 'once', ['once', 'day', 'week'], 'period') });
        break;
      case 'declaration':
        law.provisions.push({ type: 'declaration', label, text: str(p.text).slice(0, 300), rename_town: str(p.rename_town).slice(0, 30) });
        break;
      case 'repeal':
        law.provisions.push({ type: 'repeal', label, law_ids: (Array.isArray(p.law_ids) ? p.law_ids : []).map((x) => str(x)).slice(0, 12) });
        break;
      default:
        errors.push(`${at}: unknown provision type`);
    }
  });
  return { law, errors };
}

// ------------------------------------------------------------- JSON schema

const S_ = { type: 'string' }, N_ = { type: 'number' }, B_ = { type: 'boolean' };
const en = (list) => ({ type: 'string', enum: list });
const obj = (props) => ({ type: 'object', properties: props, required: Object.keys(props), additionalProperties: false });
const PENALTY = obj({ fine: S_, jail_days: N_, confiscate: B_, exile: B_, execute: B_ });
const prov = (type, props) => obj({ type: { type: 'string', enum: [type] }, label: S_, ...props });

/** JSON schema for structured output from the model. */
export const LAW_SCHEMA = obj({
  kind: en(['law', 'not_a_law']),
  title: S_,
  summary: S_,
  provisions: {
    type: 'array',
    items: {
      anyOf: [
        prov('tax', { base: en(TAX_BASES), rate: S_, applies_to: S_, good: en(['any', ...GOODS]), evasion_fine_multiple: N_, evasion_jail_days: N_ }),
        prov('transfer', { amount: S_, period: en(['day', 'week']), applies_to: S_ }),
        prov('subsidy', { target: en(SUBSIDY_TARGETS), fraction: S_, applies_to: S_ }),
        prov('price_control', { target: en(PRICE_TARGETS), min: S_, max: S_, applies_to: S_, penalty: PENALTY }),
        prov('prohibit', { action: en(ACTIONS), condition: S_, penalty: PENALTY }),
        prov('require', { behavior: en(BEHAVIORS), applies_to: S_, hours_per_week: N_, location: en(PLACES), when: S_, penalty: PENALTY }),
        prov('property_transfer', { select: S_, to: en(TRANSFER_TARGETS), to_agent_id: S_, compensation: N_ }),
        prov('government', { system_name: S_, decision: en(['decree', 'vote']), franchise: S_, threshold: N_, citizen_repeal: B_ }),
        prov('enforcement', { guards: N_, guard_wage: N_, clerks: N_ }),
        prov('public_employment', { jobs: N_, wage: N_, project: en(PROJECTS), applies_to: S_ }),
        prov('mint', { amount: N_, period: en(['once', 'day', 'week']) }),
        prov('declaration', { text: S_, rename_town: S_ }),
        prov('repeal', { law_ids: { type: 'array', items: S_ } }),
      ],
    },
  },
  ideology: obj(Object.fromEntries(VALUE_DIMS.map((k) => [k, N_]))),
  unmodeled: { type: 'array', items: S_ },
  interpretation_notes: S_,
});

// ------------------------------------------------------- reference manual

/** The manual the model reads before compiling a law. Kept static so it caches. */
export const LAW_MANUAL = `
# How this town works

The town is a small agent-based society of roughly 60-120 people. Every person has an age, a household, a home, needs (hunger, rest, company, fun, health), personality traits (conformity, greed, risk tolerance, altruism, diligence, sociability, temperance, thrift) and political values. People work at firms, earn wages or profits, shop at the market, drink at the tavern, marry, raise children, grow old and die. Nothing about how a law plays out is scripted: outcomes come from each person weighing their own gains against the penalty, the odds of getting caught, their sense of duty and whether they agree with the law.

Time: one day has 24 hours; a week has 7 days (weekday 0-6, weekday 6 is the customary rest day); a year is 14 days.
Money: coins. A typical wage is about 8-10 coins a day; food costs about 2 coins a unit and an adult eats about 1 unit a day. The world snapshot gives current prices, wages and wealth: calibrate every amount against it.

Goods: food (farms), wood (lumberyard), goods (workshops, made from wood: tools, clothes, furniture), ale (brewery, made from food; drunk at the tavern or at home).
Services: healthcare (clinic), education (school, children 6-15), housing (homes, owned or rented).
Trade with the outside world happens through a travelling trader at the market who sells imports dear and buys exports cheap.
Firms hire while an extra worker earns more than their wage, and adjust prices to stock levels. Owners take the profit. State-owned firms pay profits into the treasury and are run by a salaried manager. Co-operatives split earnings among their workers.

The state has a treasury, guards (who detect and arrest violators near where they patrol; they must be paid or they quit; underpaid guards take bribes) and clerks (who audit taxes). With no guards, only conscience and custom stand behind the law, and seizures of property cannot be carried out.
Laws are either decreed, or - when the government's decision rule is "vote" - put to a referendum of everyone matching the franchise, who vote by their values and self-interest.
Unhappy people protest in the plaza, evade, bribe, emigrate, and, if enough of them are on the streets, rise up.

# The law format

A law is a list of provisions. Use as many provisions as the text needs and no more. Every condition and amount is an EXPRESSION (a string): numbers, strings in double quotes, the variables listed below, + - * / %, comparisons, && || !, cond ? a : b, x in ["a","b"], and the functions min, max, abs, floor, ceil, round, clamp(x,lo,hi), contains(text, part). Use "true" for "everyone / always".

## Variables about a person (available in every expression about people)
id ("a12"), name, surname, age (years), sex ("female"|"male"), is_child (under 16), is_adult, is_elder (60+), married, children (own children under 16 at home), household_size,
money (household coins per adult), wealth (coins plus property value, per adult), wealth_rank (1 = richest household), wealth_percentile (0 poorest - 100 richest),
income (average coins per day over the last week, all sources), wage (coins per day, 0 if none), employed, job (${JOBS.join(' | ')}), workplace (type of building they work in, or "none"), employer_type ("private" | "state" | "coop" | "none"),
is_owner (owns a business), is_landlord, owns_home, properties (number of buildings their household owns), homeless,
education (0-1), health (0-100), happiness (0-100), criminal_record (number of convictions), is_guard, is_official (guard, clerk or public worker), immigrant, in_school.

## Variables about the moment and the town
hour (0-24, fractional), weekday (0-6), day, is_night (22:00-05:00), is_restday, population, treasury, median_wage, median_wealth, median_income, price_food, price_wood, price_goods, price_ale, guards.

## Provision types

### tax
{ "type":"tax", "label", "base", "rate", "applies_to", "good", "evasion_fine_multiple", "evasion_jail_days" }
base: income (all personal income) | wage | profit (business owners' takings) | rent_income | wealth | property | head | sale | import | export | inheritance
rate: an expression giving a FRACTION of the base (0.15 = 15%), evaluated for each taxpayer, so it can be progressive: "income > 15 ? 0.3 : 0.1". For wealth and property the fraction is charged once a week. For head it is instead a number of COINS per person per week.
applies_to: who pays. For sale/import/export taxes "good" picks the good ("any" for all) and the tax is taken at each sale; "amount" is the sale value.
People may evade; a caught evader pays the tax plus evasion_fine_multiple times the evaded amount (and evasion_jail_days in jail).

### transfer
{ "type":"transfer", "label", "amount", "period", "applies_to" }
Pays "amount" coins to each matching person every "day" or "week" from the treasury (pensions, child allowances, a basic income, a stipend for one named person). If the treasury runs short everyone is paid pro rata.

### subsidy
{ "type":"subsidy", "label", "target", "fraction", "applies_to" }
The treasury pays "fraction" (0-1) of the price whenever a matching person buys the target: ${SUBSIDY_TARGETS.join(' | ')}. fraction "1" makes it free at the point of use (free healthcare, free schooling). "wages" subsidises employers for employing matching workers.

### price_control
{ "type":"price_control", "label", "target", "min", "max", "applies_to", "penalty" }
target: ${PRICE_TARGETS.join(' | ')}. min and/or max are expressions in coins (leave the unused one as ""). For goods the price is per unit; healthcare is per visit; education is per child per day; rent is per home per day; wage is per worker per day (applies_to is then evaluated on the worker, so a minimum wage for adults only is applies_to "age >= 18"). Sellers, landlords and employers may break the limit off the books.

### prohibit
{ "type":"prohibit", "label", "action", "condition", "penalty" }
Forbids an action whenever "condition" holds for the person doing it. Actions and the extra variables each offers:
- buy, sell, consume, produce: good, price, quantity, place; for sell and produce also firm_type and firm_owner ("private"|"state"|"coop")
- work: place (type of workplace), hours_today (hours already worked today), wage
- employ: evaluated on the employer; "worker" is the worker (worker.age, worker.sex, worker.immigrant ...), wage
- be_outside: being out of doors (curfews: use hour / is_night)
- visit: place (${PLACES.join(' | ')}) - going to a kind of place
- protest
- own_property: property_type, property_value, properties_owned (how many their household would own)
- emigrate, immigrate, have_child (children = children they already have), marry, steal (good, quantity), forage (gathering wild food in the forest)
Examples: ban ale = prohibit produce, sell, buy and consume each with condition good == "ale". Child labour ban = prohibit employ with condition "worker.age < 14" (and work with "age < 14"). Day of rest = prohibit work with "weekday == 6". Eight-hour day = prohibit work with "hours_today >= 8". Curfew = prohibit be_outside with "hour >= 21 || hour < 5".

### require
{ "type":"require", "label", "behavior", "applies_to", "hours_per_week", "location", "when", "penalty" }
behavior: attend_school (matching children must be enrolled; their parents answer for it) | work (matching people must hold a job) | public_labor (matching people owe hours_per_week of unpaid labour on public works) | attend (matching people must be at "location" whenever "when" holds, e.g. when "weekday == 6 && hour >= 9 && hour < 11").
Fill unused fields with defaults (hours_per_week 0, location "plaza", when "true").

### penalty (used by prohibit, require, price_control)
{ "fine": expression in coins, "jail_days": number, "confiscate": boolean (seize the goods involved), "exile": boolean, "execute": boolean }
If the law text names no punishment, choose one proportionate to how the text reads.

### property_transfer
{ "type":"property_transfer", "label", "select", "to", "to_agent_id", "compensation" }
A one-off change of ownership carried out by the guards when the law takes effect. "select" is an expression over PROPERTIES with variables: id ("b7"), name, type (${PLACES.slice(0, 12).join(' | ')}), value, owner_type ("private"|"state"|"coop"|"none"), owner (the owning household's head: owner.id, owner.wealth, owner.properties, owner.surname ... or null), workers, residents, is_rented, is_business, is_home.
to: state | agent (set to_agent_id) | occupants (workers of a business become a co-operative; residents of a home become its owners) | auction (sold to the highest bidder, proceeds to the treasury) | landless (given to households that own nothing, occupants first).
compensation: fraction of value paid to the previous owner from the treasury (0 = seizure, 1 = purchase at full value).
To claim one specific property use its id from the snapshot: select "id == \\"b7\\"". To keep something state-owned or forbid re-accumulation afterwards, add a prohibit own_property provision.

### government
{ "type":"government", "label", "system_name", "decision", "franchise", "threshold", "citizen_repeal" }
decision "decree": laws take effect at once. decision "vote": every later law (and repeal) is put to a referendum among people matching "franchise" and passes with more than "threshold" of votes cast. citizen_repeal: voters can force a repeal vote on a law most of them have come to oppose. Democracy = vote with franchise "is_adult"; an oligarchy of the rich = vote with "wealth_rank <= 8"; rule of the elders = "age >= 55".

### enforcement
{ "type":"enforcement", "label", "guards", "guard_wage", "clerks" }
Target head-counts and daily guard wage; use -1 to leave a value unchanged.

### public_employment
{ "type":"public_employment", "label", "jobs", "wage", "project", "applies_to" }
The state offers "jobs" positions at "wage" coins a day. project: amenities (public works that make the town pleasanter) | housing (builds state-owned homes; needs wood and goods bought with treasury coin) | none.

### mint
{ "type":"mint", "label", "amount", "period" }  Strike new coins into the treasury: once, or every day or week.

### declaration
{ "type":"declaration", "label", "text", "rename_town" }  A purely symbolic act (a motto, a holiday in name only, renaming the town). rename_town "" unless renaming.

### repeal
{ "type":"repeal", "label", "law_ids" }  Strike existing laws by id from the snapshot. To amend a law, repeal it and restate it.

# Your output

- kind: "law" normally; "not_a_law" if the text is not an attempt to legislate (then leave provisions empty and explain in summary).
- title: short, like a statute's name. summary: one or two plain sentences saying what the compiled law actually does, in the terms above.
- provisions: the mechanics. Translate faithfully; do not soften, improve or moralise. If the player wants something harsh, foolish or self-serving, compile exactly that - finding out what happens is the game.
- ideology: for each of liberty, equality, tradition, order, prosperity, community, a number from -1 to 1 saying how the law's CONTENT reads to someone who holds that value (liberty: personal and economic freedom from control; equality: levelling of wealth and status; tradition: keeping inherited custom, faith and family ways; order: safety, authority and rule-following; prosperity: commerce, property and growth; community: mutual care and the common good). This describes what the law is, never a guess at how it will turn out. 0 where a value is indifferent.
- unmodeled: anything in the text the mechanics above cannot express (leave empty if nothing). Model the closest mechanical core and list the rest here; never invent mechanics that do not exist.
- interpretation_notes: any judgement call you made (amounts you had to choose, ambiguous wording), briefly. Empty if none.
`;
