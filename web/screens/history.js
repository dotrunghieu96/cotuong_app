import { t, applyLangToDom, onLangChange, getLang } from "../i18n.js";
import * as router from "../router.js";

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
    const d = new Date(iso);
    return d.toLocaleString(getLang() === "vi" ? "vi-VN" : "en-US", {
      year: "numeric", month: "short", day: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  } catch (_) {
    return iso;
  }
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

function playerLabel(name) {
  return name || t("guest_label");
}

export default {
  async mount(root) {
    root.innerHTML = `
      <div class="container">
        <div class="card">
          <div class="card-header">
            <h1 class="card-title" data-i18n="history_title">Game history</h1>
          </div>
          <div class="card-content">
            <div id="history-status" class="text-muted text-sm">…</div>
            <div id="history-list" class="history-list"></div>
          </div>
        </div>
      </div>
    `;
    applyLangToDom(root);

    const $status = root.querySelector("#history-status");
    const $list   = root.querySelector("#history-list");

    let games = [];

    async function load() {
      $status.textContent = "…";
      $list.innerHTML = "";
      try {
        const r = await fetch("/api/games?finished=true&limit=100", {
          credentials: "same-origin",
        });
        if (r.status === 401) {
          $status.innerHTML = `${esc(t("history_login_required"))} `
            + `<a href="#/login" class="btn-link" data-i18n="auth_login">Log in</a>`;
          applyLangToDom($status);
          return;
        }
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        const data = await r.json();
        games = data.games || [];
        render();
      } catch (e) {
        console.warn("history load failed:", e);
        $status.textContent = "—";
        games = [];
        render();
      }
    }

    function render() {
      if (games.length === 0) {
        $status.textContent = t("history_empty");
        $list.innerHTML = "";
        return;
      }
      $status.textContent = "";
      const rows = games.map((g) => {
        const players = `${esc(playerLabel(g.red_player))} ` +
          `<span class="text-muted">vs</span> ` +
          `${esc(playerLabel(g.black_player))}`;
        const result = resultLabel(g);
        const term   = terminationLabel(g);
        const termPart = term ? ` <span class="text-muted">· ${esc(term)}</span>` : "";
        return `
          <a class="history-row" href="#/replay/${esc(g.id)}">
            <div class="history-row-main">
              <div class="history-room"><code>${esc(g.room_code)}</code></div>
              <div class="history-players">${players}</div>
            </div>
            <div class="history-row-meta">
              <div class="history-result">${esc(result)}${termPart}</div>
              <div class="history-time text-muted text-xs">${esc(fmtDate(g.finished_at || g.started_at))}</div>
            </div>
          </a>
        `;
      });
      $list.innerHTML = rows.join("");
    }

    load();

    const unsubLang = onLangChange(() => {
      applyLangToDom(root);
      render(); // dates / labels switch language
    });

    return {
      unmount() { unsubLang(); },
    };
  },
};
