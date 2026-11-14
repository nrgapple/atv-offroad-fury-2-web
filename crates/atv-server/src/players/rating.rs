//! Hidden skill rating: OpenSkill's Plackett-Luce model (Weng and Lin 2011,
//! Algorithm 4; arXiv 2401.05451) for one-rider teams. Players never see it;
//! it picks CPU difficulty and splits players between rooms.

use serde::{Deserialize, Serialize};

pub const MU: f64 = 25.0;
pub const SIGMA: f64 = MU / 3.0;
const BETA: f64 = SIGMA / 2.0;
const KAPPA: f64 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub mu: f64,
    pub sigma: f64,
}

impl Default for Rating {
    /// A new rider: average skill, high uncertainty.
    fn default() -> Self {
        Rating {
            mu: MU,
            sigma: SIGMA,
        }
    }
}

impl Rating {
    /// The conservative estimate used to compare riders (mu − 3σ); a new
    /// rider starts at 0.
    pub fn ordinal(&self) -> f64 {
        self.mu - 3.0 * self.sigma
    }
}

/// Updates `ratings` for one finishing order: `ranks[i]` is rider i's
/// place, lower is better and equal ranks tie. Fewer than two riders
/// change nothing.
pub fn rate(ratings: &mut [Rating], ranks: &[u32]) {
    let n = ratings.len();
    if n < 2 || ranks.len() != n {
        return;
    }
    let c = ratings
        .iter()
        .map(|r| r.sigma * r.sigma + BETA * BETA)
        .sum::<f64>()
        .sqrt();
    let strength: Vec<f64> = ratings.iter().map(|r| (r.mu / c).exp()).collect();
    // C_q: the strengths of everyone placed at or behind q.
    let behind: Vec<f64> = (0..n)
        .map(|q| {
            (0..n)
                .filter(|&s| ranks[s] >= ranks[q])
                .map(|s| strength[s])
                .sum()
        })
        .collect();
    // A_q: how many share q's place.
    let tied: Vec<f64> = (0..n)
        .map(|q| ranks.iter().filter(|&&r| r == ranks[q]).count() as f64)
        .collect();
    let updated: Vec<Rating> = (0..n)
        .map(|i| {
            let (mut omega, mut delta) = (0.0, 0.0);
            for q in (0..n).filter(|&q| ranks[q] <= ranks[i]) {
                let share = strength[i] / behind[q];
                omega += (if q == i { 1.0 } else { 0.0 } - share) / tied[q];
                delta += share * (1.0 - share) / tied[q];
            }
            let variance = ratings[i].sigma * ratings[i].sigma;
            let gamma = ratings[i].sigma / c;
            let omega = omega * variance / c;
            let delta = delta * gamma * variance / (c * c);
            Rating {
                mu: ratings[i].mu + omega,
                sigma: (variance * (1.0 - delta).max(KAPPA)).sqrt(),
            }
        })
        .collect();
    ratings.copy_from_slice(&updated);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn two_new_riders_match_openskill() {
        // openskill.js / openskill.py PlackettLuce defaults for a 1v1.
        let mut r = [Rating::default(); 2];
        rate(&mut r, &[1, 2]);
        assert!(close(r[0].mu, 27.635_231_383_473_217), "{:?}", r[0]);
        assert!(close(r[1].mu, 22.364_768_616_526_783), "{:?}", r[1]);
        assert!(close(r[0].sigma, 8.065_506_316_323_548), "{:?}", r[0]);
        assert!(close(r[1].sigma, r[0].sigma));
    }

    #[test]
    fn order_moves_ratings_and_uncertainty_shrinks() {
        let mut r = [Rating::default(); 4];
        rate(&mut r, &[1, 2, 3, 4]);
        assert!(r[0].mu > r[1].mu && r[1].mu > r[2].mu && r[2].mu > r[3].mu);
        assert!(r.iter().all(|x| x.sigma < SIGMA));
        // Zero-sum around the mean for equal starting ratings.
        let total: f64 = r.iter().map(|x| x.mu).sum();
        assert!((total - 4.0 * MU).abs() < 1e-6);
        // Ties split evenly.
        let mut t = [Rating::default(); 3];
        rate(&mut t, &[1, 2, 2]);
        assert!(close(t[1].mu, t[2].mu) && t[0].mu > t[1].mu);
    }

    #[test]
    fn upsets_move_more_than_expected_results() {
        let strong = Rating {
            mu: 35.0,
            sigma: 3.0,
        };
        let weak = Rating {
            mu: 15.0,
            sigma: 3.0,
        };
        let mut expected = [strong, weak];
        rate(&mut expected, &[1, 2]);
        let mut upset = [strong, weak];
        rate(&mut upset, &[2, 1]);
        assert!((upset[1].mu - weak.mu) > (expected[0].mu - strong.mu));
    }

    #[test]
    fn single_rider_and_mismatched_input_change_nothing() {
        let mut one = [Rating::default()];
        rate(&mut one, &[1]);
        assert_eq!(one[0], Rating::default());
        let mut two = [Rating::default(); 2];
        rate(&mut two, &[1]);
        assert_eq!(two[0], Rating::default());
        assert!(close(Rating::default().ordinal(), 0.0));
    }
}
