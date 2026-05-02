use crate::board::{Board, Color, PieceKind};

const INF: i32 = 1_000_000;
const MATE: i32 = 100_000;

fn piece_value(kind: PieceKind, color: Color, row: i32) -> i32 {
    match kind {
        PieceKind::King => 0,
        PieceKind::Rook => 900,
        PieceKind::Cannon => 450,
        PieceKind::Horse => 400,
        PieceKind::Elephant => 200,
        PieceKind::Advisor => 200,
        PieceKind::Pawn => {
            let crossed = match color {
                Color::Red => row <= 4,
                Color::Black => row >= 5,
            };
            if crossed {
                200
            } else {
                100
            }
        }
    }
}

fn capture_score(b: &Board, from: usize, to: usize) -> i32 {
    // MVV-LVA-ish: prioritize taking high-value pieces with low-value pieces.
    let cap_val = b.squares[to].map_or(0, |p| piece_value(p.kind, p.color, (to as i32) / 9));
    if cap_val == 0 {
        return 0;
    }
    let mover_val = b.squares[from].map_or(0, |p| piece_value(p.kind, p.color, (from as i32) / 9));
    cap_val * 16 - mover_val
}

pub fn evaluate(b: &Board) -> i32 {
    let mut score = 0;
    for s in 0..90 {
        if let Some(p) = b.squares[s] {
            let r = (s as i32) / 9;
            let v = piece_value(p.kind, p.color, r);
            match p.color {
                Color::Red => score += v,
                Color::Black => score -= v,
            }
        }
    }
    match b.turn {
        Color::Red => score,
        Color::Black => -score,
    }
}

fn order_moves(b: &Board, moves: &mut Vec<(usize, usize)>) {
    moves.sort_by_cached_key(|&(f, t)| -capture_score(b, f, t));
}

fn alphabeta(b: &mut Board, depth: u32, mut alpha: i32, beta: i32, ply: i32) -> i32 {
    if depth == 0 {
        return evaluate(b);
    }
    let mut moves = b.legal_moves();
    if moves.is_empty() {
        // No legal moves: side to move loses. Use ply to prefer faster mates / slower losses.
        return -(MATE - ply);
    }
    order_moves(b, &mut moves);

    let mut best = -INF;
    for (from, to) in moves {
        let mv = b.make_move(from, to);
        let score = -alphabeta(b, depth - 1, -beta, -alpha, ply + 1);
        b.unmake_move(mv);
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

pub fn search_best(b: &mut Board, depth: u32) -> Option<(usize, usize)> {
    let mut moves = b.legal_moves();
    if moves.is_empty() {
        return None;
    }
    order_moves(b, &mut moves);

    let mut best_move = moves[0];
    let mut best_score = -INF;
    let mut alpha = -INF;
    let beta = INF;

    for (from, to) in moves {
        let mv = b.make_move(from, to);
        let score = -alphabeta(b, depth.saturating_sub(1), -beta, -alpha, 1);
        b.unmake_move(mv);
        if score > best_score {
            best_score = score;
            best_move = (from, to);
        }
        if best_score > alpha {
            alpha = best_score;
        }
    }
    Some(best_move)
}
