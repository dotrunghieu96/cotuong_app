// Cross-screen state. Auth status is queried on demand via /auth/me (auth.js
// drives the header UI directly). Guest identity is kept here so the room
// screen can attach a stable display name without prompting on every visit.

const GUEST_NAME_KEY = "cotuong.guestName";
const GUEST_ID_KEY = "cotuong.guestId";

const state = {
  guest: {
    name: readLocal(GUEST_NAME_KEY),
    id:   readLocal(GUEST_ID_KEY) || mintGuestId(),
  },
};

function readLocal(key) {
  try { return localStorage.getItem(key); }
  catch (_) { return null; }
}

function writeLocal(key, value) {
  try { localStorage.setItem(key, value); }
  catch (_) { /* ignore */ }
}

function mintGuestId() {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  const id = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  writeLocal(GUEST_ID_KEY, id);
  return id;
}

export function getGuestName() {
  return state.guest.name;
}

export function setGuestName(name) {
  const trimmed = (name || "").trim();
  if (!trimmed) return;
  state.guest.name = trimmed;
  writeLocal(GUEST_NAME_KEY, trimmed);
}

export function getGuestId() {
  return state.guest.id;
}

const PLACEHOLDERS = [
  "RedKnight", "BlackHorse", "RiverPawn", "PalaceGuard", "FlyingCannon",
  "SilentRook", "Elephant7", "AdvisorOne", "QuietBishop", "JumpingHorse",
];

// Generates a fresh suggestion each call — used to seed the guest-name input
// placeholder so it doesn't look stale across visits.
export function suggestGuestName() {
  const stem = PLACEHOLDERS[Math.floor(Math.random() * PLACEHOLDERS.length)];
  const suffix = String(Math.floor(1000 + Math.random() * 9000));
  return `${stem}-${suffix}`;
}
