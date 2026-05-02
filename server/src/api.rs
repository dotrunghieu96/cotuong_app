// Read-only HTTP endpoints for browsing finished games and replaying moves.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::{GameRecord, ListGamesQuery, MoveRecord};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default)]
    pub limit: Option<i32>,
    #[serde(default)]
    pub finished: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListResp {
    pub games: Vec<GameRecord>,
}

#[derive(Debug, Serialize)]
pub struct GameDetail {
    #[serde(flatten)]
    pub game: GameRecord,
    pub moves: Vec<MoveRecord>,
}

pub async fn list_games(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<ListResp>, ApiError> {
    let q = ListGamesQuery {
        limit: p.limit.unwrap_or(50),
        finished_only: p.finished.unwrap_or(false),
    };
    let games = state.storage.list_games(q).await?;
    Ok(Json(ListResp { games }))
}

pub async fn get_game(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<GameDetail>, ApiError> {
    let id = Uuid::parse_str(&id).map_err(|_| ApiError::NotFound)?;
    let Some(game) = state.storage.get_game(id).await? else {
        return Err(ApiError::NotFound);
    };
    let moves = state.storage.list_moves(id).await?;
    Ok(Json(GameDetail { game, moves }))
}

pub async fn get_moves(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<MoveRecord>>, ApiError> {
    let id = Uuid::parse_str(&id).map_err(|_| ApiError::NotFound)?;
    let moves = state.storage.list_moves(id).await?;
    Ok(Json(moves))
}

pub enum ApiError {
    NotFound,
    Storage(crate::db::StorageError),
}

impl From<crate::db::StorageError> for ApiError {
    fn from(e: crate::db::StorageError) -> Self {
        ApiError::Storage(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            ApiError::Storage(e) => {
                tracing::warn!("storage error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "server error").into_response()
            }
        }
    }
}
