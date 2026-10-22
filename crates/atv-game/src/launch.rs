//! Venue identifiers and the original archives each one reads (web/launch.js).

/// Lobby order is part of the public URL contract: keep the first nine fixed.
pub fn environments() -> Vec<String> {
    let mut ids: Vec<String> = [
        "env01", "env02", "env03", "env04", "env05", "env06", "env07", "env08", "free05",
    ]
    .into_iter()
    .chain(["free01", "free02", "free03", "free04", "free06"])
    .map(String::from)
    .collect();
    ids.extend((1..=8).map(|i| format!("nat{i:02}")));
    ids.extend((1..=16).map(|i| format!("sx{i:02}")));
    ids.extend(["training".to_string(), "game01".to_string()]);
    ids
}

/// Every Environment, National and Supercross venue ships an original *_NAT.RES race.
pub fn is_race_course(environment: &str) -> bool {
    let family = environment.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(family, "env" | "nat" | "sx") && environment.len() == family.len() + 2
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseResource {
    pub archive: String,
    pub scene_stem: String,
    pub mode_stem: String,
    pub placements: Option<String>,
}

/// The archives a ride reads, for prefetch (game.js `prefetchRide`): the
/// course, the vehicle and its rider (an ATV's rider rig and motions),
/// overlays, start events and the audio table; then the sound bank, whose
/// first chunk (its directory) is fetched at low priority.
pub fn ride_prefetch(
    environment: &str,
    racing: bool,
    quad: u16,
    rider: u16,
) -> (Vec<String>, Vec<String>) {
    let mut paths = Vec::new();
    if let Ok(course) = course_resource(environment, racing) {
        paths.push(course.archive);
        paths.extend(course.placements);
    }
    if let Ok(model) = atv_core::player_models::selection(quad, rider) {
        paths.push(model.quad);
        paths.push(model.quad_materials);
        if !model.motorcycle {
            paths.push(model.rider);
            paths.push("/ANIMSRDR.RES".into());
            paths.push("/RIDER/RIDER_MT.RES".into());
        }
    }
    paths.extend(["/OVERLAYS.RES", "/STARTEVT.RES", "/AUDIOTBL.RES"].map(String::from));
    paths.dedup();
    (paths, vec!["/AUDIO/AUDIO.RES".into()])
}

pub fn course_resource(environment: &str, racing: bool) -> crate::Result<CourseResource> {
    if !environments().iter().any(|id| id == environment)
        || (racing && !is_race_course(environment))
    {
        return Err("unsupported course".into());
    }
    let number = &environment[environment.len() - 2..];
    let family = environment.trim_end_matches(|c: char| c.is_ascii_digit());
    let prefix = match family {
        "env" => "E",
        "free" => "F",
        "nat" => "N",
        "sx" => "S",
        "game" => "G",
        _ => "",
    };
    let group = if matches!(family, "sx" | "training") {
        "B"
    } else {
        "A"
    };
    let suffix = if racing {
        "NAT"
    } else if family == "training" {
        "TR"
    } else if family == "game" {
        "SO"
    } else {
        "FR"
    };
    let stem = if family == "training" {
        "T00".to_string()
    } else {
        format!("{prefix}{number}")
    };
    let scene_stem = if family == "env" && !racing {
        format!("{environment}fr")
    } else {
        environment.to_string()
    };
    Ok(CourseResource {
        archive: format!(
            "/TRACKS/TRACKS{group}/{}/{stem}_{suffix}.RES",
            environment.to_uppercase()
        ),
        mode_stem: if racing {
            format!("{environment}nat")
        } else {
            scene_stem.clone()
        },
        scene_stem,
        placements: (family == "env")
            .then(|| format!("/TRACKS/TRACKSA/ENV{number}/E{number}_ENDL.RES")),
    })
}

/// Original per-course loading-screen action shots (UI/LOADICON, frames
/// 02-07). Training ships only the progress-bar art.
pub fn loading_shots_resource(environment: &str) -> Option<String> {
    if !environments().iter().any(|id| id == environment) {
        return None;
    }
    let n = &environment[environment.len() - 2..];
    let family = environment.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(match family {
        "env" => {
            let index = n.parse::<usize>().ok()?.checked_sub(1)?;
            let bank = *b"AAABBBCC".get(index)? as char;
            format!("/UI/LOADICON/ENV{bank}/E{n}_OB.LSI")
        }
        "free" => format!("/UI/LOADICON/FREE/F{n}_OB.LSI"),
        "nat" => format!("/UI/LOADICON/NAT/N{n}_NAT.LSI"),
        "sx" => format!("/UI/LOADICON/SX/S{n}_NAT.LSI"),
        "game" => format!("/UI/LOADICON/GAME/G{n}_SO.LSI"),
        _ => return None,
    })
}

/// Race settings carried by a ride link (launch.js `raceSettings`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceLink {
    pub laps: u32,
    pub cpu: u32,
    /// 1 Normal, 2 Expert.
    pub difficulty: u32,
    pub course: String,
    /// EATDIRT (Aggressive AI) and BRINGIT (All Out AI).
    pub aggressive: bool,
    pub all_out: bool,
}

/// A parsed ride link (launch.js `parseLaunch`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    /// Any launch field was present; otherwise the title opens.
    pub requested: bool,
    pub errors: Vec<String>,
    /// "single" or "multiplayer".
    pub mode: &'static str,
    /// "freestyle" or "race".
    pub activity: &'static str,
    /// "join", "create" or none.
    pub action: Option<&'static str>,
    pub play: bool,
    pub map: String,
    pub quad_id: u16,
    pub rider_id: u16,
    pub player_name: String,
    pub room: String,
    pub race: RaceLink,
}

/// `application/x-www-form-urlencoded` query pairs, as URLSearchParams reads them.
pub fn query_pairs(search: &str) -> Vec<(String, String)> {
    let decode = |text: &str| -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < bytes.len()
                    && bytes[i + 1].is_ascii_hexdigit()
                    && bytes[i + 2].is_ascii_hexdigit() =>
                {
                    let hex = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
                    out.push(hex(bytes[i + 1]) << 4 | hex(bytes[i + 2]));
                    i += 2;
                }
                b => out.push(b),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(k), decode(v))
        })
        .collect()
}

fn slug(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// What a shared ride link starts (launch.js `buildRideUrl`'s setup).
pub struct RideLink<'a> {
    pub map: &'a str,
    pub quad: u16,
    pub rider: u16,
    pub player: &'a str,
    /// A race: laps, CPU riders, difficulty (2 Expert), EATDIRT, BRINGIT.
    pub race: Option<(u32, usize, u32, bool, bool)>,
    /// A multiplayer invite's room code (`mode=multiplayer&action=join`).
    pub room: Option<&'a str>,
}

/// launch.js `buildRideUrl`: `page` (e.g. `https://host/engine.html`) with
/// the single-player setup's query, in the browser's key order.
pub fn ride_url(page: &str, link: &RideLink) -> String {
    let mut pairs: Vec<(&str, String)> = vec![
        (
            "mode",
            if link.room.is_some() {
                "multiplayer"
            } else {
                "single"
            }
            .into(),
        ),
        (
            "activity",
            if link.race.is_some() {
                "race"
            } else {
                "freestyle"
            }
            .into(),
        ),
        ("map", link.map.into()),
        ("quad", link.quad.to_string()),
        ("rider", link.rider.to_string()),
    ];
    if let Some(room) = link.room {
        pairs.push(("action", "join".into()));
        pairs.push(("room", room.into()));
    }
    if !link.player.is_empty() {
        pairs.push(("player", link.player.into()));
    }
    if let Some((laps, cpu, difficulty, aggressive, all_out)) = link.race {
        pairs.push(("laps", laps.to_string()));
        pairs.push(("cpu", cpu.to_string()));
        if difficulty == 2 {
            pairs.push(("difficulty", "expert".into()));
        }
        let cheats: Vec<&str> = [(aggressive, "eatdirt"), (all_out, "bringit")]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .collect();
        if !cheats.is_empty() {
            pairs.push(("cheats", cheats.join(",")));
        }
    }
    pairs.push(("play", "1".into()));
    let encode = |v: &str| {
        // URLSearchParams form encoding.
        let mut out = String::new();
        for b in v.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                    out.push(b as char)
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };
    let query: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    format!("{page}?{}", query.join("&"))
}

/// launch.js `parseLaunch`: `tracks` are (id, display name) for map names.
pub fn parse_launch(search: &str, tracks: &[(String, String)]) -> Launch {
    const ALIASES: [(&str, &[&str]); 13] = [
        ("map", &["map", "environment"]),
        ("quad", &["quad", "vehicle", "atv"]),
        ("rider", &["rider"]),
        ("player", &["player", "name"]),
        ("mode", &["mode"]),
        ("activity", &["activity"]),
        ("action", &["action"]),
        ("room", &["room", "lobby"]),
        ("play", &["play", "drive"]),
        ("laps", &["laps"]),
        ("cpu", &["cpu"]),
        ("difficulty", &["difficulty"]),
        ("cheats", &["cheats", "cheat"]),
    ];
    let params = query_pairs(search);
    let mut errors = Vec::new();
    let mut values: Vec<(&str, String)> = Vec::new();
    for (field, keys) in ALIASES {
        let entries: Vec<&String> = keys
            .iter()
            .flat_map(|key| params.iter().filter(move |(k, _)| k == key).map(|(_, v)| v))
            .collect();
        if entries.len() > 1 {
            errors.push(format!("Use {} only once (including aliases).", keys[0]));
        }
        if let Some(first) = entries.first() {
            values.push((field, first.trim().to_string()));
        }
    }
    let value = |field: &str| {
        values
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| v.clone())
    };
    let has = |field: &str| value(field).is_some();
    let mut launch = Launch {
        requested: !values.is_empty(),
        errors: Vec::new(),
        mode: "single",
        activity: "freestyle",
        action: None,
        play: true,
        map: "env01".into(),
        quad_id: 1,
        rider_id: 0,
        player_name: "Rider".into(),
        room: String::new(),
        race: RaceLink {
            laps: 3,
            cpu: 3,
            difficulty: 1,
            course: "env01".into(),
            aggressive: false,
            all_out: false,
        },
    };
    if !launch.requested {
        launch.errors = errors;
        return launch;
    }
    let shown = |v: &str| {
        if v.is_empty() {
            "(empty)".to_string()
        } else {
            v.to_string()
        }
    };
    let pick = |field: &str,
                table: &[(&str, &'static str)],
                fallback: Option<&'static str>,
                errors: &mut Vec<String>| {
        let Some(v) = value(field) else {
            return fallback;
        };
        match table.iter().find(|(k, _)| *k == v.to_lowercase()) {
            Some((_, out)) => Some(*out),
            None => {
                errors.push(format!("Unsupported {field}: {}.", shown(&v)));
                fallback
            }
        }
    };
    let multiplayer_default = if has("room") || has("action") {
        "multiplayer"
    } else {
        "single"
    };
    launch.mode = pick(
        "mode",
        &[
            ("single", "single"),
            ("single-player", "single"),
            ("singleplayer", "single"),
            ("sp", "single"),
            ("multiplayer", "multiplayer"),
            ("multi", "multiplayer"),
            ("mp", "multiplayer"),
        ],
        Some(multiplayer_default),
        &mut errors,
    )
    .unwrap_or("single");
    launch.activity = pick(
        "activity",
        &[
            ("freestyle", "freestyle"),
            ("freeride", "freestyle"),
            ("race", "race"),
        ],
        Some("freestyle"),
        &mut errors,
    )
    .unwrap_or("freestyle");
    launch.action = pick(
        "action",
        &[("join", "join"), ("create", "create"), ("host", "create")],
        has("room").then_some("join"),
        &mut errors,
    );
    launch.play = pick(
        "play",
        &[("1", "1"), ("true", "1"), ("0", "0"), ("false", "0")],
        Some("1"),
        &mut errors,
    ) == Some("1");
    let map = match value("map") {
        None => Some("env01".to_string()),
        Some(v) => {
            let lower = v.to_lowercase();
            tracks
                .iter()
                .find(|(id, name)| *id == lower || slug(name) == slug(&lower))
                .map(|(id, _)| id.clone())
                .or_else(|| environments().into_iter().find(|id| *id == lower))
        }
    };
    let supported = map.as_ref().is_some_and(|m| environments().contains(m));
    if !supported {
        errors.push(format!(
            "Unsupported map: {}.",
            shown(&value("map").unwrap_or_default())
        ));
    }
    let integer =
        |field: &str, allowed: &dyn Fn(u64) -> bool, fallback: u64, errors: &mut Vec<String>| {
            let Some(v) = value(field) else {
                return fallback;
            };
            let parsed = (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                .then(|| v.parse::<u64>().ok())
                .flatten();
            match parsed {
                Some(n) if allowed(n) => n,
                _ => {
                    errors.push(format!("Invalid {field}: {}.", shown(&v)));
                    fallback
                }
            }
        };
    launch.quad_id = integer("quad", &|v| (1..=46).contains(&v), 1, &mut errors) as u16;
    launch.rider_id = match pick(
        "rider",
        &[("0", "0"), ("male", "0"), ("1", "1"), ("female", "1")],
        Some("0"),
        &mut errors,
    ) {
        Some("1") => 1,
        _ => 0,
    };
    launch.player_name = value("player").unwrap_or_else(|| "Rider".into());
    let control = |c: char| (c as u32) <= 0x1f || (0x7f..=0x9f).contains(&(c as u32));
    if launch.player_name.is_empty()
        || launch.player_name.len() > 32
        || launch.player_name.chars().any(control)
    {
        errors.push("Player name must contain 1–32 UTF-8 bytes without control characters.".into());
    }
    launch.room = value("room").unwrap_or_default().to_uppercase();
    if has("room")
        && !(launch.room.len() == 8
            && launch
                .room
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
    {
        errors.push("Room must contain exactly 8 letters or numbers.".into());
    }
    let laps = integer("laps", &|v| (1..=5).contains(&v), 3, &mut errors) as u32;
    let cpu = integer("cpu", &|v| [0, 3, 4].contains(&v), 3, &mut errors) as u32;
    let difficulty = match pick(
        "difficulty",
        &[("normal", "1"), ("1", "1"), ("expert", "2"), ("2", "2")],
        Some("1"),
        &mut errors,
    ) {
        Some("2") => 2,
        _ => 1,
    };
    let (mut aggressive, mut all_out) = (false, false);
    if let Some(cheats) = value("cheats") {
        for code in cheats.to_lowercase().split(',') {
            match code.trim() {
                "eatdirt" => aggressive = true,
                "bringit" => all_out = true,
                other => errors.push(format!("Unsupported cheat: {}.", shown(other))),
            }
        }
    }
    if launch.mode == "single" && (has("room") || has("action")) {
        errors.push("Room/action requires mode=multiplayer.".into());
    }
    if launch.mode == "multiplayer" {
        if launch.action.is_none() {
            errors.push("Multiplayer needs room=CODE to join, or action=create.".into());
        }
        if launch.action == Some("join") && launch.room.is_empty() {
            errors.push("Joining requires room=CODE.".into());
        }
        if launch.quad_id > 26 {
            errors
                .push("Multiplayer supports ATVs 1–26; motorcycles are Single Player only.".into());
        }
        if launch.activity != "freestyle" {
            errors.push("Multiplayer supports Freeride only.".into());
        }
    }
    if launch.activity == "race" && supported && !is_race_course(map.as_deref().unwrap_or("")) {
        errors.push("Races use the original Environment, National and Supercross courses.".into());
    }
    if launch.activity != "race"
        && ["laps", "cpu", "difficulty", "cheats"]
            .iter()
            .any(|k| has(k))
    {
        errors.push("Laps/cpu/difficulty/cheats requires activity=race.".into());
    }
    launch.map = map.filter(|_| supported).unwrap_or_else(|| "env01".into());
    launch.race = RaceLink {
        laps,
        cpu,
        difficulty,
        course: launch.map.clone(),
        aggressive,
        all_out,
    };
    launch.errors = errors;
    launch
}

/// launch.js `menuShortcut`: `?menu=freeride|race` alone opens that menu.
pub fn menu_shortcut(search: &str) -> Option<&'static str> {
    let params = query_pairs(search);
    if params.len() != 1 || params[0].0 != "menu" {
        return None;
    }
    match params[0].1.to_lowercase().as_str() {
        "freeride" => Some("tracks"),
        "race" => Some("race"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ride_prefetch_lists_the_ride_archives() {
        let (paths, bank) = ride_prefetch("env01", false, 1, 0);
        let course = course_resource("env01", false).unwrap();
        assert!(paths.contains(&course.archive));
        assert!(paths.iter().any(|p| p == "/ANIMSRDR.RES"));
        assert!(paths.iter().any(|p| p == "/AUDIOTBL.RES"));
        assert_eq!(bank, ["/AUDIO/AUDIO.RES"]);
        // Motorcycles carry their rider in the bike archive.
        let (bike, _) = ride_prefetch("env01", false, 29, 0);
        assert!(!bike.iter().any(|p| p == "/ANIMSRDR.RES"));
    }

    #[test]
    fn ride_urls_match_build_ride_url() {
        let freestyle = ride_url(
            "https://furyremastered.com/engine.html",
            &RideLink {
                map: "env03",
                quad: 4,
                rider: 1,
                player: "Ada L",
                race: None,
                room: None,
            },
        );
        assert_eq!(
            freestyle,
            "https://furyremastered.com/engine.html?mode=single&activity=freestyle&map=env03&quad=4&rider=1&player=Ada+L&play=1"
        );
        let race = ride_url(
            "/engine.html",
            &RideLink {
                map: "nat01",
                quad: 1,
                rider: 0,
                player: "Rider",
                race: Some((2, 4, 2, true, false)),
                room: None,
            },
        );
        assert!(race.ends_with("activity=race&map=nat01&quad=1&rider=0&player=Rider&laps=2&cpu=4&difficulty=expert&cheats=eatdirt&play=1"));
        let parsed = parse_launch(&race[race.find('?').unwrap()..], &[]);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(parsed.race.difficulty, 2);
        // An invite (buildRideUrl with mode multiplayer, without the player).
        let invite = ride_url(
            "https://furyremastered.com/play.html",
            &RideLink {
                map: "env08",
                quad: 12,
                rider: 1,
                player: "",
                race: None,
                room: Some("ABCD1234"),
            },
        );
        assert_eq!(
            invite,
            "https://furyremastered.com/play.html?mode=multiplayer&activity=freestyle&map=env08&quad=12&rider=1&action=join&room=ABCD1234&play=1"
        );
        let parsed = parse_launch(&invite[invite.find('?').unwrap()..], &[]);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(
            (parsed.mode, parsed.action, parsed.room.as_str()),
            ("multiplayer", Some("join"), "ABCD1234")
        );
    }

    #[test]
    fn matches_browser_course_paths() {
        let env = course_resource("env01", false).unwrap();
        assert_eq!(env.archive, "/TRACKS/TRACKSA/ENV01/E01_FR.RES");
        assert_eq!(
            (env.scene_stem.as_str(), env.mode_stem.as_str()),
            ("env01fr", "env01fr")
        );
        assert_eq!(
            env.placements.as_deref(),
            Some("/TRACKS/TRACKSA/ENV01/E01_ENDL.RES")
        );
        let race = course_resource("env01", true).unwrap();
        assert_eq!(
            (
                race.archive.as_str(),
                race.scene_stem.as_str(),
                race.mode_stem.as_str()
            ),
            ("/TRACKS/TRACKSA/ENV01/E01_NAT.RES", "env01", "env01nat")
        );
        assert_eq!(
            course_resource("sx03", false).unwrap().archive,
            "/TRACKS/TRACKSB/SX03/S03_FR.RES"
        );
        assert_eq!(
            course_resource("training", false).unwrap().archive,
            "/TRACKS/TRACKSB/TRAINING/T00_TR.RES"
        );
        assert_eq!(
            course_resource("game01", false).unwrap().archive,
            "/TRACKS/TRACKSA/GAME01/G01_SO.RES"
        );
        assert!(course_resource("free05", true).is_err());
        assert!(course_resource("env09", false).is_err());
        assert_eq!(environments().len(), 40);
    }

    #[test]
    fn launch_links_follow_the_browser_contract() {
        let parse = |q: &str| parse_launch(q, &[]);
        for q in ["", "?v=123&utm_source=test"] {
            assert!(!parse(q).requested, "{q}");
        }
        let s = parse_launch(
            "?mode=sp&environment=Tioga%20Valley&vehicle=14&rider=female&name=Sam&play=false",
            &[("env02".into(), "Tioga Valley".into())],
        );
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        assert_eq!(
            (s.map.as_str(), s.quad_id, s.rider_id, s.play),
            ("env02", 14, 1, false)
        );
        let old = parse("?drive=1&environment=ENV08");
        assert!(old.errors.is_empty());
        assert_eq!(old.map, "env08");
        let room = parse("?room=abcd1234");
        assert!(room.errors.is_empty());
        assert_eq!(
            (room.action, room.room.as_str(), room.mode),
            (Some("join"), "ABCD1234", "multiplayer")
        );
        assert!(
            parse("?mode=mp&action=create&map=free05&quad=26")
                .errors
                .is_empty()
        );
        assert!(parse("?action=host&room=FURY1234&play=0").errors.is_empty());
        let race = parse("?activity=race&map=sx07");
        assert_eq!(
            (
                race.race.laps,
                race.race.cpu,
                race.race.difficulty,
                race.race.course.as_str()
            ),
            (3, 3, 1, "sx07")
        );
        let cheats = parse("?activity=race&cheats=EATDIRT,bringit&difficulty=expert");
        assert!(cheats.errors.is_empty());
        assert!(cheats.race.aggressive && cheats.race.all_out);
        assert_eq!(cheats.race.difficulty, 2);
        let name = parse("?player=A+%26+B+%F0%9F%8F%81");
        assert_eq!(name.player_name, "A & B 🏁");
        for q in [
            "?mode=mp",
            "?mode=mp&action=join",
            "?activity=race&map=free05",
            "?activity=race&map=training",
            "?mode=mp&room=ABCD1234&activity=race",
            "?room=ABCD1234&quad=27",
            "?laps=2",
            "?activity=race&cpu=5",
            "?difficulty=expert",
            "?cheats=eatdirt",
            "?activity=race&difficulty=hard",
            "?activity=race&difficulty=0",
            "?activity=race&cheats=nofog",
            "?activity=race&cheats=",
            "?map=nope",
            "?map=",
            "?map=env01&map=env02",
            "?map=env01&environment=env01",
            "?quad=1&atv=2",
            "?quad=0",
            "?quad=47",
            "?quad=1.5",
            "?quad=0x10",
            "?rider=2",
            "?player=",
            "?player=%00",
            "?player=%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81%F0%9F%8F%81",
            "?mode=single&room=ABCD1234",
            "?room=ABCD",
            "?play=maybe",
            "?activity=supercross",
            "?action=delete",
            "?activity=race&laps=6",
        ] {
            assert!(!parse(q).errors.is_empty(), "{q}");
        }
        assert_eq!(menu_shortcut("?menu=freeride"), Some("tracks"));
        assert_eq!(menu_shortcut("?menu=Race"), Some("race"));
        for q in [
            "",
            "?menu=garage",
            "?menu=race&map=env02",
            "?menu=race&menu=freeride",
            "?play=1",
        ] {
            assert_eq!(menu_shortcut(q), None, "{q}");
        }
        assert!(!parse("?menu=race").requested);
        assert_eq!(
            loading_shots_resource("env04").as_deref(),
            Some("/UI/LOADICON/ENVB/E04_OB.LSI")
        );
        assert_eq!(
            loading_shots_resource("env08").as_deref(),
            Some("/UI/LOADICON/ENVC/E08_OB.LSI")
        );
        assert_eq!(
            loading_shots_resource("sx16").as_deref(),
            Some("/UI/LOADICON/SX/S16_NAT.LSI")
        );
        assert_eq!(loading_shots_resource("training"), None);
    }
}
