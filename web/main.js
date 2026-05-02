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

const STRINGS = {
  en: {
    title: "Chinese Chess",
    red_to_move: "Red to move",
    black_to_move: "Black to move",
    ai_thinking: "AI thinking…",
    check: "Check!",
    red_wins: "Red wins by checkmate.",
    black_wins: "Black wins by checkmate.",
    new_game: "New game",
    undo: "Undo",
    opponent: "Opponent",
    mode_hh: "Human vs Human",
    mode_ai_black: "AI plays Black",
    mode_ai_red: "AI plays Red",
    ai_depth: "AI depth",
    ai_now: "AI move now",
    mode_online: "Online (room)",
    online_legend: "Online",
    online_create: "Create room",
    online_join: "Join",
    online_room: "Room",
    online_you_play: "You play as",
    online_opponent: "Opponent",
    online_resign: "Resign",
    online_leave: "Leave",
    online_color_red: "Red",
    online_color_black: "Black",
    online_opp_waiting: "waiting…",
    online_opp_present: "connected",
    online_opp_left: "left",
    online_connecting: "Connecting…",
    online_connected: "Connected.",
    online_disconnected: "Disconnected.",
    online_room_full: "Room is full.",
    online_room_not_found: "Room not found.",
    online_resigned: "You resigned.",
    online_opp_resigned: "Opponent resigned.",
    online_opp_left_msg: "Opponent left the room.",
    help:
      "Click a piece, then click a highlighted square to move. Red moves first. " +
      "The flying-general rule, blocked horse legs, cannon-screen captures and " +
      "post-river soldiers are all enforced.",
  },
  vi: {
    title: "Cờ Tướng",
    red_to_move: "Đỏ đi",
    black_to_move: "Đen đi",
    ai_thinking: "Máy đang nghĩ…",
    check: "Chiếu!",
    red_wins: "Đỏ thắng (chiếu hết).",
    black_wins: "Đen thắng (chiếu hết).",
    new_game: "Ván mới",
    undo: "Hoàn lại",
    opponent: "Đối thủ",
    mode_hh: "Người đấu Người",
    mode_ai_black: "Máy cầm Đen",
    mode_ai_red: "Máy cầm Đỏ",
    ai_depth: "Độ sâu của máy",
    ai_now: "Máy đi ngay",
    mode_online: "Trực tuyến (phòng)",
    online_legend: "Trực tuyến",
    online_create: "Tạo phòng",
    online_join: "Vào phòng",
    online_room: "Phòng",
    online_you_play: "Bạn cầm",
    online_opponent: "Đối thủ",
    online_resign: "Xin thua",
    online_leave: "Rời phòng",
    online_color_red: "Đỏ",
    online_color_black: "Đen",
    online_opp_waiting: "đang chờ…",
    online_opp_present: "đã sẵn sàng",
    online_opp_left: "đã rời",
    online_connecting: "Đang kết nối…",
    online_connected: "Đã kết nối.",
    online_disconnected: "Mất kết nối.",
    online_room_full: "Phòng đã đầy.",
    online_room_not_found: "Không tìm thấy phòng.",
    online_resigned: "Bạn đã xin thua.",
    online_opp_resigned: "Đối thủ đã xin thua.",
    online_opp_left_msg: "Đối thủ đã rời phòng.",
    help:
      "Chọn một quân, rồi bấm vào ô được tô sáng để di chuyển. Đỏ đi trước. " +
      "Luật tướng đối mặt, cản chân ngựa, pháo cần ngòi và lính qua sông đều " +
      "được áp dụng.",
  },
};

const LANG_STORAGE_KEY = "cotuong.lang";

function detectInitialLang() {
  try {
    const saved = localStorage.getItem(LANG_STORAGE_KEY);
    if (saved && STRINGS[saved]) return saved;
  } catch (_) { /* ignore */ }
  const nav = (navigator.language || "en").toLowerCase();
  return nav.startsWith("vi") ? "vi" : "en";
}

let lang = detectInitialLang();
function t(key) {
  return (STRINGS[lang] && STRINGS[lang][key]) || STRINGS.en[key] || key;
}

const ANIM_MS = 220;

let game = null;
let selectedSquare = null;
let legalDestForSel = [];
let mode = "ai-black"; // "hh" | "ai-black" | "ai-red" | "online"
let aiDepth = 3;
let aiThinking = false;
// Identifies the last move we've already animated so re-renders triggered
// by selection / language changes don't replay the slide.
let lastMoveAnimKey = null;
function lastMoveKey(mv) {
  return mv ? `${mv.from}-${mv.to}` : null;
}

// Online state
let onlineWs = null;
let onlineColor = null; // "red" | "black"
let onlineRoom = null;
let onlineOpponentPresent = false;
let onlineGameOver = false;

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
const $depthRow = document.getElementById("depth-row");
const $aiFieldset = document.querySelector("fieldset.ai");
const $onlinePanel = document.getElementById("online-panel");
const $onlineDisconnected = document.getElementById("online-disconnected");
const $onlineConnected = document.getElementById("online-connected");
const $onlineCreate = document.getElementById("online-create");
const $onlineJoin = document.getElementById("online-join");
const $onlineCode = document.getElementById("online-code");
const $onlineRoomCode = document.getElementById("online-room-code");
const $onlineColor = document.getElementById("online-color");
const $onlineOpponentState = document.getElementById("online-opponent-state");
const $onlineResign = document.getElementById("online-resign");
const $onlineLeave = document.getElementById("online-leave");
const $onlineMsg = document.getElementById("online-msg");

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

  // Pieces. Each <g> is positioned via a CSS transform translate so we can
  // transition it. The piece that just moved starts at its `from` square and
  // is re-targeted to its `to` square on the next animation frame, producing
  // a slide animation. All other pieces are placed at their target directly.
  const animatedPieces = [];
  for (let s = 0; s < 90; s++) {
    const code = boardArr[s];
    if (!code) continue;
    const target = squareCenter(s);
    const isMover = lastMv && s === lastMv.to && lastMoveAnimKey !== lastMoveKey(lastMv);
    const initial = isMover ? squareCenter(lastMv.from) : target;

    const g = svg("g", { class: "piece-group", "data-dyn": "1", "data-square": s });
    g.style.transform = `translate(${initial.cx}px, ${initial.cy}px)`;
    g.appendChild(svg("circle", {
      cx: 0, cy: 0, r: RADIUS,
      class: "piece-bg" + (selectedSquare === s ? " selected" : ""),
    }));
    const t = svg("text", {
      x: 0, y: 0,
      class: "piece-text " + (code[0] === "r" ? "red" : "black"),
    });
    t.textContent = PIECE_TEXT[code] || "?";
    g.appendChild(t);
    g.addEventListener("click", () => onPieceClick(s));
    $board.appendChild(g);

    if (isMover) animatedPieces.push({ g, target });
  }

  if (animatedPieces.length > 0) {
    // Force a layout flush so the initial transform is registered, then
    // re-target on the next frame to trigger the CSS transition.
    void $board.getBoundingClientRect();
    requestAnimationFrame(() => {
      for (const { g, target } of animatedPieces) {
        g.style.transform = `translate(${target.cx}px, ${target.cy}px)`;
      }
    });
    lastMoveAnimKey = lastMoveKey(lastMv);
  } else if (!lastMv) {
    lastMoveAnimKey = null;
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
  $turnText.textContent = turn === 0 ? t("red_to_move") : t("black_to_move");
  $turnDot.classList.toggle("black", turn === 1);
  $checkLine.hidden = !checkOn;

  const status = game.status();
  if (status === "playing") {
    $resultLine.hidden = true;
  } else {
    $resultLine.hidden = false;
    $resultLine.textContent =
      status === "red_wins" ? t("red_wins") : t("black_wins");
  }
}

function humanCanMove() {
  const turn = game.turn();
  if (mode === "online") {
    if (!onlineWs || onlineGameOver) return false;
    if (!onlineOpponentPresent) return false;
    return (
      (turn === 0 && onlineColor === "red") ||
      (turn === 1 && onlineColor === "black")
    );
  }
  if (mode === "hh") return true;
  if (mode === "ai-red") return turn === 1; // human is Black
  if (mode === "ai-black") return turn === 0; // human is Red
  return false;
}

function onPieceClick(s) {
  if (aiThinking) return;
  if (game.status() !== "playing") return;
  if (!humanCanMove()) return;

  const turn = game.turn();

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

  if (mode === "online") {
    // Send to server; the server broadcasts back and we apply on receipt.
    if (!onlineWs || onlineWs.readyState !== WebSocket.OPEN) return;
    onlineWs.send(JSON.stringify({ t: "move", from: selectedSquare, to: s }));
    clearSelection();
    rerenderDynamic();
    return;
  }

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
  if (mode === "online") return;
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
  $turnText.textContent = t("ai_thinking");
  // Wait for the prior move's animation to settle, then yield so the
  // "thinking" message paints before search blocks the thread.
  setTimeout(() => {
    try {
      game.ai_move(aiDepth);
    } finally {
      aiThinking = false;
      rerenderDynamic();
    }
  }, ANIM_MS + 20);
}

$newGame.addEventListener("click", () => {
  if (mode === "online") return; // online resets only on rematch / new room
  game.reset();
  clearSelection();
  lastMoveAnimKey = null;
  rerenderDynamic();
  maybeAIMove();
});

$undo.addEventListener("click", () => {
  if (aiThinking) return;
  if (mode === "online") return; // server has no undo
  const ai = mode === "ai-black" || mode === "ai-red";
  game.undo();
  if (ai) game.undo();
  clearSelection();
  // Don't replay the prior move's slide animation when stepping backward.
  lastMoveAnimKey = lastMoveKey(JSON.parse(game.last_move_json()));
  rerenderDynamic();
});

$aiNow.addEventListener("click", () => {
  if (mode === "online") return;
  runAI();
});

document.querySelectorAll('input[name="mode"]').forEach((el) => {
  el.addEventListener("change", () => {
    if (!el.checked) return;
    const prevMode = mode;
    mode = el.value;
    if (prevMode === "online" && mode !== "online") {
      // Switching out of online: drop the WS.
      disconnectOnline();
    }
    applyModeUI();
    if (mode !== "online") {
      // Reset the local game state for a clean local game.
      game.reset();
      clearSelection();
      lastMoveAnimKey = null;
      rerenderDynamic();
      maybeAIMove();
    } else {
      // Entering online mode: clear any stale local state, wait for user
      // to click Create / Join.
      clearSelection();
      rerenderDynamic();
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

// ----- Online play -----------------------------------------------------------

function applyModeUI() {
  const online = mode === "online";
  $onlinePanel.hidden = !online;
  $aiFieldset.classList.toggle("locked", online);
  // Hide AI-only controls when online.
  $depthRow.style.display = online ? "none" : "";
  $aiNow.style.display = online ? "none" : "";
  $undo.disabled = online;
  $newGame.disabled = online;
  if (!online) {
    setOnlineMsg("");
  }
}

function setOnlineMsg(msg) {
  $onlineMsg.textContent = msg || "";
  $onlineMsg.classList.toggle("empty", !msg);
}

function setOnlineConnectedUI(connected) {
  $onlineDisconnected.hidden = connected;
  $onlineConnected.hidden = !connected;
}

function updateOnlineStatusUI() {
  if (!onlineWs) {
    setOnlineConnectedUI(false);
    return;
  }
  setOnlineConnectedUI(true);
  $onlineRoomCode.textContent = onlineRoom || "—";
  $onlineColor.textContent =
    onlineColor === "red" ? t("online_color_red") :
    onlineColor === "black" ? t("online_color_black") : "—";
  if (onlineGameOver) {
    // Leave the message field as set by the game-over handler.
  } else if (!onlineOpponentPresent) {
    $onlineOpponentState.textContent = t("online_opp_waiting");
  } else {
    $onlineOpponentState.textContent = t("online_opp_present");
  }
}

function wsUrl() {
  const u = new URL("/ws", window.location.href);
  u.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return u.toString();
}

function connectOnline(onOpen) {
  if (onlineWs) {
    try { onlineWs.close(); } catch (_) {}
    onlineWs = null;
  }
  onlineColor = null;
  onlineRoom = null;
  onlineOpponentPresent = false;
  onlineGameOver = false;
  setOnlineMsg(t("online_connecting"));

  let ws;
  try {
    ws = new WebSocket(wsUrl());
  } catch (e) {
    setOnlineMsg(String(e));
    return;
  }
  onlineWs = ws;

  ws.addEventListener("open", () => {
    setOnlineMsg("");
    onOpen(ws);
  });
  ws.addEventListener("message", (ev) => {
    let msg;
    try { msg = JSON.parse(ev.data); }
    catch (_) { return; }
    handleServerMessage(msg);
  });
  ws.addEventListener("close", () => {
    if (onlineWs === ws) {
      onlineWs = null;
      onlineColor = null;
      onlineRoom = null;
      onlineOpponentPresent = false;
      setOnlineConnectedUI(false);
      setOnlineMsg(t("online_disconnected"));
    }
  });
  ws.addEventListener("error", () => {
    setOnlineMsg(t("online_disconnected"));
  });
}

function handleServerMessage(msg) {
  switch (msg.t) {
    case "joined":
      onlineRoom = msg.room;
      onlineColor = msg.color;
      onlineOpponentPresent = !!msg.opponent_present;
      onlineGameOver = msg.status !== "playing";
      // Server-authoritative: replay from a fresh game then catch up. For
      // the scaffolding, fresh rooms always start at the initial position
      // so just reset.
      game.reset();
      clearSelection();
      lastMoveAnimKey = null;
      rerenderDynamic();
      updateOnlineStatusUI();
      break;
    case "opponent_joined":
      onlineOpponentPresent = true;
      updateOnlineStatusUI();
      break;
    case "opponent_left":
      onlineOpponentPresent = false;
      setOnlineMsg(t("online_opp_left_msg"));
      updateOnlineStatusUI();
      break;
    case "move": {
      const ok = game.play_move(msg.from, msg.to);
      if (!ok) {
        console.error("desync: server move not legal locally", msg);
      }
      clearSelection();
      rerenderDynamic();
      if (msg.status !== "playing") onlineGameOver = true;
      break;
    }
    case "game_over":
      onlineGameOver = true;
      if (msg.reason === "resignation") {
        const iWon = msg.winner === onlineColor;
        setOnlineMsg(iWon ? t("online_opp_resigned") : t("online_resigned"));
      }
      updateOnlineStatusUI();
      break;
    case "error":
      setOnlineMsg(msg.reason || "error");
      if (msg.reason === "room not found") setOnlineMsg(t("online_room_not_found"));
      if (msg.reason === "room full") setOnlineMsg(t("online_room_full"));
      break;
    case "pong":
      break;
    default:
      // ignore
  }
}

function disconnectOnline() {
  if (onlineWs) {
    try { onlineWs.close(); } catch (_) {}
    onlineWs = null;
  }
  onlineColor = null;
  onlineRoom = null;
  onlineOpponentPresent = false;
  onlineGameOver = false;
  setOnlineConnectedUI(false);
  setOnlineMsg("");
}

$onlineCreate.addEventListener("click", () => {
  connectOnline((ws) => ws.send(JSON.stringify({ t: "create" })));
});

$onlineJoin.addEventListener("click", () => {
  const code = ($onlineCode.value || "").trim().toUpperCase();
  if (!code) return;
  connectOnline((ws) => ws.send(JSON.stringify({ t: "join", room: code })));
});

$onlineCode.addEventListener("keydown", (e) => {
  if (e.key === "Enter") $onlineJoin.click();
});

$onlineResign.addEventListener("click", () => {
  if (!onlineWs || onlineGameOver) return;
  onlineWs.send(JSON.stringify({ t: "resign" }));
});

$onlineLeave.addEventListener("click", () => {
  disconnectOnline();
});

function applyLang() {
  document.documentElement.lang = lang;
  document.title = t("title");
  for (const el of document.querySelectorAll("[data-i18n]")) {
    el.textContent = t(el.dataset.i18n);
  }
  for (const btn of document.querySelectorAll("#lang-switch button")) {
    btn.classList.toggle("active", btn.dataset.lang === lang);
  }
  if (game) rerenderDynamic();
  updateOnlineStatusUI();
}

document.querySelectorAll("#lang-switch button").forEach((btn) => {
  btn.addEventListener("click", () => {
    lang = btn.dataset.lang;
    try {
      localStorage.setItem(LANG_STORAGE_KEY, lang);
    } catch (_) { /* ignore */ }
    applyLang();
  });
});

(async function start() {
  applyLang();
  applyModeUI();
  await init();
  game = new Game();
  buildStaticBoard();
  rerenderDynamic();
  maybeAIMove();
})();
