# Cờ Tướng

Chinese chess (called *cờ tướng* in Vietnamese) as a Rust engine compiled to
WebAssembly, with a vanilla-JS SVG frontend. The engine enforces the full
ruleset; the browser handles input, rendering, and a built-in AI opponent.
UI is bilingual (English and Vietnamese), switchable in the side panel and
remembered across reloads. An optional Rust WebSocket server linking the
**same** engine crate enables online play between two browsers; the server
is the authoritative rules judge so a tampered client can't cheat.

## What it does

- Full xiangqi rules: palace bounds, horse-leg blocking, elephant eye + river,
  cannon-screen captures, soldier sideways after crossing the river, and the
  flying-general rule.
- Move generation, check / checkmate detection, undo, reset.
- Simple AI: negamax alpha-beta with MVV-LVA move ordering and mate-distance
  scoring. Depth is configurable in the UI (1–5).
- Single-page web UI: click to select, click a highlighted square to move.
  Last-move highlight, capture rings, check / result banners, mode toggle for
  H-vs-H or AI on either side.

## Layout

```
cotuong/
├── Cargo.toml          # workspace: members = [engine, server]
├── build.sh            # cargo build → wasm-bindgen → web/pkg/
├── engine/             # Rust crate (cdylib + rlib, used by both wasm and server)
│   ├── Cargo.toml      # wasm-bindgen is target-cfg'd so native builds stay clean
│   └── src/
│       ├── board.rs    # types, board state, move generation, check tests
│       ├── search.rs   # alpha-beta search + evaluation
│       ├── lib.rs      # exports board + search; gates wasm_api on wasm32
│       └── wasm_api.rs # wasm-bindgen Game wrapper (wasm32 only)
├── server/             # Axum WebSocket server
│   ├── Cargo.toml
│   ├── migrations/sqlite/  # sqlx migrations (incl. users + sessions)
│   └── src/
│       ├── main.rs     # TCP listen, /ws upgrade, /api/*, /auth/*, /healthz, static
│       ├── state.rs    # `AppState` (hub + storage + auth config)
│       ├── proto.rs    # Client/Server JSON message types
│       ├── room.rs     # synchronous GameRoom state machine + unit tests
│       ├── hub.rs      # in-memory room map, per-session WS handler, write-through
│       ├── api.rs      # read-only HTTP endpoints (game list / replay)
│       ├── auth/       # password, sessions, OAuth, email
│       │   ├── mod.rs      # password hashing, token gen, validation
│       │   ├── handlers.rs # signup / login / logout / me / verify / reset
│       │   ├── session.rs  # session cookies + Axum extractors
│       │   ├── oauth.rs    # Google OAuth (`oauth` feature, default on)
│       │   └── email.rs    # SMTP via lettre (`email` feature, default off)
│       └── db/         # storage abstraction
│           ├── mod.rs      # `Storage` trait + DTOs + `connect(url)`
│           ├── sqlite.rs   # SQLite backend
│           └── backup.rs   # periodic SQLite -> R2 snapshot (`r2-backup` feature)
└── web/
    ├── index.html
    ├── style.css
    ├── main.js         # SVG board, click handling, AI driver, WS client
    └── pkg/            # generated wasm + JS glue (build output)
```

## Prerequisites

- Rust toolchain (1.95+ tested) with the `wasm32-unknown-unknown` target:
  ```
  rustup target add wasm32-unknown-unknown
  ```
- `wasm-bindgen-cli` matching the `wasm-bindgen` version pinned in
  `engine/Cargo.lock` (currently 0.2.100):
  ```
  cargo install wasm-bindgen-cli --version 0.2.100
  ```
- Python 3 (or any static-file server) for serving the `web/` folder.
- Node (only if you want to run `engine/smoke.js`).

## Build and run

### Local play (browser only, no backend)

```
./build.sh
python3 -m http.server -d web 8000
```

Then open http://localhost:8000.

### Online play (multiplayer through the Rust server)

```
./build.sh
cargo run --release -p cotuong_server
```

The server listens on `127.0.0.1:8000` by default (override with
`COTUONG_ADDR`) and serves the `web/` folder as static files in addition to
the `/ws` WebSocket endpoint. Open http://localhost:8000 in two browsers,
pick *Online (room)* in both, click **Create room** in one, copy the
6-character code, and **Join** with it in the other.

Server endpoints:
- `GET  /healthz` → `ok`
- `GET  /ws` → WebSocket upgrade (session cookie, if present, attaches the user to the seat)
- `GET  /api/games?limit=N&finished=true` → recent games (most recent first)
- `GET  /api/games/:id` → game metadata + full move list (for replay)
- `GET  /api/games/:id/moves` → just the move list
- `POST /auth/signup` `{username,email,password}` → create account, set session cookie
- `POST /auth/login` `{identifier,password}` → username **or** email + password
- `POST /auth/logout` → clear session
- `GET  /auth/me` → current user (401 if not logged in)
- `GET  /auth/google/login` / `/auth/google/callback` → Google OAuth
- `GET  /auth/verify/:token` → email verification (when enabled)
- `POST /auth/password-reset/request` / `/auth/password-reset/confirm` → password reset
- `GET  /*` → falls back to `web/` (override with `COTUONG_WEB`)

`build.sh` produces:
- `target/wasm32-unknown-unknown/release/cotuong_engine.wasm`
- `web/pkg/cotuong_engine.{js,d.ts}` and `cotuong_engine_bg.wasm`

The frontend uses `import init, { Game } from "./pkg/cotuong_engine.js"` and
runs the engine in the browser regardless of mode. Online mode just routes
moves through the server (which also runs the *same* engine crate, so rules
can't drift between client and server).

## Engine API (wasm-bindgen)

```ts
class Game {
  constructor();
  reset(): void;
  turn(): 0 | 1;                 // 0 = Red, 1 = Black
  ply(): number;
  board_json(): string;          // JSON array, length 90, e.g. "rR" / "bP" / null
  legal_moves_from(from: number): string;  // JSON array of square indices
  play_move(from: number, to: number): boolean;
  ai_move(depth: number): string;          // JSON {from,to} | null
  suggest_move(depth: number): string;
  undo(): boolean;
  status(): "playing" | "red_wins" | "black_wins";
  in_check(): boolean;
  last_move_json(): string;      // JSON {from,to} | null
}
```

Squares are indexed `row * 9 + col`, where `row 0` is Black's back rank (top
of the board) and `row 9` is Red's back rank.

## Tests

### Engine surface (Node, via the wasm bindings)

```
wasm-bindgen --target nodejs \
  --out-dir engine/pkg-node \
  target/wasm32-unknown-unknown/release/cotuong_engine.wasm
node engine/smoke.js
```

Covers initial layout, cannon-screen captures, horse leg blocking, elephant
river constraint, palace-bounded king moves, pre-river soldier movement, and
the `play_move` / `ai_move` / `undo` / `reset` surface.

### Server room logic

```
cargo test -p cotuong_server
```

Covers turn enforcement, illegal-move rejection, and resignation transitions
on the synchronous `GameRoom` state machine, plus SQLite storage round-trip
(create → record moves → finish → list / replay), user uniqueness rules,
session lookup + expiry, one-shot email-token consumption, password hashing,
and input validation (username / email / password).

## Persistence

Every room's lifecycle is recorded so games can be browsed and replayed after
the fact. The schema is two tables — `games` (id, room_code, started_at,
finished_at, result, termination) and `moves` (game_id, ply, from_sq, to_sq,
played_at) — with the move list alone sufficient to reconstruct any game by
re-running the engine from the standard initial position.

Configure with `COTUONG_DB_URL`:

```
COTUONG_DB_URL=sqlite:cotuong.db                  # default
COTUONG_DB_URL=sqlite::memory:                    # ephemeral
```

Migrations are embedded at compile time (`sqlx::migrate!`) from
`server/migrations/sqlite/` and applied automatically on startup. The
storage layer is behind a `Storage` trait so a second backend can be added
later without disturbing call sites.

Storage failures during a game are logged but never abort live play — the
WebSocket protocol stays authoritative and a transient DB hiccup just costs
a missing tail of moves in history. A room that disconnects without finishing
is recorded with `termination = 'abandoned'` and a NULL result.

## Authentication

Optional but enabled by default — accounts are not required to play (anonymous
play through the WS still works), but signed-in players' games are recorded
against their user id and visible in `games.red_user_id` / `black_user_id`.

The shape mirrors chess.com: separate **username** + **email** + **password**,
with Google OAuth as a one-click alternative. Email verification is **off by
default**; turning it on requires a `--features email` build plus SMTP config.

```
COTUONG_PUBLIC_URL=http://127.0.0.1:8000   # used to build OAuth redirect + email links
COTUONG_COOKIE_SECURE=1                    # set when behind HTTPS
COTUONG_EMAIL_VERIFY=1                     # opt in to verification (off by default)
```

Sessions are 30-day HTTP-only cookies (`cotuong_session`). Only the SHA-256
of the cookie value is stored, so a DB compromise doesn't reveal active
sessions. Passwords are hashed with Argon2id.

### Google OAuth

Requires the default `oauth` feature. Register an app at
[Google Cloud Console](https://console.cloud.google.com/apis/credentials),
add `<COTUONG_PUBLIC_URL>/auth/google/callback` as an authorized redirect URI,
then:

```
COTUONG_GOOGLE_CLIENT_ID=...
COTUONG_GOOGLE_CLIENT_SECRET=...
```

First-time OAuth users get an auto-generated username derived from their
display name or email local-part, with a numeric suffix on collision.
The state nonce + PKCE verifier round-trip through a short-lived signed
cookie; if either is missing or mismatched on callback, login is rejected.

### Email verification + password reset (optional)

Both flows are gated behind the `email` cargo feature and an SMTP
configuration. They are off by default and the rest of auth works without
them — you simply lose the verify-on-signup gate and the password-reset
mailer (sign-in still works for anyone whose password they remember).

To turn them on:

```
cargo run --release -p cotuong_server --features email
```

```
COTUONG_EMAIL_VERIFY=1                       # require verification before login
COTUONG_SMTP_HOST=smtp.example.com
COTUONG_SMTP_PORT=587                        # default 587 (STARTTLS)
COTUONG_SMTP_USERNAME=...
COTUONG_SMTP_PASSWORD=...
COTUONG_SMTP_FROM='Cờ Tướng <noreply@example.com>'
```

The server refuses to boot if `COTUONG_EMAIL_VERIFY=1` is set without either
the `email` feature or full SMTP config — fail fast beats sending nothing.

### R2 / S3 backup

Periodic snapshots of the SQLite file to any S3-compatible bucket
(Cloudflare R2 in particular) are available behind the `r2-backup` feature:

```
cargo run --release -p cotuong_server --features r2-backup
```

Each tick runs SQLite's `VACUUM INTO` to a temp file, uploads it as
`<prefix>/<dbname>-<timestamp>.db`, and removes the temp. Configure with:

```
COTUONG_R2_BUCKET=cotuong-backups
COTUONG_R2_ENDPOINT=https://<account-id>.r2.cloudflarestorage.com
COTUONG_R2_ACCESS_KEY_ID=...
COTUONG_R2_SECRET_ACCESS_KEY=...
COTUONG_R2_REGION=auto                   # default
COTUONG_R2_PREFIX=cotuong                # default
COTUONG_R2_INTERVAL_SECS=3600            # default 1h
```


## Notes and limitations

- The engine has no opening book, no quiescence search, no transposition
  table. Depth 3 is fast; depth 5 is noticeably slower in WASM and a strong
  improvement over depth 3.
- Repetition / perpetual-check rules are not enforced; in this version the
  game only ends when a side has no legal move (or by resignation online).
- The UI orients the board with Red at the bottom regardless of who you
  play. (Reasonable next step: flip it for Black online.)
- The online server stays minimal: in-memory live rooms (with write-through
  persistence and optional account attribution — see *Persistence* and
  *Authentication* above), no time control, no rematch flow, no reconnection.
  Rooms are dropped from memory when both players disconnect; the game
  record stays in storage. Anonymous play is allowed alongside authed play —
  unauth'd seats simply leave NULLs in `games.{red,black}_user_id`.

## License

MIT — see [LICENSE](LICENSE).
