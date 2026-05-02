use wasm_bindgen::prelude::*;

use crate::board::{Board, Color, Move};
use crate::search;

#[wasm_bindgen]
pub struct Game {
    board: Board,
    history: Vec<Move>,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Game {
            board: Board::new_initial(),
            history: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.board = Board::new_initial();
        self.history.clear();
    }

    /// Returns the side to move: 0 = Red, 1 = Black.
    pub fn turn(&self) -> u8 {
        match self.board.turn {
            Color::Red => 0,
            Color::Black => 1,
        }
    }

    pub fn ply(&self) -> u32 {
        self.history.len() as u32
    }

    /// JSON array of 90 entries (row-major, row 0 = top/Black side). Each
    /// entry is either `null` or a two-letter piece code like `"rR"`, `"bP"`.
    pub fn board_json(&self) -> String {
        crate::board::board_to_json(&self.board)
    }

    /// JSON array of legal destination square indices for the piece at `from`.
    pub fn legal_moves_from(&mut self, from: u8) -> String {
        let dests = self.board.legal_moves_from(from as usize);
        let mut s = String::with_capacity(8 + dests.len() * 4);
        s.push('[');
        for (i, d) in dests.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&d.to_string());
        }
        s.push(']');
        s
    }

    /// Returns true if the move was legal and was played.
    pub fn play_move(&mut self, from: u8, to: u8) -> bool {
        let dests = self.board.legal_moves_from(from as usize);
        if !dests.contains(&(to as usize)) {
            return false;
        }
        let mv = self.board.make_move(from as usize, to as usize);
        self.history.push(mv);
        true
    }

    /// Undo the last move. Returns true if a move was undone.
    pub fn undo(&mut self) -> bool {
        if let Some(mv) = self.history.pop() {
            self.board.unmake_move(mv);
            true
        } else {
            false
        }
    }

    /// Search and play the best move for the side to move at the given depth.
    /// Returns JSON `{"from":N,"to":N}` or `null` if there is no legal move.
    pub fn ai_move(&mut self, depth: u32) -> String {
        let best = search::search_best(&mut self.board, depth.max(1));
        match best {
            None => "null".to_string(),
            Some((from, to)) => {
                let mv = self.board.make_move(from, to);
                self.history.push(mv);
                format!("{{\"from\":{},\"to\":{}}}", from, to)
            }
        }
    }

    /// Suggest the best move without playing it.
    pub fn suggest_move(&mut self, depth: u32) -> String {
        let best = search::search_best(&mut self.board, depth.max(1));
        match best {
            None => "null".to_string(),
            Some((from, to)) => format!("{{\"from\":{},\"to\":{}}}", from, to),
        }
    }

    /// Returns one of: "playing", "red_wins", "black_wins".
    /// Xiangqi has no stalemate — a side with no legal moves loses.
    pub fn status(&mut self) -> String {
        let moves = self.board.legal_moves();
        if !moves.is_empty() {
            return "playing".to_string();
        }
        match self.board.turn {
            Color::Red => "black_wins".to_string(),
            Color::Black => "red_wins".to_string(),
        }
    }

    pub fn in_check(&self) -> bool {
        self.board.in_check(self.board.turn)
    }

    /// JSON `{"from":N,"to":N}` for the last move, or `null`.
    pub fn last_move_json(&self) -> String {
        match self.history.last() {
            None => "null".to_string(),
            Some(mv) => format!("{{\"from\":{},\"to\":{}}}", mv.from, mv.to),
        }
    }
}
