import { Game } from "../pkg/cotuong_engine.js";
import { t, applyLangToDom, onLangChange } from "../i18n.js";
import { createBoard, ANIM_MS } from "../board.js";
import * as router from "../router.js";

const BOARD_VIEWBOX = "0 0 520 576";

function html(strings, ...vals) {
  return strings.reduce((a, s, i) => a + s + (vals[i] ?? ""), "");
}

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export default {
  async mount(root) {
    // ── Markup ──────────────────────────────────────────────────────────────
    root.innerHTML = html`
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

          <div class="panel" id="ai-panel">
            <div class="panel-title" data-i18n="game_setting"></div>
            <div class="tabs" role="tablist">
              <input type="radio" name="mode" id="mode-hh" value="hh" />
              <label for="mode-hh" data-i18n="mode_hh"></label>
              <input type="radio" name="mode" id="mode-ai-black" value="ai-black" checked />
              <label for="mode-ai-black" data-i18n="mode_ai_black"></label>
              <input type="radio" name="mode" id="mode-ai-red" value="ai-red" />
              <label for="mode-ai-red" data-i18n="mode_ai_red"></label>
            </div>

            <div class="slider-row lock-target" id="depth-row">
              <label for="depth" data-i18n="ai_depth"></label>
              <input type="range" id="depth" min="1" max="5" value="3" />
              <span class="slider-value" id="depth-val">3</span>
            </div>
          </div>

          <button class="btn btn-primary btn-block" id="play-toggle" data-i18n="play"></button>

          <div class="controls">
            <button class="btn btn-secondary" id="new-game" data-i18n="new_game" disabled></button>
            <button class="btn btn-outline" id="undo" data-i18n="undo" disabled></button>
          </div>

          <div class="panel">
            <div class="panel-title" data-i18n="online_legend"></div>
            <button class="btn btn-primary" id="online-create" data-i18n="online_create"></button>
            <div class="join-row">
              <input class="input" id="online-code" placeholder="ABCDEF" maxlength="6" autocomplete="off" />
              <button class="btn btn-secondary" id="online-join" data-i18n="online_join"></button>
            </div>
          </div>

          <div class="footer-help" data-i18n="help"></div>
        </aside>
      </div>
    `;

    applyLangToDom(root);

    // ── State ────────────────────────────────────────────────────────────────
    const game = new Game();
    let mode = "ai-black";
    let aiDepth = 3;
    let aiThinking = false;
    let playing = false;
    let selectedSquare = null;
    let legalDestForSel = [];

    // ── DOM refs ─────────────────────────────────────────────────────────────
    const $svg       = root.querySelector("#board");
    const $turnText  = root.querySelector("#turn-text");
    const $turnDot   = root.querySelector("#turn-dot");
    const $checkLine = root.querySelector("#check-line");
    const $resultLine= root.querySelector("#result-line");
    const $newGame   = root.querySelector("#new-game");
    const $undo      = root.querySelector("#undo");
    const $depth     = root.querySelector("#depth");
    const $depthVal  = root.querySelector("#depth-val");
    const $depthRow  = root.querySelector("#depth-row");
    const $create    = root.querySelector("#online-create");
    const $join      = root.querySelector("#online-join");
    const $code      = root.querySelector("#online-code");
    const $playToggle= root.querySelector("#play-toggle");
    const $modeRadios= root.querySelectorAll("input[name='mode']");

    // ── Helpers ───────────────────────────────────────────────────────────────
    function humanCanMove() {
      const turn = game.turn();
      if (mode === "hh") return true;
      if (mode === "ai-red")   return turn === 1; // human is Black
      if (mode === "ai-black") return turn === 0; // human is Red
      return false;
    }

    function rerender(opts = {}) {
      board.render({
        board:          JSON.parse(game.board_json()),
        lastMove:       JSON.parse(game.last_move_json()),
        selectedSquare,
        legalDests:     legalDestForSel,
      }, opts);
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

    function clearSelection() {
      selectedSquare = null;
      legalDestForSel = [];
    }

    function maybeAIMove() {
      if (game.status() !== "playing") return;
      const turn = game.turn();
      const aiTurn =
        (mode === "ai-black" && turn === 1) ||
        (mode === "ai-red"   && turn === 0);
      if (!aiTurn) return;
      runAI();
    }

    function runAI() {
      if (aiThinking) return;
      if (game.status() !== "playing") return;
      aiThinking = true;
      $turnText.textContent = t("ai_thinking");
      setTimeout(() => {
        try {
          game.ai_move(aiDepth);
        } finally {
          aiThinking = false;
          rerender();
        }
      }, ANIM_MS + 20);
    }

    // ── Click handlers passed to board controller ─────────────────────────────
    function onPieceClick(s) {
      if (!playing) return;
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
      if (!playing) return;
      if (aiThinking) return;
      if (selectedSquare === null) return;
      if (!legalDestForSel.includes(s)) return;
      game.play_move(selectedSquare, s);
      clearSelection();
      rerender();
      maybeAIMove();
    }

    // ── Board controller ──────────────────────────────────────────────────────
    const board = createBoard($svg, {
      onPieceClick,
      onDestClick,
      onEmptyClick: () => { clearSelection(); rerender(); },
    });

    rerender();

    // ── Play / Stop ──────────────────────────────────────────────────────────
    // While `playing` is false, the user is in setup: mode radios are live, the
    // board is frozen, and game-action buttons are disabled. Pressing Play
    // resets the board and locks mode in. Pressing Stop ends the game and
    // returns to setup.
    function setPlaying(next) {
      playing = next;
      game.reset();
      clearSelection();

      $modeRadios.forEach((r) => { r.disabled = playing; });
      $newGame.disabled = !playing;
      $undo.disabled    = !playing;

      $playToggle.dataset.i18n = playing ? "stop" : "play";
      $playToggle.textContent  = t(playing ? "stop" : "play");
      $playToggle.classList.toggle("btn-primary", !playing);
      $playToggle.classList.toggle("btn-destructive", playing);

      rerender();
      if (playing) maybeAIMove();
    }

    setPlaying(false);

    $playToggle.addEventListener("click", () => {
      if (aiThinking) return;
      setPlaying(!playing);
    });

    // ── Controls ──────────────────────────────────────────────────────────────
    $newGame.addEventListener("click", () => {
      if (!playing) return;
      if (aiThinking) return;
      game.reset();
      clearSelection();
      rerender();
      maybeAIMove();
    });

    $undo.addEventListener("click", () => {
      if (!playing) return;
      if (aiThinking) return;
      const ai = mode === "ai-black" || mode === "ai-red";
      game.undo();
      if (ai) game.undo();
      clearSelection();
      rerender({ animateMove: false });
    });

    $depth.addEventListener("input", () => {
      aiDepth = parseInt($depth.value, 10);
      $depthVal.textContent = String(aiDepth);
    });

    $modeRadios.forEach((el) => {
      el.addEventListener("change", () => {
        if (!el.checked) return;
        if (playing) return; // safety: radios are disabled while playing
        mode = el.value;
        $depthRow.style.display = (mode === "hh") ? "none" : "";
        game.reset();
        clearSelection();
        rerender();
      });
    });

    // ── Online entry points (navigate, don't play here) ───────────────────────
    $create.addEventListener("click", () => router.go("/room/new"));

    $join.addEventListener("click", () => {
      const code = ($code.value || "").trim().toUpperCase();
      if (code) router.go(`/room/${code}`);
    });

    $code.addEventListener("keydown", (e) => {
      if (e.key === "Enter") $join.click();
    });

    // ── Lang change ────────────────────────────────────────────────────────────
    const unsubLang = onLangChange(() => {
      applyLangToDom(root);
      updateStatus();
    });

    return {
      unmount() {
        unsubLang();
      },
    };
  },
};
