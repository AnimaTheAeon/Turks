"use strict";
/* Settings: every section of the Windows app's Settings page. Plain
   settings save as soon as they change; records open an editor. */

const SECTIONS = [
  ["media", "Media Management", false], ["profiles", "Profiles", false], ["quality", "Quality", true], ["formats", "Custom Formats", false],
  ["indexers", "Indexers", false], ["clients", "Download Clients", false], ["lists", "Import Lists", false], ["connect", "Connect", false],
  ["metadata", "Metadata", false], ["tags", "Tags", false], ["general", "General", false], ["ui", "UI", false],
];

// ---- settings bound to Turks.cfg, saved on change

const pendingSaves = {};
function saveSoon(key, value, after) {
  Turks.cfg[key] = value;
  clearTimeout(pendingSaves[key]);
  pendingSaves[key] = setTimeout(async () => { await saveConfig({ [key]: value }); if (after) after(); }, 500);
}
function cToggle(key, name, help, after) { return field(name, help, toggleBtn(!!Turks.cfg[key], (v) => saveSoon(key, v, after))); }
function cNum(key, name, help, unit, min, max) {
  const input = el("input", { type: "number", value: Turks.cfg[key], min, max, oninput: () => {
    let v = Number(input.value || 0);
    if (min !== undefined && v < min) return;
    if (max !== undefined && v > max) return;
    saveSoon(key, v);
  } });
  return field(name, help, [input, unit ? el("span.dim", unit) : null]);
}
function cText(key, name, help, after, secret) {
  return field(name, help, el("input", { type: secret ? "password" : "text", value: Turks.cfg[key] ?? "", autocomplete: secret ? "new-password" : "off", oninput: (e) => saveSoon(key, e.target.value, after) }));
}
function cSelect(key, name, help, options) { return field(name, help, select(options, Turks.cfg[key], (v) => saveSoon(key, typeof Turks.cfg[key] === "number" ? Number(v) : v))); }
function cPath(key, name, help) { return field(name, help, pathInput(Turks.cfg[key] || "", (v) => saveSoon(key, v))); }
/** append, skipping the nulls left by advanced-only rows */
function add(node, ...items) { node.append(...items.flat(Infinity).filter((x) => x !== null && x !== undefined && x !== false)); }
const head = (title, help) => [el("div.head", title), help ? el("div.dim.small", { style: { marginBottom: "4px" } }, help) : null];

Turks.pages.settings = {
  async render(main, arg) {
    await loadConfig();
    const advanced = Turks.cfg.advanced_mode;
    let section = arg || store.get("settingsSection", "media");
    if (!SECTIONS.some(([k, , a]) => k === section && (advanced || !a))) section = "media";
    store.set("settingsSection", section);
    const nav = el("div.subnav", SECTIONS.filter(([, , a]) => advanced || !a).map(([k, l]) => menuItem(l, k === section, () => go("settings", k), "sub-item")));
    const body = el("div.scroll");
    main.append(el("div.page",
      el("div.page-head", el("h1", "Settings"),
        el("span.dim", advanced ? "Advanced mode: showing everything" : isGreen() ? "Simple mode: the settings most people need" : "Simple mode: the settings most people need (switch with MODE in the side window)"),
        isGreen() ? el("button", { onclick: async () => { await saveConfig({ advanced_mode: !advanced }); renderInfo(); rerender(); } }, advanced ? "MODE: ADVANCED" : "MODE: SIMPLE") : null),
      el("div.divider"), el("div.split", nav, body)));
    await loadLists(["tag", "root", "profile"]);
    await ({ media: sMedia, profiles: sProfiles, quality: sQuality, formats: sFormats, indexers: sIndexers, clients: sClients, lists: sLists,
      connect: sConnect, metadata: sMetadata, tags: sTags, general: sGeneral, ui: sUi }[section])(body, advanced);
  },
};

// ---------------------------------------------------------------- media management

async function sMedia(body, advanced) {
  const roots = el("div");
  const drawRoots = async () => {
    await loadList("root", true);
    const st = (Turks.state && Turks.state.roots) || [];
    roots.replaceChildren(...Turks.lists.root.map((r) => {
      const info = st.find((x) => x.id === r.id) || {};
      return el("div.field",
        el("div.lbl", el("div.name", el("span.badge." + (info.exists === false ? "bad" : "good")), " ", r.path),
          el("div.help", info.free ? size(info.free) + " free" : "", info.unmapped ? ` · ${info.unmapped} folders not in Turks — ` : "", info.unmapped ? el("button.link", { onclick: () => go("add", "import") }, "Library import") : null)),
        el("div.ctl", el("button.danger", { onclick: async () => {
          if (await confirmBox("Remove root folder?", esc(r.path) + "<br>Movies and files stay where they are.", "Remove", true)) { await act("delete", {}, { kind: "root", id: r.id }); drawRoots(); }
        } }, "Remove")));
    }), el("div.row", el("button.primary", { onclick: async () => {
      const p = await pickFolder("", "Add a root folder", "Use this folder");
      if (p && (await act("save", { id: 0, path: p }, { kind: "root" }, "Root folder added"))) { await poll(); drawRoots(); }
    } }, "+ Add root folder")));
  };
  drawRoots();

  const exFile = el("div.example"), exFolder = el("div.example");
  const example = async () => {
    const r = await api("naming/example", { standard_movie_format: Turks.cfg.standard_movie_format, movie_folder_format: Turks.cfg.movie_folder_format, colon_replacement: Turks.cfg.colon_replacement }).catch(() => null);
    if (r) { exFile.textContent = "Example: " + r.file; exFolder.textContent = "Example: " + r.folder; }
  };
  const fmt = (key, name, help, ex) => {
    const input = el("input.wide", { type: "text", value: Turks.cfg[key], oninput: () => { Turks.cfg[key] = input.value; saveSoon(key, input.value); clearTimeout(fmt.t); fmt.t = setTimeout(example, 300); } });
    return el("div.field.stack", el("div.lbl", el("div.name", name), el("div.help", help)), el("div.ctl", input), ex);
  };
  example();
  add(body, 
    ...head("Root folders", "Folders that hold your movie folders. Each movie lives in its own folder inside one of these. The Turks system user needs Read/Write on the shared folder."), roots,
    ...head("Movie naming", "How Turks names folders and files when it imports. Uses Radarr's tokens."),
    cToggle("rename_movies", "Rename movies", "Rename imported files using the format below. Off keeps the downloaded file name."),
    cToggle("replace_illegal_chars", "Replace illegal characters", "Swap characters some systems can't use (like ? and *) for safe ones instead of dropping them."),
    field("Colon replacement", "", select(Turks.meta.colon_replacements.map((c) => [c.value, c.label]), Turks.cfg.colon_replacement, (v) => { saveSoon("colon_replacement", v); setTimeout(example, 600); })),
    fmt("standard_movie_format", "Movie file format", "", exFile),
    fmt("movie_folder_format", "Movie folder format", "", exFolder),
    el("details", el("summary.dim.small", { style: { cursor: "pointer", margin: "6px 0" } }, "Tokens"), el("div.chips", Turks.meta.naming_tokens.map((t) => el("span.tag.mono", t)))),
    ...head("Importing", "What happens when a download finishes."),
    cToggle("use_hardlinks", "Use hardlinks instead of copy", "For torrents on the same volume: the file appears in your library without using extra space, and keeps seeding. Falls back to copying across volumes."),
    cToggle("import_extra_files", "Import extra files", "Bring subtitles and other extras along with the movie."),
    cText("extra_file_extensions", "Extra file extensions", "Comma separated, e.g. srt,ass,nfo"),
    cToggle("analyse_video_files", "Analyse video files", "Read codec, HDR and audio info with ffprobe when it's available."),
    advanced ? cToggle("skip_free_space_check", "Skip free space check", "Import even when the root folder looks full.") : null,
    advanced ? cNum("minimum_free_space_mb", "Minimum free space", "Keep at least this much free on the library volume.", "MB", 0, 1000000) : null,
    advanced ? cNum("minimum_age_for_import_secs", "Minimum file age", "Wait until a file hasn't changed for this long before importing it.", "s", 0, 3600) : null,
    ...head("File management", "Upgrades, deletions and the recycle bin."),
    cToggle("unmonitor_deleted_movies", "Unmonitor deleted movies", "When a movie's file disappears from disk, stop monitoring it so it isn't downloaded again."),
    cSelect("propers", "Propers and repacks", "", Turks.meta.propers.map((p) => [p.value, p.label])),
    cPath("recycle_bin", "Recycle bin", "Deleted and replaced files go here instead of being deleted. Empty = delete for real."),
    cNum("recycle_bin_cleanup_days", "Recycle bin cleanup", "Delete files from the recycle bin after this many days (0 = never).", "days", 0, 365),
    advanced ? cToggle("create_empty_folders", "Create empty movie folders", "Make a folder for each movie as soon as it's added.") : null,
    advanced ? cToggle("delete_empty_folders", "Delete empty folders", "Remove download folders left empty after an import.") : null,
    advanced ? cSelect("rescan_after_refresh", "Rescan folder after refresh", "", [["Always", "Always"], ["AfterManual", "After manual refresh"], ["Never", "Never"]]) : null,
    advanced ? cSelect("file_date", "Change file date", "", [["None", "None"], ["InCinemas", "In cinemas date"], ["PhysicalRelease", "Physical release date"]]) : null,
    ...(advanced ? [...head("Remote path mappings", "If a download client runs on another computer (or in Docker), tell Turks how its paths look from here."), recordCards("remotepath", "Add mapping")] : []),
  );
}

// ---------------------------------------------------------------- profiles

async function sProfiles(body, advanced) {
  await loadLists(["profile", "format"], true);
  const cards = el("div.cards");
  const draw = async () => {
    await loadList("profile", true);
    cards.replaceChildren(...Turks.lists.profile.map((p) => {
      const allowed = p.items.slice().reverse().filter((i) => i.allowed).slice(0, 4).map((i) => i.name || qualityName(i.id));
      const cut = p.items.find((i) => i.id === p.cutoff);
      return el("div.card", { onclick: async () => { if (await editProfile(p)) draw(); } },
        el("div.top", el("span.t", p.name)), el("div.s", allowed.join(", ")), el("div.s", "Cutoff: " + (cut ? cut.name || qualityName(cut.id) : "—") + (p.upgrade_allowed ? "" : " · no upgrades")));
    }), el("div.card.add", { onclick: async () => { const t = await api("templates", undefined, { kind: "profile" }); if (await editProfile(t[0].value, true)) draw(); } }, "+ Add profile"));
  };
  draw();
  add(body, ...head("Quality profiles", "Which qualities a movie may be downloaded in, which one is good enough (the cutoff), and how custom formats are scored."), cards);
  if (advanced) {
    await loadLists(["indexer"]);
    add(body, ...head("Delay profiles", "Wait a while before grabbing, so a better release (or your preferred protocol) has time to show up."), recordCards("delay", "Add delay profile"));
    add(body, ...head("Release profiles", "Words a release must contain, or must not contain. Wrap a term in slashes for a regex: /\\bHC\\b/"), recordCards("restriction", "Add release profile"));
  }
}

function editProfile(p, isNew) {
  p = JSON.parse(JSON.stringify(p));
  return new Promise((resolve) => {
    let changed = false;
    const items = el("div.qlist");
    const cutoffSel = el("span");
    const drawItems = () => {
      const shown = p.items.slice().reverse();
      items.replaceChildren(...shown.map((it, i) => el("div.qi",
        check("", it.allowed, (v) => { it.allowed = v; drawCutoff(); }),
        el("span", { style: { flex: "1" } }, it.name || qualityName(it.id), it.qualities.length > 1 ? el("span.members", "  " + it.qualities.map(qualityName).join(", ")) : null),
        it.id === p.cutoff ? el("span.tag.warn", "CUTOFF") : null,
        el("button.small", { disabled: i === 0, title: "Higher", onclick: () => move(it, 1) }, "▲"),
        el("button.small", { disabled: i === shown.length - 1, title: "Lower", onclick: () => move(it, -1) }, "▼"))));
    };
    const move = (it, dir) => {
      const idx = p.items.indexOf(it);
      const j = idx + dir;
      if (j < 0 || j >= p.items.length) return;
      [p.items[idx], p.items[j]] = [p.items[j], p.items[idx]];
      drawItems();
    };
    const drawCutoff = () => {
      const allowed = p.items.filter((i) => i.allowed).reverse();
      if (!allowed.some((i) => i.id === p.cutoff) && allowed.length) p.cutoff = allowed[allowed.length - 1].id;
      cutoffSel.replaceChildren(select(allowed.map((i) => [i.id, i.name || qualityName(i.id)]), p.cutoff, (v) => { p.cutoff = Number(v); drawItems(); }));
      drawItems();
    };
    drawCutoff();
    const formats = Turks.lists.format || [];
    const scoreOf = (id) => (p.format_items.find((f) => f.format_id === id) || {}).score || 0;
    const setScore = (id, score) => { p.format_items = p.format_items.filter((f) => f.format_id !== id); if (score) p.format_items.push({ format_id: id, score }); };
    const langs = [["Any", "Any"], ["Original", "Original"], ...Turks.meta.languages.map((l) => [l, l])];
    const body = [
      ...specRows(p, [["name", "Name", "text"], ["upgrade_allowed", "Upgrades allowed", "bool", "Off: once a movie has a file, Turks stops looking."]]),
      field("Upgrade until", "The quality that's good enough. Turks stops upgrading once it has this.", cutoffSel),
      field("Language", "Original = the movie's own language. Any = no language check.", select(langs, p.language, (v) => { p.language = v; })),
      el("div.head", "Qualities"), el("div.dim.small", "Ticked qualities may be downloaded. Higher in the list is better."), items,
      el("div.head", "Custom format scores"),
      formats.length ? el("div",
        ...specRows(p, [
          ["min_format_score", "Minimum score", "num", "Releases scoring below this are rejected."],
          ["cutoff_format_score", "Upgrade until score", "num", "Turks stops upgrading once the file scores this."],
          ["min_upgrade_format_score", "Minimum score increment", "num", "An upgrade must beat the current file by at least this much."],
        ]),
        el("table", el("tbody", formats.map((f) => el("tr", el("td", f.name), el("td.right", el("input", { type: "number", value: scoreOf(f.id), oninput: (e) => setScore(f.id, Number(e.target.value || 0)) }))))))) :
        el("div.dim.small", "No custom formats yet. Add some in Settings › Custom Formats, then score them here."),
    ];
    const buttons = [];
    if (!isNew) buttons.push(el("button.danger", { onclick: async () => {
      if (!(await confirmBox(`Delete ${esc(p.name)}?`, "Movies using it switch to the first remaining profile.", "Delete", true))) return;
      if (await act("delete", {}, { kind: "profile", id: p.id })) { changed = true; m.close(); }
    } }, "Delete"));
    buttons.push(el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: async () => {
      if (!p.name.trim()) return toast("Give the profile a name.", true);
      if (!p.items.some((i) => i.allowed)) return toast("Allow at least one quality.", true);
      if (await act("save", p, { kind: "profile" })) { changed = true; m.close(); }
    } }, "Save"));
    const m = openModal({ title: isNew ? "Add profile" : p.name, body, buttons, size: "wide", onclose: () => { if (changed) invalidate("profile"); resolve(changed); } });
  });
}

// ---------------------------------------------------------------- quality sizes

async function sQuality(body) {
  const sizes = await api("list", undefined, { kind: "size" });
  sizes.sort((a, b) => (Turks.meta.qualities.findIndex((q) => q.id === a.quality)) - (Turks.meta.qualities.findIndex((q) => q.id === b.quality)));
  const save = (s) => { clearTimeout(s._t); s._t = setTimeout(() => { const { _t, ...rec } = s; act("save", rec, { kind: "size" }); }, 600); };
  const num = (s, k, nullable) => el("input", { type: "number", step: "any", value: s[k] ?? "", placeholder: nullable ? "no limit" : "", oninput: (e) => { s[k] = e.target.value === "" ? (nullable ? null : 0) : Number(e.target.value); save(s); } });
  add(body, ...head("Quality sizes", "Smallest, preferred and largest size for each quality, in MB per minute of runtime. Releases outside these are rejected; preferred breaks ties."),
    el("table", el("thead", el("tr", el("th", "Quality"), el("th", "Min"), el("th", "Preferred"), el("th", "Max"))),
      el("tbody", sizes.map((s) => el("tr", el("td", s.title || qualityName(s.quality)), el("td", num(s, "min")), el("td", num(s, "preferred", true)), el("td", num(s, "max", true)))))));
}

// ---------------------------------------------------------------- custom formats

async function sFormats(body) {
  const cards = el("div.cards");
  const draw = async () => {
    await loadList("format", true);
    cards.replaceChildren(...Turks.lists.format.map((f) => el("div.card", { onclick: async () => { if (await editFormat(f)) draw(); } },
      el("div.top", el("span.t", f.name)), el("div.s", f.specs.map((s) => s.name || s.kind.type).join(", ") || "no conditions"))),
      el("div.card.add", { onclick: async () => { if (await editFormat({ id: 0, name: "", include_when_renaming: false, specs: [] }, true)) draw(); } }, "+ Add custom format"));
  };
  draw();
  add(body, ...head("Custom formats", "Named patterns (HDR, Atmos, x265, a favourite group…) that quality profiles score. Import TRaSH Guides or Radarr JSON straight in."),
    el("div.toolbar", el("button", { onclick: () => importFormats(draw) }, "Import JSON…")), cards);
}

function importFormats(after) {
  const ta = el("textarea", { rows: 16, placeholder: "Paste one custom format or a list of them (TRaSH Guides / Radarr export JSON)" });
  const m = openModal({
    title: "Import custom formats", body: ta,
    buttons: [el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: async () => {
      const r = await act("formats/import", { text: ta.value }, null, (x) => `Imported ${x.imported} custom format(s). Give them scores in your quality profiles.`);
      if (r) { invalidate("format"); m.close(); after(); }
    } }, "Import")],
    size: "wide",
  });
}

const CF_FIELDS = {
  ReleaseTitle: [["pattern", "Regular expression", "text"]], ReleaseGroup: [["pattern", "Regular expression", "text"]], Edition: [["pattern", "Regular expression", "text"]],
  Source: [["source", "Source", "select", "", () => Turks.meta.sources.map((n, i) => [i, n])]],
  Resolution: [["resolution", "Resolution", "select", "", [[0, "Unknown"], [360, "360p"], [480, "480p"], [540, "540p"], [576, "576p"], [720, "720p"], [1080, "1080p"], [2160, "2160p"]]]],
  QualityModifier: [["modifier", "Modifier", "select", "", () => Turks.meta.modifiers.map((n, i) => [i, n])]],
  Size: [["min_gb", "Min GB", "float"], ["max_gb", "Max GB", "float"]],
  Language: [["language", "Language", "select", "", () => [["Original", "Original"], ["Unknown", "Unknown"], ...Turks.meta.languages.map((l) => [l, l])]], ["except", "Except this language", "bool"]],
  IndexerFlag: [["flag", "Flag", "select", "", () => Turks.meta.indexer_flags.map((f) => [f, f])]],
  Year: [["min", "From", "num"], ["max", "To", "num"]],
};

function editFormat(f, isNew) {
  f = JSON.parse(JSON.stringify(f));
  return new Promise((resolve) => {
    let changed = false;
    const specs = el("div");
    const drawSpecs = () => specs.replaceChildren(...f.specs.map((s, i) => el("div.spec",
      el("div.row", { style: { margin: 0 } }, el("span.label", s.kind.type.replace(/([a-z])([A-Z])/g, "$1 $2")),
        el("input", { type: "text", value: s.name, placeholder: "Name", oninput: (e) => { s.name = e.target.value; } }),
        check("Negate", s.negate, (v) => { s.negate = v; }), check("Required", s.required, (v) => { s.required = v; }),
        el("span.grow"), el("button.x", { onclick: () => { f.specs.splice(i, 1); drawSpecs(); } }, "×")),
      ...specRows(s.kind, CF_FIELDS[s.kind.type] || []))));
    drawSpecs();
    const add = select([["", "+ Add condition…"], ...Turks.meta.cf_templates.map((t, i) => [i, t.label])], "", (v) => {
      if (v === "") return;
      const t = Turks.meta.cf_templates[Number(v)];
      f.specs.push({ name: t.label, kind: JSON.parse(JSON.stringify(t.kind)), negate: false, required: false });
      drawSpecs();
      add.value = "";
    });
    const buttons = [];
    if (!isNew) {
      buttons.push(el("button.danger", { onclick: async () => {
        if (await confirmBox(`Delete ${esc(f.name)}?`, "Profiles stop scoring it.", "Delete", true)) if (await act("delete", {}, { kind: "format", id: f.id })) { changed = true; m.close(); }
      } }, "Delete"));
      buttons.push(el("button", { onclick: async () => {
        const j = await api("formats/export", undefined, { id: f.id });
        const ta = el("textarea", { rows: 16, readonly: true }, JSON.stringify(j, null, 2));
        openModal({ title: "Export " + f.name, body: ta, size: "wide" });
        ta.select();
      } }, "Export"));
    }
    buttons.push(el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: async () => {
      if (!f.name.trim()) return toast("Give the format a name.", true);
      if (await act("save", f, { kind: "format" })) { changed = true; m.close(); }
    } }, "Save"));
    const m = openModal({
      title: isNew ? "Add custom format" : f.name,
      body: [...specRows(f, [["name", "Name", "text"], ["include_when_renaming", "Include in {Custom Formats}", "bool", "Show this format's name in file names that use the {Custom Formats} token."]]),
        el("div.head", "Conditions"), el("div.dim.small", "Required conditions must all match; otherwise any one matching is enough."), specs, el("div.row", add)],
      buttons, size: "wide", onclose: () => { if (changed) invalidate("format"); resolve(changed); },
    });
  });
}

// ---------------------------------------------------------------- indexers / clients / lists / connect

async function sIndexers(body, advanced) {
  await loadLists(["client", "indexer"]);
  add(body, ...head("Indexers", "Where Turks finds releases. Torznab for torrent sites (Prowlarr, Jackett), Newznab for usenet. Prowlarr can also push its indexers here: point it at this NAS on the API port in Settings › General."),
    el("div.toolbar", el("button", { onclick: () => prowlarrImport() }, "Import from Prowlarr…")),
    recordCards("indexer", "Add indexer"),
    ...head("Options", "Apply to every indexer."),
    cNum("rss_sync_interval_minutes", "RSS sync interval", "How often Turks checks indexers' latest releases. Minimum 10 minutes.", "min", 10, 720),
    cNum("availability_delay_days", "Availability delay", "Days before or after the release date to start searching (negative = earlier).", "days", -365, 365),
    cToggle("allow_hardcoded_subs", "Allow hardcoded subs", "Grab releases with subtitles burned into the picture."),
    advanced ? cNum("minimum_age_minutes", "Minimum age (usenet)", "Wait this long after a usenet post before grabbing it.", "min", 0, 1440) : null,
    advanced ? cNum("retention_days", "Retention (usenet)", "Ignore usenet posts older than this (0 = no limit).", "days", 0, 10000) : null,
    advanced ? cNum("maximum_size_mb", "Maximum size", "Ignore any release bigger than this (0 = no limit).", "MB", 0, 1000000) : null,
    advanced ? cToggle("prefer_indexer_flags", "Prefer indexer flags", "Prefer freeleech and similar releases when everything else ties.") : null,
    advanced ? cText("whitelisted_hardcoded_subs", "Whitelisted hardcoded subs", "Burned-in subtitle tags that are OK anyway, comma separated (e.g. 'korsub').") : null,
  );
}

function prowlarrImport() {
  const o = { url: "http://localhost:9696", api_key: "" };
  const out = el("div");
  let found = [];
  const addBtn = el("button.big.primary", { disabled: true }, "Add selected");
  const m = openModal({
    title: "Import from Prowlarr",
    body: [...specRows(o, [["url", "Prowlarr URL", "text"], ["api_key", "API key", "secret", "Prowlarr › Settings › General."]]),
      el("div.row", el("button", { onclick: async (e) => {
        e.target.disabled = true; out.replaceChildren(el("span.spin"));
        try {
          found = (await api("prowlarr", o)).map((ix) => ({ on: true, ix }));
          out.replaceChildren(...found.map((f) => el("div", check(`${f.ix.name} (${f.ix.kind === "Torznab" ? "Torrent" : "Usenet"})`, true, (v) => { f.on = v; }))));
          addBtn.disabled = !found.length;
        } catch (err) { out.replaceChildren(el("div.notice.bad", err.message)); }
        e.target.disabled = false;
      } }, "Load indexers")), out],
    buttons: [el("button", { onclick: () => m.close() }, "Cancel"), addBtn],
  });
  addBtn.onclick = async () => {
    let n = 0;
    for (const f of found.filter((x) => x.on)) if (await act("save", f.ix, { kind: "indexer" })) n++;
    toast(`Added ${n} indexers.`);
    invalidate("indexer");
    m.close();
    rerender();
  };
}

async function sClients(body, advanced) {
  await loadLists(["client"]);
  add(body, ...head("Download clients", "Where releases are sent. Lifestream speaks the Transmission protocol, so pick 'Transmission / Lifestream' for it. Synology Download Station works too."),
    recordCards("client", "Add download client"),
    ...head("Completed download handling", "Turks imports finished downloads by itself."),
    cToggle("completed_download_handling", "Enable", "Import completed downloads automatically."),
    cToggle("redownload_failed", "Redownload failed", "When a download fails, blocklist it and search for another release."),
    advanced ? cToggle("redownload_failed_from_interactive", "…from interactive search too", "Also replace failed releases you picked by hand.") : null,
    advanced ? cNum("check_downloads_seconds", "Check every", "How often to ask download clients for progress.", "s", 15, 600) : null,
    ...(advanced ? [] : [el("div.dim.small", { style: { marginTop: "8px" } }, "Remote path mappings are in Media Management (advanced mode).")]));
}

async function sLists(body, advanced) {
  await loadLists(["list", "exclusion"], true);
  const excl = el("div");
  const drawEx = async () => {
    const list = await loadList("exclusion", true);
    excl.replaceChildren(list.length ? el("table", el("tbody", list.map((x) => el("tr", el("td", x.title, " ", el("span.dim", x.year || "")), el("td.right",
      el("button.x", { onclick: async () => { await act("delete", {}, { kind: "exclusion", id: x.id }); drawEx(); } }, "×")))))) : el("div.dim.small", "None."));
  };
  drawEx();
  add(body, ...head("Import lists", "Lists that add movies automatically: TMDb lists and people, IMDb lists, Trakt, Plex watchlist, RSS, or another Turks/Radarr."),
    el("div.toolbar", el("button", { onclick: () => act("command", { name: "lists" }, null, "Syncing lists…") }, "Sync now")),
    recordCards("list", "Add list"),
    cNum("list_sync_interval_hours", "Sync every", "How often lists are checked.", "h", 6, 168),
    advanced ? cSelect("list_sync_clean_library", "Clean library", "What to do with movies that leave every list.", [["Disabled", "Do nothing"], ["LogOnly", "Just log it"], ["KeepAndUnmonitor", "Keep and unmonitor"], ["RemoveAndKeep", "Remove, keep files"], ["RemoveAndDelete", "Remove and delete files"]]) : null,
    ...head("Exclusions", "Movies lists won't add again (you deleted them with 'add exclusion')."), excl);
}

async function sConnect(body) {
  add(body, ...head("Connections", "Messages when things happen (Discord, Telegram, ntfy…), library refreshes for Plex/Jellyfin/Emby/Kodi, webhooks and scripts."), recordCards("notification", "Add connection"));
}

async function sMetadata(body) {
  add(body, ...head("Kodi / Emby / Jellyfin metadata", "Write movie.nfo files and artwork next to your movies so media servers pick up the same info."),
    cToggle("kodi_metadata", "Write metadata", "Create an .nfo for each movie when it's imported or refreshed."),
    cToggle("kodi_movie_images", "Movie images", "Save poster.jpg and fanart.jpg in the movie folder."),
    cToggle("kodi_collection_name", "Collection name", "Include the TMDb collection as the movie's set."),
    cToggle("kodi_url_in_nfo", "TMDb URL in .nfo", "Add the TMDb link at the end (helps Kodi's scraper)."),
    cText("kodi_language", "Metadata language", "Language for the .nfo text (e.g. en)."));
}

async function sTags(body) {
  await loadLists(["tag", "indexer", "client", "notification", "list", "delay", "restriction"], true);
  const movies = await api("movies");
  const uses = (id) => {
    const n = (k) => (Turks.lists[k] || []).filter((x) => (x.tags || []).includes(id)).length;
    const parts = [["movies", movies.filter((m) => m.tags.includes(id)).length], ["indexers", n("indexer")], ["clients", n("client")], ["connections", n("notification")], ["lists", n("list")], ["delay profiles", n("delay")], ["release profiles", n("restriction")]].filter((p) => p[1]);
    return parts.map(([l, c]) => `${c} ${l}`).join(" · ") || "unused";
  };
  const input = el("input", { type: "text", placeholder: "New tag" });
  add(body, ...head("Tags", "Tags connect movies to indexers, download clients, delay/release profiles and connections that have the same tag."),
    el("div.row", input, el("button.primary", { onclick: async () => {
      const label = input.value.trim().toLowerCase();
      if (label && (await act("save", { id: 0, label }, { kind: "tag" }))) { invalidate("tag"); rerender(); }
    } }, "Add tag")),
    el("table", el("tbody", Turks.lists.tag.map((t) => el("tr", el("td", el("span.tag.info", t.label)), el("td.dim.small", uses(t.id)),
      el("td.right", el("button.x", { onclick: async () => { if (await confirmBox(`Delete tag ${esc(t.label)}?`, "It's removed from everything that uses it.", "Delete", true)) { await act("delete", {}, { kind: "tag", id: t.id }); invalidate("tag"); rerender(); } } }, "×")))))));
}

// ---------------------------------------------------------------- general / ui

async function sGeneral(body, advanced) {
  const tmdbMsg = el("div.notice.hidden");
  const st = Turks.state || {};
  const keyInput = el("input", { type: "text", value: Turks.cfg.api_key, readonly: true, style: { width: "300px" } });
  const lan = `http://${location.hostname}:${Turks.cfg.api_port}`;
  const update = el("div");
  add(body, 
    ...head("Metadata (TMDb)", "Movie info and artwork come from The Movie Database."),
    field("TMDb API key", "Free at themoviedb.org › Settings › API. The v3 key or the v4 read access token both work.", [
      el("input", { type: "password", value: Turks.cfg.tmdb_api_key, autocomplete: "new-password", oninput: (e) => saveSoon("tmdb_api_key", e.target.value.trim(), poll) }),
      el("button", { onclick: async () => {
        tmdbMsg.className = "notice"; tmdbMsg.replaceChildren(el("span.spin"), " Checking…");
        try { const r = await api("test", { key: Turks.cfg.tmdb_api_key }, { kind: "tmdb" }); tmdbMsg.className = "notice good"; tmdbMsg.textContent = r.message; }
        catch (e) { tmdbMsg.className = "notice bad"; tmdbMsg.textContent = e.message; }
      } }, "Test")]),
    tmdbMsg,
    cText("metadata_language", "Metadata language", "Language for titles and overviews (e.g. en, de, fr, pt-BR)."),
    cText("certification_country", "Certification country", "Country for age ratings and release dates (e.g. US, GB, DE)."),
    ...head("API", "Lets Prowlarr, Overseerr/Jellyseerr and other apps talk to Turks the way they'd talk to Radarr."),
    cToggle("api_enabled", "Enable API", "Changes take effect after a restart."),
    cNum("api_port", "Port", "7878 is Radarr's port; change it if Radarr is also running on this NAS.", "", 1024, 65535),
    cToggle("api_bind_all", "Allow other computers", "Listen on the network, not just this NAS. Needed for apps on other machines or in Docker."),
    field("API key", "Other apps use this to connect.", [keyInput, el("button", { onclick: () => { keyInput.select(); navigator.clipboard && navigator.clipboard.writeText(keyInput.value); toast("Copied"); } }, "Copy"),
      el("button", { onclick: async () => { if (await confirmBox("New API key?", "Apps using the old key stop working until you update them.", "Regenerate", true)) { const r = await act("config/apikey", {}); if (r) { keyInput.value = r.api_key; Turks.cfg.api_key = r.api_key; } } } }, "Regenerate")]),
    el("div.dim.small", "Address for other apps: ", el("span.mono", lan), Turks.cfg.api_bind_all ? "" : " (turn on 'Allow other computers' first)"),
    advanced ? cText("url_base", "URL base", "For reverse proxies, e.g. /turks") : null,
    ...head("Backups", "Turks backs up its database and settings automatically."),
    cPath("backup_dir", "Backup folder", ""),
    cNum("backup_interval_days", "Interval", "Days between automatic backups.", "days", 1, 7),
    cNum("backup_retention_days", "Retention", "Automatic backups older than this are deleted.", "days", 1, 90),
    ...head("Updates", "New versions come from GitHub."),
    cToggle("auto_update", "Install updates automatically", "Needs the one-time Task Scheduler setup below."),
    update,
    ...(advanced ? [
      ...head("Other"),
      cText("proxy_url", "Proxy", "http://host:port for all outgoing connections. Empty = none."),
      cText("ffprobe_path", "ffprobe", st.ffprobe ? "Found: " + st.ffprobe : "Not found. Install ffmpeg (e.g. from SynoCommunity) or enter the path."),
      cToggle("log_debug", "Debug logging", "Write more detail to the log."),
    ] : []),
  );
  updatePanel(update);
}

/** Update status + the DSM Task Scheduler instructions. */
async function updatePanel(box) {
  let u;
  try { u = await api("update"); } catch (e) { box.replaceChildren(el("div.dim.small", "Update status isn't available.")); return; }
  const last = u.last_install;
  box.replaceChildren(
    el("div.row",
      u.available ? el("span.warn", `Turks ${u.latest} is available (you have ${u.current}).`) : el("span", u.latest ? `Turks ${u.current} is up to date.` : `Turks ${u.current}`),
      u.checking ? el("span.spin") : null, u.error ? el("span.bad.small", u.error) : null,
      el("span.grow"),
      u.page ? el("a.small", { href: u.page, target: "_blank" }, "Release notes") : null,
      el("button.small", { onclick: async () => { await act("update/check", {}); setTimeout(() => updatePanel(box), 4000); } }, "Check now")),
    el("div.dim.small", u.checked_at ? "Checked " + ago(u.checked_at) : ""),
    last && last.time ? el("div.small." + (last.ok ? "good" : "bad"), `Last automatic run ${ago(last.time)}: ${last.message}`) : null,
    el("div.notice", el("b", "One-time setup. "), "DSM only lets packages install updates through a scheduled task:",
      el("ol",
        el("li", el("b", "Control Panel"), " → ", el("b", "Task Scheduler"), " → ", el("b", "Create"), " → ", el("b", "Scheduled Task"), " → ", el("b", "User-defined script")),
        el("li", el("b", "General:"), " name it ", el("i", "Turks update"), ", set ", el("b", "User"), " to ", el("b", "root")),
        el("li", el("b", "Schedule:"), " Daily, at a quiet time such as 4:25"),
        el("li", el("b", "Task Settings"), " → ", el("b", "Run command"), ", paste:", el("input.command", { type: "text", readonly: true, value: u.command, onclick: (e) => e.target.select() })),
        el("li", "Click ", el("b", "OK"), ". To update right now, select the task and click ", el("b", "Run"), "."))),
  );
}

async function sUi(body) {
  add(body, ...head("Look"),
    field("Theme", "FFVII menu: blue windows and the pointing hand. Box of X: a green dashboard with a glowing orb; pick a page from the home screen, and BACK (Esc or the mouse's back button) returns to it.",
      select([["ff", "FFVII menu"], ["green", "Box of X"]], Turks.cfg.theme || "ff", async (v) => {
        const r = await saveConfig({ theme: v });
        if (r) applyTheme(true);
      })),
    ...head("Calendar"),
    cSelect("first_day_of_week", "First day of the week", "", [[0, "Sunday"], [1, "Monday"]]),
    ...head("Dates and times"),
    cToggle("time_24h", "24-hour time", ""),
    cToggle("show_relative_dates", "Relative dates", "Show '3 h ago' instead of a date for recent things."),
    cToggle("runtime_hours_minutes", "Runtime as hours and minutes", "2h 16m instead of 136 min."),
    el("div.dim.small", { style: { marginTop: "10px" } }, "Dates follow your browser's language and region."));
}
