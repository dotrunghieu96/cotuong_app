use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
pub enum ClientMsg {
    #[serde(rename = "create")]
    Create,
    #[serde(rename = "join")]
    Join { room: String },
    #[serde(rename = "move")]
    Move { from: u8, to: u8 },
    #[serde(rename = "resign")]
    Resign,
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
        opponent_present: bool,
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
