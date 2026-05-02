// Header auth driver: shows avatar+menu when signed in, login/signup links
// otherwise. Login and signup themselves live on dedicated screens; this file
// only manages the header state and the logout action.

const $ = (id) => document.getElementById(id);

async function fetchMe() {
  const r = await fetch("/auth/me", { credentials: "same-origin" });
  if (r.status === 401) return null;
  if (!r.ok) throw new Error(`me ${r.status}`);
  return r.json();
}

function renderLoggedIn(user) {
  $("auth-loggedin").hidden = false;
  $("auth-loggedout").hidden = true;
  $("auth-username").textContent = user.username;
  const letter = (user.username || "?").trim().charAt(0).toUpperCase() || "·";
  $("auth-avatar-letter").textContent = letter;
  // History is per-user, so it's only meaningful when signed in.
  const navHistory = $("nav-history");
  if (navHistory) navHistory.hidden = false;
}

function renderLoggedOut() {
  $("auth-loggedin").hidden = true;
  $("auth-loggedout").hidden = false;
  const navHistory = $("nav-history");
  if (navHistory) navHistory.hidden = true;
  closeMenu();
}

function openMenu() {
  const menu = $("auth-menu");
  const btn = $("auth-avatar");
  if (!menu || !btn) return;
  menu.hidden = false;
  btn.setAttribute("aria-expanded", "true");
}

function closeMenu() {
  const menu = $("auth-menu");
  const btn = $("auth-avatar");
  if (!menu || !btn) return;
  menu.hidden = true;
  btn.setAttribute("aria-expanded", "false");
}

function toggleMenu() {
  const menu = $("auth-menu");
  if (!menu) return;
  if (menu.hidden) openMenu();
  else closeMenu();
}

async function onLogout() {
  await fetch("/auth/logout", { method: "POST", credentials: "same-origin" });
  renderLoggedOut();
  window.dispatchEvent(new CustomEvent("auth:changed", { detail: null }));
}

export async function initAuth() {
  $("auth-avatar").addEventListener("click", (e) => {
    e.stopPropagation();
    toggleMenu();
  });

  $("auth-logout").addEventListener("click", onLogout);

  // Close menu on outside click / Escape.
  document.addEventListener("click", (e) => {
    const menu = $("auth-menu");
    if (!menu || menu.hidden) return;
    if (!menu.contains(e.target) && e.target !== $("auth-avatar")) closeMenu();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeMenu();
  });

  // Other screens (login/register) dispatch this when they succeed.
  window.addEventListener("auth:changed", (e) => {
    if (e.detail) renderLoggedIn(e.detail);
    else renderLoggedOut();
  });

  try {
    const user = await fetchMe();
    if (user) renderLoggedIn(user);
    else renderLoggedOut();
  } catch (e) {
    console.warn("auth me check failed:", e);
    renderLoggedOut();
  }
}
