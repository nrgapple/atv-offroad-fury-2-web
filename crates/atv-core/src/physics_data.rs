//! Observed text physics configuration, preserving order and duplicate ambiguity.
//! This reads tuning data; it does not implement or infer the game's physics.
use crate::Result;
use serde::Serialize;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_LINE: usize = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub section: String,
    pub key: String,
    /// Original trimmed value; quotes are preserved.
    pub value: String,
    /// One-based source line, including comments and blank lines.
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct PhysicsData {
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct PowerPoint {
    pub rpm: f32,
    pub horsepower: f32,
}

impl PhysicsData {
    /// Strictly accepts the section/key syntax observed in this build's .phy files.
    /// Duplicate keys are retained for inspection and explicit lookup policy.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() > MAX_BYTES || data.contains(&0) {
            return Err("physics text exceeds limit or contains NUL".into());
        }
        let text = std::str::from_utf8(data).map_err(|_| "physics text is not UTF-8")?;
        let mut section = String::new();
        let mut entries = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            if raw.len() > MAX_LINE {
                return Err(format!("physics line {line} exceeds limit"));
            }
            let raw = raw.trim();
            if raw.is_empty() || raw.starts_with(';') {
                continue;
            }
            if raw.starts_with('[') {
                let name = raw
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .ok_or_else(|| format!("invalid physics section at line {line}"))?
                    .trim();
                if name.is_empty() || name.contains(['[', ']']) {
                    return Err(format!("invalid physics section at line {line}"));
                }
                section = name.to_owned();
                continue;
            }
            let (key, value) = raw
                .split_once('=')
                .ok_or_else(|| format!("missing physics assignment at line {line}"))?;
            let (key, value) = (key.trim(), value.trim());
            if section.is_empty()
                || key.is_empty()
                || value.is_empty()
                || key.contains(['[', ']', ';'])
                || value.contains(';')
                || value.starts_with('"') != value.ends_with('"')
                || value == "\""
            {
                return Err(format!("unsupported physics assignment at line {line}"));
            }
            if entries.len() == MAX_ENTRIES {
                return Err("too many physics entries".into());
            }
            entries.push(Entry {
                section: section.clone(),
                key: key.to_owned(),
                value: value.to_owned(),
                line,
            });
        }
        if entries.is_empty() {
            return Err("empty physics configuration".into());
        }
        Ok(Self { entries })
    }

    /// Exact spelling only: original case-folding behavior is not yet verified.
    pub fn matches<'a>(
        &'a self,
        section: &'a str,
        key: &'a str,
    ) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| e.section == section && e.key == key)
    }

    pub fn unique(&self, section: &str, key: &str) -> Result<&Entry> {
        let mut found = self
            .entries
            .iter()
            .filter(|e| e.section == section && e.key == key);
        let entry = found
            .next()
            .ok_or_else(|| format!("missing [{section}] {key}"))?;
        if found.next().is_some() {
            return Err(format!("ambiguous duplicate [{section}] {key}"));
        }
        Ok(entry)
    }

    /// Selects the first authored key using verified ASCII-insensitive comparison.
    /// The original indexed mode appends entries and returns its first key match.
    /// Section spelling stays exact until its hash prefilter is also verified.
    /// This does not claim full compatibility with the original text parser.
    pub fn first(&self, section: &str, key: &str) -> Result<&Entry> {
        self.entries
            .iter()
            .find(|e| e.section == section && e.key.eq_ignore_ascii_case(key))
            .ok_or_else(|| format!("missing [{section}] {key}"))
    }

    pub fn first_scalar(&self, section: &str, key: &str) -> Result<f32> {
        finite(&self.first(section, key)?.value)
    }

    pub fn scalar(&self, section: &str, key: &str) -> Result<f32> {
        finite(&self.unique(section, key)?.value)
    }

    pub fn vector3(&self, section: &str, key: &str) -> Result<[f32; 3]> {
        let parts = self
            .unique(section, key)?
            .value
            .split(',')
            .map(finite)
            .collect::<Result<Vec<_>>>()?;
        parts
            .try_into()
            .map_err(|_| format!("[{section}] {key} is not a three-component vector"))
    }

    fn count(&self, key: &str) -> Result<usize> {
        let count = self
            .unique("Engine", key)?
            .value
            .parse::<usize>()
            .map_err(|_| format!("invalid engine count {key}"))?;
        if !(1..=256).contains(&count) {
            return Err(format!("engine count {key} outside supported bounds"));
        }
        Ok(count)
    }

    /// Reads authored samples without inventing an interpolation/extrapolation rule.
    pub fn power_curve(&self) -> Result<Vec<PowerPoint>> {
        let count = self.count("NumberOfRPMPoints")?;
        let mut result = Vec::with_capacity(count);
        let mut previous = 0.0;
        for index in 1..=count {
            let rpm = self.scalar("Engine", &format!("RPM{index}"))?;
            let horsepower = self.scalar("Engine", &format!("HP{index}"))?;
            if rpm <= previous || horsepower < 0.0 {
                return Err(
                    "power curve requires increasing positive RPM and nonnegative HP".into(),
                );
            }
            result.push(PowerPoint { rpm, horsepower });
            previous = rpm;
        }
        Ok(result)
    }

    pub fn gear_ratios(&self) -> Result<Vec<f32>> {
        let count = self.count("NumberOfGears")?;
        (1..=count)
            .map(|index| {
                let ratio = self.scalar("Engine", &format!("GearRatio{index}"))?;
                if ratio <= 0.0 {
                    return Err("gear ratio must be positive".into());
                }
                Ok(ratio)
            })
            .collect()
    }
}

fn finite(text: &str) -> Result<f32> {
    let value = text
        .trim()
        .parse::<f32>()
        .map_err(|_| "invalid physics scalar")?;
    if !value.is_finite() {
        return Err("non-finite physics scalar".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_duplicates_and_provenance_without_selecting_precedence() {
        let data = PhysicsData::parse(b"; synthetic\r\n[Tires]\r\nGrip = 0.4\r\nGrip = 0.8\r\n[Pose]\r\nOffset = -2, 0, 3\r\n").unwrap();
        let entries: Vec<_> = data.matches("Tires", "Grip").collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line, 3);
        assert_eq!(entries[1].value, "0.8");
        assert!(data.scalar("Tires", "Grip").is_err());
        assert_eq!(data.first_scalar("Tires", "Grip").unwrap(), 0.4);
        assert_eq!(data.first_scalar("Tires", "gRiP").unwrap(), 0.4);
        assert_eq!(data.vector3("Pose", "Offset").unwrap(), [-2., 0., 3.]);
    }

    #[test]
    fn validates_power_samples_and_gears_without_interpolating() {
        let fixture = "[Engine]\nNumberOfRPMPoints=2\nRPM1=100\nHP1=2\nRPM2=300\nHP2=6\nNumberOfGears=2\nGearRatio1=2.1\nGearRatio2=1.2";
        let data = PhysicsData::parse(fixture.as_bytes()).unwrap();
        assert_eq!(data.power_curve().unwrap()[1].horsepower, 6.);
        assert_eq!(data.gear_ratios().unwrap(), [2.1, 1.2]);
        let unordered =
            PhysicsData::parse(fixture.replace("RPM2=300", "RPM2=50").as_bytes()).unwrap();
        assert!(unordered.power_curve().is_err());
        let missing = PhysicsData::parse(
            fixture
                .replace("NumberOfGears=2", "NumberOfGears=3")
                .as_bytes(),
        )
        .unwrap();
        assert!(missing.gear_ratios().is_err());
    }

    #[test]
    fn rejects_malformed_and_nonfinite_data() {
        for fixture in [
            "",
            "x=1",
            "[Empty]\n",
            "[Broken\nx=1",
            "[A]\nx=",
            "[A]\nx=\"bad",
            "[A]\nx=1;unknown",
        ] {
            assert!(PhysicsData::parse(fixture.as_bytes()).is_err(), "{fixture}");
        }
        let data = PhysicsData::parse(b"[A]\nx=NaN\ny=1,2").unwrap();
        assert!(data.scalar("A", "x").is_err());
        assert!(data.vector3("A", "y").is_err());
        assert!(PhysicsData::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        assert!(PhysicsData::parse(b"[A]\nx=1\0").is_err());
    }
}
