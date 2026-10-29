//! # Stats Component (0x0007) & Match Report Persistence
//!
//! Handles post-match statistics reporting, match outcome persistence,
//! and player leaderboard queries for FIFA 22.

use std::collections::HashMap;
use std::io;
use std::sync::OnceLock;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfValue};
use crate::session::SessionState;
use crate::session_store::SessionStore;

pub const CMD_GET_STATS: u16 = 0x0001;
pub const CMD_UPDATE_STATS: u16 = 0x0002;
pub const CMD_GET_LEADERBOARD: u16 = 0x0003;
pub const CMD_GET_CENTERED_LEADERBOARD: u16 = 0x0004;
pub const CMD_GET_FILTERED_LEADERBOARD: u16 = 0x0005;
pub const CMD_GET_STAT_GROUP_LIST: u16 = 0x000B;
pub const CMD_GET_LEADERBOARD_TREE: u16 = 0x000E;
pub const CMD_SUBMIT_GAME_REPORT: u16 = 0x001C;

/// Represents the completed result of an in-game match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchReport {
    pub game_id: u64,
    pub host_user_id: u64,
    pub host_persona: String,
    pub guest_user_id: u64,
    pub guest_persona: String,
    pub home_score: u32,
    pub away_score: u32,
    pub winner_user_id: Option<u64>,
    pub timestamp_epoch_secs: u64,
}

/// Cumulative match record and performance metrics for a player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub user_id: u64,
    pub persona_name: String,
    pub matches_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub goals_scored: u32,
    pub goals_conceded: u32,
    pub points: u32,
}

impl PlayerStats {
    pub fn new(user_id: u64, persona_name: String) -> Self {
        Self {
            user_id,
            persona_name,
            matches_played: 0,
            wins: 0,
            losses: 0,
            draws: 0,
            goals_scored: 0,
            goals_conceded: 0,
            points: 0,
        }
    }

    pub fn apply_match(&mut self, scored: u32, conceded: u32) {
        self.matches_played += 1;
        self.goals_scored += scored;
        self.goals_conceded += conceded;

        if scored > conceded {
            self.wins += 1;
            self.points += 3;
        } else if scored < conceded {
            self.losses += 1;
        } else {
            self.draws += 1;
            self.points += 1;
        }
    }
}

/// Global thread-safe statistics database and leaderboard manager.
pub struct StatsStore {
    players: RwLock<HashMap<u64, PlayerStats>>,
    history: RwLock<Vec<MatchReport>>,
}

impl Default for StatsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsStore {
    pub fn new() -> Self {
        Self {
            players: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
        }
    }

    /// Global singleton instance.
    pub fn global() -> &'static StatsStore {
        static INSTANCE: OnceLock<StatsStore> = OnceLock::new();
        INSTANCE.get_or_init(StatsStore::new)
    }

    /// Records a completed match outcome and updates both participants' records.
    pub fn record_match(&self, report: MatchReport) {
        let mut players = self.players.write();

        // Update Host
        let host_stat = players
            .entry(report.host_user_id)
            .or_insert_with(|| PlayerStats::new(report.host_user_id, report.host_persona.clone()));
        host_stat.apply_match(report.home_score, report.away_score);

        // Update Guest
        let guest_stat = players
            .entry(report.guest_user_id)
            .or_insert_with(|| PlayerStats::new(report.guest_user_id, report.guest_persona.clone()));
        guest_stat.apply_match(report.away_score, report.home_score);

        tracing::info!(
            "[Stats] Match {} recorded: '{}' ({}) vs '{}' ({}) -> Winner: {:?}",
            report.game_id,
            report.host_persona,
            report.home_score,
            report.guest_persona,
            report.away_score,
            report.winner_user_id
        );

        self.history.write().push(report);
    }

    /// Queries stats for a specific user ID.
    pub fn get_player(&self, user_id: u64) -> Option<PlayerStats> {
        self.players.read().get(&user_id).cloned()
    }

    /// Returns ranked leaderboard sorted by Points, then Goal Difference.
    pub fn get_leaderboard(&self, limit: usize) -> Vec<PlayerStats> {
        let players = self.players.read();
        let mut list: Vec<PlayerStats> = players.values().cloned().collect();

        list.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then_with(|| {
                    let gd_a = a.goals_scored as i64 - a.goals_conceded as i64;
                    let gd_b = b.goals_scored as i64 - b.goals_conceded as i64;
                    gd_b.cmp(&gd_a)
                })
                .then_with(|| b.goals_scored.cmp(&a.goals_scored))
        });

        if list.len() > limit {
            list.truncate(limit);
        }
        list
    }

    /// Returns total match count in history.
    pub fn match_count(&self) -> usize {
        self.history.read().len()
    }
}

// =============================================================================
// Stats Component Handlers
// =============================================================================

/// Handles getStatsGroup / getStats (0x0001).
pub fn handle_get_stats(
    req: &BlazePacket,
    session: &SessionState,
) -> io::Result<BlazePacket> {
    let store = StatsStore::global();
    let pstat = store.get_player(session.user_id).unwrap_or_else(|| {
        PlayerStats::new(session.user_id, session.display_name.clone())
    });

    let mut resp = TdfStruct::new();

    // STAT List containing entity stat entries
    let mut stat_entries = Vec::new();
    let mut stat_item = TdfStruct::new();
    stat_item.set("EID", TdfValue::Integer(pstat.user_id));
    stat_item.set("PLYD", TdfValue::Integer(pstat.matches_played as u64));
    stat_item.set("WINS", TdfValue::Integer(pstat.wins as u64));
    stat_item.set("LOSS", TdfValue::Integer(pstat.losses as u64));
    stat_item.set("DRAW", TdfValue::Integer(pstat.draws as u64));
    stat_item.set("GF", TdfValue::Integer(pstat.goals_scored as u64));
    stat_item.set("GA", TdfValue::Integer(pstat.goals_conceded as u64));
    stat_item.set("PTS", TdfValue::Integer(pstat.points as u64));
    stat_entries.push(TdfValue::Struct(stat_item));

    resp.set("STAT", TdfValue::List(TdfList::new(TdfType::Struct, stat_entries)));

    BlazePacket::response(&req.header, resp)
}

/// Handles updateStats (0x0002).
pub fn handle_update_stats(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles getLeaderboard / getCenteredLeaderboard (0x0003 / 0x0004).
pub fn handle_get_leaderboard(req: &BlazePacket) -> io::Result<BlazePacket> {
    let store = StatsStore::global();
    let top_players = store.get_leaderboard(50);

    let mut resp = TdfStruct::new();
    let mut rows = Vec::new();

    for (idx, p) in top_players.iter().enumerate() {
        let mut row = TdfStruct::new();
        row.set("EID", TdfValue::Integer(p.user_id));
        row.set("ENAM", TdfValue::String(p.persona_name.clone()));
        row.set("RANK", TdfValue::Integer((idx + 1) as u64));
        row.set("PTS", TdfValue::Integer(p.points as u64));
        row.set("WINS", TdfValue::Integer(p.wins as u64));
        row.set("LOSS", TdfValue::Integer(p.losses as u64));

        let mut stats_list = Vec::new();
        stats_list.push(TdfValue::Integer(p.points as u64));
        stats_list.push(TdfValue::Integer(p.wins as u64));
        stats_list.push(TdfValue::Integer(p.goals_scored as u64));
        row.set("RSTA", TdfValue::List(TdfList::new(TdfType::Integer, stats_list)));

        rows.push(TdfValue::Struct(row));
    }

    resp.set("LDST", TdfValue::List(TdfList::new(TdfType::Struct, rows)));
    BlazePacket::response(&req.header, resp)
}

/// Handles submitGameReport (0x001C).
pub fn handle_submit_game_report(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
) -> io::Result<BlazePacket> {
    let game_id = match req.payload.get("GID") {
        Some(TdfValue::Integer(gid)) => *gid,
        _ => 1,
    };

    let home_score = match req.payload.get("SCOR") {
        Some(TdfValue::Integer(s)) => *s as u32,
        _ => 0,
    };

    let away_score = match req.payload.get("OSCR") {
        Some(TdfValue::Integer(s)) => *s as u32,
        _ => 0,
    };

    let opponent_uid = match req.payload.get("OUID") {
        Some(TdfValue::Integer(uid)) => *uid,
        _ => session.user_id.wrapping_add(1),
    };

    let opp_name = store
        .get_session(opponent_uid)
        .map(|s| s.persona_name)
        .unwrap_or_else(|| "Opponent".to_string());

    let winner_user_id = if home_score > away_score {
        Some(session.user_id)
    } else if away_score > home_score {
        Some(opponent_uid)
    } else {
        None
    };

    let report = MatchReport {
        game_id,
        host_user_id: session.user_id,
        host_persona: session.display_name.clone(),
        guest_user_id: opponent_uid,
        guest_persona: opp_name,
        home_score,
        away_score,
        winner_user_id,
        timestamp_epoch_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    StatsStore::global().record_match(report);

    BlazePacket::response(&req.header, TdfStruct::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_store_record_match_and_leaderboard() {
        let store = StatsStore::new();

        let report1 = MatchReport {
            game_id: 101,
            host_user_id: 1001,
            host_persona: "RealMadridMaster".to_string(),
            guest_user_id: 1002,
            guest_persona: "LiverpoolKing".to_string(),
            home_score: 3,
            away_score: 1,
            winner_user_id: Some(1001),
            timestamp_epoch_secs: 1000000,
        };

        store.record_match(report1);
        assert_eq!(store.match_count(), 1);

        let p1 = store.get_player(1001).unwrap();
        assert_eq!(p1.matches_played, 1);
        assert_eq!(p1.wins, 1);
        assert_eq!(p1.losses, 0);
        assert_eq!(p1.goals_scored, 3);
        assert_eq!(p1.goals_conceded, 1);
        assert_eq!(p1.points, 3);

        let p2 = store.get_player(1002).unwrap();
        assert_eq!(p2.matches_played, 1);
        assert_eq!(p2.wins, 0);
        assert_eq!(p2.losses, 1);
        assert_eq!(p2.goals_scored, 1);
        assert_eq!(p2.goals_conceded, 3);
        assert_eq!(p2.points, 0);

        let report2 = MatchReport {
            game_id: 102,
            host_user_id: 1002,
            host_persona: "LiverpoolKing".to_string(),
            guest_user_id: 1001,
            guest_persona: "RealMadridMaster".to_string(),
            home_score: 2,
            away_score: 2,
            winner_user_id: None,
            timestamp_epoch_secs: 1000100,
        };

        store.record_match(report2);
        assert_eq!(store.match_count(), 2);

        let lb = store.get_leaderboard(10);
        assert_eq!(lb.len(), 2);
        assert_eq!(lb[0].user_id, 1001); // 4 points
        assert_eq!(lb[0].points, 4);
        assert_eq!(lb[1].user_id, 1002); // 1 point
        assert_eq!(lb[1].points, 1);
    }
}
