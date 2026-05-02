CREATE TABLE games (
    id           TEXT PRIMARY KEY NOT NULL,
    room_code    TEXT NOT NULL,
    started_at   TEXT NOT NULL,
    finished_at  TEXT,
    result       TEXT CHECK (result IN ('red_wins', 'black_wins', 'draw')),
    termination  TEXT CHECK (termination IN ('checkmate', 'resignation', 'abandoned'))
);

CREATE INDEX games_started_at_idx ON games (started_at DESC);
CREATE INDEX games_room_code_idx  ON games (room_code);

CREATE TABLE moves (
    game_id    TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    ply        INTEGER NOT NULL,
    from_sq    INTEGER NOT NULL CHECK (from_sq BETWEEN 0 AND 89),
    to_sq      INTEGER NOT NULL CHECK (to_sq   BETWEEN 0 AND 89),
    played_at  TEXT NOT NULL,
    PRIMARY KEY (game_id, ply)
);
