export const PAD_X = 36;
export const PAD_Y = 36;
export const STEP = 56;
export const RADIUS = 24;
export const NS = "http://www.w3.org/2000/svg";
export const ANIM_MS = 220;

export const PIECE_TEXT = {
  rK: "帥", rA: "仕", rE: "相", rH: "傌", rR: "俥", rC: "炮", rP: "兵",
  bK: "將", bA: "士", bE: "象", bH: "馬", bR: "車", bC: "砲", bP: "卒",
};

export function squareCenter(sq) {
  const row = Math.floor(sq / 9);
  const col = sq % 9;
  return { cx: PAD_X + col * STEP, cy: PAD_Y + row * STEP };
}

export function svg(tag, attrs = {}, children = []) {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  for (const c of children) el.appendChild(c);
  return el;
}

export function buildStaticBoard($board) {
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
  for (const [r, c] of markPoints) drawPointMarker($board, r, c);
}

function drawPointMarker($board, r, c) {
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

// Create a board controller bound to an <svg> element. Draws the static board
// once, then renders pieces / move suggestions / last-move highlights from a
// snapshot via render(). Animation of the most recent move is internal: the
// controller remembers which last-move it has already animated, so re-renders
// triggered by selection or language changes don't replay the slide.
//
// snapshot shape:
//   {
//     board:           Array<string|null>,   // 90 cells, e.g. "rK", "bP", null
//     lastMove:        {from:number, to:number} | null,
//     selectedSquare:  number | null,
//     legalDests:      number[],              // for the selected piece
//     flipped:         boolean,               // render rotated 180° (player on
//                                             // black's side); click handlers
//                                             // still receive logical squares
//   }
//
// options:
//   { animateMove?: boolean }   // default true; pass false to suppress slide
//                                // (e.g. on undo or initial state load)
export function createBoard(svgEl, opts = {}) {
  const onPieceClick = opts.onPieceClick || (() => {});
  const onDestClick  = opts.onDestClick  || (() => {});
  const onEmptyClick = opts.onEmptyClick || (() => {});

  let lastMoveAnimKey = null;

  buildStaticBoard(svgEl);

  svgEl.addEventListener("click", (e) => {
    if (e.target === svgEl) onEmptyClick();
  });

  function lastMoveKey(mv) {
    return mv ? `${mv.from}-${mv.to}` : null;
  }

  function render(snapshot, options = {}) {
    const animateMove = options.animateMove !== false;
    const {
      board,
      lastMove = null,
      selectedSquare = null,
      legalDests = [],
      flipped = false,
    } = snapshot;

    // Logical square → display center. The static board is symmetric, so only
    // dynamic placements need flipping; click handlers still emit logical
    // indices so all move logic upstream stays unchanged.
    const displayCenter = (s) => squareCenter(flipped ? 89 - s : s);

    svgEl.querySelectorAll('[data-dyn="1"]').forEach((n) => n.remove());

    if (lastMove) {
      for (const sq of [lastMove.from, lastMove.to]) {
        const { cx, cy } = displayCenter(sq);
        svgEl.appendChild(svg("circle", {
          cx, cy, r: RADIUS + 2,
          class: "last-move-dot",
          "data-dyn": "1",
        }));
      }
    }

    const animatedPieces = [];
    const shouldAnimate = animateMove && lastMove
      && lastMoveAnimKey !== lastMoveKey(lastMove);
    for (let s = 0; s < 90; s++) {
      const code = board[s];
      if (!code) continue;
      const target = displayCenter(s);
      const isMover = shouldAnimate && s === lastMove.to;
      const initial = isMover ? displayCenter(lastMove.from) : target;

      const g = svg("g", { class: "piece-group", "data-dyn": "1", "data-square": s });
      g.style.transform = `translate(${initial.cx}px, ${initial.cy}px)`;
      g.appendChild(svg("circle", {
        cx: 0, cy: 0, r: RADIUS,
        class: "piece-bg" + (selectedSquare === s ? " selected" : ""),
      }));
      const txt = svg("text", {
        x: 0, y: 0,
        class: "piece-text " + (code[0] === "r" ? "red" : "black"),
      });
      txt.textContent = PIECE_TEXT[code] || "?";
      g.appendChild(txt);
      g.addEventListener("click", () => onPieceClick(s));
      svgEl.appendChild(g);

      if (isMover) animatedPieces.push({ g, target });
    }

    if (animatedPieces.length > 0) {
      // Force a layout flush so the initial transform is registered, then
      // re-target on the next frame to trigger the CSS transition.
      void svgEl.getBoundingClientRect();
      requestAnimationFrame(() => {
        for (const { g, target } of animatedPieces) {
          g.style.transform = `translate(${target.cx}px, ${target.cy}px)`;
        }
      });
    }
    lastMoveAnimKey = lastMoveKey(lastMove);

    if (selectedSquare !== null) {
      for (const dest of legalDests) {
        const { cx, cy } = displayCenter(dest);
        const occupied = !!board[dest];
        if (occupied) {
          const ring = svg("circle", {
            cx, cy, r: RADIUS + 4,
            class: "capture-ring",
            "data-dyn": "1",
          });
          ring.addEventListener("click", () => onDestClick(dest));
          svgEl.appendChild(ring);
        } else {
          const target = svg("circle", {
            cx, cy, r: RADIUS,
            class: "click-target",
            "data-dyn": "1",
          });
          target.addEventListener("click", () => onDestClick(dest));
          svgEl.appendChild(target);
          svgEl.appendChild(svg("circle", {
            cx, cy, r: 9,
            class: "move-dot",
            "data-dyn": "1",
          }));
        }
      }
    }
  }

  return { render };
}
