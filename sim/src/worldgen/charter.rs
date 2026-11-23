//! The founding constitution and the first three statutes.
use super::names::WARD_NAMES;
use crate::polity::*;
use crate::statute::*;
use crate::types::*;

/// Coins in the town's treasury on the first morning.
pub const FOUNDING_TREASURY: f32 = 260.0;
/// The office the player begins in.
pub const SOVEREIGN: &str = "Sovereign";

/// The town and its four wards, and the one office there is: the Sovereign,
/// held by the player for life with every power, who alone proposes and
/// assents to law. The wards have no lawmakers of their own.
/// Expects `w.town_name` to be set and the polity to be empty.
pub(super) fn found_polity(w: &mut World) {
    let rule = ProcedureDef {
        constitutional: false,
        proposers: vec![Proposer::Office { name: SOVEREIGN.into() }],
        steps: vec![Step::Assent { office: SOVEREIGN.into() }],
        veto: None,
    };
    let town = JurId(0);
    let jurisdiction = |id: JurId, name: &str, parent: Option<JurId>, treasury: f32, procedure: ProcedureDef, amend: ProcedureDef| Jurisdiction {
        id, name: name.to_string(), parent, treasury, procedure, amend,
        court: CourtDef::Summary, revenue_today: 0.0, spending_today: 0.0, shortfall_today: 0.0, amenity: 0.0,
    };
    let mut p = Polity::default();
    p.jurisdictions.push(jurisdiction(town, &w.town_name, None, FOUNDING_TREASURY, rule.clone(), ProcedureDef { constitutional: true, ..rule }));
    for (i, name) in WARD_NAMES.iter().enumerate() {
        let id = JurId(i as u32 + 1);
        p.jurisdictions.push(jurisdiction(id, name, Some(town), 0.0, ProcedureDef::default(), ProcedureDef::default()));
        p.wards.push(id);
    }
    p.town = town;
    p.offices.push(Office {
        id: OfficeId(0),
        jur: town,
        def: OfficeDef {
            name: SOVEREIGN.into(),
            powers: vec![Power::Decree, Power::Command, Power::Pardon, Power::Judge],
            selection: Selection::Fixed,
            term_days: 0,
            salary: 0.0,
            eligibility: "is_adult".into(),
            holder: "player".into(),
        },
        holder: Some(Actor::Player),
        since_day: w.day(),
        term_ends: None,
        abolished: false,
        created_by: None,
    });
    p.next_bill = 1;
    w.polity = p;
}

fn charter(title: &str, summary: &str, clauses: Vec<Clause>) -> Statute {
    let mut s = Statute::draft(title, clauses);
    s.summary = summary.to_string();
    s.text = summary.to_string();
    s.sponsor = Some(Actor::Player);
    s
}

/// The statutes the town is founded with, as drafts for the whole town.
pub fn founding_statutes() -> Vec<Statute> {
    vec![
        charter(
            "The Common Peace",
            "No one may steal. A thief pays three times the worth of what was taken and spends two days in the watch house.",
            vec![Clause::Conduct(Conduct {
                label: "theft".into(),
                mode: Mode::Forbid,
                action: Action::Steal,
                who: "true".into(),
                when: "true".into(),
                place: String::new(),
                hours_per_week: 0.0,
                sanction: Sanction { fine: "3 * amount".into(), jail_days: 2.0, ..Default::default() },
            })],
        ),
        charter(
            "The Tithe",
            "A tenth of everyone's income from wages, profit and rent goes to the town's treasury.",
            vec![Clause::Levy(Levy { label: "tithe".into(), base: LevyBase::Income, rate: "0.1".into(), who: "true".into(), good: None, evasion_fine_multiple: 2.0, evasion_jail_days: 0.0 })],
        ),
        charter(
            "The Town Watch",
            "The town keeps a watch of three at ten coins a day each, commanded by the Sovereign, and one clerk to keep its books.",
            vec![
                Clause::Unit(UnitDef {
                    name: "Town Watch".into(),
                    staff: 3,
                    wage: 10.0,
                    fine_share: 0.0,
                    bounty: 0.0,
                    quota_per_week: 0.0,
                    quota_penalty: 0.0,
                    posts: "type in [\"plaza\", \"market\", \"street\", \"park\", \"tavern\"]".into(),
                    shift: Shift::Split,
                    mandate: Vec::new(),
                    commander: SOVEREIGN.into(),
                    armed: true,
                    eligibility: "is_adult".into(),
                }),
                Clause::Clerks { count: 1, wage: 10.0 },
            ],
        ),
    ]
}

/// Put the founding statutes into force.
pub(super) fn enact_founding(w: &mut World) {
    for s in founding_statutes() { crate::law::enact(w, s, "founding charter"); }
}
