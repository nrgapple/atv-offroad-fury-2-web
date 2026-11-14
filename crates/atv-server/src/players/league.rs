//! Weekly league points, tiers that only go up within a week, and lifetime
//! levels (research/ONLINE_IDENTITY.md "League and levels").

/// Weeks start Monday 00:00 UTC. 1970-01-01 was a Thursday, so day 4 is the
/// first Monday.
pub fn week(now_ms: u64) -> u32 {
    let day = now_ms / 86_400_000;
    ((day + 3) / 7) as u32
}

/// When `week` starts, in ms since the epoch.
pub fn week_start_ms(week: u32) -> u64 {
    (u64::from(week) * 7).saturating_sub(3) * 86_400_000
}

/// Tier names and the weekly points that reach them. Points never go down
/// within a week, so neither does the tier.
pub const TIERS: [(&str, u32); 6] = [
    ("bronze", 0),
    ("silver", 100),
    ("gold", 250),
    ("platinum", 500),
    ("diamond", 900),
    ("champion", 1400),
];

pub fn tier(points: u32) -> usize {
    TIERS.iter().rposition(|&(_, at)| points >= at).unwrap_or(0)
}

/// Points at which the next tier starts, if any.
pub fn next_tier_at(points: u32) -> Option<u32> {
    TIERS.get(tier(points) + 1).map(|&(_, at)| at)
}

/// Points for one verified match: 10 for taking part, 5 for every rider
/// (CPUs too) placed behind, 10 more for a win. A rider who left gets 0.
pub fn match_points(place: u32, riders: u32, left: bool) -> u32 {
    if left || place == 0 {
        return 0;
    }
    let beaten = riders.saturating_sub(place).min(19);
    10 + 5 * beaten + if place == 1 && riders > 1 { 10 } else { 0 }
}

/// Total XP needed to reach `level` (level 1 needs none, each level needs
/// 100 more than the one before).
pub fn level_xp(level: u32) -> u64 {
    let l = u64::from(level.max(1));
    50 * l * (l - 1)
}

/// The level for lifetime `xp`, starting at 1.
pub fn level(xp: u64) -> u32 {
    // 50 L (L - 1) <= xp: start from the closed form and correct rounding.
    let mut l = ((1.0 + (1.0 + 0.08 * xp as f64).sqrt()) / 2.0) as u32;
    l = l.max(1);
    while level_xp(l + 1) <= xp {
        l += 1;
    }
    while l > 1 && level_xp(l) > xp {
        l -= 1;
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400_000;

    #[test]
    fn weeks_turn_over_monday_midnight_utc() {
        // 2026-11-16 is a Monday.
        let monday = 20_773 * DAY;
        assert_eq!(week(monday) - 1, week(monday - 1));
        assert_eq!(week(monday), week(monday + 7 * DAY - 1));
        assert_eq!(week(monday) + 1, week(monday + 7 * DAY));
        assert_eq!(week_start_ms(week(monday)), monday);
        assert_eq!(week_start_ms(week(monday + 3 * DAY + 5)), monday);
        // 1970-01-05 was the first Monday.
        assert_eq!(week(4 * DAY), 1);
        assert_eq!(week(4 * DAY - 1), 0);
    }

    #[test]
    fn tiers_climb_with_points() {
        assert_eq!(TIERS[tier(0)].0, "bronze");
        assert_eq!(TIERS[tier(99)].0, "bronze");
        assert_eq!(TIERS[tier(100)].0, "silver");
        assert_eq!(TIERS[tier(5000)].0, "champion");
        assert_eq!(next_tier_at(120), Some(250));
        assert_eq!(next_tier_at(1400), None);
    }

    #[test]
    fn points_reward_places_beaten() {
        assert_eq!(match_points(1, 6, false), 10 + 25 + 10);
        assert_eq!(match_points(6, 6, false), 10);
        assert_eq!(match_points(3, 6, false), 25);
        assert_eq!(match_points(1, 6, true), 0);
        // Alone: no win bonus.
        assert_eq!(match_points(1, 1, false), 10);
    }

    #[test]
    fn levels_follow_the_xp_curve() {
        assert_eq!(level(0), 1);
        assert_eq!(level(99), 1);
        assert_eq!(level(100), 2);
        assert_eq!(level(299), 2);
        assert_eq!(level(300), 3);
        for l in 1..300 {
            assert_eq!(level(level_xp(l)), l);
            assert_eq!(level(level_xp(l + 1) - 1), l);
        }
    }
}
