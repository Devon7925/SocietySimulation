//! Given names and surnames, from the prototype's lists.
use crate::types::*;

pub const FEMALE: [&str; 50] = [
    "Ada", "Agnes", "Alba", "Beatrix", "Bryn", "Cecily", "Clara", "Dara", "Edith", "Elin",
    "Elsa", "Faye", "Greta", "Hazel", "Helga", "Ida", "Ines", "Isolde", "Jana", "Juno",
    "Katla", "Lena", "Liv", "Mabel", "Maren", "Mina", "Nell", "Nora", "Odette", "Olwen",
    "Petra", "Rhea", "Rosa", "Runa", "Sable", "Signe", "Tamsin", "Thea", "Una", "Vera",
    "Wren", "Ysolde", "Zora", "Marta", "Elke", "Hilde", "Corra", "Linnet", "Maud", "Sunniva",
];
pub const MALE: [&str; 50] = [
    "Alder", "Anselm", "Arlo", "Bram", "Cai", "Cedric", "Corin", "Dag", "Edmund", "Egon",
    "Emil", "Fenn", "Finn", "Garrick", "Gunnar", "Hale", "Hugo", "Ivo", "Jarl", "Joss",
    "Kell", "Lars", "Leif", "Lorcan", "Magnus", "Milo", "Nils", "Odo", "Osric", "Otto",
    "Perrin", "Rafe", "Roland", "Rurik", "Silas", "Stig", "Tobias", "Torin", "Ulf", "Viggo",
    "Wendell", "Wim", "Yorick", "Alaric", "Bertram", "Conrad", "Dunstan", "Evander", "Halvar", "Jorun",
];
pub const SURNAMES: [&str; 50] = [
    "Ashdown", "Barrow", "Birch", "Brandt", "Calloway", "Carver", "Dunmore", "Eastlake", "Fairweather", "Fenwick",
    "Garrow", "Greaves", "Hartley", "Holt", "Ironwood", "Kestrel", "Larkin", "Lindqvist", "Marlowe", "Merrow",
    "Northcott", "Oakes", "Pell", "Quill", "Ravensworth", "Rook", "Sedge", "Stroud", "Tallow", "Thorne",
    "Underhill", "Vance", "Wainwright", "Whitlock", "Yarrow", "Aldous", "Blackwood", "Crane", "Dellacroix", "Emberly",
    "Frost", "Gable", "Hawke", "Juniper", "Knoll", "Lowe", "Mead", "Nettle", "Orchard", "Penrose",
];

/// A given name for someone of this surname, avoiding a namesake already in town where possible.
pub fn given(w: &mut World, sex: Sex, surname: &str) -> String {
    let list: &[&str] = if sex == Sex::Female { &FEMALE } else { &MALE };
    let mut name = list[w.rng.index(list.len())];
    for _ in 0..6 {
        if !w.persons.iter().any(|p| p.present && p.name == name && p.surname == surname) { break; }
        name = list[w.rng.index(list.len())];
    }
    name.to_string()
}

/// A surname for a household arriving from elsewhere, preferring one no household in town bears.
pub fn surname(w: &mut World) -> String {
    let mut name = SURNAMES[w.rng.index(SURNAMES.len())];
    for _ in 0..8 {
        if !w.households.iter().any(|h| !h.members.is_empty() && h.name == name) { break; }
        name = SURNAMES[w.rng.index(SURNAMES.len())];
    }
    name.to_string()
}
