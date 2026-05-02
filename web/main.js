import init, { Game } from "./pkg/cotuong_engine.js";

const PAD_X = 36;
const PAD_Y = 36;
const STEP = 56;
const RADIUS = 24;
const NS = "http://www.w3.org/2000/svg";

const PIECE_TEXT = {
  rK: "帥", rA: "仕", rE: "相", rH: "傌", rR: "俥", rC: "炮", rP: "兵",
  bK: "將", bA: "士", bE: "象", bH: "馬", bR: "車", bC: "砲", bP: "卒",
};

let game = null;
let selectedSquare = null;
let legalDestForSel = [];
let mode = "ai-black"; // "hh" | "ai-black" | "ai-red"
let aiDepth = 3;
let aiThinking = false;

const $board = document.getElementById("board");
const $turnText = document.getElementById("turn-text");
const $turnDot = document.getElementById("turn-dot");
const $checkLine = document.getElementById("check-line");
const $resultLine = document.getElementById("result-line");
const $newGame = document.getElementById("new-game");
const $undo = document.getElementById("undo");
const $aiNow = document.getElementById("ai-now");
const $depth = document.getElementById("depth");
const $depthVal = document.getElementById("depth-val");

function squareCenter(sq) {
  const row = Math.floor(sq / 9);
  const col = sq % 9;
  return { cx: PAD_X + col * STEP, cy: PAD_Y + row * STEP };
}

function svg(tag, attrs = {}, children = []) {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  for (const c of children) el.appendChild(c);
  return el;
}

function buildStaticBoard() {
  // Border
  $board.appendChild(svg("rect", {
    x: PAD_X - 12,
    y: PAD_Y - 12,
    width: STEP * 8 + 24,
    height: STEP * 9 + 24,
    class: "edge-line",
  }));

  // Horizontal grid lines (10)
  for (let r = 0; r < 10; r++) {
    const y = PAD_Y + r * STEP;
    $board.appendChild(svg("line", {
      x1: PAD_X, x2: PAD_X + 8 * STEP, y1: y, y2: y, class: "grid-line",
    }));
  }

  // Vertical lines: edges full, inner files split at the river
  for (let c = 0; c < 9; c++) {
    const x = PAD_X + c * STEP;
    if (c === 0 || c === 8) {
      $board.appendChild(svg("line", {
        x1: x, x2: x, y1: PAD_Y, y2: PAD_Y + 9 * STEP, class: "grid-line",
      }));
    } else {
      $board.appendChild(svg("line", {
        x1: x, x2: x, y1: PAD_Y, y2: PAD_Y + 4 * STEP, class: "grid-line",
      }));
      $board.appendChild(svg("line", {
        x1: x, x2: x, y1: PAD_Y + 5 * STEP, y2: PAD_Y + 9 * STEP, class: "grid-line",
      }));
    }
  }

  // Palace diagonals
  for (const palace of [{ r0: 0, r1: 2 }, { r0: 7, r1: 9 }]) {
    $board.appendChild(svg("line", {
      x1: PAD_X + 3 * STEP, y1: PAD_Y + palace.r0 * STEP,
      x2: PAD_X + 5 * STEP, y2: PAD_Y + palace.r1 * STEP,
      class: "grid-line",
    }));
    $board.appendChild(svg("line", {
      x1: PAD_X + 5 * STEP, y1: PAD_Y + palace.r0 * STEP,
      x2: PAD_X + 3 * STEP, y2: PAD_Y + palace.r1 * STEP,
      class: "grid-line",
    }));
  }

  // River label
  const riverY = PAD_Y + 4 * STEP + STEP / 2;
  const riverChu = svg("text", {
    x: PAD_X + 1.5 * STEP, y: riverY,
    "text-anchor": "middle", "dominant-baseline": "central",
    class: "river-text",
  });
  riverChu.textContent = "楚 河";
  $board.appendChild(riverChu);
  const riverHan = svg("text", {
    x: PAD_X + 6.5 * STEP, y: riverY,
    "text-anchor": "middle", "dominant-baseline": "central",
    class: "river-text",
  });
  riverHan.textContent = "漢 界";
  $board.appendChild(riverHan);

  // Landmark point markers
  const markPoints = [
    [2, 1], [2, 7], [7, 1], [7, 7],
    [3, 0], [3, 2], [3, 4], [3, 6], [3, 8],
    [6, 0], [6, 2], [6, 4], [6, 6], [6, 8],
  ];
  for (const [r, c] of markPoints) drawPointMarker(r, c);
}

function drawPointMarker(r, c) {
  const cx = PAD_X + c * STEP;
  const cy = PAD_Y + r * STEP;
  const off = 5, len = 7;
  const variants = [[-1, -1], [1, -1], [-1, 1], [1, 1]];
  for (const [sx, sy] of variants) {
    if (c + sx < 0 || c + sx > 8) continue;
    if (r + sy < 0 || r + sy > 9) continue;
    const x0 = cx + sx * off;
    const y0 = cy + sy * off;
    $board.appendChild(svg("path", {
      d: `M ${x0 + sx * len} ${y0} L ${x0} ${y0} L ${x0} ${y0 + sy * len}`,
      class: "point-marker",
    }));
  }
}

function rerenderDynamic() {
  $board.querySelectorAll('[data-dyn="1"]').forEach((n) => n.remove());

  const boardArr = JSON.parse(game.board_json());
  const lastMv = JSON.parse(game.last_move_json());
  const checkOn = game.in_check();

  // Last-move highlights
  if (lastMv) {
    for (const sq of [lastMv.from, lastMv.to]) {
      const { cx, cy } = squareCenter(sq);
      $board.appendChild(svg("circle", {
        cx, cy, r: RADIUS + 2,
        class: "last-move-dot",
        "data-dyn": "1",
      }));
    }
  }

  // Pieces
  for (let s = 0; s < 90; s++) {
    const code = boardArr[s];
    if (!code) continue;
    const { cx, cy } = squareCenter(s);
    const g = svg("g", { class: "piece-group", "data-dyn": "1", "data-square": s });
    g.appendChild(svg("circle", {
      cx, cy, r: RADIUS,
      class: "piece-bg" + (selectedSquare === s ? " selected" : ""),
    }));
    const t = svg("text", {
      x: cx, y: cy,
      class: "piece-text " + (code[0] === "r" ? "red" : "black"),
    });
    t.textContent = PIECE_TEXT[code] || "?";
    g.appendChild(t);
    g.addEventListener("click", () => onPieceClick(s));
    $board.appendChild(g);
  }

  // Move suggestions on top
  if (selectedSquare !== null) {
    for (const dest of legalDestForSel) {
      const { cx, cy } = squareCenter(dest);
      const occupied = !!boardArr[dest];
      if (occupied) {
        const ring = svg("circle", {
          cx, cy, r: RADIUS + 4,
          class: "capture-ring",
          "data-dyn": "1",
        });
        ring.addEventListener("click", () => onDestClick(dest));
        $board.appendChild(ring);
      } else {
        const target = svg("circle", {
          cx, cy, r: RADIUS,
          class: "click-target",
          "data-dyn": "1",
        });
        target.addEventListener("click", () => onDestClick(dest));
        $board.appendChild(target);
        $board.appendChild(svg("circle", {
          cx, cy, r: 9,
          class: "move-dot",
          "data-dyn": "1",
        }));
      }
    }
  }

  // Status
  const turn = game.turn();
  $turnText.textContent = turn === 0 ? "Red to move" : "Black to move";
  $turnDot.classList.toggle("black", turn === 1);
  $checkLine.hidden = !checkOn;

  const status = game.status();
  if (status === "playing") {
    $resultLine.hidden = true;
  } else {
    $resultLine.hidden = false;
    $resultLine.textContent =
      status === "red_wins" ? "Red wins by checkmate." : "Black wins by checkmate.";
  }
}

function onPieceClick(s) {
  if (aiThinking) return;
  if (game.status() !== "playing") return;

  const turn = game.turn();
  const humanIsRed = mode !== "ai-red";
  const humanIsBlack = mode !== "ai-black";
  const humanTurn =
    mode === "hh" ||
    (turn === 0 && humanIsRed) ||
    (turn === 1 && humanIsBlack);
  if (!humanTurn) return;

  const boardArr = JSON.parse(game.board_json());
  const code = boardArr[s];

  if (!code) {
    if (selectedSquare !== null && legalDestForSel.includes(s)) {
      onDestClick(s);
      return;
    }
    clearSelection();
    rerenderDynamic();
    return;
  }

  const isRed = code[0] === "r";
  const ownTurn = (isRed && turn === 0) || (!isRed && turn === 1);

  if (ownTurn) {
    selectedSquare = s;
    legalDestForSel = JSON.parse(game.legal_moves_from(s));
  } else if (selectedSquare !== null && legalDestForSel.includes(s)) {
    onDestClick(s);
    return;
  } else {
    clearSelection();
  }
  rerenderDynamic();
}

function onDestClick(s) {
  if (aiThinking) return;
  if (selectedSquare === null) return;
  if (!legalDestForSel.includes(s)) return;
  const ok = game.play_move(selectedSquare, s);
  clearSelection();
  rerenderDynamic();
  if (!ok) return;
  maybeAIMove();
}

function clearSelection() {
  selectedSquare = null;
  legalDestForSel = [];
}

function maybeAIMove() {
  if (game.status() !== "playing") return;
  const turn = game.turn();
  const aiTurn =
    (mode === "ai-black" && turn === 1) || (mode === "ai-red" && turn === 0);
  if (!aiTurn) return;
  runAI();
}

function runAI() {
  if (aiThinking) return;
  if (game.status() !== "playing") return;
  aiThinking = true;
  $turnText.textContent = "AI thinking…";
  // Yield so the "thinking" message paints before search blocks the thread.
  setTimeout(() => {
    try {
      game.ai_move(aiDepth);
    } finally {
      aiThinking = false;
      rerenderDynamic();
    }
  }, 30);
}

$newGame.addEventListener("click", () => {
  game.reset();
  clearSelection();
  rerenderDynamic();
  maybeAIMove();
});

$undo.addEventListener("click", () => {
  if (aiThinking) return;
  const ai = mode === "ai-black" || mode === "ai-red";
  game.undo();
  if (ai) game.undo();
  clearSelection();
  rerenderDynamic();
});

$aiNow.addEventListener("click", () => runAI());

document.querySelectorAll('input[name="mode"]').forEach((el) => {
  el.addEventListener("change", () => {
    if (el.checked) {
      mode = el.value;
      maybeAIMove();
    }
  });
});

$depth.addEventListener("input", () => {
  aiDepth = parseInt($depth.value, 10);
  $depthVal.textContent = String(aiDepth);
});

$board.addEventListener("click", (e) => {
  if (e.target === $board) {
    clearSelection();
    rerenderDynamic();
  }
});

(async function start() {
  await init();
  game = new Game();
  buildStaticBoard();
  rerenderDynamic();
  maybeAIMove();
})();
