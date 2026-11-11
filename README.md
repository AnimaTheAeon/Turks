# Turks

A movie collection manager for Windows and Synology NAS with a classic JRPG party-menu look,
built from [Radarr](https://github.com/Radarr/Radarr)'s source. Sibling app to
[Jenova](https://github.com/AnimaTheAeon/Jenova) (subtitles) and
[Lifestream](https://github.com/AnimaTheAeon/Lifestream) (torrents).

Turks watches for the movies you want, finds releases on your indexers, sends
the best one to your download client, then renames and files it in your
library and upgrades it later if something better shows up.

Two builds from one Rust engine:

- **Windows**: an egui desktop app with a tray icon and an installer. No web browser or .NET
  runtime needed. Your library lives in `%APPDATA%\Turks`.
- **Synology DSM 7** (x86-64 models such as the DS923+): a background service plus a Turks window
  in the DSM main menu, in the same menu style. Only DSM administrators can open it.

Both update themselves from GitHub Releases, and both have two looks, picked in Settings › UI ›
Theme: the **FFVII menu** (blue windows, pointing hand) and **Box of X** (a green console dashboard:
pick a page from the orb's menu with the mouse or the arrow keys and Enter; BACK, Esc or the mouse's
back button returns to it).

## Features

**Library**
- Add movies by title, `title 2019`, `tmdb:603` or an IMDb id (`tt0133093`)
- Import an existing library: scans a root folder, matches each folder on TMDb, adds them in place
- Posters, party-list and table views; filter (missing, upgrades, downloading…), sort, search
- Movie "Status" screen: details, file and media info, history, cast and crew, and a
  **Materia** panel showing the custom formats on the file and its score
- Bulk select: edit monitoring, profile, availability, root folder (moves files), tags; search, refresh, rename, delete
- Six pages: Movies, Add Movie, Calendar, Activity, Settings, System
- Add Movie has four tabs: Search, Discover (trending, popular, upcoming, in cinemas, top rated),
  Collections (follow a TMDb collection and add its missing movies) and Import library
- Calendar (month/week) with cinema, digital and physical release dates, plus an **iCal link** for
  your phone. The link has its own read-only key, so it can't be used to change anything
- Missing and upgrade-wanted movies are filters on the Movies page, with a "Search all" button and
  a colour key for the poster bars
- Deleted and replaced files go to the Windows Recycle Bin, or the shared folder's #recycle on a
  Synology, unless you choose your own recycle folder
- Downloads that stop making progress are flagged (and can be replaced automatically)
- Simple mode shows only what you need to get going; Advanced shows everything Radarr has

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

**Download clients**: Lifestream and Transmission, qBittorrent,
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
4. **Download client**: Settings › Download Clients. For Lifestream, pick "Transmission / Lifestream" (localhost, port 9091, category `turks`) and turn on Config › Remote control in Lifestream 0.1.6+.
5. Add movies (or *Import existing library*).

### On a Synology

1. Download `Turks-<version>-x86_64.spk` from [Releases](https://github.com/AnimaTheAeon/Turks/releases),
   then Package Center › Manual Install.
2. Give the `Turks` system user Read/Write on your movie and download shares: Control Panel › Shared
   Folder › Edit › Permissions › *System internal user* › Turks.
3. Open Turks from the main menu and follow the steps above. Folder pickers show your shared folders.
4. Download Station works as a download client, as does Lifestream on a PC or Transmission/qBittorrent
   in Docker (add a remote path mapping if the paths differ).
5. **Updates**: Turks checks GitHub twice a day. DSM only lets packages install updates as root, so
   create one Task Scheduler task (User: root, daily) that runs
   `/var/packages/Turks/target/bin/turks update`. Settings › General › Updates shows the exact steps.

The Radarr-compatible API listens on port 7878 on the network by default on a NAS, so Overseerr,
Jellyseerr and Prowlarr can connect with the API key from Settings › General.

With Jenova: point Jenova's watch folder at your movie root folder and subtitles appear after each import.

## Building

```
cargo build --release                                     # Windows app
cargo build --release --no-default-features --features nas \
    --target x86_64-unknown-linux-musl                    # Synology service
nas/build-spk.sh target/x86_64-unknown-linux-musl/release/turks dist
```

GitHub Actions builds `turks.exe`, the installer and the `.spk` on every push; bumping the version in
`Cargo.toml` and pushing to `main` publishes one release with all three, which both updaters pick up.

The Synology window is plain HTML/JS in `nas/ui`. To work on it without a NAS, run the service with
sign-in checks off and serve the window with the dev server, which runs the real CGI bridge:

```
cargo build --no-default-features --features nas
TURKS_NO_AUTH=1 TURKS_DATA_DIR=/tmp/turks-dev target/debug/turks daemon &
python3 nas/dev/serve.py target/debug/turks   # then open http://127.0.0.1:5080/
```

Developer extras: `turks --headless` runs the engine and API without a window. `TURKS_DATA_DIR`
changes where data is kept.

## License

GPL-3.0, because Turks is derived from Radarr's source (parser, decision engine, quality
definitions and API shapes). Radarr is © the Radarr team.
