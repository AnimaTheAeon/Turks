"use strict";
/* Turks for DSM — the window. Talks to the Turks service through api.cgi
   with the DSM session (cookie + SynoToken). Pages live in the other .js
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
  ["movies", "Movies"], ["add", "Add Movie"], ["discover", "Discover"], ["collections", "Collections"],
  ["calendar", "Calendar"], ["activity", "Activity"], ["wanted", "Wanted"], ["settings", "Settings"], ["system", "System"],
];

// ---------------------------------------------------------------- API

async function getToken() {
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

async function loadConfig() { Turks.cfg = await api("config"); return Turks.cfg; }

/** Save some settings; keeps Turks.cfg current. */
async function saveConfig(patch) {
  const r = await act("config", patch);
  if (r) {
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

function renderMenu() {
  const nav = $("#menu");
  nav.classList.toggle("menu-used", Turks.menuUsed);
  nav.replaceChildren(...MENU.map(([key, label]) => menuItem(label, Turks.page === key, () => { Turks.menuUsed = true; go(key); })));
}

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
  const p = Turks.pages[page];
  if (!p) return;
  Promise.resolve(p.render(main, Turks.arg)).catch((e) => {
    if (e.message !== "auth" && e.message !== "down") main.replaceChildren(el("div.empty", el("h2", "Couldn't load this page"), e.message));
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
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && modalStack.length) modalStack[modalStack.length - 1].close(); });

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

// ---------------------------------------------------------------- start

Turks.start = async function () {
  document.addEventListener("visibilitychange", () => { if (!document.hidden) poll(); });
  $("#i-missing-row").onclick = () => go("wanted");
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
  setInterval(() => { if (Turks.state) { Turks.state.uptime_secs++; $("#i-time").textContent = clock(Turks.state.uptime_secs); } }, 1000);
  go(Turks.cfg.tmdb_api_key ? "movies" : "settings", Turks.cfg.tmdb_api_key ? null : "general");
};
