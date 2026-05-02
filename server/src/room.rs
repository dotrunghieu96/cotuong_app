// Synchronous game-room state machine. Independent of any networking so we
// can unit-test the rules-driven state transitions directly.

use cotuong_engine::board::{Board, Color, Move};

use crate::proto::MovePayload;

pub struct GameRoom {
    pub board: Board,
    pub history: Vec<Move>,
    pub last_move: Option<MovePayload>,
    pub finished: bool,
    /// Result, if the game is finished. None while playing.
    pub winner: Option<Color>,
}

#[derive(Debug, PartialEq)]
pub enum MoveError {
    NotYourTurn,
    GameFinished,
    Illegal,
}

#[derive(Debug)]
pub struct MoveOutcome {
    pub status: &'static str, // "playing" | "red_wins" | "black_wins"
    pub in_check: bool,
}

impl GameRoom {
    pub fn new() -> Self {
        Self {
            board: Board::new_initial(),
            history: Vec::new(),
            last_move: None,
            finished: false,
            winner: None,
        }
    }

    #[cfg(test)]
    pub fn turn(&self) -> Color {
        self.board.turn
    }

    pub fn try_play(
        &mut self,
        actor: Color,
        from: u8,
        to: u8,
    ) -> Result<MoveOutcome, MoveError> {
        if self.finished {
            return Err(MoveError::GameFinished);
        }
        if self.board.turn != actor {
            return Err(MoveError::NotYourTurn);
        }
        let dests = self.board.legal_moves_from(from as usize);
        if !dests.contains(&(to as usize)) {
            return Err(MoveError::Illegal);
        }
        let mv = self.board.make_move(from as usize, to as usize);
        self.history.push(mv);
        self.last_move = Some(MovePayload { from, to });

        let in_check = self.board.in_check(self.board.turn);
        let any_move = !self.board.legal_moves().is_empty();
        let status = if any_move {
            "playing"
        } else {
            self.finished = true;
            // The side to move has no legal move and therefore loses.
            self.winner = Some(self.board.turn.opp());
            match self.board.turn {
                Color::Red => "black_wins",
                Color::Black => "red_wins",
            }
        };
        Ok(MoveOutcome { status, in_check })
    }

    /// Mark the game finished by resignation. Returns the winning color.
    pub fn resign(&mut self, actor: Color) -> Option<Color> {
        if self.finished {
            return None;
        }
        self.finished = true;
        let winner = actor.opp();
        self.winner = Some(winner);
        Some(winner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(row: i32, col: i32) -> u8 {
        (row * 9 + col) as u8
    }

    #[test]
    fn red_must_move_first() {
        let mut r = GameRoom::new();
        let err = r.try_play(Color::Black, sq(3, 0), sq(4, 0)).unwrap_err();
        assert_eq!(err, MoveError::NotYourTurn);
    }

    #[test]
    fn rejects_illegal_move() {
        let mut r = GameRoom::new();
        // Red king at (9,4) cannot teleport to (0,0)
        let err = r.try_play(Color::Red, sq(9, 4), sq(0, 0)).unwrap_err();
        assert_eq!(err, MoveError::Illegal);
    }

    #[test]
    fn legal_move_advances_turn() {
        let mut r = GameRoom::new();
        // Red pawn (6,0) -> (5,0) is legal
        let outcome = r.try_play(Color::Red, sq(6, 0), sq(5, 0)).unwrap();
        assert_eq!(outcome.status, "playing");
        assert_eq!(r.turn(), Color::Black);
        assert_eq!(r.last_move.as_ref().unwrap().from, sq(6, 0));
    }

    #[test]
    fn resign_finishes_game() {
        let mut r = GameRoom::new();
        let winner = r.resign(Color::Red).unwrap();
        assert_eq!(winner, Color::Black);
        assert!(r.finished);
        // Subsequent move attempts are rejected.
        let err = r.try_play(Color::Red, sq(6, 0), sq(5, 0)).unwrap_err();
        assert_eq!(err, MoveError::GameFinished);
    }
}
