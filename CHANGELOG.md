# Changelog

## 1.2.0 – 2026-11-01

### New
- **AI assistant** (title bar → *Assistant*, or **Ctrl+J**): a pop-up window that chats with an AI model running on your own PC – Ollama, LM Studio, llama.cpp, Jan or any OpenAI-compatible server. Nothing goes to the internet.
  - It can use the whole app: read consoles, crash reports, logs, config files, mods, players, worlds, backups and network status; change server settings and `server.properties`, edit or create config files (`.yml`, `.toml`, `.json`, `.ini`, `.cfg`…), set up automation (restarts, backups, schedules), run console commands, start/stop/restart, install/remove/update mods and plugins, make and restore backups, switch versions, set up Bedrock crossplay, forward ports and create servers.
  - Every change shows an *Allow / Decline* card first (or turn on *Allow automatically*). It can't accept the Minecraft EULA or delete servers.
  - It gets a briefing on how the app works, plus a live snapshot of your servers, and can look up a detailed reference (every setting, Java rules, common fixes) when it needs it – tuned so 8B–14B models with an 8k context work.
  - Shows reasoning models' thinking separately, streams answers, Stop button, quick asks (*Why did it crash?*, *Make it faster*, *Friends can't join*…), and *Ask the assistant* buttons on crashed servers and the Health page.
- Crash notifications for the wrong Java now say which Java the server needs (read from `class file version 69` / `requires version 21` lines).

### Fixed
- Paths with `..` segments counted as “inside the server folder” (e.g. `D:\srv\mc\..\..\Windows`). They are now refused everywhere – log reading, config files, modpack files.
- A crafted backup or `.mcserver` pack with a `worlds/../…` entry could make restore/import work on the server's *parent* folder.
- `eula.txt` can no longer be written through the config editor – the EULA comes only from the checkbox in Server Settings.

## 1.1.0 – 2026-10-14

### New
- **Bedrock crossplay** (Mods & Plugins → Essentials): one click installs Geyser + Floodgate so phone, console and Windows-edition players can join, and adds UDP 19132 to the forwarded ports. Works on Paper, Purpur, Fabric, Quilt and NeoForge. Minecraft versions Geyser doesn't support yet are refused up front (with the newest supported version) instead of installing a plugin that fails to load.
- **Essentials**: hand-picked add-ons for the server's software, one click each – Lithium, FerriteCore, ModernFix, Krypton, spark, Chunky, BlueMap, LuckPerms, EssentialsX, ViaVersion, Simple Voice Chat. Ports they need (BlueMap 8100, voice chat 24454) are added to Port Forwarding automatically.
- **World pre-generation** (Worlds): generate the area around spawn with Chunky – pick dimension and radius, live progress bar with ETA, pause / continue / cancel.
- **Player statistics** (Players → Stats): leaderboard from the world's stats files – play time, last seen, deaths, kills, distance travelled, blocks mined, items crafted. Old (pre-1.13) and current formats.

### Fixed
- Automatic build updates on an empty server restarted it without installing the new build – and then did it again at every update check. They now install the build.
- Deleting a server whose folder is on another drive than the app data could delete its backups. They are now copied out first (and nothing is deleted if that fails).
- Installing a modpack onto an existing server left the previous mods next to the pack's (crash on start). They are now moved to `.mcsm\quarantine` first – file by file, so a jar briefly held open by antivirus can't block it.
- Installing a modpack for an older Minecraft version onto an existing world now asks before downgrading it.
- TPS stayed hidden after switching a server from old vanilla (no `tick query`) to newer software until the manager restarted.
- Cancelling a stop/update countdown announced “Restart cancelled”.

### Faster
- Modpack files download six at a time with retries (a 62-file pack installs in about ten seconds); stalled downloads time out instead of hanging.
- Backups store region files, jars and images with light compression – much quicker on big worlds, nearly the same size.
- RCON reply parsing no longer recompiles its patterns every 10 seconds.
- Interrupted backup leftovers (`.zip.part`) are cleaned up; two backups in the same second no longer overwrite each other.

## 1.0.0 – 2026-10-13

First release.

- Servers: Vanilla (all releases & snapshots), Paper, Purpur, Folia, Fabric, Quilt, Forge, NeoForge; Modrinth modpacks; import of existing servers and `.mcserver` packs.
- Live version catalogs from Mojang, PaperMC Fill, Purpur, Fabric/Quilt meta, Forge and NeoForge maven; new-release announcements.
- Automatic Java (exact major, Eclipse Temurin downloads), Aikar's flags, memory recommendations.
- Mods/plugins/datapacks from Modrinth, Hangar and CurseForge with dependency resolution, update checks, client-only detection, client pack export.
- Console with autocomplete, RCON-based TPS and player list, players & bans management, worlds (level.dat incl. the 26.x layout), datapacks, backups with save-off/flush.
- Port forwarding: UPnP, router guide, Windows Firewall (incl. Java block rules), CGNAT/double-NAT detection, external reachability test.
- Automation: crash recovery, schedules with in-game countdowns, interval backups, build auto-updates, webhooks, tray mode, autostart.
