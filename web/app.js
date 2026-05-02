import init from "./pkg/cotuong_engine.js";
import { initAuth } from "./auth.js";
import { setLang, applyLangToDom, onLangChange, getLang } from "./i18n.js";
import * as router from "./router.js";
import landing  from "./screens/landing.js";
import room     from "./screens/room.js";
import login    from "./screens/login.js";
import register from "./screens/register.js";

router.register("/",            landing);
router.register("/login",       login);
router.register("/register",    register);
router.register("/room/:code",  room);

(async function bootstrap() {
  // Load wasm engine once; screens construct Game instances after this.
  await init();

  // Auth header UI (avatar/menu) — drives its own DOM ids.
  initAuth().catch((e) => console.warn("auth init failed:", e));

  // Lang toggle in the persistent header — one button, full-area click flips
  // EN ↔ VI. Both labels stay rendered so the header never reflows.
  const langToggle = document.getElementById("lang-toggle");
  if (langToggle) {
    langToggle.addEventListener("click", () => {
      setLang(getLang() === "vi" ? "en" : "vi");
    });
  }

  // Re-translate the persistent header on language change. Screens manage
  // their own subtrees via their own onLangChange subscriptions.
  onLangChange(() => applyLangToDom());

  applyLangToDom();

  router.start(document.getElementById("app"));
})();
