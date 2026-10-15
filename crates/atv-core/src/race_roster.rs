//! CPU race roster from the original `quad\ATVGroups.pb` and `AIRiders.pb`.
//!
//! Exact-build 00210d20 gathers ATV groups — the course `ChampQuadGroup` when
//! set, otherwise each human's group (groups 5 and 6 fold into 3) — plus every
//! `GroupN` fallback listed in them, pools all their ATVs and draws each CPU
//! slot's quad at random without replacement, refilling an exhausted pool.
//! 0021f680 then reads the slot quad's `[QuadN]` male or female AI list and
//! draws an unused AI rider, whose `[AIn]` section names its outfit.
//! The original seeds libc rand from the RTC; the caller supplies the seed.
use crate::{Result, engine_audio::OriginalRandom, ui::Document};
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Outfit {
    pub helmet: String,
    pub glove: String,
    pub boot: String,
    pub sleeve: String,
    pub pant: String,
    pub goggle: String,
    pub shirt: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub ai: u32,
    pub name: String,
    pub number: u32,
    pub quad: u32,
    pub female: bool,
    pub outfit: Outfit,
}

fn integer(doc: &Document, section: &str, key: &str) -> Option<i64> {
    doc.value(section, key)?.trim().parse().ok()
}
/// Material stem of an outfit path such as `rider\textures\atv2_White_helmet`.
fn stem(path: &str) -> String {
    path.trim()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}
fn draw(random: &mut OriginalRandom, count: usize) -> usize {
    ((count as f32 * random.next_u31() as f32 * 4.656613e-10) as usize).min(count.saturating_sub(1))
}

/// Groups to draw CPU quads from, after 00210d20.
fn groups(doc: &Document, player_quad: u32, champ_group: i64) -> Vec<i64> {
    let count = integer(doc, "ATVGroups", "NumGroups").unwrap_or(0);
    let section = |g: i64| format!("ATVGroup{g}");
    let mut chosen = Vec::new();
    let add = |group: i64, chosen: &mut Vec<i64>| {
        if !chosen.contains(&group) {
            chosen.push(group);
        }
        for n in 1..=integer(doc, &section(group), "NumGroups").unwrap_or(0) {
            if let Some(extra) = integer(doc, &section(group), &format!("Group{n}"))
                && !chosen.contains(&extra)
            {
                chosen.push(extra);
            }
        }
    };
    if champ_group > 0 {
        add(champ_group, &mut chosen);
    } else {
        for g in 1..=count {
            let atvs = integer(doc, &section(g), "NumATVs").unwrap_or(0);
            if (1..=atvs)
                .any(|n| integer(doc, &section(g), &format!("ATV{n}")) == Some(player_quad.into()))
            {
                add(if g == 5 || g == 6 { 3 } else { g }, &mut chosen);
                break;
            }
        }
    }
    chosen
}

pub fn select(
    atv_groups: &[u8],
    ai_riders: &[u8],
    player_quad: u32,
    female: bool,
    champ_group: i64,
    count: usize,
    seed: u64,
) -> Result<Vec<Entry>> {
    if count > 4 {
        return Err("roster supports four CPU riders".into());
    }
    let groups_doc = Document::parse(atv_groups)?;
    let riders = Document::parse(ai_riders)?;
    let mut random = OriginalRandom::with_seed(seed);
    let mut pool_source = Vec::new();
    for g in groups(&groups_doc, player_quad, champ_group) {
        let section = format!("ATVGroup{g}");
        for n in 1..=integer(&groups_doc, &section, "NumATVs").unwrap_or(0) {
            if let Some(atv) = integer(&groups_doc, &section, &format!("ATV{n}"))
                && (1..=26).contains(&atv)
            {
                pool_source.push(atv as u32);
            }
        }
    }
    if pool_source.is_empty() {
        pool_source.push(player_quad);
    }
    let mut pool = pool_source.clone();
    let mut used = Vec::new();
    let mut roster = Vec::with_capacity(count);
    for _ in 0..count {
        if pool.is_empty() {
            pool = pool_source.clone();
        }
        let quad = pool.remove(draw(&mut random, pool.len()));
        let section = format!("Quad{quad}");
        let (count_key, prefix) = if female {
            ("NumFemaleAI", "Female")
        } else {
            ("NumMaleAI", "Male")
        };
        let candidates: Vec<u32> = (1..=integer(&riders, &section, count_key).unwrap_or(0))
            .filter_map(|n| integer(&riders, &section, &format!("{prefix}{n}")))
            .filter_map(|id| u32::try_from(id).ok())
            .collect();
        // Draw a start index, then walk forward to the first unused rider.
        let ai = if candidates.is_empty() {
            None
        } else {
            let start = draw(&mut random, candidates.len());
            (0..candidates.len())
                .map(|k| candidates[(start + k) % candidates.len()])
                .find(|id| !used.contains(id))
        };
        // Fall back to any unused named rider of the same sex.
        let ai = ai
            .or_else(|| {
                (1..=64u32).find(|id| {
                    !used.contains(id)
                        && (*id >= 22) == female
                        && riders.value(&format!("AI{id}"), "Name").is_some()
                })
            })
            .ok_or("AIRiders has no unused rider")?;
        used.push(ai);
        let ai_section = format!("AI{ai}");
        let field = |key: &str| stem(riders.value(&ai_section, key).unwrap_or_default());
        roster.push(Entry {
            ai,
            name: riders
                .value(&ai_section, "Name")
                .ok_or("AI rider has no name")?
                .trim()
                .to_string(),
            number: integer(&riders, &ai_section, "Number").unwrap_or(0) as u32,
            quad,
            // 0021f680 marks AI ids below 22 male.
            female: ai >= 22,
            outfit: Outfit {
                helmet: field("Helmet"),
                glove: field("Glove"),
                boot: field("Boot"),
                sleeve: field("Sleeve"),
                pant: field("Pant"),
                goggle: field("Goggle"),
                // Per-rider `_AIn` jerseys are composed at runtime (number
                // decal, unrecovered); keep the base shirt, whose mesh-slot
                // variants (`<base>001`) ship on the disc.
                shirt: {
                    let shirt = field("Shirt");
                    match shirt.rfind("_ai") {
                        Some(at)
                            if shirt[at + 3..].bytes().all(|b| b.is_ascii_digit())
                                && at + 3 < shirt.len() =>
                        {
                            shirt[..at].to_string()
                        }
                        _ => shirt,
                    }
                },
            },
        });
    }
    Ok(roster)
}

#[cfg(test)]
mod tests {
    use super::*;
    const GROUPS: &[u8] = b"[ATVGroups]\nNumGroups=6\n[ATVGroup1]\nNumATVs=2\nATV1=14\nATV2=3\n[ATVGroup2]\nNumATVs=2\nATV1=11\nATV2=4\n[ATVGroup3]\nNumATVs=2\nATV1=9\nATV2=10\n[ATVGroup4]\nNumATVs=1\nATV1=23\nNumGroups=1\nGroup1=2\n[ATVGroup5]\nNumATVs=1\nATV1=1\n[ATVGroup6]\nNumATVs=1\nATV1=21\n";
    fn riders() -> Vec<u8> {
        let mut text = String::new();
        for quad in [1, 3, 4, 9, 10, 11, 14, 21, 23] {
            text += &format!(
                "[Quad{quad}]\nNumMaleAI = 2\nNumFemaleAI = 1\nMale1={}\nMale2={}\nFemale1=22\n",
                quad % 5 + 1,
                quad % 7 + 6
            );
        }
        for ai in 1..=23 {
            text += &format!(
                "[AI{ai}]\nName=R{ai}\nNumber={}\nHelmet=rider\\textures\\atv2_White_helmet\nShirt=rider\\textures\\atv2_yellow_shirt_AI{ai}\n",
                20 + ai
            );
        }
        text.into_bytes()
    }
    #[test]
    #[ignore = "requires privately extracted AIRiders.pb and ATVGroups.pb"]
    fn original_roster_files_select_named_riders_for_every_quad() {
        let dir = std::path::PathBuf::from(std::env::var("ATV_ROSTER_DIR").unwrap());
        let groups = std::fs::read(dir.join("atvgroups.pb")).unwrap();
        let riders = std::fs::read(dir.join("airiders.pb")).unwrap();
        for quad in 1..=26 {
            for female in [false, true] {
                for champ in [-1, 1, 2] {
                    let r = select(&groups, &riders, quad, female, champ, 4, quad as u64).unwrap();
                    assert_eq!(r.len(), 4);
                    assert!(
                        r.iter()
                            .all(|e| !e.outfit.helmet.is_empty() && !e.outfit.shirt.is_empty()),
                        "{r:?}"
                    );
                    if quad == 14 && champ == -1 {
                        eprintln!(
                            "{female} {:?}",
                            r.iter()
                                .map(|e| (e.name.as_str(), e.quad, e.number))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn quads_come_from_the_player_group_or_the_course_group_and_riders_are_unique() {
        let riders = riders();
        // Player on the Honda 250EX (group 1): CPUs ride group-1 ATVs.
        let r = select(GROUPS, &riders, 14, false, -1, 4, 7).unwrap();
        assert!(r.iter().all(|e| [14, 3].contains(&e.quad)), "{r:?}");
        let mut ids: Vec<_> = r.iter().map(|e| e.ai).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4);
        assert!(
            r.iter()
                .all(|e| !e.female && e.name == format!("R{}", e.ai))
        );
        assert_eq!(r[0].outfit.helmet, "atv2_white_helmet");
        assert_eq!(r[0].outfit.shirt, "atv2_yellow_shirt");
        assert_eq!(r[0].number, 20 + r[0].ai);
        // Course ChampQuadGroup 4 adds its fallback group 2.
        let r = select(GROUPS, &riders, 14, false, 4, 4, 9).unwrap();
        assert!(r.iter().all(|e| [23, 11, 4].contains(&e.quad)), "{r:?}");
        // Group 5 folds into group 3.
        let r = select(GROUPS, &riders, 1, false, -1, 3, 1).unwrap();
        assert!(r.iter().all(|e| [9, 10].contains(&e.quad)), "{r:?}");
        // Female players race female AI; exhausted lists fall back to unused riders.
        let r = select(GROUPS, &riders, 14, true, -1, 2, 3).unwrap();
        assert!(r.iter().all(|e| e.female));
        assert_ne!(r[0].ai, r[1].ai);
        assert!(select(GROUPS, &riders, 14, false, -1, 5, 3).is_err());
    }
}
