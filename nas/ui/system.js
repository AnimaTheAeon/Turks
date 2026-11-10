"use strict";
/* System: status and health, scheduled tasks, backups, updates, events,
   the log, and the release-title parser. */

const TASK_COMMANDS = {
  "Check Downloads": "downloads", "RSS Sync": "rss", "Refresh Movies": "refresh", "Import List Sync": "lists", "Refresh Collections": "collections",
  "Backup": "backup", "Housekeeping": "housekeeping", "Check Health": "health", "Clean Recycle Bin": "recycle",
};

Turks.pages.system = {
  async render(main, arg) {
    const tab = arg || "status";
    const body = el("div.scroll");
    const tools = el("span.row", { style: { margin: 0 } });
    main.append(el("div.page",
      el("div.page-head", el("h1", "System"),
        el("div.tabs", ...[["status", "Status"], ["tasks", "Tasks"], ["backup", "Backup"], ["updates", "Updates"], ["events", "Events"], ["logs", "Log"], ["parse", "Parse"]].map(([k, l]) => pill(l, k === tab, () => go("system", k)))),
        el("span.grow"), tools),
      el("div.divider"), body));
    return ({ status: sysStatus, tasks: sysTasks, backup: sysBackup, updates: sysUpdates, events: sysEvents, logs: sysLogs, parse: sysParse }[tab])(body, tools);
  },
};

async function sysStatus(body, tools) {
  tools.replaceChildren(
    el("button", { onclick: async () => { await act("command", { name: "health" }); setTimeout(rerender, 1500); } }, "Check health"),
    el("button", { onclick: async () => { if (await confirmBox("Restart Turks?", "The service restarts; this window reconnects by itself.", "Restart")) { await act("restart", {}); toast("Restarting…"); setTimeout(() => location.reload(), 6000); } } }, "Restart"));
  await poll();
  const s = Turks.state;
  const lvl = { error: "bad", warning: "warn", notice: "info" };
  body.append(
    el("div.head", "Health"),
    s.health.length ? el("table", el("tbody", s.health.map((h) => el("tr", el("td", el("span.badge." + (lvl[h.level] === "info" ? "good" : lvl[h.level]))), el("td.small.label", h.source), el("td.wrap", h.message)))))
      : el("div.row", el("span.badge.good"), "No problems found."),
    el("div.head", "Disk space"),
    s.roots.length ? el("table", el("tbody", s.roots.map((r) => el("tr", el("td", r.path), el("td.right.nowrap", r.exists ? (r.free ? size(r.free) + " free" : "") : el("span.bad", "missing")))))) : el("div.dim", "No root folders yet."),
    el("div.head", "About"),
    el("div.kv",
      el("span.k", "Version"), el("span", s.version),
      el("span.k", "Running for"), el("span", clock(s.uptime_secs)),
      el("span.k", "Data folder"), el("span.mono", s.data_dir),
      el("span.k", "Library"), el("span", `${s.counts.movies} movies · ${s.counts.files} files · ${size(s.counts.size)}`),
      el("span.k", "API"), el("span", s.api.enabled ? `port ${s.api.port}${s.api.bind_all ? " (network)" : " (this NAS only)"}` : "off"),
      el("span.k", "ffprobe"), el("span", s.ffprobe || el("span.warn", "not found")),
      el("span.k", "Source"), el("span", el("a", { href: "https://github.com/AnimaTheAeon/Turks", target: "_blank" }, "github.com/AnimaTheAeon/Turks"), " · GPLv3, based on Radarr")),
  );
}

async function sysTasks(body) {
  const draw = () => {
    const s = Turks.state;
    body.replaceChildren(el("table",
      el("thead", el("tr", el("th", "Task"), el("th", "Every"), el("th", "Last run"), el("th", "Took"), el("th", "Next"), el("th"))),
      el("tbody", s.tasks.map((t) => {
        const iv = t.interval_secs;
        const every = iv >= 86400 ? Math.round(iv / 86400) + " days" : iv >= 3600 ? Math.round(iv / 3600) + " h" : Math.round(iv / 60) + " min";
        const cmd = TASK_COMMANDS[t.name];
        const running = t.running;
        return el("tr", el("td", t.name), el("td.dim", every), el("td.dim", ago(t.last_run)), el("td.dim", t.last_duration_secs != null ? t.last_duration_secs.toFixed(1) + "s" : ""), el("td.dim", ago(t.next_run)),
          el("td.right", running ? el("span.spin") : cmd ? el("button.small", { onclick: async () => { await act("command", { name: cmd }, null, t.name + " started"); setTimeout(poll, 800); } }, "Run now") : null));
      }))));
  };
  draw();
  every(3000, async () => { await poll(); draw(); });
}

async function sysBackup(body, tools) {
  tools.replaceChildren(el("button.primary", { onclick: async () => { await act("command", { name: "backup" }, null, "Backing up…"); setTimeout(rerender, 2500); } }, "Back up now"));
  const list = await api("backups");
  body.append(el("p.sub", "Folder: ", el("span.mono", Turks.cfg.backup_dir), ". Restoring replaces the library and settings, then restarts Turks."));
  if (!list.length) { body.append(el("div.empty", "No backups yet.")); return; }
  body.append(el("table",
    el("thead", el("tr", el("th", "Backup"), el("th", "Size"), el("th", "Made"), el("th"))),
    el("tbody", list.map((b) => el("tr", el("td", b.name), el("td.nowrap", size(b.size)), el("td.nowrap.dim", ago(b.time)),
      el("td.right.nowrap",
        el("button.small", { onclick: async () => {
          if (!(await confirmBox("Restore this backup?", esc(b.name) + "<br>Everything changed since then is lost.", "Restore", true))) return;
          const r = await act("backup/restore", { path: b.path });
          if (r) { toast(r.message); setTimeout(() => location.reload(), 7000); }
        } }, "Restore"),
        el("button.x", { title: "Delete", onclick: async () => { if (await confirmBox("Delete this backup?", esc(b.name), "Delete", true)) if (await act("backup/delete", { path: b.path })) rerender(); } }, "×")))))));
}

async function sysUpdates(body) {
  await loadConfig();
  const box = el("div");
  body.append(cToggle("auto_update", "Install updates automatically", "The Task Scheduler task below installs new versions when this is on."), box);
  updatePanel(box);
}

async function sysEvents(body) {
  const draw = () => {
    const cmds = Turks.state.commands;
    body.replaceChildren(cmds.length ? el("table",
      el("thead", el("tr", el("th"), el("th", "Command"), el("th", "Result"), el("th", "Started"), el("th", "Took"))),
      el("tbody", cmds.map((c) => {
        const took = c.finished ? ((new Date(c.finished) - new Date(c.started)) / 1000).toFixed(1) + "s" : "";
        return el("tr", el("td", c.finished ? el("span.badge." + (c.failed ? "bad" : "good")) : el("span.spin")), el("td", c.label), el("td.wrap.small" + (c.failed ? ".bad" : ".dim"), c.message), el("td.nowrap.dim", ago(c.started)), el("td.dim", took));
      }))) : el("div.empty", "Nothing has run yet."));
  };
  draw();
  every(2000, async () => { await poll(); draw(); });
}

async function sysLogs(body, tools) {
  let filter = "";
  tools.replaceChildren(
    el("input", { type: "search", placeholder: "Filter", oninput: (e) => { filter = e.target.value.toLowerCase(); load(); } }),
    el("button", { onclick: async () => { if (await act("logs/clear", {})) load(); } }, "Clear"));
  const box = el("div.mono");
  body.append(box);
  async function load() {
    const lines = await api("logs");
    box.replaceChildren(...lines.filter((l) => !filter || l.text.toLowerCase().includes(filter)).reverse().map((l) => el("div.log-line." + l.level, el("span.t", l.time), el("span.x", l.text))));
  }
  load();
  every(4000, load);
}

async function sysParse(body) {
  const out = el("div");
  const input = el("input.wide", { type: "search", placeholder: "Paste a release title, e.g. The.Matrix.1999.2160p.UHD.BluRay.REMUX.HDR.HEVC.Atmos-GROUP", style: { fontSize: "15px" } });
  let t = null;
  input.oninput = () => { clearTimeout(t); t = setTimeout(run, 300); };
  body.append(el("p.sub", "See how Turks reads a release title: the movie, quality, languages, group and custom formats."), input, el("div", { style: { height: "10px" } }), out);
  async function run() {
    const title = input.value.trim();
    if (!title) { out.replaceChildren(); return; }
    const r = await api("parse", undefined, { title }).catch((e) => { out.replaceChildren(el("div.bad", e.message)); return null; });
    if (!r) return;
    const p = r.parsed;
    if (!p) { out.replaceChildren(el("div.notice.bad", "Turks can't read a movie title from that.")); return; }
    out.replaceChildren(el("div.kv",
      el("span.k", "Title"), el("span", (p.titles || []).join(" / ")),
      el("span.k", "Year"), el("span", p.year || "—"),
      el("span.k", "Movie"), el("span", r.movie ? el("button.link", { onclick: () => go("movies", r.movie.id) }, r.movie.title) : el("span.dim", "not in your library")),
      el("span.k", "Quality"), el("span", r.quality_name, p.quality.revision.version > 1 ? ` (v${p.quality.revision.version})` : "", p.quality.revision.real > 0 ? " REAL" : ""),
      el("span.k", "Languages"), el("span", (p.languages || []).join(", ") || "—"),
      el("span.k", "Group"), el("span", p.release_group || "—"),
      el("span.k", "Edition"), el("span", p.edition || "—"),
      p.imdb_id ? el("span.k", "IMDb") : null, p.imdb_id ? el("span", p.imdb_id) : null,
      el("span.k", "Formats"), el("span", r.formats.length ? r.formats.map((f) => el("span.tag.info", f)) : el("span.dim", "none")),
      el("span.k", "Score"), el("span", r.score)));
  }
}
