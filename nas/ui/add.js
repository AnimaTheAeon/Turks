"use strict";
/* Add Movie (TMDb search + library import), Discover and Collections. */

/** The add dialog for a TMDb hit. Resolves the new movie or null. */
function addDialog(hit) {
  return new Promise(async (resolve) => {
    await loadLists(["profile", "root", "tag"]);
    const cfg = Turks.cfg;
    if (!Turks.lists.root.length) {
      toast("Add a root folder in Settings › Media Management first.", true);
      go("settings", "media");
      return resolve(null);
    }
    const rec = {
      tmdb_id: hit.tmdb_id,
      root_folder: Turks.lists.root.some((r) => r.path === cfg.last_add_root) ? cfg.last_add_root : Turks.lists.root[0].path,
      quality_profile_id: byId(Turks.lists.profile, cfg.last_add_profile) ? cfg.last_add_profile : (Turks.lists.profile[0] || {}).id,
      minimum_availability: cfg.last_add_availability || "Released",
      monitor: "MovieOnly",
      tags: [],
      search: cfg.last_add_search !== false,
    };
    let added = null;
    const thumb = el("div.thumb");
    thumb.append(img("poster", hit.tmdb_id, hit.poster_path));
    const m = openModal({
      title: `${hit.title}${hit.year ? " (" + hit.year + ")" : ""}`,
      body: [
        el("div.hit", { style: { border: 0, background: "none", padding: 0 } }, thumb, el("div.body", el("div.o", { style: { maxHeight: "none" } }, hit.overview || ""))),
        ...specRows(rec, [
          ["root_folder", "Root folder", "select", "", rootOptions],
          ["quality_profile_id", "Quality profile", "select", "", profileOptions],
          ["minimum_availability", "Minimum availability", "select", "When Turks starts looking for it.", availOptions],
          ["monitor", "Monitor", "select", "", [["MovieOnly", "Movie only"], ["MovieAndCollection", "Movie and collection"], ["None", "None"]]],
          ["tags", "Tags", "tags"],
          ["search", "Start search for the movie", "bool"],
        ]),
      ],
      buttons: [el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: async (e) => {
        e.target.disabled = true;
        added = await act("add", rec, null, (r) => `Added ${r.title}`);
        e.target.disabled = false;
        if (added) { await loadConfig(); m.close(); }
      } }, "Add Movie")],
      onclose: () => resolve(added),
    });
  });
}

function hitRow(h, inLib, onAdded) {
  const thumb = el("div.thumb");
  if (h.poster_path) thumb.append(img("poster", h.tmdb_id, h.poster_path));
  const btn = inLib
    ? el("button", { onclick: () => go("movies", inLib.id) }, "In library ›")
    : el("button.primary", { onclick: async () => { const m = await addDialog(h); if (m) { btn.replaceWith(el("button", { onclick: () => go("movies", m.id) }, "In library ›")); if (onAdded) onAdded(m); } } }, "+ Add");
  return el("div.hit", thumb, el("div.body",
    el("div.row", { style: { margin: 0 } }, el("span.t", h.title), el("span.dim", h.year || ""), el("span.grow"), h.vote ? el("span.dim.small", "★ " + h.vote.toFixed(1)) : null, btn),
    h.original_title && h.original_title !== h.title ? el("div.dim.small", h.original_title) : null,
    el("div.o", h.overview || "")));
}

Turks.pages.add = {
  async render(main, arg) {
    const tab = arg === "import" ? "import" : "search";
    const body = el("div.scroll");
    main.append(el("div.page",
      el("div.page-head", el("h1", "Add Movie"), el("span.grow"),
        el("div.tabs", pill("Search TMDb", tab === "search", () => go("add")), pill("Library import", tab === "import", () => go("add", "import")))),
      el("div.divider"), body));
    if (tab === "import") return libraryImport(body);

    const movies = await api("movies");
    const results = el("div.hits");
    let timer = null;
    const input = el("input", { type: "search", placeholder: "Movie name, tmdb:603 or imdb:tt0133093", style: { width: "100%", fontSize: "16px" },
      oninput: () => { clearTimeout(timer); timer = setTimeout(run, 450); }, onkeydown: (e) => { if (e.key === "Enter") { clearTimeout(timer); run(); } } });
    body.append(input, el("div", { style: { height: "10px" } }), results);
    input.focus();
    async function run() {
      const term = input.value.trim();
      if (!term) { results.replaceChildren(); return; }
      results.replaceChildren(el("div.row", el("span.spin"), "Searching…"));
      try {
        const hits = await api("tmdb/search", undefined, { term });
        results.replaceChildren(...(hits.length ? hits.map((h) => hitRow(h, movies.find((x) => x.tmdb_id === h.tmdb_id))) : [el("div.empty", "Nothing found.")]));
      } catch (e) { results.replaceChildren(el("div.notice.bad", e.message)); }
    }
  },
};

async function libraryImport(body) {
  await loadLists(["root", "profile"]);
  const roots = Turks.lists.root;
  if (!roots.length) { body.append(el("div.empty", el("h2", "No root folders"), "Add one in ", el("button.link", { onclick: () => go("settings", "media") }, "Settings › Media Management"), " first.")); return; }
  const opts = { root_folder: roots[0].path, quality_profile_id: (Turks.lists.profile[0] || {}).id, minimum_availability: "Released", monitor: "MovieOnly" };
  const list = el("div");
  const importBtn = el("button.big.primary", { disabled: true }, "Import");
  body.append(el("p.sub", "Folders in a root folder that aren't movies in Turks yet. Turks matches each to TMDb; check the matches, then import. Folders keep their names."),
    el("div.toolbar",
      select(rootOptions(), opts.root_folder, (v) => { opts.root_folder = v; scan(); }),
      select(profileOptions(), opts.quality_profile_id, (v) => { opts.quality_profile_id = Number(v); }),
      select(availOptions(), opts.minimum_availability, (v) => { opts.minimum_availability = v; }),
      check("Monitored", true, (v) => { opts.monitor = v ? "MovieOnly" : "None"; }),
      el("span.grow"), importBtn),
    el("div.divider"), list);
  let rows = [];
  const refresh = () => { const n = rows.filter((r) => r.on && r.hit).length; importBtn.disabled = !n; importBtn.textContent = `Import ${n} movies`; };
  async function scan() {
    list.replaceChildren(el("div.row", el("span.spin"), "Looking at folders…"));
    const found = await api("import/unmapped", undefined, { root: opts.root_folder });
    if (!found.length) { list.replaceChildren(el("div.empty", "Every folder here is already in Turks.")); rows = []; refresh(); return; }
    rows = found.map((f) => ({ f, on: false, hit: null, hits: [] }));
    const tbody = el("tbody");
    list.replaceChildren(el("table", el("thead", el("tr", el("th"), el("th", "Folder"), el("th", "Match"), el("th", "Files"))), tbody));
    for (const r of rows) {
      const matchCell = el("td", el("span.spin"));
      const box = check("", false, (v) => { r.on = v; refresh(); });
      tbody.append(el("tr", el("td", box), el("td.wrap", r.f.name), matchCell, el("td", r.f.video_count)));
      r.draw = () => {
        if (!r.hits.length) { matchCell.replaceChildren(el("span.warn", "No match"), " ", el("button.small", { onclick: () => manualMatch(r) }, "Search…")); return; }
        matchCell.replaceChildren(select(r.hits.map((h, i) => [i, `${h.title} (${h.year || "?"})`]), r.hits.indexOf(r.hit), (v) => { r.hit = r.hits[Number(v)]; }), " ", el("button.small", { onclick: () => manualMatch(r) }, "Search…"));
      };
      r.box = box;
    }
    // Match a few at a time so TMDb isn't flooded.
    let i = 0;
    const worker = async () => {
      while (i < rows.length) {
        const r = rows[i++];
        try { r.hits = await api("import/match", undefined, { title: r.f.parsed_title, year: r.f.parsed_year || "", tmdb: r.f.tmdb_id_hint || "" }); } catch (e) { r.hits = []; }
        r.hit = r.hits[0] || null;
        r.on = !!r.hit;
        r.box.querySelector("input").checked = r.on;
        r.draw();
        refresh();
      }
    };
    await Promise.all([worker(), worker(), worker()]);
  }
  function manualMatch(r) {
    const input = el("input.wide", { type: "search", value: r.f.parsed_title });
    const out = el("div.hits");
    const go2 = async () => {
      out.replaceChildren(el("span.spin"));
      const hits = await api("tmdb/search", undefined, { term: input.value }).catch((e) => { toast(e.message, true); return []; });
      out.replaceChildren(...hits.map((h) => el("div.hit", { style: { cursor: "pointer" }, onclick: () => { r.hits = [h, ...r.hits.filter((x) => x.tmdb_id !== h.tmdb_id)]; r.hit = h; r.on = true; r.box.querySelector("input").checked = true; r.draw(); refresh(); m.close(); } },
        el("div.body", el("span.t", h.title), " ", el("span.dim", h.year || ""), el("div.o", h.overview)))));
    };
    input.onkeydown = (e) => { if (e.key === "Enter") go2(); };
    const m = openModal({ title: "Match " + r.f.name, body: [input, el("div", { style: { height: "8px" } }), out], buttons: [el("button", { onclick: () => m.close() }, "Cancel")] });
    go2();
  }
  importBtn.onclick = async () => {
    const items = rows.filter((r) => r.on && r.hit).map((r) => ({ tmdb_id: r.hit.tmdb_id, path: r.f.path }));
    if (await act("import/library", { ...opts, items }, null, `Importing ${items.length} movies in the background…`)) scan();
  };
  scan();
}

Turks.pages.discover = {
  async render(main, arg) {
    const which = arg || "popular";
    const body = el("div.scroll");
    main.append(el("div.page",
      el("div.page-head", el("h1", "Discover"), el("span.grow"),
        el("div.tabs", ...[["popular", "Popular"], ["top_rated", "Top rated"], ["now_playing", "In cinemas"], ["upcoming", "Upcoming"]].map(([k, l]) => pill(l, k === which, () => go("discover", k))))),
      el("p.sub", "From TMDb. Click a poster to add it."),
      el("div.divider"), body));
    body.append(el("div.row", el("span.spin"), "Loading…"));
    const [hits, movies] = await Promise.all([api("tmdb/discover", undefined, { which }), api("movies")]);
    body.replaceChildren(el("div.posters", hits.map((h) => {
      const inLib = movies.find((x) => x.tmdb_id === h.tmdb_id);
      const art = posterArt(h);
      if (inLib) art.append(el("div.bar.st-" + movieState(inLib)[0]));
      const p = el("div.poster", { title: h.overview, onclick: async () => { if (inLib) return go("movies", inLib.id); const m = await addDialog(h); if (m) { art.append(el("div.bar.st-missing")); p.onclick = () => go("movies", m.id); } } },
        art, el("div.name", h.title), el("div.yr", inLib ? "In library" : (h.year || "") + (h.vote ? " · ★ " + h.vote.toFixed(1) : "")));
      return p;
    })));
  },
};

Turks.pages.collections = {
  async render(main) {
    await loadLists(["collection", "profile", "root", "tag"], true);
    const movies = await api("movies");
    const body = el("div.scroll");
    main.append(el("div.page",
      el("div.page-head", el("h1", "Collections"), el("span.grow"),
        el("button", { onclick: () => act("command", { name: "collections" }, null, "Refreshing collections…") }, "Refresh")),
      el("p.sub", "TMDb collections of movies in your library. Follow one to add its other movies automatically."),
      el("div.divider"), body));
    // Collections Turks follows, plus ones only known from movies.
    const followed = Turks.lists.collection;
    const known = new Map();
    for (const m of movies) if (m.collection && !followed.some((c) => c.tmdb_id === m.collection.tmdb_id)) known.set(m.collection.tmdb_id, m.collection);
    if (!followed.length && !known.size) { body.append(el("div.empty", el("h2", "No collections"), "Movies that belong to a collection bring it here.")); return; }
    for (const c of followed) {
      const have = c.parts.filter((p) => movies.some((m) => m.tmdb_id === p.tmdb_id)).length;
      body.append(el("div.party", { style: { cursor: "default" } },
        el("div.body",
          el("div.row", { style: { margin: 0 } }, el("span.name", c.title), el("span.grow"),
            el("button.toggle." + (c.monitored ? "on" : "off"), { onclick: async () => { if (await act("save", { ...c, monitored: !c.monitored }, { kind: "collection" })) rerender(); } }, c.monitored ? "MONITORED" : "UNMONITORED"),
            el("button.small", { onclick: () => editCollection(c) }, "Edit"),
            el("button.small", { onclick: async () => { const r = await act("collection/addmissing", { id: c.id }); if (r) { toast(`Added ${r.added} movies.`); rerender(); } } }, "Add missing")),
          el("div.stats", el("span", hp(c.parts.length ? have / c.parts.length : 0, have === c.parts.length), " ", `${have} of ${c.parts.length} in library`), el("span.dim", profileName(c.quality_profile_id)), el("span.dim", c.root_folder)),
          el("div.posters", { style: { "--poster": "96px", marginTop: "8px" } }, c.parts.map((p) => {
            const inLib = movies.find((x) => x.tmdb_id === p.tmdb_id);
            const art = posterArt(p);
            art.append(el("div.bar.st-" + (inLib ? movieState(inLib)[0] : "unmonitored")));
            return el("div.poster", { title: p.title, onclick: () => (inLib ? go("movies", inLib.id) : addDialog(p).then((m) => m && rerender())) }, art, el("div.yr", p.year || ""));
          })))));
    }
    if (known.size) {
      body.append(el("div.head", "Not followed"));
      body.append(el("table", el("tbody", [...known.values()].map((c) => el("tr",
        el("td", c.title),
        el("td.right", el("button.small.primary", { onclick: async () => {
          const m0 = movies.find((m) => m.collection && m.collection.tmdb_id === c.tmdb_id);
          if (await act("collection/follow", { tmdb_id: c.tmdb_id, monitored: true, quality_profile_id: m0.quality_profile_id, root_folder: m0.root_folder }, null, "Following " + c.title)) rerender();
        } }, "Follow")))))));
    }
  },
};

function editCollection(c) {
  const rec = JSON.parse(JSON.stringify(c));
  const m = openModal({
    title: c.title,
    body: specRows(rec, [
      ["monitored", "Monitored", "bool", "Add new movies from this collection automatically."],
      ["search_on_add", "Search when added", "bool"],
      ["root_folder", "Root folder", "select", "", rootOptions],
      ["quality_profile_id", "Quality profile", "select", "", profileOptions],
      ["minimum_availability", "Minimum availability", "select", "", availOptions],
      ["tags", "Tags", "tags"],
    ]),
    buttons: [
      el("button.danger", { onclick: async () => { if (await act("delete", {}, { kind: "collection", id: c.id }, "Stopped following")) { m.close(); rerender(); } } }, "Unfollow"),
      el("button", { onclick: () => m.close() }, "Cancel"),
      el("button.big.primary", { onclick: async () => { if (await act("save", rec, { kind: "collection" })) { m.close(); rerender(); } } }, "Save"),
    ],
  });
}
