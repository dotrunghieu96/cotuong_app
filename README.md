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
│   └── src/
│       ├── main.rs     # TCP listen, /ws upgrade, /healthz, static fallback
│       ├── proto.rs    # Client/Server JSON message types
│       ├── room.rs     # synchronous GameRoom state machine + unit tests
│       └── hub.rs      # in-memory room map, per-session WS handler
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
- `GET  /ws` → WebSocket upgrade
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
on the synchronous `GameRoom` state machine.

## Notes and limitations

- The engine has no opening book, no quiescence search, no transposition
  table. Depth 3 is fast; depth 5 is noticeably slower in WASM and a strong
  improvement over depth 3.
- Repetition / perpetual-check rules are not enforced; in this version the
  game only ends when a side has no legal move (or by resignation online).
- The UI orients the board with Red at the bottom regardless of who you
  play. (Reasonable next step: flip it for Black online.)
- The online server is intentionally minimal: in-memory rooms, no
  authentication, no persistence, no time control, no rematch flow, no
  reconnection. Rooms are dropped when both players disconnect.

## License

MIT — see [LICENSE](LICENSE).
