import { Game } from "../pkg/cotuong_engine.js";
import { t, applyLangToDom, onLangChange, getLang } from "../i18n.js";
import { createBoard } from "../board.js";

const BOARD_VIEWBOX = "0 0 520 576";

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function fmtDate(iso) {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString(getLang() === "vi" ? "vi-VN" : "en-US", {
      year: "numeric", month: "short", day: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  } catch (_) { return iso; }
}

const squareToUci = (sq) =>
  String.fromCharCode("a".charCodeAt(0) + (sq % 9)) + (9 - Math.floor(sq / 9));

function playerLabel(name) {
  return name || t("guest_label");
}

function resultLabel(g) {
  if (!g.finished_at) return t("history_unfinished");
  if (g.result === "red_wins")   return t("red_wins");
  if (g.result === "black_wins") return t("black_wins");
  return "—";
}

function terminationLabel(g) {
  if (!g.termination) return "";
  if (g.termination === "checkmate")    return t("termination_checkmate");
  if (g.termination === "resignation")  return t("termination_resignation");
  if (g.termination === "abandoned")    return t("termination_abandoned");
  return g.termination;
}

export default {
  async mount(root, { params }) {
    const gameId = params.id || "";

    root.innerHTML = `
      <div class="layout">
        <div class="board-wrap">
          <svg id="board" viewBox="${BOARD_VIEWBOX}" xmlns="http://www.w3.org/2000/svg"></svg>
        </div>
        <aside class="side">
          <div class="card">
            <div class="card-header">
              <a class="btn btn-ghost btn-sm" href="#/history" data-i18n="replay_back">Back to history</a>
            </div>
            <div class="card-content">
              <div id="replay-meta" class="text-sm text-muted">…</div>
            </div>
          </div>

          <div class="panel">
            <div class="panel-title" data-i18n="move_list_title">Moves</div>
            <div class="move-list" id="move-list"></div>
          </div>

          <div class="controls">
            <button class="btn btn-outline btn-sm" id="replay-first" data-i18n="replay_first" disabled></button>
            <button class="btn btn-outline btn-sm" id="replay-prev"  data-i18n="replay_prev"  disabled></button>
            <button class="btn btn-outline btn-sm" id="replay-next"  data-i18n="replay_next"  disabled></button>
            <button class="btn btn-outline btn-sm" id="replay-last"  data-i18n="replay_last"  disabled></button>
          </div>
        </aside>
      </div>
    `;
    applyLangToDom(root);

    const $svg      = root.querySelector("#board");
    const $meta     = root.querySelector("#replay-meta");
    const $list     = root.querySelector("#move-list");
    const $first    = root.querySelector("#replay-first");
    const $prev     = root.querySelector("#replay-prev");
    const $next     = root.querySelector("#replay-next");
    const $last     = root.querySelector("#replay-last");

    const game = new Game();
    const board = createBoard($svg, {
      onPieceClick: () => {},
      onDestClick:  () => {},
      onEmptyClick: () => {},
    });

    let detail = null; // { id, room_code, ..., moves: [...] }
    let plyIndex = 0;  // 0 = initial position; 1..N = after move N

    function renderBoard() {
      const moves = detail ? detail.moves : [];
      const lastMv = plyIndex > 0
        ? { from: moves[plyIndex - 1].from_sq, to: moves[plyIndex - 1].to_sq }
        : null;
      board.render({
        board:    JSON.parse(game.board_json()),
        lastMove: lastMv,
        selectedSquare: null,
        legalDests: [],
      }, { animateMove: false });
    }

    function renderMoveList() {
      if (!detail) { $list.innerHTML = ""; return; }
      const moves = detail.moves;
      let html = "";
      for (let i = 0; i < moves.length; i += 2) {
        const turnNo = (i / 2) + 1;
        const r = moves[i];
        const b = moves[i + 1];
        const cellRed   = r ? squareToUci(r.from_sq) + "→" + squareToUci(r.to_sq) : "";
        const cellBlack = b ? squareToUci(b.from_sq) + "→" + squareToUci(b.to_sq) : "";
        const isCurR = (i + 1) === plyIndex;
        const isCurB = (i + 2) === plyIndex;
        html += `<div class="move-row">`
          + `<span class="ply-num">${turnNo}.</span>`
          + `<span class="move-cell red${isCurR ? " current" : ""}" data-ply="${i + 1}">${cellRed}</span>`
          + `<span class="move-cell black${isCurB ? " current" : ""}" data-ply="${i + 2}">${cellBlack}</span>`
          + `</div>`;
      }
      $list.innerHTML = html;

      // Scroll the current cell into view.
      const cur = $list.querySelector(".move-cell.current");
      if (cur) cur.scrollIntoView({ block: "nearest" });
    }

    function renderMeta() {
      if (!detail) { $meta.textContent = "…"; return; }
      const players = `${esc(playerLabel(detail.red_player))} <span class="text-muted">vs</span> ${esc(playerLabel(detail.black_player))}`;
      const term = terminationLabel(detail);
      const termPart = term ? ` <span class="text-muted">· ${esc(term)}</span>` : "";
      const finished = detail.finished_at ? fmtDate(detail.finished_at) : fmtDate(detail.started_at);
      const plyLabel = plyIndex === 0 ? t("replay_initial") : `${t("replay_ply")} ${plyIndex} / ${detail.moves.length}`;
      $meta.innerHTML = `
        <div><strong>${esc(detail.room_code)}</strong> · ${players}</div>
        <div class="text-xs text-muted" style="margin-top: 4px;">${esc(resultLabel(detail))}${termPart} · ${esc(finished)}</div>
        <div class="text-xs" style="margin-top: 8px;">${esc(plyLabel)}</div>
      `;
    }

    function setPly(n) {
      if (!detail) return;
      const total = detail.moves.length;
      n = Math.max(0, Math.min(total, n));
      // Cheap: replay from start. Total moves <= a few hundred so this is fine.
      game.reset();
      for (let i = 0; i < n; i++) {
        const m = detail.moves[i];
        game.play_move(m.from_sq, m.to_sq);
      }
      plyIndex = n;
      renderBoard();
      renderMoveList();
      renderMeta();
      $first.disabled = plyIndex === 0;
      $prev.disabled  = plyIndex === 0;
      $next.disabled  = plyIndex === total;
      $last.disabled  = plyIndex === total;
    }

    async function load() {
      try {
        const r = await fetch(`/api/games/${encodeURIComponent(gameId)}`, {
          credentials: "same-origin",
        });
        if (r.status === 401) {
          $meta.innerHTML = `${esc(t("history_login_required"))} `
            + `<a href="#/login" class="btn-link" data-i18n="auth_login">Log in</a>`;
          applyLangToDom($meta);
          return;
        }
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        detail = await r.json();
        // Sort moves defensively.
        detail.moves.sort((a, b) => a.ply - b.ply);
        setPly(detail.moves.length); // start at the end so the user sees the result
      } catch (e) {
        console.warn("replay load failed:", e);
        $meta.textContent = t("replay_not_found");
      }
    }
    load();

    $first.addEventListener("click", () => setPly(0));
    $prev.addEventListener("click",  () => setPly(plyIndex - 1));
    $next.addEventListener("click",  () => setPly(plyIndex + 1));
    $last.addEventListener("click",  () => setPly(detail ? detail.moves.length : 0));

    $list.addEventListener("click", (e) => {
      const cell = e.target.closest(".move-cell[data-ply]");
      if (!cell) return;
      const n = parseInt(cell.dataset.ply, 10);
      if (Number.isFinite(n)) setPly(n);
    });

    const onKey = (e) => {
      if (e.key === "ArrowLeft")  { $prev.click(); }
      if (e.key === "ArrowRight") { $next.click(); }
      if (e.key === "Home")       { $first.click(); }
      if (e.key === "End")        { $last.click(); }
    };
    document.addEventListener("keydown", onKey);

    const unsubLang = onLangChange(() => {
      applyLangToDom(root);
      renderMoveList();
      renderMeta();
    });

    return {
      unmount() {
        unsubLang();
        document.removeEventListener("keydown", onKey);
      },
    };
  },
};
