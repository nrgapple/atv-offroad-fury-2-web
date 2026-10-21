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

#[cfg(test)]
mod tests {
    use super::*;

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
}
