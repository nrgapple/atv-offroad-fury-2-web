//! Scene zones and portals (research/VISIBILITY.md).
//!
//! `001fc850` reads `[SceneInfo] NumZones` (default 1) and `NumPortals`
//! (default 0) and builds one zone object per zone. TOY animations and
//! emitters register into `StartingZone` (`0023bcc0`/`00240eb0`, default 1,
//! stored zero-based) through `0011e520`/`0011e5c0`, which insert them into
//! that zone's spatial tree (`001a4ff0`/`001a5230`); scene lights carry
//! `InitialZone` (`00203310`, `001fe840`) and triggers `Zone`. Drawing walks
//! the zone the camera is in. No shipped scene declares zones or portals and
//! every authored zone value is 1, so every venue is one zone that holds
//! every model, actor, emitter and light: the zone gate admits everything.
//! Portal traversal between several zones is not recovered.

/// One authored zone reference outside the scene's zones.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Outside {
    pub key: String,
    pub value: String,
}

/// Zone references in TOY/SCN text: how many there are and which name a
/// zone the scene does not have.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Census {
    pub records: usize,
    pub outside: Vec<Outside>,
}

impl Census {
    pub fn add(&mut self, text: &str, zones: i64) {
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("");
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if !["StartingZone", "Zone", "InitialZone"]
                .iter()
                .any(|k| key.eq_ignore_ascii_case(k))
            {
                continue;
            }
            self.records += 1;
            let index = value.trim().parse::<i64>().ok().map(|z| z - 1);
            if !index.is_some_and(|i| admitted(i, 0, zones)) {
                self.outside.push(Outside {
                    key: key.to_string(),
                    value: value.trim().to_string(),
                });
            }
        }
    }
}

/// Whether an object registered in zero-based `zone` draws while the camera
/// is in `active`: only the active zone's tree is walked. With the shipped
/// single zone, every valid registration is admitted.
pub fn admitted(zone: i64, active: i64, zones: i64) -> bool {
    (0..zones).contains(&zone) && zone == active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_zone_admits_every_default_registration() {
        assert!(admitted(0, 0, 1));
        assert!(!admitted(1, 0, 1), "no second zone");
        assert!(!admitted(-1, 0, 1), "StartingZone 0 has no zone");
        assert!(!admitted(1, 0, 2), "another zone's tree is not walked");
    }

    #[test]
    fn census_counts_zone_keys_and_reports_outside_values() {
        let mut census = Census::default();
        census.add(
            "[Animation1]\nStartingZone = 1\n[Trigger1]\nZone=1 ; c\n[Light2]\nInitialZone = 1\nTerrainZone=3\nNumberOfTripleLightZones=2\n",
            1,
        );
        assert_eq!(census.records, 3);
        assert!(census.outside.is_empty());
        census.add("StartingZone = 2\nZone=x\n", 1);
        assert_eq!(census.records, 5);
        assert_eq!(census.outside.len(), 2);
    }
}
