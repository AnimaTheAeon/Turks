"use strict";
/* Record editors: indexers, download clients, connections, import lists,
   delay/release profiles and remote path mappings. A form is a list of
   field specs; each edits the record object in place. */

const LABELS = {
  webhook: "Webhook URL", username: "Username", bot_token: "Bot token", chat_id: "Chat id", silent: "Send silently",
  user_key: "User key", api_token: "API token", devices: "Devices", priority: "Priority", server: "Server", topic: "Topic",
  token: "Token", app_token: "App token", url: "URL", method_put: "Use PUT", password: "Password", port: "Port",
  require_tls: "Encrypted (TLS)", from: "From", to: "To", host: "Host", use_ssl: "Use SSL", api_key: "API key",
  jellyfin: "Jellyfin", path: "Path", arguments: "Arguments", channel: "Channel", device: "Device id", urls: "Apprise URLs",
  tag: "Tag", list_id: "List id", collection_id: "Collection id", person_id: "Person id", cast: "As cast",
  crew: "As director/writer/producer", company_id: "Company id", keyword_id: "Keyword id", which: "Chart",
  client_id: "Trakt client id", list: "List", rss_url: "Watchlist RSS URL",
};
const HELPS = {
  "Discord.webhook": "Discord › channel settings › Integrations › Webhooks.",
  "Telegram.bot_token": "From @BotFather.",
  "Pushover.devices": "Comma separated; empty = all.", "Pushover.priority": "-2 to 2",
  "Ntfy.token": "Optional.", "Ntfy.priority": "1 to 5",
  "Webhook.url": "Receives Radarr-style JSON.", "Webhook.method_put": "Off sends POST.",
  "Email.to": "Comma separated.", "Email.server": "SMTP server",
  "Plex.token": "See Plex's 'Finding an authentication token' help page.",
  "Emby.api_key": "Dashboard › API Keys.", "Emby.jellyfin": "On for Jellyfin, off for Emby.",
  "Slack.webhook": "Slack › Apps › Incoming Webhooks.", "Slack.channel": "Optional override, e.g. #movies",
  "Pushbullet.api_key": "Pushbullet › Settings › Account.", "Pushbullet.device": "Optional; empty sends to all devices.",
  "Apprise.server": "Your Apprise API, e.g. http://nas:8000", "Apprise.urls": "Space separated (or leave empty and use a tag).",
  "Apprise.tag": "Send to the Apprise config's URLs with this tag.",
  "CustomScript.path": "A script on the NAS. Turks passes Radarr-style environment variables.",
  "TmdbList.list_id": "The number in the list's TMDb URL.", "TmdbPerson.person_id": "From the person's TMDb URL.",
  "ImdbList.list_id": "ls123456789 (the list must be public).", "TraktList.client_id": "Create an app at trakt.tv/oauth/applications.",
  "TraktList.list": "'watchlist' or a list's slug.", "PlexWatchlist.rss_url": "Plex › Settings › Watchlist › RSS feed.",
  "TmdbPopular.which": "popular, top_rated, upcoming or now_playing",
};
const SECRET = /(password|token|api_key|user_key|webhook)$/;
const human = (k) => LABELS[k] || k.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

/** Inputs for the fields of a tagged enum value ({type: "Discord", …}). */
function kindFields(kind) {
  const rows = [];
  for (const [k, v] of Object.entries(kind)) {
    if (k === "type") continue;
    const help = HELPS[kind.type + "." + k] || "";
    let ctl;
    if (typeof v === "boolean") ctl = toggleBtn(v, (x) => { kind[k] = x; });
    else if (typeof v === "number") ctl = el("input", { type: "number", value: v, step: Number.isInteger(v) ? 1 : "any", oninput: (e) => { kind[k] = Number(e.target.value) || 0; } });
    else if (kind.type === "CustomScript" && k === "path") ctl = pathInput(v, (x) => { kind[k] = x; }, true);
    else ctl = el("input", { type: SECRET.test(k) ? "password" : "text", value: v, autocomplete: "off", oninput: (e) => { kind[k] = e.target.value; } });
    rows.push(field(human(k), help, ctl));
  }
  return rows;
}

function pathInput(value, onchange, files) {
  const input = el("input", { type: "text", value, oninput: () => onchange(input.value) });
  const browse = el("button.small", { onclick: async () => { const p = await pickFolder(input.value); if (p) { input.value = p; onchange(p); } } }, "Browse");
  return el("span.row", { style: { margin: 0, flexWrap: "nowrap" } }, input, files ? null : browse);
}

/* Field spec: [key, label, type, help, extra]
   types: text secret num float optnum bool csvint csvstr select tags path multi */
function specRows(rec, specs) {
  return specs.filter(Boolean).map(([key, label, type, help, extra]) => {
    const v = rec[key];
    let ctl;
    switch (type) {
      case "bool": ctl = toggleBtn(!!v, (x) => { rec[key] = x; }); break;
      case "num": ctl = el("input", { type: "number", value: v ?? 0, oninput: (e) => { rec[key] = parseInt(e.target.value || "0", 10); } }); break;
      case "float": ctl = el("input", { type: "number", step: "any", value: v ?? 0, oninput: (e) => { rec[key] = parseFloat(e.target.value || "0"); } }); break;
      case "optnum": ctl = el("input", { type: "number", step: "any", value: v ?? "", placeholder: "client default", oninput: (e) => { rec[key] = e.target.value === "" ? null : Number(e.target.value); } }); break;
      case "csvint": ctl = el("input", { type: "text", value: (v || []).join(", "), oninput: (e) => { rec[key] = e.target.value.split(/[ ,]+/).map(Number).filter((n) => n > 0); } }); break;
      case "csvstr": ctl = el("input", { type: "text", value: (v || []).join(", "), oninput: (e) => { rec[key] = e.target.value.split(",").map((s) => s.trim()).filter(Boolean); } }); break;
      case "select": ctl = select(typeof extra === "function" ? extra() : extra, v, (x) => { rec[key] = /^-?\d+$/.test(x) && typeof v === "number" ? Number(x) : x; }); break;
      case "tags": ctl = tagPicker(v || [], (ids) => { rec[key] = ids; }); break;
      case "path": ctl = pathInput(v || "", (x) => { rec[key] = x; }); break;
      case "multi": {
        const set = new Set(v || []);
        ctl = el("div.chips", (typeof extra === "function" ? extra() : extra).map((o) => check(o, set.has(o), (on) => { on ? set.add(o) : set.delete(o); rec[key] = [...set]; })));
        break;
      }
      case "secret": ctl = el("input", { type: "password", value: v ?? "", autocomplete: "new-password", oninput: (e) => { rec[key] = e.target.value; } }); break;
      default: ctl = el("input", { type: "text", value: v ?? "", oninput: (e) => { rec[key] = e.target.value; } });
    }
    return field(label, help || "", ctl, type === "multi");
  });
}

const adv = () => Turks.cfg && Turks.cfg.advanced_mode;
const rootOptions = () => (Turks.lists.root || []).map((r) => [r.path, r.path]);
const profileOptions = () => (Turks.lists.profile || []).map((p) => [p.id, p.name]);
const availOptions = () => AVAIL;
const clientOptions = () => [[0, "Any"], ...(Turks.lists.client || []).map((c) => [c.id, c.name])];
const indexerOptions = () => [[0, "Any"], ...(Turks.lists.indexer || []).map((c) => [c.id, c.name])];

const CLIENT_FIELDS = {
  common: ["host", "port", "use_ssl", "url_base", "username", "password", "category", "directory", "priority", "add_paused", "remove_completed", "remove_failed"],
  Transmission: ["host", "port", "use_ssl", "url_base", "username", "password", "category", "directory", "priority", "add_paused", "remove_completed", "remove_failed"],
  QBittorrent: ["host", "port", "use_ssl", "url_base", "username", "password", "category", "priority", "add_paused", "remove_completed", "remove_failed"],
  Deluge: ["host", "port", "use_ssl", "url_base", "password", "category", "directory", "priority", "add_paused", "remove_completed", "remove_failed"],
  Sabnzbd: ["host", "port", "use_ssl", "url_base", "api_key", "username", "password", "category", "priority", "remove_completed", "remove_failed"],
  Nzbget: ["host", "port", "use_ssl", "url_base", "username", "password", "category", "priority", "add_paused", "remove_completed", "remove_failed"],
  DownloadStation: ["host", "port", "use_ssl", "username", "password", "directory", "priority", "remove_completed", "remove_failed"],
  TorrentBlackhole: ["drop_folder", "watch_folder", "save_magnet_files", "priority"],
  UsenetBlackhole: ["drop_folder", "watch_folder", "priority"],
};
const CLIENT_SPEC = {
  host: ["host", "Host", "text", "Usually localhost when it runs on this NAS."],
  port: ["port", "Port", "num"],
  use_ssl: ["use_ssl", "Use SSL", "bool"],
  url_base: ["url_base", "URL base", "text", "Only if the client lives under a path, e.g. /transmission"],
  username: ["username", "Username", "text"],
  password: ["password", "Password", "secret"],
  api_key: ["api_key", "API key", "secret", "SABnzbd › Config › General."],
  category: ["category", "Category", "text", "Keeps Turks' downloads apart from everything else."],
  directory: ["directory", "Directory", "text", "Optional: where the client saves Turks' downloads."],
  priority: ["priority", "Client priority", "num", "When several clients fit, the lowest number is used first."],
  add_paused: ["add_paused", "Add paused", "bool"],
  remove_completed: ["remove_completed", "Remove completed", "bool", "Remove from the client after import (when seeding goals are met)."],
  remove_failed: ["remove_failed", "Remove failed", "bool"],
  drop_folder: ["drop_folder", "Drop folder", "path", "Where Turks puts .torrent/.nzb files for the client to pick up."],
  watch_folder: ["watch_folder", "Watch folder", "path", "Where finished downloads appear."],
  save_magnet_files: ["save_magnet_files", "Save magnet files", "bool", "Write .magnet files when there's no .torrent."],
};

const KINDS = {
  indexer: {
    title: (r) => r.name || "Indexer",
    sub: (r) => `${r.kind} · ${[r.enable_rss && "RSS", r.enable_auto_search && "Auto", r.enable_interactive_search && "Interactive"].filter(Boolean).join(" · ") || "off"}`,
    ok: (r) => r.enable_rss || r.enable_auto_search || r.enable_interactive_search,
    form: (r) => specRows(r, [
      ["name", "Name", "text"],
      ["url", "URL", "text", "The Torznab/Newznab feed, e.g. from Prowlarr or Jackett."],
      adv() && ["api_path", "API path", "text", "Usually /api"],
      ["api_key", "API key", "secret"],
      ["categories", "Categories", "csvint", "Newznab category ids, comma separated. 2000 = Movies, 2040 = HD, 2045 = UHD."],
      ["enable_rss", "RSS", "bool", "Check this indexer's newest releases regularly."],
      ["enable_auto_search", "Automatic search", "bool", "Use it when Turks searches by itself."],
      ["enable_interactive_search", "Interactive search", "bool", "Use it when you search by hand."],
      r.kind === "Torznab" && ["minimum_seeders", "Minimum seeders", "num"],
      r.kind === "Torznab" && ["seed_ratio", "Seed ratio", "optnum", "Sent to the download client. Empty = use the client's setting."],
      r.kind === "Torznab" && ["seed_time", "Seed time (minutes)", "optnum"],
      adv() && ["priority", "Indexer priority", "num", "1 (highest) to 50. Breaks ties between equal releases."],
      adv() && ["download_client_id", "Download client", "select", "Send this indexer's releases to one client.", clientOptions],
      adv() && ["required_flags", "Required flags", "multi", "Only take releases with these flags.", () => Turks.meta.indexer_flags],
      adv() && ["multi_languages", "Multi languages", "csvstr", "What 'MULTi' means on this indexer, e.g. English, French."],
      adv() && ["extra_params", "Additional parameters", "text"],
      ["tags", "Tags", "tags", "Only use for movies with one of these tags (empty = all)."],
    ]),
    testable: true,
  },
  client: {
    title: (r) => r.name || r.kind,
    sub: (r) => `${(Turks.meta.client_kinds.find((k) => k.kind === r.kind) || {}).label || r.kind} · ${r.drop_folder || (r.host + (r.port ? ":" + r.port : ""))}`,
    ok: (r) => r.enabled,
    form: (r) => [
      ...specRows(r, [["name", "Name", "text"], ["enabled", "Enabled", "bool"]]),
      ...specRows(r, (CLIENT_FIELDS[r.kind] || CLIENT_FIELDS.common).map((k) => CLIENT_SPEC[k])),
      ...specRows(r, [["tags", "Tags", "tags", "Only use for movies with one of these tags (empty = all)."]]),
    ],
    testable: true,
  },
  notification: {
    title: (r) => r.name || r.kind.type,
    sub: (r) => r.kind.type + " · " + [r.on_grab && "Grab", r.on_download && "Import", r.on_upgrade && "Upgrade", r.on_health_issue && "Health"].filter(Boolean).join(", "),
    ok: () => true,
    form: (r) => [
      ...specRows(r, [["name", "Name", "text"]]),
      ...kindFields(r.kind),
      el("div.head", "When"),
      el("div.chips", [["on_grab", "Grab"], ["on_download", "Import"], ["on_upgrade", "Upgrade"], ["on_rename", "Rename"], ["on_movie_added", "Movie added"],
        ["on_movie_delete", "Movie deleted"], ["on_movie_file_delete", "File deleted"], ["on_health_issue", "Health issue"], ["on_health_restored", "Health restored"],
        ["include_health_warnings", "Include warnings"], ["on_manual_interaction", "Needs attention"], ["on_app_update", "App updated"]]
        .map(([k, l]) => el("span", { style: { width: "180px" } }, check(l, r[k], (x) => { r[k] = x; })))),
      ...specRows(r, [["tags", "Tags", "tags", "Only for movies with one of these tags (empty = all)."]]),
    ],
    testable: true,
  },
  list: {
    title: (r) => r.name || r.kind.type,
    sub: (r) => `${r.kind.type} · ${r.enabled ? (r.auto_add ? "adds automatically" : "enabled") : "disabled"}${r.last_sync ? " · synced " + ago(r.last_sync) : ""}`,
    ok: (r) => r.enabled,
    form: (r) => [
      ...specRows(r, [["name", "Name", "text"], ["enabled", "Enabled", "bool"], ["auto_add", "Add automatically", "bool", "Off only shows what the list would add."]]),
      ...kindFields(r.kind),
      ...specRows(r, [
        ["monitor", "Monitor", "select", "", [["MovieOnly", "Movie"], ["MovieAndCollection", "Movie and collection"], ["None", "None"]]],
        ["search_on_add", "Search on add", "bool"],
        ["root_folder", "Root folder", "select", "", rootOptions],
        ["quality_profile_id", "Quality profile", "select", "", profileOptions],
        ["minimum_availability", "Minimum availability", "select", "", availOptions],
        ["tags", "Tags", "tags"],
      ]),
    ],
    testable: true,
  },
  delay: {
    title: (r) => (r.tags.length ? r.tags.map(tagName).join(", ") : "Default (all movies)"),
    sub: (r) => `Prefer ${r.preferred} · usenet ${r.usenet_delay} min · torrent ${r.torrent_delay} min`,
    ok: () => true,
    form: (r) => specRows(r, [
      ["enable_usenet", "Usenet", "bool"], ["enable_torrent", "Torrent", "bool"],
      ["preferred", "Preferred protocol", "select", "", [["Usenet", "Usenet"], ["Torrent", "Torrent"]]],
      ["usenet_delay", "Usenet delay (minutes)", "num"], ["torrent_delay", "Torrent delay (minutes)", "num"],
      ["bypass_if_highest_quality", "Bypass if highest quality", "bool", "Grab right away when the release is the profile's best quality."],
      ["bypass_if_above_score", "Bypass if above score", "bool"], ["minimum_score", "Minimum custom format score", "num"],
      ["order", "Order", "num", "Lower numbers are checked first."],
      ["tags", "Tags", "tags", "Movies with these tags use this profile (empty = default)."],
    ]),
  },
  restriction: {
    title: (r) => r.name || "Release profile",
    sub: (r) => `${r.required.length} required · ${r.ignored.length} ignored${r.enabled ? "" : " · off"}`,
    ok: (r) => r.enabled,
    form: (r) => specRows(r, [
      ["name", "Name", "text"], ["enabled", "Enabled", "bool"],
      ["required", "Must contain", "csvstr", "Comma separated; a release needs at least one."],
      ["ignored", "Must not contain", "csvstr", "Comma separated; any match rejects the release."],
      ["indexer_id", "Indexer", "select", "", indexerOptions],
      ["tags", "Tags", "tags"],
    ]),
  },
  remotepath: {
    title: (r) => r.host || "Mapping",
    sub: (r) => `${r.remote} → ${r.local}`,
    ok: () => true,
    form: (r) => specRows(r, [
      ["host", "Host", "text", "The download client's host, as entered in its settings."],
      ["remote", "Remote path", "text", "The path as the client reports it."],
      ["local", "Local path", "path", "The same folder as Turks sees it on the NAS."],
    ]),
  },
};

/** Opens the editor for one record. Resolves true when something changed. */
function editRecord(kind, rec, isNew) {
  const K = KINDS[kind];
  rec = JSON.parse(JSON.stringify(rec));
  return new Promise((resolve) => {
    let changed = false;
    const msg = el("div.notice.hidden");
    const body = el("div", K.form(rec), msg);
    const buttons = [];
    if (!isNew) buttons.push(el("button.danger", { onclick: async () => {
      if (!(await confirmBox("Delete " + K.title(rec) + "?", "This can't be undone.", "Delete", true))) return;
      if (await act("delete", {}, { kind, id: rec.id })) { changed = true; m.close(); }
    } }, "Delete"));
    if (K.testable) buttons.push(el("button", { onclick: async (e) => {
      const b = e.target; b.disabled = true; msg.className = "notice"; msg.replaceChildren(el("span.spin"), " Testing…");
      try { const r = await api("test", rec, { kind }); msg.className = "notice good"; msg.textContent = r.message || "OK"; }
      catch (err) { msg.className = "notice bad"; msg.textContent = err.message; }
      b.disabled = false;
    } }, "Test"));
    buttons.push(el("button", { onclick: () => m.close() }, "Cancel"));
    buttons.push(el("button.big.primary", { onclick: async () => {
      if ("name" in rec && !String(rec.name).trim()) { toast("Give it a name.", true); return; }
      if (await act("save", rec, { kind })) { changed = true; m.close(); }
    } }, "Save"));
    const m = openModal({ title: (isNew ? "Add " : "Edit ") + K.title(rec), body, buttons, onclose: () => { if (changed) invalidate(kind); resolve(changed); } });
  });
}

/** Pick a template (Discord, Telegram…) then open the editor. */
async function addRecord(kind) {
  const list = await api("templates", undefined, { kind });
  if (list.length === 1) return editRecord(kind, list[0].value, true);
  return new Promise((resolve) => {
    let picked = false;
    const m = openModal({
      title: "Add " + ({ indexer: "indexer", client: "download client", notification: "connection", list: "import list" }[kind] || kind),
      body: el("div.cards", list.map((t) => el("div.card.add", { onclick: async () => { picked = true; m.close(); resolve(await editRecord(kind, t.value, true)); } }, t.label))),
      buttons: [el("button", { onclick: () => m.close() }, "Cancel")],
      size: "wide",
      onclose: () => { if (!picked) resolve(false); },
    });
  });
}

/** Cards for every record of a kind, plus "+ Add". Redraws after edits. */
function recordCards(kind, addLabel, extraFilter) {
  const wrap = el("div.cards");
  const draw = async () => {
    await loadList(kind);
    const K = KINDS[kind];
    const items = (Turks.lists[kind] || []).filter(extraFilter || (() => true));
    wrap.replaceChildren(
      ...items.map((r) => el("div.card", { onclick: async () => { if (await editRecord(kind, r, false)) draw(); } },
        el("div.top", el("span.badge." + (K.ok(r) ? "good" : "bad")), el("span.t", K.title(r))),
        el("div.s", K.sub(r)))),
      el("div.card.add", { onclick: async () => { if (await addRecord(kind)) draw(); } }, "+ " + addLabel),
    );
  };
  draw();
  return wrap;
}
