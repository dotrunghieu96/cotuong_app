// Hash-based router. Routes are registered with a pattern (e.g. "/room/:code")
// mapped to a screen module. A screen module exports a default object with:
//   { mount(rootEl, { params, query }) -> Promise<{ unmount? }> }
//
// On hashchange, the active screen is unmounted, the root cleared, and the
// matched screen is mounted. Unknown paths redirect to "/".

const routes = [];
let mountEl = null;
let current = null;
let navigating = false;

export function register(pattern, screen) {
  routes.push({ pattern, screen, ...compile(pattern) });
}

function compile(pattern) {
  const paramNames = [];
  const re = pattern.replace(/:[A-Za-z_][A-Za-z0-9_]*/g, (m) => {
    paramNames.push(m.slice(1));
    return "([^/]+)";
  });
  return { regex: new RegExp(`^${re}$`), paramNames };
}

function match(path) {
  for (const r of routes) {
    const m = path.match(r.regex);
    if (!m) continue;
    const params = {};
    r.paramNames.forEach((n, i) => { params[n] = decodeURIComponent(m[i + 1]); });
    return { screen: r.screen, params };
  }
  return null;
}

function parseQuery(q) {
  return Object.fromEntries(new URLSearchParams(q || ""));
}

async function navigate() {
  if (navigating) return;
  navigating = true;
  try {
    const hash = location.hash.replace(/^#/, "") || "/";
    const [path, query] = hash.split("?");
    const matched = match(path);
    if (!matched) {
      location.hash = "#/";
      return;
    }
    if (current && current.unmount) {
      try { await current.unmount(); }
      catch (e) { console.warn("unmount failed:", e); }
    }
    current = null;
    mountEl.replaceChildren();
    const handle = await matched.screen.mount(mountEl, {
      params: matched.params,
      query: parseQuery(query),
    });
    current = handle || {};
  } finally {
    navigating = false;
  }
}

export function start(rootEl) {
  mountEl = rootEl;
  window.addEventListener("hashchange", navigate);
  navigate();
}

export function go(path) {
  const target = "#" + path;
  if (location.hash === target) {
    navigate();
  } else {
    location.hash = target;
  }
}
