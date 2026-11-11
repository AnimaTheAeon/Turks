"use strict";
/* Activity (queue, history, blocklist), Wanted and Calendar. */

const DL_COLOR = { Queued: "dim", Downloading: "info", Paused: "warn", Completed: "good", ImportPending: "warn", Importing: "info", Imported: "good", Failed: "bad", Warning: "warn", Delayed: "dim" };
const DL_LABEL = { Queued: "QUEUED", Downloading: "DOWNLOADING", Paused: "PAUSED", Completed: "COMPLETED", ImportPending: "IMPORT PENDING", Importing: "IMPORTING", Imported: "IMPORTED", Failed: "FAILED", Warning: "WARNING", Delayed: "DELAYED" };

function eta(secs) {
  if (secs === null || secs === undefined || secs < 0) return "";
  if (secs < 60) return secs + "s";
  if (secs < 3600) return Math.round(secs / 60) + "m";
  return Math.floor(secs / 3600) + "h " + Math.round((secs % 3600) / 60) + "m";
}

Turks.pages.activity = {
  async render(main, arg) {
    const tab = arg || "queue";
    const body = el("div.scroll");
    const tabs = el("div.tabs");
    const tools = el("span.row", { style: { margin: 0 } });
    main.append(el("div.page", el("div.page-head", el("h1", "Activity"), tabs, el("span.grow"), tools), el("div.divider"), body));
    const s = Turks.state ? Turks.state.counts : { queue: 0, blocklist: 0 };
    tabs.replaceChildren(pill(`Queue ${s.queue || ""}`, tab === "queue", () => go("activity", "queue")), pill("History", tab === "history", () => go("activity", "history")), pill(`Blocklist ${s.blocklist || ""}`, tab === "blocklist", () => go("activity", "blocklist")));
    if (tab === "history") return activityHistory(body, tools);
    if (tab === "blocklist") return activityBlocklist(body, tools);
    return activityQueue(body, tools);
  },
};

async function activityQueue(body, tools) {
  let showDone = false;
  tools.replaceChildren(check("Show imported and failed", false, (v) => { showDone = v; draw(); }),
    el("button", { onclick: () => act("command", { name: "downloads" }, null, "Checking downloads…") }, "Refresh"),
    el("button", { onclick: () => manualImport("") }, "Manual import…"));
  let downloads = [], pending = [], movies = [];
  const draw = () => {
    const list = downloads.filter((d) => showDone || !["Imported", "Failed"].includes(d.state)).sort((a, b) => (b.added || "").localeCompare(a.added || ""));
    const errs = (Turks.state && Turks.state.client_errors) || [];
    const mt = (id) => { const m = movies.find((x) => x.id === id); return m ? el("button.link", { onclick: () => go("movies", m.id) }, m.title) : el("span.dim", "?"); };
    body.replaceChildren(
      ...errs.map((e) => el("div.notice.bad", e)),
      list.length || pending.length ? el("table",
        el("thead", el("tr", el("th", "Status"), el("th", "Movie"), el("th", "Release"), el("th", "Quality"), el("th", "Progress"), el("th", "Time left"), el("th"))),
        el("tbody",
          list.map((d) => {
            const frac = d.size ? 1 - d.remaining / d.size : (d.state === "Completed" || d.state === "Imported" ? 1 : 0);
            return el("tr",
              el("td", el("span.tag." + (DL_COLOR[d.state] || ""), DL_LABEL[d.state] || d.state)),
              el("td", mt(d.movie_id)),
              el("td.wrap.small", d.title, (d.messages || []).map((x) => el("div.warn", x))),
              el("td.nowrap", qualityName(d.quality.quality)),
              el("td.nowrap", hp(frac, frac >= 1), " ", Math.round(frac * 100) + "%", el("div.dim.small", size(d.size - d.remaining) + " / " + size(d.size))),
              el("td.nowrap", eta(d.eta_secs)),
              el("td.nowrap",
                d.state === "ImportPending" || d.state === "Warning" ? el("button.small", { onclick: () => manualImport(d.output_path, d.movie_id) }, "Import…") : null,
                el("button.x", { title: "Remove", onclick: () => removeDownload(d) }, "×")));
          }),
          pending.map((p) => el("tr",
            el("td", el("span.tag.dim", "DELAYED")), el("td", mt(p.movie_id)), el("td.wrap.small", p.release.title),
            el("td"), el("td.small.dim", "waits until " + (p.release_at ? dateStr(p.release_at) + " " + timeStr(p.release_at) : "?")), el("td"),
            el("td", el("button.x", { title: "Drop", onclick: async () => { if (await act("delete", {}, { kind: "pending", id: p.id })) load(); } }, "×")))))) :
        el("div.empty", el("h2", "Nothing is downloading"), "Grabbed releases show up here until they're imported."));
  };
  const load = async () => {
    [downloads, pending, movies] = await Promise.all([api("list", undefined, { kind: "download" }), api("list", undefined, { kind: "pending" }), movies.length ? movies : api("movies")]);
    draw();
  };
  async function removeDownload(d) {
    const o = { remove_from_client: true, blocklist: false, search_again: false };
    const extra = el("div",
      check("Remove from the download client", true, (v) => { o.remove_from_client = v; }), el("br"),
      check("Blocklist this release", false, (v) => { o.blocklist = v; }), el("br"),
      check("Search for another release", false, (v) => { o.search_again = v; }));
    if (await confirmBox("Remove from queue?", esc(d.title), "Remove", true, extra)) if (await act("delete", o, { kind: "download", id: d.id }, "Removed")) load();
  }
  await load();
  every(3000, load);
}

async function activityHistory(body, tools) {
  let event = "", offset = 0;
  const limit = 50;
  const movies = await api("movies");
  const pager = el("span.row", { style: { margin: 0 } });
  tools.replaceChildren(
    select([["", "All events"], ["Grabbed", "Grabbed"], ["Imported", "Imported"], ["DownloadFailed", "Failed"], ["FileDeleted", "Deleted"], ["FileRenamed", "Renamed"], ["Ignored", "Ignored"]], "", (v) => { event = v; offset = 0; load(); }),
    pager,
    el("button.danger", { onclick: async () => { if (await confirmBox("Clear all history?", "Turks forgets what it grabbed and imported. Your files stay.", "Clear", true)) if (await act("history/clear", {})) load(); } }, "Clear"));
  async function load() {
    const r = await api("history", undefined, { offset, limit, event });
    pager.replaceChildren(
      el("button.small", { disabled: offset === 0, onclick: () => { offset = Math.max(0, offset - limit); load(); } }, "‹"),
      el("span.dim.small", r.total ? `${offset + 1}–${Math.min(offset + limit, r.total)} of ${r.total}` : "0"),
      el("button.small", { disabled: offset + limit >= r.total, onclick: () => { offset += limit; load(); } }, "›"));
    body.replaceChildren(r.items.length ? historyTable(r.items, true, movies) : el("div.empty", "No history yet."));
  }
  load();
}

async function activityBlocklist(body, tools) {
  const [list, movies] = await Promise.all([api("list", undefined, { kind: "blocklist" }), api("movies")]);
  tools.replaceChildren(el("button.danger", { disabled: !list.length, onclick: async () => {
    if (!(await confirmBox("Clear the blocklist?", "Every blocked release may be grabbed again.", "Clear", true))) return;
    for (const b of list) await act("delete", {}, { kind: "blocklist", id: b.id });
    rerender();
  } }, "Clear"));
  if (!list.length) { body.append(el("div.empty", el("h2", "Nothing blocked"), "Releases that fail or that you block are kept here so they aren't grabbed again.")); return; }
  body.append(el("table",
    el("thead", el("tr", el("th", "Movie"), el("th", "Release"), el("th", "Quality"), el("th", "Indexer"), el("th", "Date"), el("th"))),
    el("tbody", list.sort((a, b) => (b.date || "").localeCompare(a.date || "")).map((b) => {
      const m = movies.find((x) => x.id === b.movie_id);
      return el("tr",
        el("td", m ? el("button.link", { onclick: () => go("movies", m.id) }, m.title) : ""),
        el("td.wrap.small", b.source_title, b.message ? el("div.dim", b.message) : null),
        el("td.nowrap", qualityName(b.quality.quality)), el("td.small", b.indexer), el("td.nowrap.dim", ago(b.date)),
        el("td", el("button.x", { title: "Unblock", onclick: async () => { if (await act("delete", {}, { kind: "blocklist", id: b.id })) rerender(); } }, "×")));
    }))));
}

// ---------------------------------------------------------------- calendar

Turks.pages.calendar = {
  async render(main, arg) {
    // arg: "YYYY-MM" (month view) or "YYYY-MM-DD" (the week holding that day)
    const week = store.get("calWeek", false);
    const showUnmon = store.get("calUnmonitored", false);
    const now = new Date();
    const iso = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const anchor = arg ? new Date((arg.length === 7 ? arg + "-01" : arg) + "T00:00:00") : now;
    const y = anchor.getFullYear(), mo = anchor.getMonth();
    const movies = (await api("movies")).filter((m) => showUnmon || m.monitored);
    const firstDow = (Turks.cfg && Turks.cfg.first_day_of_week) || 0;
    const start = week ? new Date(y, mo, anchor.getDate()) : new Date(y, mo, 1);
    start.setDate(start.getDate() - ((start.getDay() - firstDow + 7) % 7));
    const events = {};
    for (const m of movies) {
      for (const [k, cls, l] of [["in_cinemas", "cinema", "Cinemas"], ["digital_release", "digital", "Digital"], ["physical_release", "physical", "Disc"]]) {
        if (m[k]) (events[m[k]] = events[m[k]] || []).push({ m, cls, l });
      }
    }
    const dows = Array.from({ length: 7 }, (_, i) => new Date(2023, 0, 1 + ((i + firstDow) % 7)).toLocaleDateString(undefined, { weekday: "short" }));
    const cells = [];
    const today = iso(now);
    // Only as many week rows as the month needs.
    let rows = 1;
    if (!week) { rows = 4; for (let r = 4; r < 6; r++) { const d = new Date(start); d.setDate(start.getDate() + r * 7); if (d.getMonth() === mo) rows = r + 1; } }
    for (let i = 0; i < rows * 7; i++) {
      const d = new Date(start); d.setDate(start.getDate() + i);
      const ds = iso(d);
      cells.push(el("div.day" + (!week && d.getMonth() !== mo ? ".other" : "") + (ds === today ? ".today" : ""), el("div.n", week ? d.toLocaleDateString(undefined, { month: "short", day: "numeric" }) : d.getDate()),
        (events[ds] || []).map((e) => el("div.ev." + e.cls + (e.m.file ? ".has" : ""), { title: `${e.m.title} — ${e.l}${e.m.file ? " · downloaded" : ""}`, onclick: () => go("movies", e.m.id) }, e.m.title))));
    }
    const step = (dir) => { const d = week ? new Date(y, mo, anchor.getDate() + 7 * dir) : new Date(y, mo + dir, 1); return week ? iso(d) : iso(d).slice(0, 7); };
    const end = new Date(start); end.setDate(start.getDate() + 6);
    const title = week ? `${start.toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${end.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`
      : anchor.toLocaleDateString(undefined, { month: "long", year: "numeric" });
    const api_ = Turks.state && Turks.state.api;
    const ical = api_ && api_.enabled && api_.calendar_key ? `http://${location.hostname}:${api_.port}/feed/v3/calendar/turks.ics?key=${api_.calendar_key}` : "";
    main.append(el("div.page",
      el("div.page-head",
        el("button", { onclick: () => go("calendar", step(-1)) }, "‹"), el("button", { onclick: () => go("calendar") }, "Today"), el("button", { onclick: () => go("calendar", step(1)) }, "›"),
        el("h1.keep", title),
        el("div.tabs", pill("Month", !week, () => { store.set("calWeek", false); go("calendar", iso(anchor).slice(0, 7)); }), pill("Week", week, () => { store.set("calWeek", true); go("calendar", iso(arg ? anchor : now)); })),
        check("Unmonitored", showUnmon, (v) => { store.set("calUnmonitored", v); rerender(); }),
        el("span.grow"),
        el("span.small", el("span.warn", "▌"), "Cinemas ", el("span.info", "▌"), "Digital ", el("span", { style: { color: "#c79cff" } }, "▌"), "Disc"),
        ical ? el("button", { title: "Subscribe to this in your phone or desktop calendar", onclick: () => { navigator.clipboard && navigator.clipboard.writeText(ical); openModal({ title: "Calendar link", body: [el("p", "Subscribe to this address in Google Calendar, Outlook or your phone. The link is read-only: it can't change anything in Turks. 'Allow other computers' must be on in Settings › General."), el("input.command", { type: "text", readonly: true, value: ical })], size: "narrow" }); } }, "Copy iCal link") : null),
      el("div.divider"),
      el("div.cal" + (week ? ".week" : ""), { style: { "--rows": rows } }, dows.map((d) => el("div.dow", d)), cells)));
  },
};
