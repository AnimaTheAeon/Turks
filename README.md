# Turks

A movie collection manager for Windows with a classic JRPG party-menu look,
built from [Radarr](https://github.com/Radarr/Radarr)'s source. Sibling app to
[Jenova](https://github.com/AnimaTheAeon/Jenova) (subtitles) and
[Lifestream](https://github.com/AnimaTheAeon/Lifestream) (torrents).

Turks watches for the movies you want, finds releases on your indexers, sends
the best one to your download client, then renames and files it in your
library and upgrades it later if something better shows up.

Rust + egui desktop app. No web browser or .NET runtime needed. Your library
lives in `%APPDATA%\Turks`.

## Features

**Library**
- Add movies by title, `title 2019`, `tmdb:603` or an IMDb id (`tt0133093`)
- Import an existing library: scans a root folder, matches each folder on TMDb, adds them in place
- Posters, party-list and table views; filter (missing, upgrades, downloading…), sort, search
- Movie "Status" screen: details, file and media info, history, cast and crew, and a
  **Materia** panel showing the custom formats on the file and its score
- Bulk select: edit monitoring, profile, availability, root folder (moves files), tags; search, refresh, rename, delete
- Collections: follow TMDb collections and add the missing movies
- Discover: trending, popular, upcoming, in cinemas, top rated, and recommendations from your library
- Calendar (month/week) with cinema, digital and physical release dates, plus an **iCal feed** for your phone
- Wanted: missing and cutoff-unmet lists with search buttons

**Finding and grabbing** (ported from Radarr's decision engine)
- Radarr's release parser (title, year, edition, quality, proper/repack, group, languages),
  checked against Radarr's own test cases: 478 of 478 pass
- Quality profiles with groups, cutoffs and upgrades; quality size limits per minute of runtime
- Custom formats with Radarr's matching rules; **import TRaSH Guides / Radarr JSON** directly
- Delay profiles, release profiles (must / must not contain, regex), tags
- Every Radarr check: quality, size, language, seeders, retention, minimum age, blocklist,
  already in queue, upgrade rules, propers/repacks, hardcoded subs, indexer flags…
- Same sort order as Radarr: quality, custom format score, protocol, indexer priority, flags, seeders, size
- Automatic search, interactive search (see every release and why it was rejected; grab anyway), RSS sync

**Indexers**: Torznab and Newznab, one-click **import from Prowlarr**, or let Prowlarr push them
(Turks speaks Radarr's API).

**Download clients**: Transmission (and Lifestream once it has its RPC endpoint), qBittorrent,
Deluge, **Synology Download Station**, SABnzbd, NZBGet, torrent and usenet blackhole folders.
Remote path mappings for clients on another machine.

**Importing**
- Completed download handling: imports finished downloads, hardlinks torrents so they keep seeding
- Radarr naming tokens (`{Movie Title} ({Release Year}) {Quality Full}`, `{imdb-{ImdbId}}`, case and separator rules)
- Subtitles and other extras come along; upgrades replace the old file (recycle bin optional)
- Failed downloads are blocklisted and searched again
- Manual import for anything that couldn't be matched automatically
- Real media info from ffprobe when it's installed (put `ffprobe.exe` next to `turks.exe`)

**Lists**: TMDb lists, collections, people, companies, keywords and charts; IMDb lists; Trakt;
Plex watchlist; RSS; another Turks/Radarr. Exclusions and library cleanup options.

**Connect**: Discord, Telegram, Pushover, ntfy, Gotify, Slack, Pushbullet, Apprise, email, webhooks
(Radarr's JSON), custom scripts (Radarr's `radarr_*` variables), and library refreshes for Plex,
Jellyfin, Emby and Kodi.

**Metadata**: Kodi/Jellyfin/Emby `.nfo` files with poster and fanart.

**API**: a Radarr-compatible v3 API on port 7878 so Overseerr, Jellyseerr, Ombi and Prowlarr work
with Turks the way they work with Radarr.

**App**: Simple / Advanced mode, a Shinra help icon on every setting, tray icon, start with Windows,
scheduled backups and restore, health checks, task list, event log, "test parsing" tool, in-app updates
from GitHub Releases.

## Getting started

1. **TMDb key**: Radarr uses its own metadata server; Turks talks to TMDb directly, so it needs a free
   key from [themoviedb.org › Settings › API](https://www.themoviedb.org/settings/api). The first-run
   window asks for it.
2. **Root folder**: the folder that holds your movie folders (e.g. `D:\Media\Movies`).
3. **Indexers**: Settings › Indexers › *Import from Prowlarr*, or add Torznab/Newznab by hand.
4. **Download client**: Settings › Download Clients.
5. Add movies (or *Import existing library*).

With Jenova: point Jenova's watch folder at your movie root folder and subtitles appear after each import.

## Building

```
cargo build --release
```

GitHub Actions builds `turks.exe` and the installer on every push; bumping the version in
`Cargo.toml` and pushing to `main` publishes a release (which the in-app updater picks up).

Developer extras: `turks --headless` runs the engine and API without a window. `TURKS_DATA_DIR`
changes where data is kept.

## License

GPL-3.0, because Turks is derived from Radarr's source (parser, decision engine, quality
definitions and API shapes). Radarr is © the Radarr team.
