"use strict";
/* Movies: library views, the movie details screen, interactive search,
   manual import and the mass editor. */

const store = {
  get(k, d) { try { const v = localStorage.getItem("turks." + k); return v === null ? d : JSON.parse(v); } catch (e) { return d; } },
  set(k, v) { try { localStorage.setItem("turks." + k, JSON.stringify(v)); } catch (e) { /* private mode */ } },
};

const FILTERS = [
  ["all", "All"], ["monitored", "Monitored"], ["unmonitored", "Unmonitored"], ["missing", "Missing"],
  ["downloaded", "Downloaded"], ["downloading", "Downloading"], ["cutoff", "Cutoff unmet"],
];
const SORTS = [
  ["title", "Title"], ["year", "Year"], ["added", "Added"], ["rating", "Rating"], ["size", "Size on disk"],
  ["runtime", "Runtime"], ["release", "Release date"], ["popularity", "Popularity"],
];

function movieState(m) {
  if (m.downloading) return ["downloading", "DOWNLOADING"];
  if (m.file) return m.cutoff_unmet ? ["cutoff", "CUTOFF UNMET"] : ["downloaded", "DOWNLOADED"];
  if (!m.monitored) return ["unmonitored", "UNMONITORED"];
  return m.available ? ["missing", "MISSING"] : ["unavailable", "NOT AVAILABLE"];
}
const stateColor = { downloading: "info", cutoff: "warn", downloaded: "good", unmonitored: "dim", missing: "bad", unavailable: "dim" };

function posterArt(m, cls = "art") {
  const art = el("div." + cls, el("div.ph", m.title, el("small", m.year || "")));
  if (m.poster_path || m.tmdb_id) art.append(img("poster", m.tmdb_id, m.poster_path));
  return art;
}

function releaseDate(m) { return m.digital_release || m.physical_release || m.in_cinemas || ""; }

function sortMovies(list, key, desc) {
  const f = {
    title: (m) => (m.sort_title || m.title || "").toLowerCase(),
    year: (m) => m.year, added: (m) => m.added || "", rating: (m) => (m.ratings && m.ratings.tmdb) || 0,
    size: (m) => (m.file ? m.file.size : 0), runtime: (m) => m.runtime, release: releaseDate, popularity: (m) => m.popularity,
  }[key] || ((m) => m.title);
  list.sort((a, b) => { const x = f(a), y = f(b); return x < y ? -1 : x > y ? 1 : 0; });
  if (desc) list.reverse();
  return list;
}

Turks.pages.movies = {
  async render(main, arg) {
    if (arg) return renderDetail(main, arg);
    let movies = await api("movies");
    await loadLists(["profile", "tag", "root"]);
    const ui = { filter: store.get("filter", "all"), sort: store.get("sort", "title"), desc: store.get("desc", false), view: store.get("view", "posters"), q: "", selecting: false, sel: new Set() };
    const count = el("span.dim");
    const content = el("div.scroll");
    const massBar = el("div.toolbar.hidden");
    const search = el("input", { type: "search", placeholder: "Find: title, year, genre, tag", style: { width: "240px" }, oninput: () => { ui.q = search.value.toLowerCase(); draw(); } });

    const viewPills = el("span.row", { style: { margin: 0 } });
    const drawPills = () => viewPills.replaceChildren(...[["posters", "Posters"], ["party", "Party"], ["table", "Table"]].map(([k, l]) => pill(l, ui.view === k, () => { ui.view = k; store.set("view", k); drawPills(); draw(); })));
    drawPills();

    main.append(el("div.page",
      el("div.page-head", el("h1", "Movies"), search,
        select(FILTERS, ui.filter, (v) => { ui.filter = v; store.set("filter", v); draw(); }),
        select(SORTS.map(([k, l]) => [k, "Sort: " + l]), ui.sort, (v) => { ui.sort = v; store.set("sort", v); draw(); }),
        el("button", { title: "Reverse order", onclick: () => { ui.desc = !ui.desc; store.set("desc", ui.desc); draw(); } }, "⇅"),
        viewPills),
      el("div.toolbar",
        el("button", { onclick: () => act("command", { name: "refresh", ids: [] }, null, "Refreshing all movies…") }, "Refresh & scan"),
        el("button", { onclick: () => act("command", { name: "rss" }, null, "RSS sync started") }, "RSS sync"),
        el("button", { onclick: () => act("command", { name: "missing" }, null, "Searching all missing movies…") }, "Search all missing"),
        el("button", { onclick: () => { ui.selecting = !ui.selecting; ui.sel.clear(); drawMass(); draw(); } }, "Select"),
        el("span.grow"), count),
      massBar,
      el("div.divider"),
      content));

    function visible() {
      const q = ui.q;
      let list = movies.filter((m) => {
        const st = movieState(m)[0];
        switch (ui.filter) {
          case "monitored": if (!m.monitored) return false; break;
          case "unmonitored": if (m.monitored) return false; break;
          case "missing": if (m.file || !m.monitored) return false; break;
          case "downloaded": if (!m.file) return false; break;
          case "downloading": if (st !== "downloading") return false; break;
          case "cutoff": if (!m.cutoff_unmet) return false; break;
        }
        if (!q) return true;
        const hay = [m.title, m.year, ...(m.genres || []), ...(m.tags || []).map(tagName), m.studio, m.imdb_id].join(" ").toLowerCase();
        return q.split(/\s+/).every((w) => hay.includes(w));
      });
      return sortMovies(list, ui.sort, ui.desc);
    }

    function selBox(m) {
      return el("input.sel", { type: "checkbox", checked: ui.sel.has(m.id), onclick: (e) => { e.stopPropagation(); e.target.checked ? ui.sel.add(m.id) : ui.sel.delete(m.id); drawMass(); } });
    }

    function open(m) { if (ui.selecting) { ui.sel.has(m.id) ? ui.sel.delete(m.id) : ui.sel.add(m.id); drawMass(); draw(); } else go("movies", m.id); }

    function draw() {
      const list = visible();
      count.textContent = `${list.length} of ${movies.length}`;
      if (!movies.length) {
        content.replaceChildren(el("div.empty", el("h2", "No movies yet"), "Add some with ", el("button.link", { onclick: () => go("add") }, "Add Movie"), " or import the folders you already have."));
        return;
      }
      if (ui.view === "posters") {
        content.replaceChildren(el("div.posters", list.map((m) => {
          const [st, label] = movieState(m);
          const art = posterArt(m);
          art.append(el("div.bar.st-" + st));
          return el("div.poster" + (ui.sel.has(m.id) ? ".selected" : ""), { onclick: () => open(m), title: `${m.title} (${m.year})\n${label}` },
            ui.selecting ? selBox(m) : null, art, el("div.name", m.title), el("div.yr", m.year || ""));
        })));
      } else if (ui.view === "party") {
        content.replaceChildren(...list.map((m) => {
          const [st, label] = movieState(m);
          const thumb = el("div.thumb");
          thumb.append(img("poster", m.tmdb_id, m.poster_path));
          return el("div.party", { onclick: () => open(m) }, ui.selecting ? selBox(m) : null, thumb,
            el("div.body",
              el("div.row", { style: { margin: 0 } }, el("span.name", m.title), el("span.dim", m.year || ""), el("span.grow"), el("span.tag." + stateColor[st], label)),
              el("div.stats",
                el("span", el("span.label", "PROFILE "), profileName(m.quality_profile_id)),
                el("span", el("span.label", "QUALITY "), m.file ? m.file.quality_name : "—"),
                el("span", el("span.label", "SIZE "), m.file ? size(m.file.size) : "—"),
                m.runtime ? el("span", el("span.label", "TIME "), runtime(m.runtime)) : null),
              el("div.stats", hp(m.file ? 1 : 0, m.file && !m.cutoff_unmet), el("span.dim.small", m.path))));
        }));
      } else {
        content.replaceChildren(el("table",
          el("thead", el("tr", ui.selecting ? el("th") : null, ...["Title", "Year", "Profile", "Quality", "Size", "Runtime", "Status", "Added"].map((h) => el("th", h)))),
          el("tbody", list.map((m) => {
            const [st, label] = movieState(m);
            return el("tr.click", { onclick: () => open(m) }, ui.selecting ? el("td", selBox(m)) : null,
              el("td", m.title), el("td", m.year || ""), el("td", profileName(m.quality_profile_id)),
              el("td", m.file ? m.file.quality_name : ""), el("td.nowrap", m.file ? size(m.file.size) : ""),
              el("td.nowrap", runtime(m.runtime)), el("td", el("span.tag." + stateColor[st], label)), el("td.nowrap.dim", m.added ? dateStr(m.added) : ""));
          }))));
      }
    }

    function drawMass() {
      massBar.classList.toggle("hidden", !ui.selecting);
      if (!ui.selecting) return;
      const ids = [...ui.sel];
      const n = ids.length;
      const run = async (body, msg) => { if (await act("movies/edit", { ids, ...body }, null, msg)) { movies = await api("movies"); draw(); } };
      massBar.replaceChildren(
        el("span.label", n + " selected"),
        el("button.small", { onclick: () => { visible().forEach((m) => ui.sel.add(m.id)); drawMass(); draw(); } }, "All"),
        el("button.small", { onclick: () => { ui.sel.clear(); drawMass(); draw(); } }, "None"),
        el("button", { disabled: !n, onclick: () => run({ monitored: true }, "Monitoring " + n) }, "Monitor"),
        el("button", { disabled: !n, onclick: () => run({ monitored: false }, "Unmonitored " + n) }, "Unmonitor"),
        select([["", "Profile…"], ...profileOptions()], "", (v) => v && run({ quality_profile_id: Number(v) }, "Profile changed")),
        select([["", "Availability…"], ...AVAIL], "", (v) => v && run({ minimum_availability: v }, "Availability changed")),
        el("button", { disabled: !n, onclick: () => massTags(ids, run) }, "Tags…"),
        el("button", { disabled: !n, onclick: () => massRoot(ids, run) }, "Move…"),
        el("button", { disabled: !n, onclick: () => act("command", { name: "search", ids }, null, "Searching " + n + " movies…") }, "Search"),
        el("button", { disabled: !n, onclick: () => act("command", { name: "refresh", ids }, null, "Refreshing…") }, "Refresh"),
        el("button", { disabled: !n, onclick: () => renamePreview(ids) }, "Rename…"),
        el("button.danger", { disabled: !n, onclick: async () => { if (await deleteMovies(ids)) { ui.sel.clear(); movies = await api("movies"); drawMass(); draw(); } } }, "Delete…"),
      );
    }

    draw();
    // Keep download state fresh while the list is open.
    every(15000, async () => { movies = await api("movies"); draw(); });
  },
};

function massTags(ids, run) {
  let tags = [];
  let mode = "add";
  const m = openModal({
    title: "Tags for " + ids.length + " movies",
    body: [field("Tags", "", tagPicker([], (t) => { tags = t; })), field("Apply", "", select([["add", "Add"], ["remove", "Remove"], ["replace", "Replace"]], mode, (v) => { mode = v; }))],
    buttons: [el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: () => { m.close(); run({ tags, tag_mode: mode }, "Tags changed"); } }, "Apply")],
    size: "narrow",
  });
}

function massRoot(ids, run) {
  let root = (Turks.lists.root[0] || {}).path || "";
  let move = true;
  const m = openModal({
    title: "Move " + ids.length + " movies",
    body: [field("Root folder", "", select(rootOptions(), root, (v) => { root = v; })), field("Move files", "Off only changes where Turks looks.", toggleBtn(move, (v) => { move = v; }))],
    buttons: [el("button", { onclick: () => m.close() }, "Cancel"), el("button.big.primary", { onclick: () => { m.close(); run({ root_folder: root, move_files: move }, "Moving…"); } }, "Move")],
    size: "narrow",
  });
}

async function deleteMovies(ids) {
  let files = false, excl = false;
  const extra = el("div", check("Delete the movie folders and files too", false, (v) => { files = v; }), el("br"), check("Don't add again from import lists", false, (v) => { excl = v; }));
  const ok = await confirmBox(ids.length === 1 ? "Delete this movie?" : `Delete ${ids.length} movies?`, "They're removed from Turks.", "Delete", true, extra);
  if (!ok) return false;
  let n = 0;
  for (const id of ids) if (await act("delete", { delete_files: files, add_exclusion: excl }, { kind: "movie", id })) n++;
  toast(`Deleted ${n} movie${n === 1 ? "" : "s"}.`);
  return true;
}

async function renamePreview(ids) {
  const list = await api("rename", undefined, { ids: ids.join(",") });
  const m = openModal({
    title: "Rename files",
    body: list.length ? el("table", el("thead", el("tr", el("th", "Now"), el("th", "Will be"))), el("tbody", list.map((r) => el("tr", el("td.wrap.dim", r.existing), el("td.wrap", r.new)))))
      : el("div.empty", "Every file already matches the naming format."),
    buttons: [el("button", { onclick: () => m.close() }, "Close"), list.length ? el("button.big.primary", { onclick: () => { m.close(); act("command", { name: "rename", ids }, null, "Renaming…"); } }, `Rename ${list.length}`) : null].filter(Boolean),
    size: "wide",
  });
}

// ---------------------------------------------------------------- details

async function renderDetail(main, id) {
  let d;
  try { d = await api("movie", undefined, { id }); } catch (e) { toast(e.message, true); return go("movies"); }
  await loadLists(["profile", "tag", "root"]);
  const m = d.movie;
  const tab = store.get("detailTab", "files");
  const back = el("div.backdrop");
  if (m.fanart_path) back.style.backgroundImage = `url("${imgUrl("fanart", m.tmdb_id, m.fanart_path)}")`;
  const poster = el("div.poster-big");
  poster.append(img("poster", m.tmdb_id, m.poster_path));
  const [st, label] = movieState({ ...m, file: d.file, downloading: d.downloads.length > 0, available: true, cutoff_unmet: false });
  const r = m.ratings || {};
  const monitorBtn = el("button.toggle." + (m.monitored ? "on" : "off"), { title: "Monitored movies are searched for and upgraded" }, m.monitored ? "MONITORED" : "UNMONITORED");
  monitorBtn.onclick = async () => { const s = await act("save", { id: m.id, monitored: !m.monitored }, { kind: "movie" }); if (s) rerender(); };

  const tabsEl = el("div.tabs");
  const body = el("div.scroll");
  const tabs = [["files", "Files"], ["search", "Search"], ["history", "History"], ["cast", "Cast & Crew"], d.collection || m.collection ? ["collection", "Collection"] : null].filter(Boolean);
  const showTab = (t) => {
    store.set("detailTab", t);
    tabsEl.replaceChildren(...tabs.map(([k, l]) => pill(l, k === t, () => showTab(k))));
    body.replaceChildren();
    ({ files: tabFiles, search: tabSearch, history: tabHistory, cast: tabCast, collection: tabCollection }[t] || tabFiles)(body, d);
  };

  main.append(el("div.page.detail",
    el("div.hero", back, poster, el("div.facts",
      el("div.row", { style: { margin: 0 } }, el("button.small", { onclick: () => go("movies") }, "‹ Movies"), el("span.grow"), monitorBtn),
      el("h1", m.title, " ", el("span.dim", { style: { fontSize: "20px" } }, m.year ? `(${m.year})` : "")),
      el("div.line",
        m.certification ? el("span.tag", m.certification) : null,
        m.runtime ? el("span", runtime(m.runtime)) : null,
        el("span", (m.genres || []).join(", ")),
        r.tmdb ? el("span", "★ " + r.tmdb.toFixed(1) + " TMDb") : null,
        r.imdb ? el("span", "IMDb " + r.imdb.toFixed(1)) : null,
        el("span.tag." + stateColor[st], label)),
      el("div.overview", m.overview || ""),
      el("div.kv",
        el("span.k", "Path"), el("span.ellipsis", m.path),
        el("span.k", "Profile"), el("span", profileName(m.quality_profile_id), " · ", (AVAIL.find((a) => a[0] === m.minimum_availability) || [, ""])[1]),
        el("span.k", "Status"), el("span", STATUS_LABEL[m.status] || m.status, m.in_cinemas ? " · cinemas " + dateStr(m.in_cinemas) : "", m.digital_release ? " · digital " + dateStr(m.digital_release) : "", m.physical_release ? " · disc " + dateStr(m.physical_release) : ""),
        m.tags.length ? el("span.k", "Tags") : null, m.tags.length ? el("span", m.tags.map(tagName).join(", ")) : null),
      el("div.toolbar",
        el("button.primary", { onclick: () => act("command", { name: "search", ids: [m.id] }, null, "Searching…") }, "Search"),
        el("button", { onclick: () => showTab("search") }, "Interactive"),
        el("button", { onclick: () => act("command", { name: "refresh", ids: [m.id] }, null, "Refreshing…") }, "Refresh & scan"),
        el("button", { onclick: () => renamePreview([m.id]) }, "Rename"),
        el("button", { onclick: () => manualImport(m.path, m.id) }, "Manual import"),
        el("button", { onclick: () => editMovie(m) }, "Edit"),
        el("button.danger", { onclick: async () => { if (await deleteMovies([m.id])) go("movies"); } }, "Delete"),
        m.imdb_id ? el("a.small", { href: "https://www.imdb.com/title/" + m.imdb_id, target: "_blank" }, "IMDb") : null,
        el("a.small", { href: "https://www.themoviedb.org/movie/" + m.tmdb_id, target: "_blank" }, "TMDb"),
        m.youtube_trailer ? el("a.small", { href: "https://www.youtube.com/watch?v=" + m.youtube_trailer, target: "_blank" }, "Trailer") : null))),
    el("div.divider"), tabsEl, el("div", { style: { height: "8px" } }), body));
  showTab(tabs.some((t) => t[0] === tab) ? tab : "files");
}

function tabFiles(body, d) {
  const f = d.file;
  for (const dl of d.downloads) {
    body.append(el("div.notice", el("b", dl.state.toUpperCase(), " "), dl.title, " ", hp(dl.size ? 1 - dl.remaining / dl.size : 0), " ", dl.size ? Math.round((1 - dl.remaining / dl.size) * 100) + "%" : ""));
  }
  if (!f) { body.append(el("div.empty", el("h2", "No file yet"), d.movie.monitored ? "Turks will grab it when a release that fits the profile shows up." : "This movie isn't monitored.")); return; }
  const mi = f.media_info || {};
  body.append(el("table", el("tbody",
    el("tr", el("td.label", "File"), el("td.wrap", f.relative_path)),
    el("tr", el("td.label", "Quality"), el("td", d.quality_name, f.quality.revision && f.quality.revision.version > 1 ? " (Proper)" : "")),
    el("tr", el("td.label", "Size"), el("td", size(f.size))),
    el("tr", el("td.label", "Formats"), el("td", d.formats.length ? d.formats.map((x) => el("span.tag.info", x)) : el("span.dim", "none"), " ", el("span.dim", "score " + d.format_score))),
    el("tr", el("td.label", "Languages"), el("td", (f.languages || []).join(", ") || "—")),
    f.release_group ? el("tr", el("td.label", "Group"), el("td", f.release_group)) : null,
    f.edition ? el("tr", el("td.label", "Edition"), el("td", f.edition)) : null,
    el("tr", el("td.label", "Video"), el("td", [mi.resolution, mi.video_codec, mi.video_bit_depth ? mi.video_bit_depth + "-bit" : "", mi.video_dynamic_range].filter(Boolean).join(" · ") || "—")),
    el("tr", el("td.label", "Audio"), el("td", [mi.audio_codec, mi.audio_channels ? mi.audio_channels.toFixed(1) : "", (mi.audio_languages || []).join("/")].filter(Boolean).join(" · ") || "—")),
    (mi.subtitles || []).length ? el("tr", el("td.label", "Subtitles"), el("td", mi.subtitles.join(", "))) : null,
    el("tr", el("td.label", "Added"), el("td", f.date_added ? dateStr(f.date_added) : "")),
    f.scene_name ? el("tr", el("td.label", "Release"), el("td.wrap.dim", f.scene_name)) : null)),
    el("div.toolbar", el("button.danger", { onclick: async () => {
      if (await confirmBox("Delete this file?", esc(f.relative_path) + "<br>It goes to the recycle bin if one is set.", "Delete", true)) {
        if (await act("delete", {}, { kind: "file", id: f.id }, "File deleted")) rerender();
      }
    } }, "Delete file")));
}

async function tabSearch(body, d) {
  const id = d.movie.id;
  const status = el("div.row");
  const table = el("div");
  body.append(status, table);
  let sortKey = "score";
  async function load(start) {
    if (start) await act("command", { name: "interactive", ids: [id] });
    const r = await api("interactive", undefined, { id });
    if (r.none && !start) {
      status.replaceChildren(el("button.primary", { onclick: () => load(true) }, "Search indexers"), el("span.dim", "Look through every indexer and pick a release yourself."));
      return;
    }
    status.replaceChildren(r.running ? el("span.spin") : null, el("span", r.running ? "Searching indexers…" : `${r.candidates.length} releases`),
      ...r.errors.map((e) => el("span.bad.small", e)), el("span.grow"), el("button", { onclick: () => load(true) }, "Search again"));
    const list = [...r.candidates];
    const keys = {
      score: (c) => [c.rejections.length ? 1 : 0, -c.score, -(c.release.seeders || 0)],
      age: (c) => [new Date(c.release.publish_date || 0) * -1], size: (c) => [-c.release.size], seeders: (c) => [-(c.release.seeders || 0)],
      quality: (c) => [-c.parsed.quality.quality],
    };
    list.sort((a, b) => { const x = keys[sortKey](a), y = keys[sortKey](b); for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1; return 0; });
    const th = (k, l) => el("th.sortable", { onclick: () => { sortKey = k; load(false); } }, l + (sortKey === k ? " ▾" : ""));
    table.replaceChildren(list.length ? el("table",
      el("thead", el("tr", el("th", "Release"), th("age", "Age"), th("size", "Size"), th("seeders", "Peers"), th("quality", "Quality"), th("score", "Score"), el("th", "Indexer"), el("th"))),
      el("tbody", list.map((c) => {
        const rel = c.release;
        const rejected = c.rejections.length > 0;
        const ageDays = rel.publish_date ? Math.floor((Date.now() - new Date(rel.publish_date)) / 86400000) : null;
        return el("tr",
          el("td.wrap", el("div.result-title" + (rejected ? ".dim" : ""), rel.info_url ? el("a", { href: rel.info_url, target: "_blank" }, rel.title) : rel.title),
            c.format_names.length ? el("div", c.format_names.map((n) => el("span.tag.info", n))) : null,
            rejected ? el("div.small." + (c.temporarily_rejected ? "warn" : "bad"), c.rejections.map((x) => x.reason).join(" · ")) : null),
          el("td.nowrap", ageDays === null ? "" : ageDays + "d"), el("td.nowrap", size(rel.size)),
          el("td.nowrap", rel.protocol === "Usenet" ? "nzb" : `${rel.seeders ?? "?"}/${rel.peers ?? "?"}`),
          el("td.nowrap", qualityName(c.parsed.quality.quality)), el("td", c.score),
          el("td.small", rel.indexer),
          el("td", el("button.small" + (rejected ? "" : ".primary"), { onclick: async (e) => {
            if (rejected && !(await confirmBox("Grab anyway?", esc(c.rejections.map((x) => x.reason).join("<br>")), "Grab", false))) return;
            e.target.disabled = true;
            if (await act("grab", c, null, "Sent to the download client")) e.target.textContent = "✓";
          } }, "Grab")));
      }))) : (r.running ? null : el("div.empty", "Nothing found.")));
    if (r.running) setTimeout(() => { if (body.isConnected) load(false); }, 2000);
  }
  load(false);
}

async function tabHistory(body, d) {
  const list = d.history;
  if (!list.length) { body.append(el("div.empty", "Nothing has happened to this movie yet.")); return; }
  body.append(historyTable(list, false));
}

function historyTable(list, showMovie, movies) {
  const evColor = { Grabbed: "info", Imported: "good", DownloadFailed: "bad", FileDeleted: "warn", FileRenamed: "info", Ignored: "dim" };
  const evLabel = { Grabbed: "GRABBED", Imported: "IMPORTED", DownloadFailed: "FAILED", FileDeleted: "DELETED", FileRenamed: "RENAMED", Ignored: "IGNORED" };
  return el("table",
    el("thead", el("tr", el("th", "Event"), showMovie ? el("th", "Movie") : null, el("th", "Release"), el("th", "Quality"), el("th", "When"))),
    el("tbody", list.map((h) => {
      const mv = showMovie && movies ? movies.find((x) => x.id === h.movie_id) : null;
      const info = Object.entries(h.data || {}).map(([k, v]) => `${k}: ${v}`).join("\n");
      return el("tr", { title: info },
        el("td", el("span.tag." + (evColor[h.event] || ""), evLabel[h.event] || h.event)),
        showMovie ? el("td", mv ? el("button.link", { onclick: () => go("movies", mv.id) }, mv.title) : "") : null,
        el("td.wrap.small", h.source_title, h.data && h.data.message ? el("div.dim", h.data.message) : null),
        el("td.nowrap", qualityName(h.quality.quality)),
        el("td.nowrap.dim", ago(h.date)));
    })));
}

function tabCast(body, d) {
  const m = d.movie;
  const people = (list) => el("div.people", list.slice(0, 40).map((p) => {
    const face = el("div.face");
    if (p.profile_path) face.style.backgroundImage = `url("${imgUrl("person", p.person_id, p.profile_path)}")`;
    return el("div.person", face, el("div", p.name), el("div.dim", p.role || p.department));
  }));
  if (!(m.cast || []).length && !(m.crew || []).length) { body.append(el("div.empty", "No cast information.")); return; }
  body.append(el("div.head", "Cast"), people(m.cast || []), el("div.head", "Crew"), people(m.crew || []));
}

async function tabCollection(body, d) {
  const c = d.collection;
  const ref = d.movie.collection;
  if (!c) {
    body.append(el("div.empty", el("h2", ref ? ref.title : "Collection"), "Turks isn't following this collection yet. ",
      el("button.primary", { onclick: async () => {
        if (await act("collection/follow", { tmdb_id: ref.tmdb_id, monitored: true, quality_profile_id: d.movie.quality_profile_id, root_folder: d.movie.root_folder }, null, "Following " + ref.title)) rerender();
      } }, "Follow collection")));
    return;
  }
  const movies = await api("movies");
  body.append(el("div.row", el("span.head", { style: { margin: 0 } }, c.title), el("span.grow"),
    el("button", { onclick: async () => { const r = await act("collection/addmissing", { id: c.id }); if (r) toast(`Added ${r.added} movies.`); } }, "Add missing")));
  body.append(el("div.posters", c.parts.map((p) => {
    const inLib = movies.find((x) => x.tmdb_id === p.tmdb_id);
    const art = posterArt({ ...p, poster_path: p.poster_path });
    art.append(el("div.bar.st-" + (inLib ? movieState(inLib)[0] : "unmonitored")));
    return el("div.poster", { onclick: () => (inLib ? go("movies", inLib.id) : addDialog({ tmdb_id: p.tmdb_id, title: p.title, year: p.year, poster_path: p.poster_path })) }, art, el("div.name", p.title), el("div.yr", inLib ? "In library" : p.year || ""));
  })));
}

function editMovie(m) {
  const rec = { id: m.id, monitored: m.monitored, quality_profile_id: m.quality_profile_id, minimum_availability: m.minimum_availability, tags: [...m.tags], path: m.path, move_files: true };
  const rows = specRows(rec, [
    ["monitored", "Monitored", "bool"],
    ["quality_profile_id", "Quality profile", "select", "", profileOptions],
    ["minimum_availability", "Minimum availability", "select", "When Turks starts looking.", availOptions],
    ["path", "Path", "path", "Changing it moves the folder (unless 'Move files' is off)."],
    ["move_files", "Move files", "bool"],
    ["tags", "Tags", "tags"],
  ]);
  const mm = openModal({
    title: "Edit " + m.title,
    body: rows,
    buttons: [el("button", { onclick: () => mm.close() }, "Cancel"), el("button.big.primary", { onclick: async () => { if (await act("save", rec, { kind: "movie" }, "Saved")) { mm.close(); rerender(); } } }, "Save")],
  });
}

/** Manual import: look at a folder, choose a movie for each video, import. */
async function manualImport(folder, movieId) {
  if (!folder) folder = await pickFolder("", "Folder to import from", "Look in this folder");
  if (!folder) return;
  const movies = await api("movies");
  const list = el("div", el("div.row", el("span.spin"), "Looking at the files…"));
  let mode = "move";
  let rows = [];
  const importBtn = el("button.big.primary", { disabled: true }, "Import");
  const m = openModal({
    title: "Manual import",
    body: [el("div.crumb", folder), list],
    buttons: [select([["move", "Move"], ["copy", "Copy"], ["hardlink", "Hardlink or copy"]], mode, (v) => { mode = v; }), el("button", { onclick: () => m.close() }, "Close"), importBtn],
    size: "wide",
  });
  const decisions = await api("import/manual", undefined, { folder, movie: movieId || "" }).catch((e) => { list.replaceChildren(el("div.bad", e.message)); return null; });
  if (!decisions) return;
  if (!decisions.length) { list.replaceChildren(el("div.empty", "No video files here.")); return; }
  rows = decisions.map((d) => ({ on: !!d.movie_id && !d.rejections.length, d }));
  const refresh = () => { const n = rows.filter((r) => r.on && r.d.movie_id).length; importBtn.disabled = !n; importBtn.textContent = `Import ${n}`; };
  const movieOpts = [["", "Choose a movie…"], ...movies.slice().sort((a, b) => a.title.localeCompare(b.title)).map((x) => [x.id, `${x.title} (${x.year})`])];
  list.replaceChildren(el("table",
    el("thead", el("tr", el("th"), el("th", "File"), el("th", "Movie"), el("th", "Quality"), el("th", "Size"))),
    el("tbody", rows.map((r) => el("tr",
      el("td", check("", r.on, (v) => { r.on = v; refresh(); })),
      el("td.wrap.small", r.d.path.split("/").slice(-2).join("/"), r.d.rejections.length ? el("div.warn", r.d.rejections.join(" · ")) : null),
      el("td", select(movieOpts, r.d.movie_id || "", (v) => { r.d.movie_id = v ? Number(v) : null; if (v) r.on = true; refresh(); })),
      el("td.nowrap", qualityName(r.d.quality.quality)), el("td.nowrap", size(r.d.size)))))));
  refresh();
  importBtn.onclick = async () => {
    const items = rows.filter((r) => r.on && r.d.movie_id).map((r) => r.d);
    if (await act("import/manual", { mode, items }, null, `Importing ${items.length} file(s)…`)) m.close();
  };
}
