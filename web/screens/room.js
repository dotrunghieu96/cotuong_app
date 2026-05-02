import { Game } from "../pkg/cotuong_engine.js";
import { t, applyLangToDom, onLangChange } from "../i18n.js";
import { createBoard } from "../board.js";
import { getGuestName, suggestGuestName, setGuestName } from "../state.js";
import * as router from "../router.js";

const BOARD_VIEWBOX = "0 0 520 576";

function wsUrl() {
  const u = new URL("/ws", window.location.href);
  u.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return u.toString();
}

export default {
  async mount(root, { params }) {
    const roomCode = (params.code || "").toUpperCase();

    // ── Identity gate ─────────────────────────────────────────────────────────
    // If the player has no identity yet, ask for a guest name first.
    // /auth/me is cheap (cached cookie), so try to read it opportunistically.
    // For now, fall back to guest-name modal if not logged in.
    const identity = await resolveIdentity(root, roomCode);
    if (!identity) return {}; // user cancelled / navigated away

    // ── Markup ────────────────────────────────────────────────────────────────
    root.innerHTML = `
      <div class="layout">
        <div class="board-wrap">
          <svg id="board" viewBox="${BOARD_VIEWBOX}" xmlns="http://www.w3.org/2000/svg"></svg>
        </div>

        <aside class="side">
          <div class="status" id="status">
            <div class="turn-line">
              <span class="dot" id="turn-dot"></span>
              <span id="turn-text"></span>
            </div>
            <div class="check-line" id="check-line" hidden data-i18n="check"></div>
            <div class="result-line" id="result-line" hidden></div>
          </div>

          <div class="room-info">
            <div class="room-info-row">
              <span class="label-text" data-i18n="online_room"></span>
              <span class="flex gap-2">
                <code id="room-code-display">${roomCode}</code>
                <button id="copy-link" class="copy-btn" title="Copy share link" aria-label="Copy">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                </button>
              </span>
            </div>
            <div class="room-info-row">
              <span class="label-text" data-i18n="online_you_play"></span>
              <span class="flex gap-2">
                <span id="my-color" class="badge badge-primary">—</span>
                <span id="my-name" class="text-sm"></span>
              </span>
            </div>
            <div class="room-info-row">
              <span class="label-text" data-i18n="online_opponent"></span>
              <span class="flex gap-2">
                <span id="opp-name" class="text-sm"></span>
                <span id="opponent-state" class="badge badge-muted">—</span>
              </span>
            </div>
          </div>

          <div class="panel scoreboard-panel" id="scoreboard-panel" hidden>
            <div class="panel-title" data-i18n="scoreboard_title">Scoreboard</div>
            <div class="scoreboard" id="scoreboard"></div>
          </div>

          <div class="controls">
            <button class="btn btn-destructive" id="resign" data-i18n="online_resign"></button>
            <button class="btn btn-outline" id="leave" data-i18n="online_leave"></button>
          </div>

          <button class="btn btn-primary btn-block" id="rematch" data-i18n="rematch_btn" hidden>Rematch</button>
          <div class="rematch-hint text-xs text-muted" id="rematch-hint" hidden></div>

          <div class="panel move-panel">
            <div class="panel-title" data-i18n="move_list_title">Moves</div>
            <div class="move-list" id="move-list"></div>
          </div>

          <div class="online-msg" id="online-msg"></div>
        </aside>
      </div>
    `;

    applyLangToDom(root);

    // ── State ─────────────────────────────────────────────────────────────────
    const game = new Game();
    let ws = null;
    let myColor = null;       // "red" | "black"
    let opponentPresent = false;
    let gameOver = false;
    let selectedSquare = null;
    let legalDestForSel = [];
    let seats = { red: null, black: null }; // { name, kind } per side
    let scoreboard = [];      // [{ name, kind, wins }, ...] in seat order
    let myRematchReady = false;
    let oppRematchReady = false;
    const moveHistory = [];   // [{from, to}] — enables replay recovery on desync

    // ── DOM refs ──────────────────────────────────────────────────────────────
    const $svg           = root.querySelector("#board");
    const $turnText      = root.querySelector("#turn-text");
    const $turnDot       = root.querySelector("#turn-dot");
    const $checkLine     = root.querySelector("#check-line");
    const $resultLine    = root.querySelector("#result-line");
    const $myColor       = root.querySelector("#my-color");
    const $myName        = root.querySelector("#my-name");
    const $oppName       = root.querySelector("#opp-name");
    const $opponentState = root.querySelector("#opponent-state");
    const $resign        = root.querySelector("#resign");
    const $leave         = root.querySelector("#leave");
    const $msg           = root.querySelector("#online-msg");
    const $copyLink      = root.querySelector("#copy-link");
    const $moveList      = root.querySelector("#move-list");
    const $scoreboard    = root.querySelector("#scoreboard");
    const $scoreboardPanel = root.querySelector("#scoreboard-panel");
    const $rematch       = root.querySelector("#rematch");
    const $rematchHint   = root.querySelector("#rematch-hint");

    // Square index 0..89 → UCI "h2" notation. Files a-i (col 0..8), ranks 0-9
    // from Red's bottom (rank = 9 - row).
    const squareToUci = (sq) =>
      String.fromCharCode("a".charCodeAt(0) + (sq % 9)) + (9 - Math.floor(sq / 9));

    function renderScoreboard() {
      // Only meaningful once both seats are filled (head-to-head tally).
      if (!scoreboard || scoreboard.length < 2) {
        $scoreboardPanel.hidden = true;
        $scoreboard.innerHTML = "";
        return;
      }
      $scoreboardPanel.hidden = false;
      $scoreboard.innerHTML = scoreboard.map((s) => `
        <div class="score-row">
          <span class="score-name">${esc(s.name)}</span>
          <span class="score-wins">${s.wins}</span>
        </div>
      `).join("");
    }

    function renderRematchUi() {
      // The rematch button only makes sense when the game has ended and the
      // opponent is still around to play another. Hide it otherwise.
      const canRematch = gameOver && opponentPresent;
      $rematch.hidden = !canRematch;
      $rematchHint.hidden = !canRematch || (!myRematchReady && !oppRematchReady);
      if (!canRematch) return;

      if (myRematchReady) {
        $rematch.dataset.i18n = "rematch_cancel";
        $rematch.textContent  = t("rematch_cancel");
        $rematch.classList.remove("btn-primary");
        $rematch.classList.add("btn-outline");
        $rematchHint.textContent = t("rematch_pending_self");
      } else {
        $rematch.dataset.i18n = "rematch_btn";
        $rematch.textContent  = t("rematch_btn");
        $rematch.classList.remove("btn-outline");
        $rematch.classList.add("btn-primary");
        $rematchHint.textContent = oppRematchReady ? t("rematch_pending_opp") : "";
      }
    }

    function esc(s) {
      return String(s)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
    }

    function renderMoveList() {
      // Two columns per row (Red move + Black move). Highlight the latest.
      let html = "";
      for (let i = 0; i < moveHistory.length; i += 2) {
        const turnNo = (i / 2) + 1;
        const red   = moveHistory[i];
        const black = moveHistory[i + 1];
        const isLatestRed   = (i === moveHistory.length - 1);
        const isLatestBlack = (i + 1 === moveHistory.length - 1);
        html += `<div class="move-row">`
          + `<span class="ply-num">${turnNo}.</span>`
          + `<span class="move-cell red${isLatestRed ? " current" : ""}">${red ? squareToUci(red.from) + "→" + squareToUci(red.to) : ""}</span>`
          + `<span class="move-cell black${isLatestBlack ? " current" : ""}">${black ? squareToUci(black.from) + "→" + squareToUci(black.to) : ""}</span>`
          + `</div>`;
      }
      $moveList.innerHTML = html;
      $moveList.scrollTop = $moveList.scrollHeight;
    }

    // ── Helpers ───────────────────────────────────────────────────────────────
    function setMsg(text) {
      $msg.textContent = text || "";
      $msg.classList.toggle("empty", !text);
    }

    function humanCanMove() {
      if (!ws || ws.readyState !== WebSocket.OPEN) return false;
      if (gameOver) return false;
      if (!opponentPresent) return false;
      const turn = game.turn();
      return (turn === 0 && myColor === "red") ||
             (turn === 1 && myColor === "black");
    }

    function rerender() {
      board.render({
        board:          JSON.parse(game.board_json()),
        lastMove:       JSON.parse(game.last_move_json()),
        selectedSquare,
        legalDests:     legalDestForSel,
        flipped:        myColor === "black",
      });
      updateStatus();
    }

    function updateStatus() {
      const turn = game.turn();
      $turnText.textContent = turn === 0 ? t("red_to_move") : t("black_to_move");
      $turnDot.classList.toggle("black", turn === 1);
      $checkLine.hidden = !game.in_check();
      const status = game.status();
      if (status === "playing") {
        $resultLine.hidden = true;
      } else {
        $resultLine.hidden = false;
        $resultLine.textContent = status === "red_wins" ? t("red_wins") : t("black_wins");
      }
    }

    function updateRoomUI() {
      $myColor.textContent = myColor === "red"
        ? t("online_color_red")
        : myColor === "black"
        ? t("online_color_black")
        : "—";

      const oppColor = myColor === "red" ? "black" : myColor === "black" ? "red" : null;
      const mySeat   = myColor  ? seats[myColor]  : null;
      const oppSeat  = oppColor ? seats[oppColor] : null;
      $myName.textContent  = mySeat  ? mySeat.name  : "";
      $oppName.textContent = oppSeat ? oppSeat.name : "";

      if (gameOver) return;
      $opponentState.textContent = opponentPresent
        ? t("online_opp_present")
        : t("online_opp_waiting");
      $opponentState.classList.toggle("badge-success", opponentPresent);
      $opponentState.classList.toggle("badge-warning", !opponentPresent);
      $opponentState.classList.toggle("badge-muted", false);
    }

    function clearSelection() {
      selectedSquare = null;
      legalDestForSel = [];
    }

    // ── Click handlers ────────────────────────────────────────────────────────
    function onPieceClick(s) {
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
        rerender();
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
      rerender();
    }

    function onDestClick(s) {
      if (selectedSquare === null) return;
      if (!legalDestForSel.includes(s)) return;
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      ws.send(JSON.stringify({ t: "move", from: selectedSquare, to: s }));
      clearSelection();
      rerender();
    }

    // ── Board controller ──────────────────────────────────────────────────────
    const board = createBoard($svg, {
      onPieceClick,
      onDestClick,
      onEmptyClick: () => { clearSelection(); rerender(); },
    });

    rerender();

    // ── WebSocket ─────────────────────────────────────────────────────────────
    setMsg(t("online_connecting"));

    function connect() {
      ws = new WebSocket(wsUrl());

      ws.addEventListener("open", () => {
        setMsg("");
        if (roomCode === "NEW") {
          console.debug("[cotuong] ws→ create");
          ws.send(JSON.stringify({ t: "create", name: identity.name }));
        } else {
          console.debug("[cotuong] ws→ join", roomCode);
          ws.send(JSON.stringify({ t: "join", room: roomCode, name: identity.name }));
        }
      });

      ws.addEventListener("message", (ev) => {
        let msg;
        try { msg = JSON.parse(ev.data); }
        catch (e) { console.error("[cotuong] ws message parse failed", e, ev.data); return; }
        handleMessage(msg);
      });

      ws.addEventListener("close", (ev) => {
        console.warn("[cotuong] ws closed code=%d reason=%s", ev.code, ev.reason);
        ws = null;
        setMsg(t("online_disconnected"));
      });

      ws.addEventListener("error", (ev) => {
        console.error("[cotuong] ws error", ev);
        setMsg(t("online_disconnected"));
      });
    }

    function applyMove(from, to) {
      if (game.play_move(from, to)) {
        moveHistory.push({ from, to });
        return true;
      }
      // Desync — reset and replay the full history including this move.
      console.warn("[cotuong] play_move(%d,%d) failed — replaying %d moves", from, to, moveHistory.length);
      game.reset();
      for (const m of [...moveHistory, { from, to }]) {
        if (!game.play_move(m.from, m.to)) {
          console.error("[cotuong] replay failed at from=%d to=%d — full desync", m.from, m.to);
          return false;
        }
      }
      moveHistory.push({ from, to });
      return true;
    }

    function handleMessage(msg) {
      console.debug("[cotuong] ws←", msg.t, msg);
      switch (msg.t) {
        case "joined":
          myColor = msg.color;
          seats = msg.seats || { red: null, black: null };
          scoreboard = msg.scoreboard || scoreboard;
          opponentPresent = !!(myColor === "red" ? seats.black : seats.red);
          gameOver = msg.status !== "playing";
          // Rematch may flip color and re-issue Joined — treat any joined as
          // a fresh game (board, history, rematch flags reset).
          myRematchReady = false;
          oppRematchReady = false;
          // Redirect URL to the real code now that we know it (create flow)
          if (msg.room !== roomCode) {
            history.replaceState(null, "", `#/room/${msg.room}`);
            root.querySelector("#room-code-display").textContent = msg.room;
          }
          moveHistory.length = 0;
          game.reset();
          clearSelection();
          rerender();
          renderMoveList();
          renderScoreboard();
          updateRoomUI();
          renderRematchUi();
          break;
        case "opponent_joined":
          opponentPresent = true;
          updateRoomUI();
          renderRematchUi();
          break;
        case "opponent_left":
          opponentPresent = false;
          oppRematchReady = false;
          setMsg(t("online_opp_left_msg"));
          updateRoomUI();
          renderRematchUi();
          break;
        case "seats":
          seats = msg.seats || { red: null, black: null };
          opponentPresent = !!(myColor === "red" ? seats.black : seats.red);
          updateRoomUI();
          renderRematchUi();
          break;
        case "scoreboard":
          scoreboard = msg.scoreboard || [];
          renderScoreboard();
          break;
        case "rematch_pending":
          if (myColor === "red") {
            myRematchReady  = !!msg.red_ready;
            oppRematchReady = !!msg.black_ready;
          } else if (myColor === "black") {
            myRematchReady  = !!msg.black_ready;
            oppRematchReady = !!msg.red_ready;
          }
          renderRematchUi();
          break;
        case "move": {
          const ok = applyMove(msg.from, msg.to);
          if (!ok) {
            console.error("[cotuong] move could not be applied even after replay", msg);
          }
          clearSelection();
          rerender();
          renderMoveList();
          if (msg.status !== "playing") gameOver = true;
          break;
        }
        case "game_over":
          gameOver = true;
          if (msg.reason === "resignation") {
            const iWon = msg.winner === myColor;
            setMsg(iWon ? t("online_opp_resigned") : t("online_resigned"));
          }
          updateRoomUI();
          renderRematchUi();
          break;
        case "error":
          console.warn("[cotuong] server error:", msg.reason);
          setMsg(msg.reason || "error");
          if (msg.reason === "room not found") setMsg(t("online_room_not_found"));
          if (msg.reason === "room full")      setMsg(t("online_room_full"));
          break;
        default:
          break;
      }
    }

    connect();

    // ── Room controls ─────────────────────────────────────────────────────────
    $resign.addEventListener("click", async () => {
      if (!ws || gameOver) return;
      const confirmed = await confirmResign();
      if (!confirmed) return;
      if (!ws || gameOver) return; // re-check after the await
      ws.send(JSON.stringify({ t: "resign" }));
    });

    $leave.addEventListener("click", () => router.go("/"));

    $rematch.addEventListener("click", () => {
      if (!ws || !gameOver || !opponentPresent) return;
      ws.send(JSON.stringify({ t: myRematchReady ? "rematch_cancel" : "rematch" }));
      // Optimistic flip; the authoritative state arrives via rematch_pending
      // (or via a fresh joined when both sides have agreed).
      myRematchReady = !myRematchReady;
      renderRematchUi();
    });

    $copyLink.addEventListener("click", () => {
      const code = root.querySelector("#room-code-display").textContent;
      const url = new URL(window.location.href);
      url.hash = `#/room/${code}`;
      navigator.clipboard.writeText(url.toString()).catch(() => {});
    });

    // ── Lang change ───────────────────────────────────────────────────────────
    const unsubLang = onLangChange(() => {
      applyLangToDom(root);
      updateStatus();
      updateRoomUI();
    });

    return {
      unmount() {
        unsubLang();
        if (ws) {
          try { ws.close(); } catch (_) {}
          ws = null;
        }
      },
    };
  },
};

// ── Identity resolution ───────────────────────────────────────────────────────
// Returns { kind: "user"|"guest", name } or null if the user cancelled.
async function resolveIdentity(root, roomCode) {
  // Check if already logged in
  try {
    const r = await fetch("/auth/me", { credentials: "same-origin" });
    if (r.ok) {
      const user = await r.json();
      return { kind: "user", name: user.username };
    }
  } catch (_) { /* offline, treat as guest */ }

  // Return existing guest identity if we have one
  const existing = getGuestName();
  if (existing) return { kind: "guest", name: existing };

  // Show name prompt modal
  return showGuestModal(root, roomCode);
}

// Small confirm dialog for the resign action. Resolves true if the user
// clicked "Yes, resign", false if they cancelled (button or Escape or outside
// click).
function confirmResign() {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";
    overlay.innerHTML = `
      <div class="modal" role="dialog" aria-modal="true">
        <h2 data-i18n="resign_confirm_title">Resign this game?</h2>
        <p data-i18n="resign_confirm_desc">Your opponent will win. This can't be undone.</p>
        <div class="modal-actions">
          <button type="button" class="btn btn-destructive btn-block" id="resign-confirm-yes" data-i18n="resign_confirm_yes">Yes, resign</button>
          <button type="button" class="btn btn-ghost btn-sm" id="resign-confirm-no" data-i18n="modal_cancel">Cancel</button>
        </div>
      </div>
    `;
    applyLangToDom(overlay);
    document.body.appendChild(overlay);

    let settled = false;
    const finish = (result) => {
      if (settled) return;
      settled = true;
      overlay.remove();
      document.removeEventListener("keydown", onKey);
      resolve(result);
    };
    const onKey = (e) => { if (e.key === "Escape") finish(false); };
    document.addEventListener("keydown", onKey);

    overlay.querySelector("#resign-confirm-yes").addEventListener("click", () => finish(true));
    overlay.querySelector("#resign-confirm-no").addEventListener("click", () => finish(false));
    overlay.addEventListener("click", (e) => { if (e.target === overlay) finish(false); });

    overlay.querySelector("#resign-confirm-yes").focus();
  });
}

function showGuestModal(root, roomCode) {
  return new Promise((resolve) => {
    const placeholder = suggestGuestName();
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";
    overlay.innerHTML = `
      <div class="modal">
        <h2><span data-i18n="modal_join_title">Join room</span> <code>${roomCode}</code></h2>
        <p data-i18n="modal_join_desc">Choose a display name to continue as guest, or log in.</p>
        <form id="guest-form">
          <div class="field">
            <input class="input" name="name" placeholder="${placeholder}" autocomplete="off" maxlength="30" />
          </div>
          <div class="modal-actions">
            <button type="submit" class="btn btn-primary btn-block" data-i18n="modal_continue_guest">Continue as guest</button>
            <a class="btn oauth-btn btn-block" href="/auth/google/login" data-i18n="modal_login_google">Continue with Google</a>
            <button type="button" id="modal-cancel" class="btn btn-ghost btn-sm" data-i18n="modal_cancel">Cancel</button>
          </div>
        </form>
      </div>
    `;
    applyLangToDom(overlay);
    document.body.appendChild(overlay);

    const form = overlay.querySelector("#guest-form");
    const nameInput = overlay.querySelector("input[name='name']");
    nameInput.focus();

    form.addEventListener("submit", (e) => {
      e.preventDefault();
      const name = (nameInput.value || "").trim() || placeholder;
      setGuestName(name);
      overlay.remove();
      resolve({ kind: "guest", name });
    });

    overlay.querySelector("#modal-cancel").addEventListener("click", () => {
      overlay.remove();
      router.go("/");
      resolve(null);
    });
  });
}
