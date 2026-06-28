use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EloUpdate {
    pub winner_before: f64,
    pub loser_before: f64,
    pub winner_after: f64,
    pub loser_after: f64,
    pub k_factor: f64,
}

pub fn expected_score(player: f64, opponent: f64) -> f64 {
    1.0 / (1.0 + 10.0_f64.powf((opponent - player) / 400.0))
}

pub fn apply_elo_win(winner: f64, loser: f64, k_factor: f64) -> EloUpdate {
    let winner_expected = expected_score(winner, loser);
    let loser_expected = expected_score(loser, winner);

    EloUpdate {
        winner_before: winner,
        loser_before: loser,
        winner_after: winner + k_factor * (1.0 - winner_expected),
        loser_after: loser + k_factor * (0.0 - loser_expected),
        k_factor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winner_gains_and_loser_loses_rating() {
        let update = apply_elo_win(1000.0, 1000.0, 32.0);

        assert_eq!(update.winner_after, 1016.0);
        assert_eq!(update.loser_after, 984.0);
    }

    #[test]
    fn underdog_win_moves_more_than_favorite_win() {
        let underdog = apply_elo_win(900.0, 1100.0, 32.0);
        let favorite = apply_elo_win(1100.0, 900.0, 32.0);

        assert!(
            underdog.winner_after - underdog.winner_before
                > favorite.winner_after - favorite.winner_before
        );
    }
}
