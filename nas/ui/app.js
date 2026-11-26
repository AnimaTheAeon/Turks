"use strict";
/* Turks for DSM — the window. Talks to the Turks service through api.cgi
   with the DSM session (cookie + SynoToken), or, as its own web page at
   http://<nas>:7878, with Turks's own sign-in cookie. Pages live in the other .js
   files and register themselves on Turks.pages. */

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

// Optional pieces (`cond ? node : null`) are skipped instead of printing "null".
for (const name of ["append", "prepend", "replaceChildren"]) {
  const orig = Element.prototype[name];
  Element.prototype[name] = function (...items) { return orig.apply(this, items.flat(Infinity).filter((x) => x !== null && x !== undefined && x !== false)); };
}

/** Tiny DOM builder: el("div.cls#id", {onclick, ...}, children…) */
function el(spec, attrs, ...kids) {
  const m = spec.match(/^([a-z0-9]+)?((?:[.#][\w-]+)*)$/i) || [];
  const node = document.createElement(m[1] || "div");
  (m[2] || "").replace(/([.#])([\w-]+)/g, (_, k, v) => { if (k === ".") node.classList.add(v); else node.id = v; });
  if (attrs !== undefined && attrs !== null && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) { kids.unshift(attrs); attrs = null; }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else if (k === "html") node.innerHTML = v;
    else if (k === "style" && typeof v === "object") for (const [sk, sv] of Object.entries(v)) { if (sk.startsWith("--")) node.style.setProperty(sk, sv); else node.style[sk] = sv; }
    else if (k === "value") node.value = v;
    else if (k === "checked") node.checked = !!v;
    else node.setAttribute(k, v === true ? "" : v);
  }
  const add = (k) => {
    if (k === undefined || k === null || k === false) return;
    if (Array.isArray(k)) k.forEach(add);
    else node.append(k instanceof Node ? k : document.createTextNode(String(k)));
  };
  kids.forEach(add);
  return node;
}

const Turks = {
  pages: {},
  state: null,
  meta: null,
  cfg: null,
  lists: {},
  token: "",
  page: "movies",
  arg: null,
  menuUsed: false,
  pollTimer: null,
  pageTimer: null,
};

const MENU = [
  ["movies", "Movies"], ["add", "Add Movie"], ["calendar", "Calendar"], ["activity", "Activity"], ["settings", "Settings"], ["system", "System"],
];
/** The menu entry a page belongs to: Discover and Collections are tabs of Add Movie. */
const menuOf = (page) => (page === "discover" || page === "collections" ? "add" : page);

// ---------------------------------------------------------------- API

/** Opened from a browser at http://<nas>:7878 instead of inside DSM: Turks's own sign-in. */
const WEB_PAGE = !location.pathname.startsWith("/webman/");

async function getToken() {
  if (WEB_PAGE) return;
  try {
    const r = await fetch("/webman/login.cgi", { credentials: "same-origin" });
    const j = await r.json();
    Turks.token = j.SynoToken || "";
  } catch (e) { Turks.token = ""; }
}

function apiUrl(route, params) {
  const [r0, q0] = route.split("?");
  let url = "api.cgi?r=" + encodeURIComponent(r0) + (q0 ? "&" + q0 : "");
  for (const [k, v] of Object.entries(params || {})) {
    if (v !== undefined && v !== null && v !== "") url += "&" + encodeURIComponent(k) + "=" + encodeURIComponent(v);
  }
  return url;
}

/** GET when body is undefined, POST (JSON) otherwise. Throws Error(message). */
async function api(route, body, params) {
  const opts = { method: body === undefined ? "GET" : "POST", credentials: "same-origin", headers: { "X-SYNO-TOKEN": Turks.token } };
  if (body !== undefined) {
    opts.headers["Content-Type"] = "application/json";
    opts.body = JSON.stringify(body);
  }
  let res;
  try { res = await fetch(apiUrl(route, params), opts); } catch (e) { throw new Error("Couldn't reach Turks"); }
  let data = {};
  try { data = await res.json(); } catch (e) { /* empty */ }
  if (res.status === 401 && WEB_PAGE) {
    // Signed out (or the password changed): the page itself shows the sign-in.
    location.reload();
    throw new Error("auth");
  }
  if (res.status === 401) {
    showOverlay("Sign in needed", esc(data.error || "Your DSM session has expired. Sign in to DSM again."));
    throw new Error(data.error || "auth");
  }
  if (res.status === 503) {
    showOverlay("Turks isn't running", esc(data.error || "Start Turks in Package Center."));
    throw new Error(data.error || "down");
  }
  hideOverlay();
  if (!res.ok) throw new Error(data.error || ("Request failed (" + res.status + ")"));
  return data;
}

/** Run an action, toast its error. Returns the result or undefined. */
async function act(route, body, params, okMsg) {
  try {
    const r = await api(route, body, params);
    if (okMsg) toast(typeof okMsg === "function" ? okMsg(r) : okMsg);
    return r;
  } catch (e) {
    if (e.message !== "auth" && e.message !== "down") toast(e.message, true);
    return undefined;
  }
}

function imgUrl(kind, id, path) {
  return apiUrl("image", { k: kind, id, p: path || "", SynoToken: Turks.token });
}

/** A poster/fanart <img> that removes itself if there's no picture. */
function img(kind, id, path, cls) {
  const i = el("img" + (cls ? "." + cls : ""), { loading: "lazy", alt: "", src: imgUrl(kind, id, path) });
  i.onerror = () => i.remove();
  return i;
}

// ---------------------------------------------------------------- data

async function loadList(kind, force) {
  if (!force && Turks.lists[kind]) return Turks.lists[kind];
  Turks.lists[kind] = await api("list", undefined, { kind });
  return Turks.lists[kind];
}
async function loadLists(kinds, force) { await Promise.all(kinds.map((k) => loadList(k, force))); }
function invalidate(...kinds) { kinds.forEach((k) => delete Turks.lists[k]); }

// Counts finished saves, so a read that was already on its way can't undo one.
let cfgSaves = 0;
async function loadConfig() {
  const savesBefore = cfgSaves;
  const cfg = await api("config");
  // A save finished while this was loading; its answer is newer than this one.
  if (cfgSaves !== savesBefore && Turks.cfg) return Turks.cfg;
  // A theme just picked may not be saved yet; don't let a reload undo it.
  if (Turks.pendingTheme) cfg.theme = Turks.pendingTheme;
  Turks.cfg = cfg;
  return Turks.cfg;
}

/** Save some settings; keeps Turks.cfg current. */
async function saveConfig(patch) {
  const r = await act("config", patch);
  if (r) {
    cfgSaves++;
    Turks.cfg = r.config;
    if (r.restart_needed) toast("Restart Turks (System › Status) for the API change to take effect.");
  }
  return r;
}

const byId = (list, id) => (list || []).find((x) => x.id === id);
const tagName = (id) => (byId(Turks.lists.tag, id) || {}).label || "?";
const profileName = (id) => (byId(Turks.lists.profile, id) || {}).name || "—";

// ---------------------------------------------------------------- formatting

function size(bytes) {
  if (!bytes) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0; let n = bytes;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return (i >= 3 ? n.toFixed(1) : Math.round(n)) + " " + u[i];
}
function runtime(min) {
  if (!min) return "";
  if (Turks.cfg && Turks.cfg.runtime_hours_minutes === false) return min + " min";
  return (min >= 60 ? Math.floor(min / 60) + "h " : "") + (min % 60) + "m";
}
function dateStr(d, long) {
  if (!d) return "";
  const dt = new Date(d.length === 10 ? d + "T00:00:00" : d);
  if (isNaN(dt)) return d;
  return dt.toLocaleDateString(undefined, long ? { weekday: "long", year: "numeric", month: "long", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
}
function timeStr(d) {
  if (!d) return "";
  const dt = new Date(d);
  return dt.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: !(Turks.cfg && Turks.cfg.time_24h) });
}
function ago(d) {
  if (!d) return "never";
  const dt = new Date(typeof d === "number" ? d * 1000 : d);
  const s = (Date.now() - dt) / 1000;
  if (Turks.cfg && Turks.cfg.show_relative_dates === false) return dateStr(dt.toISOString()) + " " + timeStr(dt);
  if (s < 0) { const f = -s; return f < 3600 ? "in " + Math.ceil(f / 60) + " min" : f < 86400 ? "in " + Math.round(f / 3600) + " h" : "in " + Math.round(f / 86400) + " days"; }
  if (s < 60) return "just now";
  if (s < 3600) return Math.floor(s / 60) + " min ago";
  if (s < 86400) return Math.floor(s / 3600) + " h ago";
  if (s < 86400 * 30) return Math.floor(s / 86400) + " days ago";
  return dateStr(dt.toISOString());
}
function clock(secs) {
  const h = Math.floor(secs / 3600), m = Math.floor(secs / 60) % 60, s = secs % 60;
  return [h, m, s].map((x) => String(x).padStart(2, "0")).join(":");
}
const AVAIL = [["Announced", "Announced"], ["InCinemas", "In Cinemas"], ["Released", "Released"]];
const STATUS_LABEL = { Tba: "TBA", Announced: "Announced", InCinemas: "In Cinemas", Released: "Released", Deleted: "Deleted" };
function qualityName(id) { const q = (Turks.meta.qualities || []).find((x) => x.id === id); return q ? q.name : "Unknown"; }

// ---------------------------------------------------------------- shell

function showOverlay(title, html) { $("#overlay-title").textContent = title; $("#overlay-text").innerHTML = html; $("#overlay").classList.add("open"); }
function hideOverlay() { $("#overlay").classList.remove("open"); }

function toast(msg, bad) {
  const t = el("div.win.toast" + (bad ? ".bad" : ""), msg);
  $("#toasts").append(t);
  setTimeout(() => t.remove(), bad ? 7000 : 4500);
}

function menuItem(label, selected, onclick, cls = "menu-item") {
  return el("button." + cls + (selected ? ".selected" : ""), { onclick }, el("span.hand", { html: '<svg width="34" height="22"><use href="#hand"/></svg>' }), label);
}

/** Xbox page bar: a tab per menu page along the bottom, so pages switch without the home screen. */
function renderPageBar() {
  const cur = menuOf(Turks.page);
  $("#gtabs").replaceChildren(...MENU.map(([key, label], i) => el("button.gnav", {
    "aria-current": key === cur ? "page" : "false",
    onclick: () => { if (key !== cur || Turks.arg) { Turks.cursor = i; switchPage(key); } },
  }, label)));
}

function renderMenu() {
  const nav = $("#menu");
  nav.classList.toggle("menu-used", Turks.menuUsed);
  nav.replaceChildren(...MENU.map(([key, label]) => menuItem(label, menuOf(Turks.page) === key, () => { Turks.menuUsed = true; go(key); })));
  glassPill();
}

/** Dark Glass: move the menu's selection pill under the open page. The
 * pill is the menu's ::before, so it glides there with a CSS transition. */
function glassPill() {
  const nav = $("#menu");
  if (!nav) return;
  const cur = nav.querySelector(".menu-item.selected");
  nav.classList.toggle("has-sel", !!cur);
  if (!cur) return;
  nav.style.setProperty("--sel-top", cur.offsetTop + "px");
  nav.style.setProperty("--sel-left", cur.offsetLeft + "px");
  nav.style.setProperty("--sel-w", cur.offsetWidth + "px");
  nav.style.setProperty("--sel-h", cur.offsetHeight + "px");
}
addEventListener("resize", glassPill);

/** Switch page. `arg` is page-specific (e.g. a movie id). */
function go(page, arg) {
  Turks.page = page;
  Turks.arg = arg ?? null;
  clearInterval(Turks.pageTimer);
  Turks.pageTimer = null;
  closeModal();
  renderMenu();
  const main = $("#main");
  main.replaceChildren();
  if (isBlades()) bladesShow(page);
  else if (mc) mcShow(page, arg);
  else if (isGreen()) {
    Turks.home = false;
    document.body.classList.remove("ghome-on");
    setGreenTitle((MENU.find((m) => m[0] === menuOf(page)) || [, page])[1]);
    $("#gback-label").textContent = page === "movies" && arg ? "MOVIES" : "BACK";
    drawGrid();
    renderPageBar();
  }
  const p = Turks.pages[page];
  if (!p) return;
  // Each visit renders into its own host, so a slow page that finishes after
  // you've moved on writes into a detached element instead of over the new page.
  const host = el("div.page-host");
  main.append(host);
  Promise.resolve(p.render(host, Turks.arg)).catch((e) => {
    if (e.message !== "auth" && e.message !== "down") host.replaceChildren(el("div.empty", el("h2", "Couldn't load this page"), e.message));
  });
}

/** Re-render the current page in place (keeps arg). */
function rerender() { go(Turks.page, Turks.arg); }

/** Run fn every ms while this page stays open. */
function every(ms, fn) { clearInterval(Turks.pageTimer); Turks.pageTimer = setInterval(() => { if (!document.hidden) fn(); }, ms); }

function renderInfo() {
  const s = Turks.state;
  if (!s) return;
  $("#i-time").textContent = clock(s.uptime_secs);
  $("#i-movies").textContent = s.counts.movies;
  const miss = $("#i-missing");
  miss.textContent = s.counts.missing;
  miss.className = s.counts.missing > 0 ? "bad" : "";
  const free = Math.max(0, ...s.roots.map((r) => r.free || 0));
  $("#i-disk").textContent = free ? size(free) : "—";
  $("#mode").textContent = Turks.cfg && Turks.cfg.advanced_mode ? "MODE: ADVANCED" : "MODE: SIMPLE";

  // Status line: the running command, else a health problem, else a summary.
  const running = s.commands.find((c) => !c.finished);
  const worst = s.health.find((h) => h.level === "error") || s.health.find((h) => h.level === "warning");
  let text, level = "good";
  if (running) { text = running.label + (running.message ? " — " + running.message : "") + "…"; }
  else if (worst) { text = worst.message; level = worst.level === "error" ? "bad" : "warn"; }
  else if (!s.tmdb_ready) { text = "Add your TMDb API key in Settings › General to get started."; level = "warn"; }
  else text = `${s.counts.movies} movies · ${s.counts.queue} downloading · ${s.counts.missing} missing`;
  $("#statusbar .badge").className = "badge " + level;
  $("#status-text").textContent = text;
  $("#status-text").className = level === "good" ? "" : level;
  drawBladesStatus(text, level);
  if (isGreen()) {
    const top = `Turks v${s.version} · ${Turks.cfg && Turks.cfg.advanced_mode ? "Advanced" : "Simple"} mode · up ${clock(s.uptime_secs)}`;
    const box = [el("span.orb" + (level === "good" ? "" : "." + level)), el("div.lines", el("div.l1", top), el("div.l2" + (level === "good" ? "" : "." + level), text))];
    $("#gstatus").replaceChildren(...box.map((n) => n.cloneNode(true)));
    $("#ghome-status").replaceChildren(...box);
    $("#gmode-label").textContent = Turks.cfg && Turks.cfg.advanced_mode ? "ADVANCED" : "SIMPLE";
  }
}

async function poll() {
  try {
    Turks.state = await api("state");
    for (const t of Turks.state.toasts) toast(t);
    renderInfo();
  } catch (e) { /* overlay already shown */ }
}

function startPolling() {
  clearInterval(Turks.pollTimer);
  Turks.pollTimer = setInterval(() => { if (!document.hidden) poll(); }, 3000);
}


// ---------------------------------------------------------------- update progress
// The update (Update now, or the DSM task) writes its progress; this shows it as a
// bar from the download until the new version answers.
const UPD = { target: null, restartAt: 0, finished: false, seenAt: 0, failed: false };
function updShow(title, frac, step, note, bad) {
  const box = $("#upd");
  box.querySelector(".updtitle").textContent = title;
  box.querySelector(".updbar i").style.width = (Math.max(0, Math.min(1, frac)) * 100).toFixed(1) + "%";
  box.querySelector(".updstep").textContent = step;
  box.querySelector(".updpct").textContent = bad ? "" : Math.round(frac * 100) + "%";
  box.querySelector(".updnote").textContent = note || "";
  box.classList.toggle("bad", !!bad);
  box.querySelector(".updact").replaceChildren(bad ? el("button", { onclick: () => { UPD.target = null; UPD.failed = false; updHide(); } }, "OK") : "");
  if (box.hidden) { box.hidden = false; requestAnimationFrame(() => box.classList.add("on")); }
}
function updHide() { const box = $("#upd"); box.classList.remove("on"); setTimeout(() => { if (!box.classList.contains("on")) box.hidden = true; }, 600); }
async function updWatch() {
  if (UPD.finished) return;
  let u = null;
  try { u = await api("update"); } catch (e) { u = null; }
  // The version this page was loaded with. If the first check at load failed,
  // take the first answer we get, as long as no update was in progress then.
  if (u && !UPD.loadedVersion && !UPD.target && !(u.progress && u.progress.to)) UPD.loadedVersion = u.current || null;
  const p = u && u.progress;
  if (p && p.to && UPD.target !== p.to) { UPD.target = p.to; UPD.restartAt = 0; UPD.failed = false; }
  if (p) { UPD.seenAt = Date.now(); UPD.sawProgress = true; }
  if (!UPD.target || UPD.failed) return;
  const title = `Updating Turks to ${UPD.target}`;
  if (u && u.current === UPD.target) {
    UPD.finished = true;
    // Reload only when this page really started on an older version and
    // watched the update happen, and never twice for the same version. Every
    // other case just closes the progress card: the page is already current,
    // or reloading wouldn't change anything.
    const older = UPD.loadedVersion && UPD.loadedVersion !== UPD.target;
    if (!older || !UPD.sawProgress || UPD.reloadedFor === UPD.target || reloadedRecently(UPD.target)) {
      updHide();
      return;
    }
    markReloaded(UPD.target);
    try { sessionStorage.setItem("turks-updated", UPD.target); } catch (e) { /* private mode */ }
    updShow(`Turks ${UPD.target} is installed`, 1, "Done", "Reloading the page…");
    setTimeout(() => location.reload(), 1800);
    return;
  }
  if (!u || (p && p.phase === "install")) {
    if (!UPD.restartAt) UPD.restartAt = Date.now();
    const t = (Date.now() - UPD.restartAt) / 1000;
    updShow(title, 0.8 + 0.17 * (1 - Math.exp(-t / 30)), "Step 3 of 3 · Installing",
      t > 240 ? "It's taking longer than usual. If Turks doesn't come back, check Package Center." : "Turks is restarting on the new version. This page reloads by itself when it's back.");
  } else if (p && p.phase === "verify") {
    updShow(title, 0.74, "Step 2 of 3 · Checking the download", "");
  } else if (p) {
    const f = p.total ? Math.min(1, p.done / p.total) : 0;
    updShow(title, 0.02 + 0.68 * f, p.total ? `Step 1 of 3 · Downloading ${(p.done / 1048576).toFixed(1)} of ${(p.total / 1048576).toFixed(1)} MB` : "Step 1 of 3 · Downloading", "Turks keeps running while it downloads.");
  } else if (u.last_install && !u.last_install.ok && u.last_install.to === UPD.target) {
    UPD.failed = true;
    updShow("The update didn't work", 0, "", u.last_install.message + " Turks keeps running the version you have.", true);
  } else if (Date.now() - UPD.seenAt > 20000) { UPD.target = null; updHide(); }
}
// Remembered in the browser (not just this tab) for 30 minutes, so closing
// and reopening the window can't start a reload loop either.
function reloadedRecently(v) {
  try { const r = JSON.parse(localStorage.getItem("turks-reloaded") || "null"); return !!(r && r.v === v && Date.now() - r.at < 30 * 60 * 1000); } catch (e) { return false; }
}
function markReloaded(v) {
  try { localStorage.setItem("turks-reloaded", JSON.stringify({ v, at: Date.now() })); } catch (e) { /* private mode */ }
}
/** Update now was pressed: show the card right away and follow the update
 * closely. Small downloads finish in seconds, faster than the regular check,
 * so without this the window could miss the whole update. */
function updStart(version) {
  Object.assign(UPD, { target: version, restartAt: 0, finished: false, failed: false, seenAt: Date.now(), sawProgress: true });
  updShow(`Updating Turks to ${version}`, 0.02, "Step 1 of 3 · Starting the download", "Turks keeps running while it downloads.");
  clearInterval(UPD.fast);
  UPD.fast = setInterval(() => { if (UPD.finished || !UPD.target) { clearInterval(UPD.fast); return; } updWatch(); }, 1000);
}
async function startUpdateWatch() {
  try { const v = sessionStorage.getItem("turks-updated"); if (v) { sessionStorage.removeItem("turks-updated"); UPD.reloadedFor = v; setTimeout(() => toast(`Turks updated to ${v}.`), 800); } } catch (e) { /* private mode */ }
  // The version this page was loaded with, so an update that's already in
  // place never triggers a reload.
  try { UPD.loadedVersion = (await api("update")).current || null; } catch (e) { UPD.loadedVersion = null; }
  updWatch();
  setInterval(() => { if (!document.hidden || UPD.target) updWatch(); }, 2500);
}

// ---------------------------------------------------------------- modals

let modalStack = [];

/** Opens a dialog. Returns {close, win, body}. */
function openModal({ title, body, buttons = [], size: sz = "", onclose }) {
  const bodyEl = el("div.body", body);
  let closeBtn = null;
  if (!buttons.length) { closeBtn = el("button", "Close"); buttons = [closeBtn]; }
  const footer = el("div.footer", el("span.grow"), ...buttons);
  const win = el("div.win" + (sz ? "." + sz : ""), el("div.page-head", el("h2", title)), el("div.divider"), bodyEl, footer);
  const m = $("#modal");
  const entry = { win, onclose };
  modalStack.push(entry);
  m.replaceChildren(win);
  m.classList.add("open");
  const close = () => {
    const i = modalStack.indexOf(entry);
    if (i < 0) return;
    modalStack.splice(i, 1);
    if (entry.onclose) entry.onclose();
    if (modalStack.length) m.replaceChildren(modalStack[modalStack.length - 1].win);
    else { m.classList.remove("open"); m.replaceChildren(); }
  };
  entry.close = close;
  if (closeBtn) closeBtn.onclick = close;
  return { close, win, body: bodyEl, footer };
}
function closeModal() { while (modalStack.length) modalStack[modalStack.length - 1].close(); }
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && modalStack.length) { modalStack[modalStack.length - 1].close(); e.preventDefault(); return; }
  if (isGreen()) greenKey(e);
  else if (isBlades()) bladesKey(e);
});

function confirmBox(title, text, okLabel = "OK", danger = false, extra = null) {
  return new Promise((resolve) => {
    let done = false;
    const ok = el("button.big" + (danger ? ".danger" : ".primary"), { onclick: () => { done = true; m.close(); resolve(true); } }, okLabel);
    const cancel = el("button", { onclick: () => m.close() }, "Cancel");
    const m = openModal({ title, body: [el("p", { html: text }), extra], buttons: [cancel, ok], size: "narrow", onclose: () => { if (!done) resolve(false); } });
  });
}

/** Folder picker for NAS folders. Resolves the chosen path or null. */
function pickFolder(start, title = "Choose a folder", chooseLabel = "Choose this folder") {
  return new Promise((resolve) => {
    let current = start || "";
    let done = false;
    const list = el("div.picker-list");
    const crumb = el("div.crumb");
    const note = el("div.notice.hidden");
    const up = el("button", "Up");
    const choose = el("button.big.primary", chooseLabel);
    const cancel = el("button", "Cancel");
    const m = openModal({ title, body: [crumb, list, note], buttons: [up, cancel, choose], onclose: () => { if (!done) resolve(null); } });
    m.footer.prepend(up);
    cancel.onclick = () => m.close();
    choose.onclick = () => { if (!current) return; done = true; m.close(); resolve(current); };
    async function show(path) {
      list.replaceChildren(el("div.dim", "Loading…"));
      let r;
      try { r = await api("fs", undefined, { path }); } catch (e) { list.replaceChildren(el("div.bad", e.message)); return; }
      current = r.path;
      crumb.textContent = r.path || "Shared folders";
      up.disabled = r.parent === null || r.parent === undefined;
      up.onclick = () => show(r.parent || "");
      choose.disabled = !r.path;
      list.replaceChildren(...(r.entries.length ? r.entries.map((e) => el("div.picker-item", { onclick: () => show(e.path) },
        el("span", "📁"), el("span", e.name), !e.readable ? el("span.bad.small", "no access") : !e.writable ? el("span.warn.small", "read-only") : null,
        e.volume ? el("span.vol", e.volume) : null)) : [el("div.dim", "No folders here")]));
      const here = r.entries.length ? null : null;
      note.classList.toggle("hidden", !here);
    }
    show(current);
  });
}

// ---------------------------------------------------------------- small widgets

function toggleBtn(on, onchange) {
  const b = el("button.toggle." + (on ? "on" : "off"), on ? "ON" : "OFF");
  b.onclick = () => { on = !on; b.textContent = on ? "ON" : "OFF"; b.className = "toggle " + (on ? "on" : "off"); onchange(on); };
  return b;
}
function check(label, on, onchange) {
  const input = el("input", { type: "checkbox", checked: on, onchange: () => onchange(input.checked) });
  return el("label.check", input, el("span.box"), label);
}
function select(options, value, onchange, cls) {
  const s = el("select" + (cls ? "." + cls : ""), { onchange: () => onchange(s.value) },
    options.map(([v, l]) => el("option", { value: v, selected: String(v) === String(value) }, l)));
  s.value = String(value);
  return s;
}
function pill(label, on, onclick) { return el("button.pill" + (on ? ".on" : ""), { onclick }, label); }
function hp(frac, full) { return el("span.hp" + (full ? ".full" : ""), el("i", { style: { width: Math.max(0, Math.min(1, frac)) * 100 + "%" } })); }

/** A button that opens a small menu. items: [label, onclick, cssClass?] or null for a divider. */
function menuButton(label, items) {
  const list = el("div.menu.hidden", items.map((it) => (it ? el("button" + (it[2] ? "." + it[2] : ""), { onclick: () => { list.classList.add("hidden"); it[1](); } }, it[0]) : el("div.divider"))));
  const btn = el("button", { onclick: (e) => { e.stopPropagation(); list.classList.toggle("hidden"); } }, label + " ▾");
  document.addEventListener("click", () => list.classList.add("hidden"));
  return el("span.menu-wrap", btn, list);
}

/** A settings row: name + help on the left, control on the right. */
function field(name, help, control, stack) {
  return el("div.field" + (stack ? ".stack" : ""), el("div.lbl", el("div.name", name), help ? el("div.help", help) : null), el("div.ctl", control));
}

/** Tag picker: chips with remove buttons, a select to add, "new tag". */
function tagPicker(ids, onchange) {
  const wrap = el("div.chips");
  const draw = () => {
    const tags = Turks.lists.tag || [];
    wrap.replaceChildren(
      ...ids.map((id) => el("span.chip", tagName(id), el("button", { onclick: () => { ids = ids.filter((x) => x !== id); onchange(ids); draw(); } }, "×"))),
      select([["", "+ tag"], ...tags.filter((t) => !ids.includes(t.id)).map((t) => [t.id, t.label]), ["new", "New tag…"]], "", async (v) => {
        if (v === "new") {
          const label = prompt("New tag name");
          if (label && label.trim()) {
            const t = await act("save", { id: 0, label: label.trim().toLowerCase() }, { kind: "tag" });
            if (t) { invalidate("tag"); await loadList("tag"); ids = [...ids, t.id]; onchange(ids); }
          }
        } else if (v) { ids = [...ids, Number(v)]; onchange(ids); }
        draw();
      }),
    );
  };
  draw();
  return wrap;
}

// ---------------------------------------------------------------- Blades theme
// Glossy silver blades; the open one wears its page's color. The page itself
// (#main) moves into the open blade, so every page works as before.
// Same look as Lifestream's NAS page.

const isBlades = () => !!(Turks.cfg && Turks.cfg.theme === "blades");
const isGlass = () => !!(Turks.cfg && Turks.cfg.theme === "glass");
let bstage = null, bcur = 0, bdrag = null;
const BARROW = (d) => `<svg aria-hidden="true" width="16" height="16" viewBox="0 0 16 16"><path d="${d}" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

function bladesBuild() {
  if (bstage) return;
  bstage = el("div#bstage", { "aria-label": "Blades" });
  document.body.insertBefore(bstage, $(".layout"));
  const nav = (dir, label, d) => { const b = el("button.bnav", { "data-dir": dir, "aria-label": label, onclick: () => bladesStep(dir) }); b.innerHTML = BARROW(d); return b; };
  MENU.forEach(([key, label]) => {
    const band = el("div.bband",
      el("h1", label),
      el("span.bchip", { title: "Open System", onclick: () => { if (Turks.page !== "system") go("system"); } }, el("span.bdot"), el("span.btxt")),
      el("span.bspeed"));
    const foot = el("div.bfoot",
      el("span.bkey.bhint", "← →"), el("span.bhint", "Blades"),
      el("span.bkey.bhint", "ENTER"), el("span.bhint", "Select"),
      el("span.bkey.bhint", "ESC"), el("span.bhint", "Back"),
      el("span.bnavs", nav(-1, "Previous blade", "M10 3 L5 8 L10 13"), nav(1, "Next blade", "M6 3 L11 8 L6 13")));
    bstage.append(el("section.bsec.k-" + key, { "aria-label": label },
      el("button.btab", { "aria-label": label, onclick: () => { if (menuOf(Turks.page) !== key) go(key); } }, el("span.blabel", label.toLowerCase())),
      el("div.bwrap", el("div.bpanel", band, el("div.bbody"), foot))));
  });
  bstage.append(el("div.bedge", { "aria-hidden": "true" }));
  bcontour = bladesContour(bstage, ".bsec.open .bbody", ".bpanel");
  // Swipe sideways: anywhere with a finger, or from a tab or the top band with the mouse.
  bstage.addEventListener("pointerdown", (e) => { bdrag = { x: e.clientX, y: e.clientY, ok: e.pointerType === "touch" || !!e.target.closest(".btab, .bband") }; });
  bstage.addEventListener("pointerup", (e) => {
    if (!bdrag || !bdrag.ok) { bdrag = null; return; }
    const dx = e.clientX - bdrag.x, dy = e.clientY - bdrag.y; bdrag = null;
    if (Math.abs(dx) > 70 && Math.abs(dy) < 60) bladesStep(dx < 0 ? 1 : -1);
  });
}

// Blades: rows follow the open blade"s curved left edge. Each row shifts
// right by how far the curve comes in at its height (the most it comes in
// over the row"s span), so nothing runs into the curve near the top and
// bottom. Re-run on scroll, resize and page changes.
function bladesContour(stage, bodySel, panelSel) {
  let queued = false, shifted = new Set();
  const visible = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  // Children, looking through display: contents wrappers (they have no box).
  const kidsOf = (el) => [...el.children].flatMap((c) => (getComputedStyle(c).display === 'contents' ? kidsOf(c) : visible(c) ? [c] : []));
  const pick = (el, out) => {
    if (!visible(el)) return;
    const h = el.getBoundingClientRect().height;
    const kids = kidsOf(el);
    const t0 = kids.length ? kids[0].getBoundingClientRect().top : 0;
    const stacked = kids.length > 1 && kids.some((k) => Math.abs(k.getBoundingClientRect().top - t0) > 4);
    if (h > 90 && (stacked || kids.length === 1)) kids.forEach((k) => pick(k, out));
    else if (h > 90 && kids.length > 1) {
      // Side-by-side columns: only the leftmost one meets the curve.
      const left = kids.reduce((a, b) => (b.getBoundingClientRect().left < a.getBoundingClientRect().left ? b : a));
      pick(left, out);
    } else out.push(el);
  };
  const run = () => {
    queued = false;
    const open = stage.querySelector(bodySel);
    const panel = open && open.closest(panelSel);
    const next = new Set();
    if (open && panel) {
      const pr = panel.getBoundingClientRect(), br = open.getBoundingClientRect();
      const r = parseFloat(getComputedStyle(stage).getPropertyValue("--r")) || 44;
      const mid = pr.top + pr.height / 2, half = pr.height / 2;
      const arc = (y) => { const d = Math.max(-1, Math.min(1, (y - mid) / half)); return r * (1 - Math.sqrt(1 - d * d)); };
      const rows = [];
      [...open.children].forEach((c) => pick(c, rows));
      for (const el of rows) {
        shiftRow(el, 0);
        const er = el.getBoundingClientRect();
        const top = Math.max(er.top, br.top), bot = Math.min(er.bottom, br.bottom);
        if (bot <= top) continue;
        const dx = Math.round(Math.max(arc(top), arc(bot)));
        if (dx > 0) { shiftRow(el, dx); next.add(el); }
      }
    }
    shifted.forEach((el) => { if (!next.has(el)) shiftRow(el, 0); });
    shifted = next;
  };
  const queue = () => { if (!queued) { queued = true; requestAnimationFrame(run); } };
  stage.addEventListener("scroll", queue, true);
  addEventListener("resize", queue);
  stage.addEventListener("transitionend", queue);
  const mo = new MutationObserver(queue);
  mo.observe(stage, { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "hidden", "open"] });
  queue();
  return {
    refresh: queue,
    stop: () => {
      mo.disconnect();
      stage.removeEventListener("scroll", queue, true);
      removeEventListener("resize", queue);
      stage.removeEventListener("transitionend", queue);
      shifted.forEach((el) => { shiftRow(el, 0); });
      shifted.clear();
    },
  };
}

// Moves a row right by dx and narrows it by the same amount, so its right
// end stays where it was instead of running past the blade's edge.
function shiftRow(el, dx) {
  el.style.translate = dx ? `${dx}px 0` : "";
  el.style.maxWidth = dx ? `calc(100% - ${dx}px)` : "";
}

let bcontour = null;

function bladesPlace(instant) {
  if (!bstage) return;
  const W = innerWidth, narrow = W < 700, p = narrow ? 30 : 52, r = narrow ? 20 : 44, edge = narrow ? 14 : 30, n = MENU.length;
  const ms = instant || calm() ? 0 : 700;
  bstage.style.cssText = `--p:${p}px;--r:${r}px;--edge:${edge}px;--bw:${W - edge - (n - 1) * p + r}px;--ms:${ms}ms`;
  $$(".bsec", bstage).forEach((s, i) => {
    s.style.transform = `translateX(${i <= bcur ? i * p : W - edge - (n - i) * p}px)`;
    s.style.zIndex = 10 + i;
    s.classList.toggle("open", i === bcur);
    $(".btab", s).tabIndex = i === bcur ? -1 : 0;
    $(".bwrap", s).inert = i !== bcur;
  });
  $$(".bnav", bstage).forEach((b) => { b.disabled = (+b.dataset.dir < 0 && bcur === 0) || (+b.dataset.dir > 0 && bcur === n - 1); });
}

/** Open the blade for `page`: #main moves into it and the blades slide. */
function bladesShow(page, instant) {
  bladesBuild();
  const i = Math.max(0, MENU.findIndex((m) => m[0] === menuOf(page)));
  const slot = $$(".bbody", bstage)[i], main = $("#main");
  if (main.parentNode !== slot) slot.append(main);
  bcur = i;
  setBladeTitle(MENU[i][1]);
  bladesPlace(instant);
  renderInfo();
  if (bcontour) bcontour.refresh();
}

function bladesOff() {
  if (bcontour) bcontour.stop();
  const main = $("#main"), layout = $(".layout");
  if (main && layout && main.parentNode !== layout) layout.insertBefore(main, $(".side"));
}

function setBladeTitle(text) {
  if (!bstage) return;
  const h = $$(".bsec", bstage)[bcur];
  if (h) $(".bband h1", h).textContent = text;
}

function bladesStep(d) {
  const j = Math.max(0, Math.min(MENU.length - 1, bcur + d));
  if (j !== bcur) go(MENU[j][0]);
}

function drawBladesStatus(text, level) {
  if (!isBlades() || !bstage) return;
  const chip = level === "good" ? "All good" : level === "bad" ? "Problem" : "Needs a look";
  $$(".bband", bstage).forEach((b) => {
    $(".btxt", b).textContent = chip;
    $(".bdot", b).className = "bdot " + (level === "good" ? "" : level);
    $(".bspeed", b).textContent = text;
    $(".bspeed", b).title = text;
  });
}

/** Esc / mouse back in Blades: a dialog, then movie details, then a blade. */
function bladesKey(e) {
  if (e.altKey || e.ctrlKey || e.metaKey) return;
  const tag = (document.activeElement || {}).tagName || "";
  if (/^(INPUT|TEXTAREA|SELECT)$/.test(tag)) return;
  if (e.key === "ArrowRight") { e.preventDefault(); bladesStep(1); }
  else if (e.key === "ArrowLeft") { e.preventDefault(); bladesStep(-1); }
  else if (e.key === "Escape") {
    if (Turks.page === "movies" && Turks.arg) { e.preventDefault(); go("movies"); }
    else if (bcur > 0) { e.preventDefault(); go(MENU[0][0]); }
  } else if (e.key === "Enter" && (!tag || tag === "BODY")) {
    const first = $(".bsec.open .bbody button, .bsec.open .bbody input, .bsec.open .bbody select", bstage);
    if (first) { e.preventDefault(); first.focus(); }
  }
}
for (const type of ["mousedown", "mouseup"]) {
  window.addEventListener(type, (e) => {
    if (e.button !== 3 || !isBlades()) return;
    e.preventDefault();
    if (type !== "mouseup") return;
    if (modalStack.length) return modalStack[modalStack.length - 1].close();
    if (Turks.page === "movies" && Turks.arg) return go("movies");
    bladesStep(-1);
  });
}
addEventListener("resize", () => { if (isBlades()) bladesPlace(true); });

// ---------------------------------------------------------------- Xbox theme

const isGreen = () => !!(Turks.cfg && Turks.cfg.theme === "green");
Turks.home = false;
Turks.cursor = 0;

/** Switch looks to match Turks.cfg.theme. `stay` keeps the current page open. */

// ---------------------------------------------------------------- MC360 look
// The MC360 dashboard skin's blades, one per menu section, each with a menu
// of that section's pages and tabs. Picking one opens it on the blade (the
// same page as in every other look). The engine lives in mc360/.

let mc = null, mcLoading = null, mcPick = null;
const isMc360 = () => !!(Turks.cfg && Turks.cfg.theme === "mc360");
const MC_LOOKS = [["ff", "FFVII Menu"], ["green", "Xbox"], ["blades", "Blades"], ["glass", "Dark Glass"], ["mc360", "MC360"]];
const MC_SETTINGS_ICON = { media: ["folder", "Naming, folders, hardlinks."], indexers: ["search", "Synced from Materia."], clients: ["plug", "Lifestream connection."], connect: ["bell", "Where alerts go."], tags: ["tag", "Tags and what uses them."], general: ["gear", "Port, sign-in, backups."], ui: ["palette", "Look; MC360's skin and sounds are in the guide (G)."] };
function mcBlades() {
  const adv = Turks.cfg && Turks.cfg.advanced_mode;
  return [
    { key: "movies", color: "blue", items: [{ id: "movies:", label: "Library", icon: "film", hint: "Your movies." }] },
    { key: "add movie", color: "green", items: [
      { id: "add:", label: "Search", icon: "search", hint: "Find a movie by name." },
      { id: "discover:", label: "Discover", icon: "eye", hint: "Popular and upcoming." },
      { id: "collections:", label: "Collections", icon: "apps", hint: "Fill in film series." }] },
    { key: "calendar", color: "orange", items: [{ id: "calendar:", label: "Calendar", icon: "cal", hint: "Releases by day." }] },
    { key: "activity", color: "red", items: [
      { id: "activity:queue", label: "Queue", icon: "queue", hint: "Downloads in progress." },
      { id: "activity:history", label: "History", icon: "clock", hint: "Grabs and imports." },
      { id: "activity:blocklist", label: "Blocklist", icon: "block", hint: "Releases that won't be grabbed again." }] },
    { key: "settings", color: "purple", items: SECTIONS.filter(([, , a]) => adv || !a).map(([k, l]) => ({ id: "settings:" + k, label: l, icon: (MC_SETTINGS_ICON[k] || ["gear"])[0], hint: (MC_SETTINGS_ICON[k] || [, l + " settings."])[1] })) },
    { key: "system", color: "grey", items: [["status", "Status", "info", "Health checks and versions."], ["tasks", "Tasks", "task", "Scheduled jobs, with Run now."], ["backup", "Backup", "disk", "Make and restore backups."], ["updates", "Updates", "refresh", "Check now and Update now."], ["events", "Events", "bell", "What the app has been doing."], ["logs", "Log", "doc", "Read the log."], ["parse", "Parse", "search", "Test how a name is read."]].map(([k, l, i, h]) => ({ id: "system:" + k, label: l, icon: i, hint: h })) },
  ];
}
/** The blade and menu entry for a page and its tab. */
function mcLocate(page, arg) {
  const bl = mcBlades();
  const b = Math.max(0, MENU.findIndex((m) => m[0] === menuOf(page)));
  const ids = bl[b].items.map((it) => it.id);
  const tab = typeof arg === "string" ? arg : page === "settings" ? store.get("settingsSection", "media") : "";
  const id = ids.find((x) => x === page + ":" + tab) || ids.find((x) => x.startsWith(page + ":")) || ids[0];
  return { b, id };
}
function mcVersion() { const t = $('script[src^="app.js"]'); return t ? new URL(t.src, location.href).searchParams.get("v") || "" : ""; }
function mcLoad() {
  if (window.MC360) return Promise.resolve();
  if (mcLoading) return mcLoading;
  document.head.append(el("link", { rel: "stylesheet", href: "mc360/mc360.css?v=" + mcVersion() }));
  mcLoading = new Promise((resolve, reject) => { const js = el("script", { src: "mc360/mc360.js?v=" + mcVersion() }); js.onload = resolve; js.onerror = reject; document.head.append(js); });
  return mcLoading;
}
async function mcOn() {
  if (mc) return;
  try { await mcLoad(); } catch (e) { toast("The MC360 look couldn't load its files.", true); return; }
  if (mc || !isMc360()) return;
  mc = MC360.mount({
    base: "mc360/",
    app: { name: "Turks", plat: "NAS", ver: mcVersion(), icon: "film" },
    blades: mcBlades(),
    card: () => {
      const s = Turks.state;
      if (!s) return ["Connecting…"];
      return [`${s.counts.movies} movies · ${s.counts.missing} missing`, `${s.counts.queue || 0} in queue`, $("#status") ? $("#status").textContent : ""];
    },
    host: () => $("#main"),
    onOpen: (b, item) => { const [page, tab] = item.id.split(":"); mcPick = item.id; go(page, tab || null); },
    beforeBack: () => { if (Turks.page === "movies" && Turks.arg) { go("movies"); return true; } return false; },
    popupOpen: () => modalStack.length > 0 || !!document.querySelector(".overlay-msg.open"),
    looks: MC_LOOKS.map(([, l]) => l),
    setLook: async (label) => {
      const v = (MC_LOOKS.find(([, l]) => l === label) || ["ff"])[0];
      if (v === Turks.cfg.theme) return;
      Turks.pendingTheme = v; Turks.cfg.theme = v;
      try { localStorage.setItem("turks-theme", v); } catch (e) {}
      applyTheme(true);
      const r = await saveConfig({ theme: v });
      if (Turks.pendingTheme === v) Turks.pendingTheme = null;
      if (!r) toast("Couldn't save the theme. It's on for now, but Turks will open in the old one next time. Try again in a moment.", true);
    },
    checkUpdates: async () => {
      go("system", "updates");
      const b = [...document.querySelectorAll("#main button")].find((x) => /Check/i.test(x.textContent));
      if (b) b.click();
      return "Checking: see System > Updates";
    },
  });
  mc.host.classList.add("bbody");
  const { b } = mcLocate(Turks.page, Turks.arg);
  // Start on the menu of the blade the current page belongs to; Enter opens the page.
  mc.home(b);
}
function mcOff() { if (mc) { mc.destroy(); mc = null; } }
function mcShow(page, arg) {
  if (!mc) return;
  mc.setBlades(mcBlades());
  const loc = mcLocate(page, arg);
  mc.showPage(loc.b, mcPick && mcPick.startsWith(page + ":") ? mcPick : loc.id);
  mcPick = null;
}

function applyTheme(stay) {
  const green = isGreen(), blades = isBlades();
  document.body.classList.toggle("green", green);
  document.body.classList.toggle("blades", blades);
  document.body.classList.toggle("glass", isGlass());
  requestAnimationFrame(glassPill);
  if (isMc360()) {
    document.body.classList.remove("ghome-on");
    Turks.home = false;
    bladesOff();
    if (stay) go(Turks.page, Turks.arg);
    mcOn();
    return;
  }
  mcOff();
  if (blades) {
    document.body.classList.remove("ghome-on");
    Turks.home = false;
    bladesShow(Turks.page, true);
    if (stay) go(Turks.page, Turks.arg);
    return;
  }
  bladesOff();
  if (!green) {
    document.body.classList.remove("ghome-on");
    Turks.home = false;
    if (stay) go(Turks.page, Turks.arg);
    return;
  }
  if (stay) { Turks.home = false; go(Turks.page, Turks.arg); } else goHome();
  renderInfo();
}

function setGreenTitle(text) {
  const t = $("#gtitle");
  t.textContent = text;
  // Long movie titles shrink to fit the bar.
  let size = 26;
  t.style.fontSize = size + "px";
  while (size > 13 && t.scrollWidth > t.clientWidth) { size--; t.style.fontSize = size + "px"; }
}

function goHome() {
  closeModal();
  clearInterval(Turks.pageTimer);
  Turks.home = true;
  const i = MENU.findIndex((m) => m[0] === menuOf(Turks.page));
  Turks.cursor = i < 0 ? 0 : i;
  document.body.classList.add("ghome-on");
  renderHome();
}

/** Esc / mouse back: close the top-most thing first, then go home. */
function stepBack() {
  if (modalStack.length) return modalStack[modalStack.length - 1].close();
  if (Turks.home) return;
  if (Turks.page === "movies" && Turks.arg) return go("movies");
  zoomHome();
}

// Opening a page from home: the orb drifts a little closer while everything
// eases into a dark veil, then the page fades in out of it. BACK plays it
// reversed. No bright flashes (the user asked for slow, gentle fades only).
const ZOOM_MS = 600, REVEAL_MS = 500;
let zooming = false;
const calm = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

function glowLayer() {
  let g = $("#gzoom");
  if (!g) { g = el("div#gzoom"); document.body.append(g); }
  return g;
}

/** Fades the dark veil from one opacity to another. */
function fade(from, to, ms) {
  const g = glowLayer();
  g.getAnimations().forEach((a) => a.cancel());
  return g.animate([{ opacity: from }, { opacity: to }], { duration: ms, easing: "ease-in-out", fill: "forwards" }).finished;
}

const orbFrames = (from, to) => [{ transform: `translate(-50%, -50%) scale(${from})` }, { transform: `translate(-50%, -50%) scale(${to})` }];

async function openFromHome(key) {
  const orb = $("#bigorb");
  if (zooming) return;
  if (!isGreen() || !Turks.home || !orb || calm()) return go(key);
  zooming = true;
  orb.animate(orbFrames(1, 1.6), { duration: ZOOM_MS, easing: "ease-in-out", fill: "forwards" });
  await fade(0, 0.96, ZOOM_MS);
  orb.getAnimations().forEach((a) => a.cancel());
  go(key);
  await fade(0.96, 0, REVEAL_MS);
  zooming = false;
}

async function zoomHome() {
  if (zooming) return;
  if (calm()) return goHome();
  zooming = true;
  await fade(0, 0.96, REVEAL_MS);
  goHome();
  const orb = $("#bigorb");
  if (orb) orb.animate(orbFrames(1.6, 1), { duration: ZOOM_MS, easing: "ease-in-out" });
  await fade(0.96, 0, ZOOM_MS);
  zooming = false;
}

/** Page bar: a short soft fade between pages. */
async function switchPage(key) {
  if (zooming) return;
  if (calm()) return go(key);
  zooming = true;
  await fade(0, 0.7, 200);
  go(key);
  await fade(0.7, 0, 300);
  zooming = false;
}

function greenKey(e) {
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement || {}).tagName || "");
  if (typing) return; // Esc in a text box is the browser's.
  if (Turks.home) {
    const n = MENU.length;
    if (e.key === "ArrowDown") Turks.cursor = (Turks.cursor + 1) % n;
    else if (e.key === "ArrowUp") Turks.cursor = (Turks.cursor + n - 1) % n;
    else if (e.key === "Enter") return openFromHome(MENU[Turks.cursor][0]);
    else return;
    e.preventDefault();
    markCursor();
  } else if (e.key === "Escape") {
    e.preventDefault();
    stepBack();
  }
}

// The mouse's side "back" button.
for (const type of ["mousedown", "mouseup"]) {
  window.addEventListener(type, (e) => {
    if (e.button !== 3 || !isGreen()) return;
    e.preventDefault();
    if (type === "mouseup") stepBack();
  });
}

function markCursor() { $$(".gmenu-item").forEach((x, i) => { x.classList.toggle("lit", i === Turks.cursor); $(".node", x).classList.toggle("lit", i === Turks.cursor); }); }

/** Home screen: the big orb with the menu on an arc around it. */
function renderHome() {
  const W = window.innerWidth, H = window.innerHeight;
  const r = Math.max(90, Math.min(H * 0.24, W * 0.16));
  const ox = W * 0.3, oy = H / 2 - 24;
  const orb = $("#bigorb");
  orb.style.cssText = `left:${ox}px;top:${oy}px;width:${2 * r}px;height:${2 * r}px;--r:${r}px`;
  orb.innerHTML = `<div class="shell"></div><div class="spin"></div><div class="jelly"></div><div class="core"></div>
    <div class="lobe" style="left:${50 - 21 - 25}%;top:${50 + 12 - 15}%;width:42%;height:30%"></div>
    <div class="lobe" style="left:${50 + 27 - 16}%;top:${50 - 5 - 22}%;width:32%;height:45%;animation-duration:9s"></div>
    <div class="lobe" style="left:${50 + 5 - 25}%;top:${50 + 31 - 11}%;width:50%;height:22%;animation-duration:11s"></div>
    <svg viewBox="${-r} ${-r} ${2 * r} ${2 * r}" fill="none" stroke-linecap="round">
      ${(() => {
        // A Turk's sunglasses.
        const S = r * 0.46, f = (x, y) => `${(x * S).toFixed(1)},${(y * S).toFixed(1)}`;
        const lens = [[-1.05, -0.3], [-0.14, -0.3], [-0.22, 0.16], [-0.42, 0.3], [-0.86, 0.3], [-1.0, 0.12]];
        const sw = Math.max(1.2, r * 0.02), pale = "rgba(255,255,230,.9)";
        const one = (k) => `<polygon points="${lens.map(([x, y]) => f(x * k, y)).join(" ")}" fill="rgba(255,255,230,.8)" stroke="${pale}" stroke-width="${sw}" stroke-linejoin="round"/>`;
        const glint = (x0, x1) => `<line x1="${x0 * S}" y1="${-0.16 * S}" x2="${x1 * S}" y2="${0.1 * S}" stroke="rgba(70,140,25,.55)" stroke-width="${Math.max(1.5, r * 0.03)}"/>`;
        return one(-1) + one(1) + glint(-0.78, -0.56) + glint(0.42, 0.58)
          + `<line x1="${-1.16 * S}" y1="${-0.33 * S}" x2="${1.16 * S}" y2="${-0.33 * S}" stroke="${pale}" stroke-width="${Math.max(2, r * 0.04)}"/>`
          + `<path d="M${f(-0.14, -0.22)} Q 0,${(-0.06 * S).toFixed(1)} ${f(0.14, -0.22)}" stroke="${pale}" stroke-width="${Math.max(1.5, r * 0.03)}"/>`;
      })()}
    </svg>`;
  const n = MENU.length;
  const rowH = Math.max(40, Math.min(66, (H - 160) / n));
  const nodeR = Math.min(rowH * 0.42, 28);
  const arcR = Math.max(r * 1.5, n * rowH * 0.62);
  const tabW = Math.max(220, Math.min(460, W - ox - arcR - 140));
  $("#gmenu").replaceChildren(...MENU.map(([key, label], i) => {
    const dy = (i - (n - 1) / 2) * rowH;
    const dx = Math.sqrt(Math.max(0, arcR * arcR - dy * dy));
    const item = el("div.gmenu-item", {
      style: { left: ox + dx - nodeR + "px", top: oy + dy - rowH * 0.38 + "px", height: rowH * 0.76 + "px" },
      onmousemove: () => { if (Turks.cursor !== i) { Turks.cursor = i; markCursor(); } },
      onclick: () => openFromHome(key),
    },
      el("span.node", { style: { width: 2 * nodeR + "px", height: 2 * nodeR + "px" } }),
      el("span.wire"),
      el("span.gtab", { style: { width: tabW + "px", fontSize: Math.min(rowH * 0.42, 25) + "px" } }, label));
    return item;
  }));
  markCursor();
  $("#gmode").style.left = Math.min(W * 0.46, 600) + 50 + "px";
  drawGrid(ox, oy);
  renderInfo();
}

/** Background: dark green fade with faint rings and spokes around a hub. */
function drawGrid(hx, hy) {
  if (!isGreen()) return;
  const W = window.innerWidth, H = window.innerHeight;
  if (hx === undefined) { hx = W * 0.2; hy = H * 0.15; }
  const far = Math.max(W, H) * 1.1;
  let rings = "", spokes = "";
  for (let k = 1; k <= 6; k++) { const rx = 230 * k * 0.85 + 60; rings += `<ellipse cx="${hx}" cy="${hy}" rx="${rx}" ry="${rx * 0.92}"/>`; }
  for (let i = 0; i < 12; i++) { const a = (i / 12) * Math.PI * 2 + 0.2; spokes += `<line x1="${hx}" y1="${hy}" x2="${hx + Math.cos(a) * far}" y2="${hy + Math.sin(a) * far}"/>`; }
  const g = $("#gbg");
  g.setAttribute("viewBox", `0 0 ${W} ${H}`);
  g.innerHTML = `<defs><radialGradient id="gfade" gradientUnits="userSpaceOnUse" cx="${hx}" cy="${hy}" r="${far}">
      <stop offset="0" stop-color="#165c10"/><stop offset=".25" stop-color="#0a3608"/><stop offset=".55" stop-color="#041c04"/><stop offset="1" stop-color="#010a01"/></radialGradient></defs>
    <rect width="${W}" height="${H}" fill="url(#gfade)"/>
    <g fill="none" stroke="rgba(120,220,60,0.13)" stroke-width="1.4">${rings}${spokes}</g>`;
}

window.addEventListener("resize", () => { if (!isGreen()) return; if (Turks.home) renderHome(); else { drawGrid(); setGreenTitle($("#gtitle").textContent); } });

// ---------------------------------------------------------------- start

Turks.start = async function () {
  document.addEventListener("visibilitychange", () => { if (!document.hidden) poll(); });
  $("#i-missing-row").onclick = () => { store.set("filter", "missing"); go("movies"); };
  $("#mode").onclick = async () => { await saveConfig({ advanced_mode: !Turks.cfg.advanced_mode }); renderInfo(); if (Turks.page === "settings") rerender(); };
  await getToken();
  try {
    [Turks.meta, Turks.cfg] = await Promise.all([api("meta"), api("config")]);
    await loadLists(["profile", "tag", "root"]);
  } catch (e) {
    if (e.message !== "auth" && e.message !== "down") showOverlay("Couldn't start", esc(e.message));
    setTimeout(Turks.start, 5000);
    return;
  }
  await poll();
  startPolling();
  startUpdateWatch();
  setInterval(() => { if (Turks.state) { Turks.state.uptime_secs++; $("#i-time").textContent = clock(Turks.state.uptime_secs); } }, 1000);
  $("#gback").onclick = () => stepBack();
  $("#genter").onclick = () => openFromHome(MENU[Turks.cursor][0]);
  $("#gmode").onclick = async () => { await saveConfig({ advanced_mode: !Turks.cfg.advanced_mode }); renderInfo(); };
  $("#gstatus").onclick = () => go("system");
  $("#ghome-status").onclick = () => go("system");
  if (!Turks.cfg.tmdb_api_key) { go("settings", "general"); applyTheme(true); }
  else if (isGreen()) { Turks.page = "movies"; applyTheme(false); }
  else { Turks.page = "movies"; applyTheme(true); }
  if (window.splashDone) splashDone(isGlass() ? "glass" : isBlades() ? "blades" : isGreen() ? "green" : "ff");
};
