use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
pub enum ClientMsg {
    #[serde(rename = "create")]
    Create {
        /// Display name for guests. Ignored for authenticated users (we use
        /// their account username instead).
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "join")]
    Join {
        room: String,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "move")]
    Move { from: u8, to: u8 },
    #[serde(rename = "resign")]
    Resign,
    #[serde(rename = "rematch")]
    Rematch,
    #[serde(rename = "rematch_cancel")]
    RematchCancel,
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "t")]
pub enum ServerMsg {
    #[serde(rename = "joined")]
    Joined {
        room: String,
        color: String, // "red" or "black"
        board: serde_json::Value,
        turn: u8,
        last_move: Option<MovePayload>,
        status: String,
        in_check: bool,
        seats: SeatsPayload,
        scoreboard: Vec<ScoreEntry>,
    },
    #[serde(rename = "move")]
    Move {
        from: u8,
        to: u8,
        board: serde_json::Value,
        turn: u8,
        status: String,
        in_check: bool,
    },
    #[serde(rename = "opponent_joined")]
    OpponentJoined,
    #[serde(rename = "opponent_left")]
    OpponentLeft,
    #[serde(rename = "seats")]
    Seats { seats: SeatsPayload },
    #[serde(rename = "scoreboard")]
    Scoreboard { scoreboard: Vec<ScoreEntry> },
    #[serde(rename = "rematch_pending")]
    RematchPending { red_ready: bool, black_ready: bool },
    #[serde(rename = "game_over")]
    GameOver { winner: String, reason: String },
    #[serde(rename = "error")]
    Error { reason: String },
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Debug, Serialize, Clone)]
pub struct MovePayload {
    pub from: u8,
    pub to: u8,
}

#[derive(Debug, Serialize, Clone)]
pub struct SeatPayload {
    pub name: String,
    /// "user" for authenticated accounts, "guest" for self-declared names.
    pub kind: &'static str,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct SeatsPayload {
    pub red: Option<SeatPayload>,
    pub black: Option<SeatPayload>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ScoreEntry {
    pub name: String,
    pub kind: &'static str,
    pub wins: u32,
}
