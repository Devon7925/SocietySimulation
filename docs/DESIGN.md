# Design

A town of roughly a hundred people with an economy and a government. The player writes laws in plain English; a drafting model compiles them into statutes; the townspeople respond. The player can follow any individual through their day.

## Principles

1. **One source of desire.** Each person has sixteen base preferences (`types::Pref`): ten weights on the components of wellbeing, six dispositions. Nothing else about what a person wants is ever specified. Wanting a job, a drink, an office, a law or a revolt is always *derived* by asking what serves their wellbeing. A selfless person seeks office because, as far as they can foresee, the things they would do with it leave their neighbours better off; a greedy one seeks it for the salary. Nobody is assigned an ideology.
2. **No scripted outcomes.** No rule says "price ceilings cause shortages" or "prohibition breeds smuggling". If such things happen it is because firms, shoppers, witnesses and enforcers each did what suited them. When you find yourself writing a rule about what a policy *results in*, replace it with a rule about what an individual *does*.
3. **Law is data.** A statute (`statute::Statute`) is a list of clauses with expressions for conditions and amounts. Statutes can define taxes, bans, duties, legal statuses, recurring events, offices, voting systems, enforcement units, surveillance, research budgets. The player's laws and the townspeople's own laws are the same kind of thing and go through the same procedure.
4. **The state sees only records.** Expressions in statutes can read age, kin, wealth, work, schooling, convictions and legal statuses (`scope.rs`), never preferences. A policy aimed at what people want has to work through proxies, or through an official assessment (`Clause::Assessment`, `assess.rs`): a test that costs clerk-time, intrudes on the person examined, and records a noisy score that later statutes can read as `score("name")`.
5. **Every link in enforcement is a person.** A witness decides whether to report, an enforcer whether to act, a judge whether to convict - each by `decide.rs`. A law nobody will enforce is words.
6. **The player is not special.** The player starts as holder of the office "Sovereign". Offices can be abolished, made elective, or taken by force; the player can end up with no power at all, and can then choose to live as one townsperson with only that person's rights.
7. **Deterministic and forkable.** `World` is plain data: `Clone` and serde. All randomness goes through `World::rng`. A fork of the world (`depth > 0`) is how people foresee consequences; forks never fork again, never write diaries or the chronicle, and skip government activity.

## Layout

```
sim/src/
  types.rs      core entities and World                      (lead)
  statute.rs    the law format                               (lead)
  polity.rs     institutions: jurisdictions, offices, bills  (lead)
  expr.rs       expression language                          (lead)
  scope.rs      what expressions can see                     (lead)
  decide.rs     shared psychology                            (lead)
  testkit.rs    tiny world for unit tests                    (lead)
  engine.rs forecast.rs stats.rs api.rs                      (lead)
  worldgen/     map, wards, people, firms, founding laws     (worldgen agent)
  economy/      firms, markets, labour, housing              (economy agent)
  law/          validate, describe, enact, conduct, fiscal,  (law agent)
                statuses, event rules
  enforce/      witnesses, enforcers, courts, punishment     (enforce agent)
  gov/          selection, procedure, NPC bills, uprisings   (gov agent)
  life/         needs, daily planner, wellbeing, opinions    (life agent)
  demography/ research/                                      (demography agent)
web/ server/    browser client and the law-drafting endpoint (later)
reference/prototype/   an earlier JavaScript prototype - useful for mechanics and calibration, not for structure
```

Each module's `mod.rs` currently holds its public contract as stubs. **Keep those signatures.** You may add public items, submodules and fields to the state structs your module owns. Do not edit files you do not own. If you truly need a change to a lead-owned type, add the field at the end of the struct under a `// <module>:` comment with `#[serde(default)]`, and say so in your report.

## Time and space

* One slot = 15 minutes; 96 slots a day; 7-day week (weekday 6 is the customary rest day); 28-day year. `World::slot` is absolute. People are adults at 16 and elders at 60.
* The town is a set of **places** (`types::Place`): buildings, plus the plaza, market, park, forest, one street per ward and the gates. Each place lies in a **ward**; each ward is a jurisdiction under the town.
* A person is always *at* a place (`Person::loc`). Walking takes `World::travel_slots(a, b)` slots, during which `Person::travel` is set and `loc` is the destination ward's street. There is no tile-level movement in the simulation; the browser animates walks along the tile map.
* Who can see an act: those at the same place in the same slot. Outdoor places (`PlaceKind::outdoor`) are seen by all present; a private home only by the household and guests.

## People

`Person::genes` are heritable values; `Person::prefs` are the expressed preferences (genes shaped by upbringing, school, peers). Both are `Prefs`, sixteen numbers in 0..=1.

**Wellbeing** (`types::Wellbeing`, maintained by `life`). Ten components, each about -1..=1, smoothed over about a week:

| part | from |
|---|---|
| comfort | hunger, energy, health, having a home, household wares and firewood |
| leisure | fun, company, free hours |
| wealth | household wealth per adult against about 60 days' cost of living (saturating) |
| family | mean `own` wellbeing of household and close kin |
| community | mean `own` wellbeing of friends (by affinity) and of the town |
| autonomy | minus `Felt::constrained` and `Felt::watched`, scaled by `decide::daily_scale` |
| fairness | minus inequality in the town (gini) and minus `Felt::slighted` |
| security | minus `Felt::victimized`, `Felt::disorder`, `Felt::punished`, and the town's offence rate |
| continuity | minus how much the rules and their own circumstances have changed lately |
| esteem | `Felt::honored`, office held, standing among neighbours, minus convictions |

`own` = the self-regarding parts weighted by the person's preferences (everything except family and community). `total` = all ten weighted by preferences, normalised by the sum of weights. **Any module may add to `Person::felt`** during the day; `life::end_of_day` folds it in and clears it.

**Trust** (`Person::trust`, 0..=1) is how rightful they find the government: it drifts toward how they are faring compared with how they used to, falls when they are punished under rules they reject or promised payments fail, and rises when decisions are made by a process they take part in.

**Opinions** (`Person::opinions`): a view of each active statute and each pending bill, -1..=1. For a bill it comes from the forecast (below) as that person perceives it (`decide::perceive`); for a statute in force it shifts toward what they have actually lived: what it has cost or paid them, and how their wellbeing has moved since. `life` maintains these; `life::form_opinions(w, sid)` computes them on demand.

**Wishes** (`Person::wishes`): a capped log of things they did or wanted to do. `law::attempt` appends to it. Replaying wishes against a proposed rule is how a person knows what the rule would cost them (`law::direct_impact`).

## Small decisions and large ones

*Small decisions* (eat, shop, work, obey, report, enforce) are made constantly and use local estimates in coin-equivalents via `decide.rs`: `would_break`, `would_report`, `would_enforce`, `would_take_bribe`, `regard`, `duty`, `sanction_cost`. **Use these; do not invent parallel psychology.**

*Large decisions* (vote on a bill, back a candidate, stand for office, join an uprising) use `forecast::forecast(w, &Change)`. Under the default setting this forks the world twice (with and without the change), runs both forward `horizon_days`, and returns each person's change in wellbeing. People read forecasts imperfectly (`decide::perceive`). `Settings::forecast` can switch this to experience-only or a blend; inside a fork, `forecast` returns first-order effects only (`law::direct_impact`). Forks are costly (tens of milliseconds): call `forecast::try_forecast`, which respects a daily budget, and pre-screen candidates with `law::direct_impact`.

## The day

`engine::step` advances one slot:

```
first slot of the day:
    stats::refresh         medians, prices, wealth ranks
    demography::start_of_day
    law::start_of_day      schedules, statuses, transfers, mint; weekly: audits, levies
    enforce::start_of_day  staffing, quotas, searches, releases
    economy::start_of_day  firm planning; job reviews
    research::start_of_day
    gov::start_of_day      (skipped in forks) terms, selections, sessions, bills
    life::start_of_day
every slot:
    law::on_slot
    for each present person, in id order: life::update_person
    economy::on_slot       production
    enforce::on_slot       patrols, checkpoints, reports
    gov::on_slot           (skipped in forks) polls, assents
last slot of the day:
    economy::end_of_day    payroll, rents, upkeep; weekly housing
    law::end_of_day        flout rates
    life::end_of_day       wellbeing, trust, opinions
    gov::end_of_day        (skipped in forks) unrest, uprisings
    stats::end_of_day
```

## Worldgen

`worldgen::generate(seed) -> World`. Port the map generator from `reference/prototype/map.js` and the population from `world.js`, adapted:

* Map about 76x54 tiles: a central plaza at the crossing of two main roads, a ring of streets with lots, farms with fields on the outskirts, forest to the north, a park with a pond. Tile kinds in `types::tile`.
* **Four wards** (name them, e.g. Northgate, Eastmarket, Southbank, Westfield), each a jurisdiction whose parent is the town. Every place gets the ward it lies in. One `Street` pseudo-place per ward (`Map::streets`, same order as `Polity::wards`), `Gate` places at the road exits, a `Forest` place, the `Plaza`, `Market` and `Park`.
* Buildings: town hall, market, tavern, chapel, watch house, clinic, school, an academy (empty until research is funded), two workshops, a brewery, a lumberyard, five farms, about 31 homes.
* `Map::travel`: slots to walk between every pair of places, from path length over the tiles at about 13 tiles per slot (roads fastest), minimum 1.
* About 29 households, 85-95 people: families with children, couples, singles, elders. Friendships among neighbours and workmates. Owners for each firm (their household is richer), two landlord households holding the rented homes, about 40% of households renting. Staff the firms (farm 3 hands + owner, lumberyard 3, workshop 3, brewery 2, tavern 2, clinic 1, school 1), leaving a few adults free for the watch.
* Preferences: `genes` drawn independently around 0.5 (sd about 0.2, clamped); `prefs` = genes plus modest environmental noise. No correlations with wealth or job are imposed.
* Founding constitution, built directly as `polity` structs: the town jurisdiction with treasury about 260; office **Sovereign** held by `Actor::Player` with all four powers, `Selection::Fixed`; procedure = proposer Sovereign, step Assent Sovereign (same for the amending procedure); wards with empty procedures.
* Founding statutes, passed to `law::enact(w, s, "founding charter")`: **The Common Peace** (forbid `steal`, fine about three times the value plus two days' jail), **The Tithe** (10% levy on income), **The Town Watch** (a unit of 3 at 10 coins a day commanded by the Sovereign, plus one clerk).
* Finish with `stats::refresh(w)`. `generate` must be deterministic in the seed.

## Economy

Port `reference/prototype/economy.js` (and the trade parts of `behavior.js`) to slots. The mechanics there are the intended ones:

* Firms post a price and adjust it daily by stock against recent sales. Producers' output per slot is `A * effort^0.7` (effort = sum of `WorkState::effort` of workers on site, accumulated in `Firm::effort_now` by `life`), scaled by `Tech`. Workshops consume wood, breweries food; taverns sell ale by the cup.
* A firm hires while the marginal worker's product at the net price exceeds the wage offer; raises the offer when a vacancy stays open; lays off when the last worker no longer pays. Co-operatives split earnings and admit members while that does not cut members' income below the going wage. State firms follow the same rules but pass profit to, and draw losses from, their jurisdiction's treasury.
* A travelling trader sells imports dear (about 1.9x base price) and buys exports cheap (about 0.5x), scaled by `Tech[Commerce]` and tariffs: this anchors prices and lets money enter and leave.
* Households hold money and stocks. Rents daily; arrears lead to eviction. Landlords reprice weekly; those with savings build when housing is short.
* Reservation wage rises with household savings and unearned income and falls with diligence: this is how transfers reduce labour supply, if they do.
* Every purchase and sale goes through `law::attempt`, `law::price_bounds`, `law::subsidy`, `law::sale_tax`; every income payment through `law::pay_income`. A manager facing a rule that makes the business unlawful decides, via `law::attempt` with the firm's daily profit as the value, whether to shut, comply, or carry on illicitly (`Firm::illicit`).
* Worker effort (set when someone starts work; `life` calls `economy::effort_of`-style helper you provide or computes via the documented formula): diligence, a stake in the result (owners fully, co-op members partly), being watched by a manager, health, hunger, and how their pay compares with the going wage.

Calibration targets for the default town: median wage 8-10 coins a day, food about 2, unemployment under 10%, no starvation, stable for 100+ days with no player action.

## Law

* `validate` must check every expression against the right variable list (`scope::*_VARS`, `PERSON_FUNCS`), names of offices/bodies/statuses referenced, sensible ranges, and that `Require` is used only with duty actions. Messages are for a drafter to act on.
* `describe` renders each clause as one exact plain-English sentence. This is what the player reads before enacting, so it must say what will mechanically happen.
* `manual()` is the complete reference for a drafter: the world in a paragraph, units and magnitudes, every clause type with its JSON shape and an example, every variable in every scope. It is the system prompt of the drafting model, so it must be accurate to the serde shapes in `statute.rs`.
* `enact` assigns an id if the statute has none (`law::next_id`), resolves `jurisdiction` to `jur`, pushes it to `World::statutes`, then applies clauses with immediate effect: `Property` (owners may defy; seizures need enforcers willing to carry them out - without any, they fail), `Office`/`Body`/`Procedure`/`Abolish` via `gov::apply_*`, `Unit`/`Instrument`/`Court`/`Clerks` via `enforce::apply_*`, one-off `Mint`, `Repeal`, `Declaration`, and one-off event rules. Standing effects of institutional clauses are undone by `repeal` where that is meaningful (a unit created by a repealed statute is disbanded; an office is not abolished by repealing the statute that created it - that takes `Abolish`).
* A statute binds acts at places within its jurisdiction (`applies_at`). Ward statutes therefore bind only in that ward.
* `attempt(w, p, act)`: record a `Wish`; find `Forbid` clauses whose `who` and `when` hold; if none, `Lawful`. Otherwise evaluate the fine, ask `enforce::detection_chance`, and let the person choose with `decide::would_break`. Obeying adds the act's value to `Felt::constrained`. Breaking calls `enforce::report_offence`; if that returns true the act was `Stopped`.
* Duties (`Mode::Require`): `life` asks `law::duty(w, p, action)` at the moments that matter and reports what the person did with `duty_outcome`.
* Fiscal: income taxes are withheld at `pay_income` unless the person is hiding that income; hidden amounts accumulate and are audited weekly, with detection depending on clerks, registers and `Tech[Administration]`. The weekly choice to hide uses `decide::would_break`. Wealth, property and head taxes fall due weekly. Transfers are paid pro rata when the treasury is short, and the shortfall is recorded on the jurisdiction (it erodes trust).
* Statuses: registered in `World::status_names`; held as bits in `Person::statuses`. `Purchase` statuses are bought by people for whom `status_value` exceeds the fee.
* Event rules: scheduled or on `emit`. Effects are in `statute::Effect`.
* Each statute keeps `StatuteStats`; `end_of_day` updates `flout_rate`.

## Enforcement

* Units (`polity::Unit`) are backed by a `Watch` firm created with `economy::found_public_firm`; their pay scheme (`UnitDef`) is settled daily: wage, share of fines, bounty per conviction, quota shortfalls.
* `detection_chance` is what a would-be offender expects: it grows with enforcers on duty where the act happens, with people around who would report it, with watchposts, checkpoints, registers and clerks for paper offences, and with `Tech[Surveillance]` / `Tech[Administration]`.
* `report_offence` runs the real chain: who was present and saw it; whether any of them reports (`decide::would_report`, with informer rewards as the reward); whether an enforcer whose writ runs there acts (`decide::would_enforce`, with the unit's incentives, and supervision from a commander or inspectors); whether a bribe changes hands (`decide::bribe_offer`, `decide::would_take_bribe`); then the court: summary, a judge (an office holder who convicts unless they reject the statute strongly enough), or a jury drawn by lot voting by their own opinions. Then `punish`.
* Enforcers on patrol are at real places; an offence somewhere no enforcer is posted and nobody will report goes unpunished. Enforcers also add to `Felt::watched` of those they check and, by being seen, subtract from `Felt::disorder`.
* Perverse incentives should be possible, not prevented: an enforcer paid by the conviction may charge people who did nothing if that suits them and their scruples allow it.
* `punish` carries out fines (to the jurisdiction's treasury, less shares and rewards), jail (the person walks to the watch house and stays), confiscation, exile and execution (via `demography`), status changes, sterilisation. It writes diaries, updates `Felt::punished`, and affects the kin and friends of the punished.
* `force_of` / `enforcers_of` feed uprisings.

## Government

* Offices and bodies are filled by `statute::Selection`: implement every variant. Elections implement every `VoteMethod` as pure functions over ballots (test them thoroughly). A voter ranks candidates by what the candidate's *platform* is forecast to do for them plus how closely the candidate's views of the standing statutes match theirs plus personal regard.
* A candidate's platform is the bill they would most like to introduce (see drafting below). Someone stands for an office when the salary, the standing and what their platform would do for the things they care about outweigh the bother.
* Lawmaking follows `Jurisdiction::procedure` (or `amend` for constitutional bills): proposers, steps, veto. An actor with `Power::Decree` may enact directly. A body votes by its members' opinions; members who want to keep their seats also weigh their voters' opinions. Referendums are voted in person at the town hall during the day (`life` sends people; `cast_ballots` records). The player votes or assents through the API; a townsperson holding an office decides by their own opinion.
* **Nobody legislates against themselves.** A townsperson introduces, assents to or votes for a bill only if, by their own reading of the forecast (or failing that of its direct effects), it leaves what they care about better off. No language model is involved in townspeople's lawmaking.
* **Townspeople legislate.** An office holder or body member with the right to propose drafts candidate bills by searching the statute space: repeal a statute they dislike; change a number in an existing one; or instantiate a clause schema (levy, transfer, subsidy, price rule, ban, public works, research line, enforcement change, constitutional reform) aimed at whatever is weighing most on the things they care about. Candidates are screened by `law::direct_impact` and the best few are forecast; they introduce what the forecast says serves them. This is a search evaluated by simulation, not a list of canned policies: schemas supply *shape*, the search supplies parameters.
* **Unrest.** People protest when they are aggrieved enough and bold enough, and more readily when others already are (`gov::Civic::protesting`; `life` sends them to the plaza). An uprising begins when enough of the adult population is in the streets for long enough. Each enforcer then chooses a side by their own lights; the outcome is decided by numbers and arms (`enforce::force_of`). If it succeeds: holders of offices the crowd most resents are removed, the statutes the rebels most oppose are repealed, and the rebels' most-regarded figure takes power and reshapes the constitution as *their* forecast of their own interests and cares suggests - which may or may not mean elections.
* `possess`: when the player holds no office they may take over a townsperson, gaining exactly that person's rights (their vote, their candidacy, any office they win) until the person dies.

## Life

Port the planner in `reference/prototype/behavior.js` to slots and places. Every time an activity ends, the person picks the next by need, hour, purse and law: sleep; meals at home, packed for work; the working day in blocks with a dinner break; school; marketing; the clinic; looking for work; duties the law imposes; voting; protest; evenings at the tavern, with friends, in the park, at chapel on the rest day, or at home; and, in want, kin, foraging, begging or theft. Enforcers patrol the places their unit is posted to.

* Needs move per slot (prototype rates are per 10 minutes: scale by 1.5).
* On starting work set `WorkState::effort` (see Economy) and each slot add it to `Firm::effort_now` and increment `WorkState::slots_today`.
* Call `law::attempt` for every act the law might care about, with a truthful `value`.
* Diary lines (`World::note`) should let a reader follow the day: meals bought, work, pay, what they could not afford, who they saw, what they thought of a law. `Person::doing` is the present activity; `Person::thought` a passing line in their own voice, derived from their situation.
* `end_of_day` updates wellbeing, trust and opinions as defined above.

## Demography

* Ageing, illness and death (mortality rises with age and poor health, falls with `Tech[Medicine]`), births to couples who can afford and want another child (`law::attempt(HaveChild)` applies), courtship among friends and marriage (`Marry`), coming of age and leaving home, households merging and dissolving, estates (`law::inheritance_tax`), emigration by the miserable (`Emigrate`) and immigration when the town looks better than elsewhere (`Immigrate`). `law::emit` the corresponding events.
* **Inheritance of preferences.** `inherit`: a child's `genes` are the mean of the parents' genes plus mutation noise; expressed `prefs` = `heritability * genes + (1 - heritability) * upbringing`, where upbringing starts as the parents' expressed preferences plus noise. Weekly until adulthood, a child's `prefs` drift slightly toward those of household adults, of friends, and - while enrolled - of their teachers. This is the only way the distribution of desires in the town changes; a policy that changes who has children, who raises them or who teaches them will change it, slowly.
* Research: each `Clause::Research { field, per_day }` in force funds scholars (educated adults, hired into an `Academy` firm via `economy::found_public_firm`). Each day, funded and staffed fields accumulate progress in proportion to scholars' ability; each field's next step costs more than the last; on completing a step the field's `Tech` multiplier rises by a random amount (a few percent, occasionally much more, sometimes nothing). Unfunded fields do not regress.

## Conventions

* No `unsafe`, no threads, no wall-clock, no dependencies beyond `serde` / `serde_json`.
* Iterate people in id order; never iterate a `HashMap` where order affects results (use `Vec` or `BTreeMap`).
* Performance matters: a day for 100 people should cost about a millisecond. Avoid allocating per person per slot; most slots a person is mid-activity and should cost almost nothing.
* Never index with an id you have not checked when it came from outside (API, statutes).
* Coins are `f32`. Nothing should create or destroy money except the trader, the mint and explicit sinks; say so where you do.
* Unit tests live beside the code and use `testkit::tiny_world()`. Other modules are stubs while you work, so test your own logic directly.
* Diary and chronicle text is plain, period-neutral English in the person's or the town's voice. No jargon, no exclamation.
