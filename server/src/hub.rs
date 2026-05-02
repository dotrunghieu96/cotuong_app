// In-memory hub mapping room codes to live games and WS senders.
// Each connected WebSocket runs `handle_session`; it owns a per-session
// outbound queue and parks on the inbound stream.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use cotuong_engine::board::{board_to_json, Color};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};

use crate::proto::{ClientMsg, ServerMsg};
use crate::room::{GameRoom, MoveError};

const ROOM_CODE_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const ROOM_CODE_LEN: usize = 6;

pub struct Hub {
    rooms: Mutex<HashMap<String, Arc<Mutex<RoomEntry>>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
        }
    }
}

struct RoomEntry {
    game: GameRoom,
    /// seats[0] = Red, seats[1] = Black.
    seats: [Option<Seat>; 2],
}

struct Seat {
    tx: mpsc::UnboundedSender<ServerMsg>,
}

fn random_code() -> String {
    let mut rng = rand::thread_rng();
    (0..ROOM_CODE_LEN)
        .map(|_| {
            let i = rng.gen_range(0..ROOM_CODE_CHARS.len());
            ROOM_CODE_CHARS[i] as char
        })
        .collect()
}

fn color_idx(c: Color) -> usize {
    match c {
        Color::Red => 0,
        Color::Black => 1,
    }
}

fn color_str(c: Color) -> String {
    match c {
        Color::Red => "red".into(),
        Color::Black => "black".into(),
    }
}

fn turn_byte(c: Color) -> u8 {
    match c {
        Color::Red => 0,
        Color::Black => 1,
    }
}

fn board_value(b: &cotuong_engine::board::Board) -> Value {
    serde_json::from_str(&board_to_json(b)).unwrap_or(Value::Null)
}

fn broadcast(entry: &RoomEntry, msg: ServerMsg) {
    for seat in entry.seats.iter().flatten() {
        let _ = seat.tx.send(msg.clone());
    }
}

fn current_status(entry: &mut RoomEntry) -> (&'static str, bool) {
    let in_check = entry.game.board.in_check(entry.game.board.turn);
    let any = !entry.game.board.legal_moves().is_empty();
    let status = if any {
        "playing"
    } else if entry.game.finished {
        match entry.game.winner {
            Some(Color::Red) => "red_wins",
            Some(Color::Black) => "black_wins",
            None => "playing",
        }
    } else {
        match entry.game.board.turn {
            Color::Red => "black_wins",
            Color::Black => "red_wins",
        }
    };
    (status, in_check)
}

pub async fn handle_session(socket: WebSocket, hub: Arc<Hub>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();

    // Forward task: ServerMsg -> JSON -> WebSocket frames.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("serialize ServerMsg failed: {e}");
                    continue;
                }
            };
            if ws_tx.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // (room_code, my_color)
    let mut state: Option<(String, Color)> = None;

    while let Some(Ok(msg)) = ws_rx.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            Message::Ping(p) => {
                let _ = out_tx.send(ServerMsg::Pong);
                let _ = p; // axum auto-replies to Ping; just ignore
                continue;
            }
            _ => continue,
        };
        let cm: ClientMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let _ = out_tx.send(ServerMsg::Error {
                    reason: format!("bad message: {e}"),
                });
                continue;
            }
        };

        match cm {
            ClientMsg::Create => {
                if state.is_some() {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "already in a room".into(),
                    });
                    continue;
                }
                let code = create_room(&hub).await;
                join_room(&hub, &code, Color::Red, &out_tx, &mut state).await;
            }
            ClientMsg::Join { room } => {
                if state.is_some() {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "already in a room".into(),
                    });
                    continue;
                }
                let code = room.to_uppercase();
                let arc_room = {
                    let rooms = hub.rooms.lock().await;
                    rooms.get(&code).cloned()
                };
                let Some(arc_room) = arc_room else {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "room not found".into(),
                    });
                    continue;
                };
                let assign = {
                    let r = arc_room.lock().await;
                    if r.seats[0].is_none() {
                        Some(Color::Red)
                    } else if r.seats[1].is_none() {
                        Some(Color::Black)
                    } else {
                        None
                    }
                };
                let Some(color) = assign else {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "room full".into(),
                    });
                    continue;
                };
                join_room(&hub, &code, color, &out_tx, &mut state).await;
            }
            ClientMsg::Move { from, to } => {
                let Some((code, my_color)) = state.clone() else {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "not in a room".into(),
                    });
                    continue;
                };
                let arc_room = {
                    let rooms = hub.rooms.lock().await;
                    rooms.get(&code).cloned()
                };
                let Some(arc_room) = arc_room else {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "room gone".into(),
                    });
                    continue;
                };
                let mut entry = arc_room.lock().await;
                match entry.game.try_play(my_color, from, to) {
                    Err(MoveError::NotYourTurn) => {
                        let _ = out_tx.send(ServerMsg::Error {
                            reason: "not your turn".into(),
                        });
                        continue;
                    }
                    Err(MoveError::GameFinished) => {
                        let _ = out_tx.send(ServerMsg::Error {
                            reason: "game finished".into(),
                        });
                        continue;
                    }
                    Err(MoveError::Illegal) => {
                        let _ = out_tx.send(ServerMsg::Error {
                            reason: "illegal move".into(),
                        });
                        continue;
                    }
                    Ok(outcome) => {
                        let board = board_value(&entry.game.board);
                        let turn = turn_byte(entry.game.board.turn);
                        broadcast(
                            &entry,
                            ServerMsg::Move {
                                from,
                                to,
                                board,
                                turn,
                                status: outcome.status.into(),
                                in_check: outcome.in_check,
                            },
                        );
                        if outcome.status != "playing" {
                            let winner = match entry.game.winner {
                                Some(Color::Red) => "red",
                                Some(Color::Black) => "black",
                                None => continue,
                            };
                            broadcast(
                                &entry,
                                ServerMsg::GameOver {
                                    winner: winner.into(),
                                    reason: "checkmate".into(),
                                },
                            );
                        }
                    }
                }
            }
            ClientMsg::Resign => {
                let Some((code, my_color)) = state.clone() else {
                    continue;
                };
                let arc_room = {
                    let rooms = hub.rooms.lock().await;
                    rooms.get(&code).cloned()
                };
                let Some(arc_room) = arc_room else { continue };
                let mut entry = arc_room.lock().await;
                if let Some(winner) = entry.game.resign(my_color) {
                    broadcast(
                        &entry,
                        ServerMsg::GameOver {
                            winner: color_str(winner),
                            reason: "resignation".into(),
                        },
                    );
                }
            }
            ClientMsg::Ping => {
                let _ = out_tx.send(ServerMsg::Pong);
            }
        }
    }

    // Disconnect cleanup.
    if let Some((code, my_color)) = state {
        let arc_room = {
            let rooms = hub.rooms.lock().await;
            rooms.get(&code).cloned()
        };
        if let Some(arc_room) = arc_room {
            let mut entry = arc_room.lock().await;
            entry.seats[color_idx(my_color)] = None;
            broadcast(&entry, ServerMsg::OpponentLeft);
            let empty = entry.seats.iter().all(|s| s.is_none());
            drop(entry);
            if empty {
                hub.rooms.lock().await.remove(&code);
                tracing::info!("room {} disposed", code);
            }
        }
    }
    send_task.abort();
}

async fn create_room(hub: &Hub) -> String {
    let mut rooms = hub.rooms.lock().await;
    loop {
        let code = random_code();
        if !rooms.contains_key(&code) {
            rooms.insert(
                code.clone(),
                Arc::new(Mutex::new(RoomEntry {
                    game: GameRoom::new(),
                    seats: [None, None],
                })),
            );
            tracing::info!("room {} created", code);
            return code;
        }
    }
}

async fn join_room(
    hub: &Hub,
    code: &str,
    color: Color,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    state: &mut Option<(String, Color)>,
) {
    let arc_room = {
        let rooms = hub.rooms.lock().await;
        rooms.get(code).cloned()
    };
    let Some(arc_room) = arc_room else {
        let _ = out_tx.send(ServerMsg::Error {
            reason: "room not found".into(),
        });
        return;
    };
    let mut entry = arc_room.lock().await;
    let i = color_idx(color);
    if entry.seats[i].is_some() {
        let _ = out_tx.send(ServerMsg::Error {
            reason: "seat taken".into(),
        });
        return;
    }
    entry.seats[i] = Some(Seat {
        tx: out_tx.clone(),
    });
    *state = Some((code.to_string(), color));

    let board = board_value(&entry.game.board);
    let turn = turn_byte(entry.game.board.turn);
    let (status, in_check) = current_status(&mut entry);
    let opponent_present = entry.seats[1 - i].is_some();
    let last_move = entry.game.last_move.clone();

    let _ = out_tx.send(ServerMsg::Joined {
        room: code.to_string(),
        color: color_str(color),
        board,
        turn,
        last_move,
        status: status.into(),
        in_check,
        opponent_present,
    });

    if let Some(other) = &entry.seats[1 - i] {
        let _ = other.tx.send(ServerMsg::OpponentJoined);
    }
}
