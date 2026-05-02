import { t, applyLangToDom, onLangChange } from "../i18n.js";
import * as router from "../router.js";

function googleIconSvg() {
  return `
    <svg viewBox="0 0 48 48" width="16" height="16" aria-hidden="true">
      <path fill="#FFC107" d="M43.6 20.5h-1.9V20H24v8h11.3c-1.6 4.6-6 8-11.3 8-6.6 0-12-5.4-12-12s5.4-12 12-12c3 0 5.8 1.1 7.9 3l5.7-5.7C34 6.1 29.3 4 24 4 12.9 4 4 12.9 4 24s8.9 20 20 20 20-8.9 20-20c0-1.2-.1-2.3-.4-3.5z"/>
      <path fill="#FF3D00" d="M6.3 14.7l6.6 4.8C14.6 16 18.9 13 24 13c3 0 5.8 1.1 7.9 3l5.7-5.7C34 6.1 29.3 4 24 4 16.3 4 9.7 8.4 6.3 14.7z"/>
      <path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2c-2 1.5-4.5 2.4-7.2 2.4-5.3 0-9.7-3.4-11.3-8l-6.6 5.1C9.6 39.5 16.2 44 24 44z"/>
      <path fill="#1976D2" d="M43.6 20.5H24v8h11.3c-.8 2.3-2.3 4.2-4.2 5.6l6.2 5.2c-.4.4 6.7-4.9 6.7-14.8 0-1.2-.1-2.3-.4-3.5z"/>
    </svg>`;
}

export default {
  async mount(root) {
    root.innerHTML = `
      <div class="auth-page">
        <div class="card auth-card">
          <div class="card-header">
            <h1 class="card-title" data-i18n="auth_register_title">Create your account</h1>
            <p class="card-description" data-i18n="auth_register_desc">Pick a username, drop in your email, and you're set.</p>
          </div>

          <div class="card-content">
            <a class="btn oauth-btn btn-block" href="/auth/google/login">
              ${googleIconSvg()}
              <span data-i18n="auth_continue_google">Continue with Google</span>
            </a>

            <div class="separator-text"><span data-i18n="auth_or">or</span></div>

            <div class="alert error" id="register-alert" hidden></div>

            <form id="register-form" novalidate>
              <div class="field">
                <label class="label" for="reg-username" data-i18n="auth_username">Username</label>
                <input class="input" id="reg-username" name="username"
                       autocomplete="username" required minlength="3" maxlength="30" />
                <div class="field-help" data-i18n="auth_username_help">3–30 characters. Letters, numbers, dashes.</div>
              </div>

              <div class="field">
                <label class="label" for="reg-email" data-i18n="auth_email">Email</label>
                <input class="input" id="reg-email" name="email" type="email"
                       autocomplete="email" required />
              </div>

              <div class="field">
                <label class="label" for="reg-password" data-i18n="auth_password">Password</label>
                <input class="input" id="reg-password" name="password" type="password"
                       autocomplete="new-password" required minlength="8" />
                <div class="field-help" data-i18n="auth_password_help">At least 8 characters.</div>
              </div>

              <button type="submit" class="btn btn-primary btn-block" id="register-submit">
                <span data-i18n="auth_register_submit">Create account</span>
              </button>
            </form>
          </div>

          <div class="card-footer" style="display:block;">
            <div class="auth-foot">
              <span data-i18n="auth_have_account">Already have an account?</span>
              <a href="#/login" data-i18n="auth_login_link">Sign in</a>
            </div>
          </div>
        </div>
      </div>
    `;

    applyLangToDom(root);

    const $form   = root.querySelector("#register-form");
    const $alert  = root.querySelector("#register-alert");
    const $submit = root.querySelector("#register-submit");

    function setError(text) {
      if (!text) { $alert.hidden = true; $alert.textContent = ""; return; }
      $alert.hidden = false;
      $alert.textContent = text;
    }

    $form.addEventListener("submit", async (e) => {
      e.preventDefault();
      setError("");
      $submit.disabled = true;
      const fd = new FormData($form);
      try {
        const r = await fetch("/auth/signup", {
          method: "POST",
          headers: { "content-type": "application/json" },
          credentials: "same-origin",
          body: JSON.stringify({
            username: fd.get("username"),
            email: fd.get("email"),
            password: fd.get("password"),
          }),
        });
        let data = null;
        try { data = await r.json(); } catch (_) {}
        if (r.ok) {
          window.dispatchEvent(new CustomEvent("auth:changed", { detail: data }));
          router.go("/");
          return;
        }
        setError(data?.message || t("auth_register_failed"));
      } catch (err) {
        console.warn("register failed:", err);
        setError(t("auth_network_error"));
      } finally {
        $submit.disabled = false;
      }
    });

    const unsubLang = onLangChange(() => applyLangToDom(root));

    return { unmount() { unsubLang(); } };
  },
};
