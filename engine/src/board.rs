// Xiangqi (Chinese Chess) board representation and rules.
//
// Coordinate system:
//   - Board is 9 files (columns) wide and 10 ranks (rows) tall.
//   - row 0 is Black's back rank (top of board), row 9 is Red's back rank (bottom).
//   - col 0 is leftmost from Red's view.
//   - Square index = row * 9 + col, range 0..90.
//   - River is between row 4 (last Black row) and row 5 (first Red row).
//   - Black palace: rows 0..=2, cols 3..=5. Red palace: rows 7..=9, cols 3..=5.
//   - Red moves first.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Color {
    Red,
    Black,
}

impl Color {
    pub fn opp(self) -> Self {
        match self {
            Color::Red => Color::Black,
            Color::Black => Color::Red,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PieceKind {
    King,
    Advisor,
    Elephant,
    Horse,
    Rook,
    Cannon,
    Pawn,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    pub fn code(self) -> &'static str {
        match (self.color, self.kind) {
            (Color::Red, PieceKind::King) => "rK",
            (Color::Red, PieceKind::Advisor) => "rA",
            (Color::Red, PieceKind::Elephant) => "rE",
            (Color::Red, PieceKind::Horse) => "rH",
            (Color::Red, PieceKind::Rook) => "rR",
            (Color::Red, PieceKind::Cannon) => "rC",
            (Color::Red, PieceKind::Pawn) => "rP",
            (Color::Black, PieceKind::King) => "bK",
            (Color::Black, PieceKind::Advisor) => "bA",
            (Color::Black, PieceKind::Elephant) => "bE",
            (Color::Black, PieceKind::Horse) => "bH",
            (Color::Black, PieceKind::Rook) => "bR",
            (Color::Black, PieceKind::Cannon) => "bC",
            (Color::Black, PieceKind::Pawn) => "bP",
        }
    }
}

pub const FILES: i32 = 9;
pub const RANKS: i32 = 10;

/// Serialize a board to a JSON array of 90 entries (each `null` or a piece
/// code string like `"rR"` / `"bP"`).
pub fn board_to_json(b: &Board) -> String {
    let mut s = String::with_capacity(512);
    s.push('[');
    for i in 0..90 {
        if i > 0 {
            s.push(',');
        }
        match b.squares[i] {
            None => s.push_str("null"),
            Some(p) => {
                s.push('"');
                s.push_str(p.code());
                s.push('"');
            }
        }
    }
    s.push(']');
    s
}

#[inline]
pub fn sq(row: i32, col: i32) -> usize {
    (row * FILES + col) as usize
}

#[inline]
pub fn row_of(s: usize) -> i32 {
    (s as i32) / FILES
}

#[inline]
pub fn col_of(s: usize) -> i32 {
    (s as i32) % FILES
}

#[inline]
pub fn in_bounds(row: i32, col: i32) -> bool {
    row >= 0 && row < RANKS && col >= 0 && col < FILES
}

fn in_palace(color: Color, row: i32, col: i32) -> bool {
    if !(col >= 3 && col <= 5) {
        return false;
    }
    match color {
        Color::Black => row >= 0 && row <= 2,
        Color::Red => row >= 7 && row <= 9,
    }
}

fn on_own_side(color: Color, row: i32) -> bool {
    match color {
        Color::Black => row <= 4,
        Color::Red => row >= 5,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub captured: Option<Piece>,
}

#[derive(Clone)]
pub struct Board {
    pub squares: [Option<Piece>; 90],
    pub turn: Color,
}

impl Board {
    pub fn new_initial() -> Self {
        let mut b = Board {
            squares: [None; 90],
            turn: Color::Red,
        };

        // Black back rank (row 0)
        let back_black = [
            PieceKind::Rook,
            PieceKind::Horse,
            PieceKind::Elephant,
            PieceKind::Advisor,
            PieceKind::King,
            PieceKind::Advisor,
            PieceKind::Elephant,
            PieceKind::Horse,
            PieceKind::Rook,
        ];
        for (c, k) in back_black.iter().enumerate() {
            b.squares[sq(0, c as i32)] = Some(Piece {
                color: Color::Black,
                kind: *k,
            });
        }
        // Black cannons (row 2, cols 1 and 7)
        b.squares[sq(2, 1)] = Some(Piece {
            color: Color::Black,
            kind: PieceKind::Cannon,
        });
        b.squares[sq(2, 7)] = Some(Piece {
            color: Color::Black,
            kind: PieceKind::Cannon,
        });
        // Black soldiers (row 3, cols 0,2,4,6,8)
        for c in [0, 2, 4, 6, 8] {
            b.squares[sq(3, c)] = Some(Piece {
                color: Color::Black,
                kind: PieceKind::Pawn,
            });
        }

        // Red back rank (row 9)
        let back_red = back_black; // mirror
        for (c, k) in back_red.iter().enumerate() {
            b.squares[sq(9, c as i32)] = Some(Piece {
                color: Color::Red,
                kind: *k,
            });
        }
        // Red cannons (row 7, cols 1 and 7)
        b.squares[sq(7, 1)] = Some(Piece {
            color: Color::Red,
            kind: PieceKind::Cannon,
        });
        b.squares[sq(7, 7)] = Some(Piece {
            color: Color::Red,
            kind: PieceKind::Cannon,
        });
        // Red soldiers (row 6, cols 0,2,4,6,8)
        for c in [0, 2, 4, 6, 8] {
            b.squares[sq(6, c)] = Some(Piece {
                color: Color::Red,
                kind: PieceKind::Pawn,
            });
        }

        b
    }

    pub fn find_king(&self, color: Color) -> Option<usize> {
        for (i, sq) in self.squares.iter().enumerate() {
            if let Some(p) = sq {
                if p.color == color && p.kind == PieceKind::King {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Make a pseudo-legal move, recording undo info. Does not check legality.
    pub fn make_move(&mut self, from: usize, to: usize) -> Move {
        let mover = self.squares[from];
        let captured = self.squares[to];
        self.squares[to] = mover;
        self.squares[from] = None;
        self.turn = self.turn.opp();
        Move {
            from: from as u8,
            to: to as u8,
            captured,
        }
    }

    pub fn unmake_move(&mut self, mv: Move) {
        let from = mv.from as usize;
        let to = mv.to as usize;
        self.squares[from] = self.squares[to];
        self.squares[to] = mv.captured;
        self.turn = self.turn.opp();
    }

    /// Generate pseudo-legal moves for the piece at `s`. Pseudo-legal means
    /// kinematically valid for that piece kind on this board, but may leave
    /// own king in check or violate the flying-general rule.
    pub fn pseudo_moves_from(&self, s: usize, out: &mut Vec<usize>) {
        let p = match self.squares[s] {
            Some(p) => p,
            None => return,
        };
        let r = row_of(s);
        let c = col_of(s);
        match p.kind {
            PieceKind::King => self.king_moves(p.color, r, c, out),
            PieceKind::Advisor => self.advisor_moves(p.color, r, c, out),
            PieceKind::Elephant => self.elephant_moves(p.color, r, c, out),
            PieceKind::Horse => self.horse_moves(p.color, r, c, out),
            PieceKind::Rook => self.rook_moves(p.color, r, c, out),
            PieceKind::Cannon => self.cannon_moves(p.color, r, c, out),
            PieceKind::Pawn => self.pawn_moves(p.color, r, c, out),
        }
    }

    fn try_push(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        if !in_bounds(r, c) {
            return;
        }
        match self.squares[sq(r, c)] {
            Some(p) if p.color == color => {} // own piece: blocked
            _ => out.push(sq(r, c)),
        }
    }

    fn king_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nr = r + dr;
            let nc = c + dc;
            if in_palace(color, nr, nc) {
                self.try_push(color, nr, nc, out);
            }
        }
    }

    fn advisor_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        for (dr, dc) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
            let nr = r + dr;
            let nc = c + dc;
            if in_palace(color, nr, nc) {
                self.try_push(color, nr, nc, out);
            }
        }
    }

    fn elephant_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        for (dr, dc) in [(-2, -2), (-2, 2), (2, -2), (2, 2)] {
            let nr = r + dr;
            let nc = c + dc;
            if !in_bounds(nr, nc) {
                continue;
            }
            // Cannot cross the river
            if !on_own_side(color, nr) {
                continue;
            }
            // "Eye" must not be blocked
            let er = r + dr / 2;
            let ec = c + dc / 2;
            if self.squares[sq(er, ec)].is_some() {
                continue;
            }
            self.try_push(color, nr, nc, out);
        }
    }

    fn horse_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        // Eight L-shaped offsets, each blocked by a "leg" piece adjacent in
        // the orthogonal direction of the longer leg.
        let jumps = [
            (-2, -1, -1, 0),
            (-2, 1, -1, 0),
            (2, -1, 1, 0),
            (2, 1, 1, 0),
            (-1, -2, 0, -1),
            (1, -2, 0, -1),
            (-1, 2, 0, 1),
            (1, 2, 0, 1),
        ];
        for (dr, dc, lr, lc) in jumps {
            let nr = r + dr;
            let nc = c + dc;
            if !in_bounds(nr, nc) {
                continue;
            }
            // Leg must be empty
            if self.squares[sq(r + lr, c + lc)].is_some() {
                continue;
            }
            self.try_push(color, nr, nc, out);
        }
    }

    fn rook_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let mut nr = r + dr;
            let mut nc = c + dc;
            while in_bounds(nr, nc) {
                match self.squares[sq(nr, nc)] {
                    None => out.push(sq(nr, nc)),
                    Some(p) => {
                        if p.color != color {
                            out.push(sq(nr, nc));
                        }
                        break;
                    }
                }
                nr += dr;
                nc += dc;
            }
        }
    }

    fn cannon_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            // Phase 1: empty squares are legal non-capture moves.
            let mut nr = r + dr;
            let mut nc = c + dc;
            while in_bounds(nr, nc) && self.squares[sq(nr, nc)].is_none() {
                out.push(sq(nr, nc));
                nr += dr;
                nc += dc;
            }
            if !in_bounds(nr, nc) {
                continue;
            }
            // We hit a screen piece. Skip past it and look for first piece beyond.
            nr += dr;
            nc += dc;
            while in_bounds(nr, nc) {
                if let Some(p) = self.squares[sq(nr, nc)] {
                    if p.color != color {
                        out.push(sq(nr, nc));
                    }
                    break;
                }
                nr += dr;
                nc += dc;
            }
        }
    }

    fn pawn_moves(&self, color: Color, r: i32, c: i32, out: &mut Vec<usize>) {
        let forward = match color {
            Color::Red => -1,
            Color::Black => 1,
        };
        // Forward step
        let nr = r + forward;
        if in_bounds(nr, c) {
            self.try_push(color, nr, c, out);
        }
        // After crossing the river, sideways steps allowed
        let crossed = match color {
            Color::Red => r <= 4,
            Color::Black => r >= 5,
        };
        if crossed {
            for dc in [-1, 1] {
                let nc = c + dc;
                if in_bounds(r, nc) {
                    self.try_push(color, r, nc, out);
                }
            }
        }
    }

    /// Returns true if `color`'s king is attacked by any opposing piece, OR
    /// if both kings face each other on the same file with no piece between.
    pub fn in_check(&self, color: Color) -> bool {
        let king_sq = match self.find_king(color) {
            Some(s) => s,
            None => return true, // king missing: treat as lost
        };

        // Flying general rule: kings cannot face on same file with no piece between.
        if let Some(opp_king) = self.find_king(color.opp()) {
            if col_of(king_sq) == col_of(opp_king) {
                let c = col_of(king_sq);
                let r1 = row_of(king_sq).min(row_of(opp_king));
                let r2 = row_of(king_sq).max(row_of(opp_king));
                let mut any_between = false;
                for r in (r1 + 1)..r2 {
                    if self.squares[sq(r, c)].is_some() {
                        any_between = true;
                        break;
                    }
                }
                if !any_between {
                    return true;
                }
            }
        }

        // Check attacks from any opposing piece.
        let mut moves = Vec::with_capacity(20);
        for s in 0..90 {
            if let Some(p) = self.squares[s] {
                if p.color == color.opp() {
                    moves.clear();
                    self.pseudo_moves_from(s, &mut moves);
                    if moves.contains(&king_sq) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Generate all fully legal moves for the side to move.
    pub fn legal_moves(&mut self) -> Vec<(usize, usize)> {
        let mut result = Vec::with_capacity(40);
        let mover = self.turn;
        let mut buf = Vec::with_capacity(20);
        for from in 0..90 {
            if let Some(p) = self.squares[from] {
                if p.color != mover {
                    continue;
                }
                buf.clear();
                self.pseudo_moves_from(from, &mut buf);
                for &to in &buf {
                    let mv = self.make_move(from, to);
                    let safe = !self.in_check(mover);
                    self.unmake_move(mv);
                    if safe {
                        result.push((from, to));
                    }
                }
            }
        }
        result
    }

    /// Generate fully legal destinations from a given square.
    pub fn legal_moves_from(&mut self, from: usize) -> Vec<usize> {
        let p = match self.squares[from] {
            Some(p) => p,
            None => return Vec::new(),
        };
        if p.color != self.turn {
            return Vec::new();
        }
        let mover = self.turn;
        let mut buf = Vec::with_capacity(20);
        self.pseudo_moves_from(from, &mut buf);
        let mut out = Vec::with_capacity(buf.len());
        for &to in &buf {
            let mv = self.make_move(from, to);
            let safe = !self.in_check(mover);
            self.unmake_move(mv);
            if safe {
                out.push(to);
            }
        }
        out
    }
}
