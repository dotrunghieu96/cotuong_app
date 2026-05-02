# Cờ Tướng

Vietnamese / Chinese chess as a Rust engine compiled to WebAssembly, with a
vanilla-JS SVG frontend. The engine enforces the full ruleset; the browser
handles input, rendering, and a built-in AI opponent.

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
├── build.sh            # cargo build → wasm-bindgen → web/pkg/
├── engine/             # Rust crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── board.rs    # types, board state, move generation, check tests
│   │   ├── search.rs   # alpha-beta search + evaluation
│   │   └── lib.rs      # wasm-bindgen Game API
│   └── smoke.js        # Node integration test against the wasm surface
└── web/
    ├── index.html
    ├── style.css
    ├── main.js         # SVG board, click handling, AI driver
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

```
./build.sh
python3 -m http.server -d web 8000
```

Then open http://localhost:8000.

`build.sh` produces:
- `engine/target/wasm32-unknown-unknown/release/cotuong_engine.wasm`
- `web/pkg/cotuong_engine.{js,d.ts}` and `cotuong_engine_bg.wasm`

The frontend uses `import init, { Game } from "./pkg/cotuong_engine.js"` and
runs entirely in the browser — no backend.

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

```
cd engine
wasm-bindgen --target nodejs \
  --out-dir pkg-node \
  target/wasm32-unknown-unknown/release/cotuong_engine.wasm
node smoke.js
```

`smoke.js` covers initial layout, cannon-screen captures, horse leg blocking,
elephant river constraint, palace-bounded king moves, pre-river soldier
movement, the wasm `play_move` / `ai_move` / `undo` / `reset` surface, and
end-of-game status reporting.

## Notes and limitations

- The engine has no opening book, no quiescence search, no transposition
  table. Depth 3 is fast; depth 5 is noticeably slower in WASM and a strong
  improvement over depth 3.
- Repetition / perpetual-check rules are not enforced; in this version the
  game only ends when a side has no legal move.
- The UI orients the board with Red at the bottom regardless of who you play.

## License

MIT — see [LICENSE](LICENSE).
