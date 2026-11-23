//! Names for people, families, farms, taverns and towns.

pub const FEMALE: [&str; 50] = [
    "Ada", "Agnes", "Alba", "Beatrix", "Bryn", "Cecily", "Clara", "Dara", "Edith", "Elin", "Elsa", "Faye", "Greta", "Hazel", "Helga", "Ida", "Ines",
    "Isolde", "Jana", "Juno", "Katla", "Lena", "Liv", "Mabel", "Maren", "Mina", "Nell", "Nora", "Odette", "Olwen", "Petra", "Rhea", "Rosa", "Runa",
    "Sable", "Signe", "Tamsin", "Thea", "Una", "Vera", "Wren", "Ysolde", "Zora", "Marta", "Elke", "Hilde", "Corra", "Linnet", "Maud", "Sunniva",
];
pub const MALE: [&str; 50] = [
    "Alder", "Anselm", "Arlo", "Bram", "Cai", "Cedric", "Corin", "Dag", "Edmund", "Egon", "Emil", "Fenn", "Finn", "Garrick", "Gunnar", "Hale", "Hugo",
    "Ivo", "Jarl", "Joss", "Kell", "Lars", "Leif", "Lorcan", "Magnus", "Milo", "Nils", "Odo", "Osric", "Otto", "Perrin", "Rafe", "Roland", "Rurik",
    "Silas", "Stig", "Tobias", "Torin", "Ulf", "Viggo", "Wendell", "Wim", "Yorick", "Alaric", "Bertram", "Conrad", "Dunstan", "Evander", "Halvar", "Jorun",
];
pub const SURNAMES: [&str; 50] = [
    "Ashdown", "Barrow", "Birch", "Brandt", "Calloway", "Carver", "Dunmore", "Eastlake", "Fairweather", "Fenwick", "Garrow", "Greaves", "Hartley",
    "Holt", "Ironwood", "Kestrel", "Larkin", "Lindqvist", "Marlowe", "Merrow", "Northcott", "Oakes", "Pell", "Quill", "Ravensworth", "Rook", "Sedge",
    "Stroud", "Tallow", "Thorne", "Underhill", "Vance", "Wainwright", "Whitlock", "Yarrow", "Aldous", "Blackwood", "Crane", "Dellacroix", "Emberly",
    "Frost", "Gable", "Hawke", "Juniper", "Knoll", "Lowe", "Mead", "Nettle", "Orchard", "Penrose",
];
pub const TOWN_NAMES: [&str; 10] = ["Brackenford", "Oakhollow", "Marrowdale", "Thistlewick", "Emberfield", "Wrenmoor", "Saltmere", "Harrowgate", "Elderbrook", "Fallowmere"];
pub const FARM_NAMES: [&str; 8] = ["Oakfield", "Brightwater", "Longacre", "Sunnymead", "Barleycroft", "Larkrise", "Stonebeck", "Greenhollow"];
pub const TAVERN_NAMES: [&str; 4] = ["The Gilded Goose", "The Crooked Lantern", "The Drowsy Fox", "The Brass Kettle"];
/// The four wards, in the order of `Polity::wards`: north, east, south and west of the plaza.
pub const WARD_NAMES: [&str; 4] = ["Northgate", "Eastmarket", "Southbank", "Westfield"];
