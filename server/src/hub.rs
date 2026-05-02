// In-memory hub mapping room codes to live games and WS senders.
// Each connected WebSocket runs `handle_session`; it owns a per-session
// outbound queue and parks on the inbound stream.
//
// Persistence is write-through via `Arc<dyn Storage>`: a `games` row is
// inserted at room creation, a `moves` row per legal move, and
// `finish_game` runs on checkmate / resignation / abandonment. Storage
// failures are logged but never abort live play.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use cotuong_engine::board::{board_to_json, Color};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::db::{GameResult, Storage, Termination, UserRecord};
use crate::proto::{ClientMsg, SeatPayload, SeatsPayload, ServerMsg};
use crate::room::{GameRoom, MoveError};

const GUEST_NAME_MAX: usize = 30;

/// Trim, drop control chars, cap length. Returns None for an empty result.
fn sanitize_guest_name(raw: Option<String>) -> Option<String> {
    let s = raw?.trim().to_string();
    if s.is_empty() {
        return None;
    }
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(GUEST_NAME_MAX)
        .collect();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

const ROOM_CODE_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const ROOM_CODE_LEN: usize = 6;

pub struct Hub {
    rooms: Mutex<HashMap<String, Arc<Mutex<RoomEntry>>>>,
    storage: Arc<dyn Storage>,
}

impl Hub {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            storage,
        }
    }
}

struct RoomEntry {
    game: GameRoom,
    /// seats[0] = Red, seats[1] = Black.
    seats: [Option<Seat>; 2],
    game_id: Uuid,
}

struct Seat {
    tx: mpsc::UnboundedSender<ServerMsg>,
    name: String,
    /// "user" for an authenticated account, "guest" for self-declared.
    kind: &'static str,
}

impl Seat {
    fn payload(&self) -> SeatPayload {
        SeatPayload {
            name: self.name.clone(),
            kind: self.kind,
        }
    }
}

fn seats_payload(entry: &RoomEntry) -> SeatsPayload {
    SeatsPayload {
        red: entry.seats[0].as_ref().map(Seat::payload),
        black: entry.seats[1].as_ref().map(Seat::payload),
    }
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

fn winner_to_result(c: Color) -> GameResult {
    match c {
        Color::Red => GameResult::RedWins,
        Color::Black => GameResult::BlackWins,
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

pub async fn handle_session(socket: WebSocket, hub: Arc<Hub>, user: Option<UserRecord>) {
    let user_id = user.as_ref().map(|u| u.id);
    let user_name = user.as_ref().map(|u| u.username.clone());
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
            ClientMsg::Create { name } => {
                if state.is_some() {
                    let _ = out_tx.send(ServerMsg::Error {
                        reason: "already in a room".into(),
                    });
                    continue;
                }
                let display = resolve_display(user_name.as_deref(), name);
                match create_room(&hub, user_id).await {
                    Ok(code) => {
                        join_room(&hub, &code, Color::Red, user_id, display, &out_tx, &mut state).await;
                    }
                    Err(e) => {
                        tracing::error!("create_room failed: {e}");
                        let _ = out_tx.send(ServerMsg::Error {
                            reason: "server unavailable".into(),
                        });
                    }
                }
            }
            ClientMsg::Join { room, name } => {
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
                let display = resolve_display(user_name.as_deref(), name);
                join_room(&hub, &code, color, user_id, display, &out_tx, &mut state).await;
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
                        let ply = entry.game.history.len() as i32;
                        let game_id = entry.game_id;
                        let winner = entry.game.winner;
                        let status_owned: String = outcome.status.into();

                        broadcast(
                            &entry,
                            ServerMsg::Move {
                                from,
                                to,
                                board,
                                turn,
                                status: status_owned.clone(),
                                in_check: outcome.in_check,
                            },
                        );

                        // Persist the move. Storage errors must not break live play.
                        if let Err(e) = hub
                            .storage
                            .record_move(game_id, ply, from as i32, to as i32)
                            .await
                        {
                            tracing::warn!("record_move failed for {game_id}: {e}");
                        }

                        if outcome.status != "playing" {
                            if let Some(w) = winner {
                                broadcast(
                                    &entry,
                                    ServerMsg::GameOver {
                                        winner: color_str(w),
                                        reason: "checkmate".into(),
                                    },
                                );
                                if let Err(e) = hub
                                    .storage
                                    .finish_game(
                                        game_id,
                                        Some(winner_to_result(w)),
                                        Termination::Checkmate,
                                    )
                                    .await
                                {
                                    tracing::warn!("finish_game (checkmate) failed: {e}");
                                }
                            }
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
                let game_id = entry.game_id;
                if let Some(winner) = entry.game.resign(my_color) {
                    broadcast(
                        &entry,
                        ServerMsg::GameOver {
                            winner: color_str(winner),
                            reason: "resignation".into(),
                        },
                    );
                    if let Err(e) = hub
                        .storage
                        .finish_game(
                            game_id,
                            Some(winner_to_result(winner)),
                            Termination::Resignation,
                        )
                        .await
                    {
                        tracing::warn!("finish_game (resignation) failed: {e}");
                    }
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
            broadcast(
                &entry,
                ServerMsg::Seats {
                    seats: seats_payload(&entry),
                },
            );
            let empty = entry.seats.iter().all(|s| s.is_none());
            let unfinished = !entry.game.finished;
            let game_id = entry.game_id;
            drop(entry);
            if empty {
                hub.rooms.lock().await.remove(&code);
                tracing::info!("room {} disposed", code);
                if unfinished {
                    if let Err(e) = hub
                        .storage
                        .finish_game(game_id, None, Termination::Abandoned)
                        .await
                    {
                        tracing::warn!("finish_game (abandoned) failed: {e}");
                    }
                }
            }
        }
    }
    send_task.abort();
}

async fn create_room(hub: &Hub, red_user_id: Option<Uuid>) -> crate::db::Result<String> {
    let mut rooms = hub.rooms.lock().await;
    loop {
        let code = random_code();
        if !rooms.contains_key(&code) {
            let game_id = hub.storage.create_game(&code, red_user_id).await?;
            rooms.insert(
                code.clone(),
                Arc::new(Mutex::new(RoomEntry {
                    game: GameRoom::new(),
                    seats: [None, None],
                    game_id,
                })),
            );
            tracing::info!("room {} created (game {})", code, game_id);
            return Ok(code);
        }
    }
}

async fn join_room(
    hub: &Hub,
    code: &str,
    color: Color,
    user_id: Option<Uuid>,
    display: SeatDisplay,
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
        name: display.name,
        kind: display.kind,
    });
    *state = Some((code.to_string(), color));

    // Attribute the seat in the games row. Red is set at create_game time;
    // black is filled in here when the second seat lands. Skip if anonymous.
    if color == Color::Black {
        if let Some(uid) = user_id {
            if let Err(e) = hub.storage.set_black_player(entry.game_id, uid).await {
                tracing::warn!("set_black_player failed: {e}");
            }
        }
    }

    let board = board_value(&entry.game.board);
    let turn = turn_byte(entry.game.board.turn);
    let (status, in_check) = current_status(&mut entry);
    let last_move = entry.game.last_move.clone();
    let seats = seats_payload(&entry);

    let _ = out_tx.send(ServerMsg::Joined {
        room: code.to_string(),
        color: color_str(color),
        board,
        turn,
        last_move,
        status: status.into(),
        in_check,
        seats: seats.clone(),
    });

    if let Some(other) = &entry.seats[1 - i] {
        let _ = other.tx.send(ServerMsg::OpponentJoined);
        let _ = other.tx.send(ServerMsg::Seats {
            seats: seats.clone(),
        });
    }
}

struct SeatDisplay {
    name: String,
    kind: &'static str,
}

fn resolve_display(account_name: Option<&str>, supplied_guest: Option<String>) -> SeatDisplay {
    if let Some(account) = account_name {
        return SeatDisplay {
            name: account.to_string(),
            kind: "user",
        };
    }
    let guest = sanitize_guest_name(supplied_guest).unwrap_or_else(|| "Guest".to_string());
    SeatDisplay {
        name: guest,
        kind: "guest",
    }
}
