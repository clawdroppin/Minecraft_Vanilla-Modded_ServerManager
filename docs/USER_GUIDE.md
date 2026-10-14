# Minecraft Server Manager – User guide

## 1. Creating a server

**New server** (Ctrl+N) opens a four-step wizard:

1. **Type** – *Pick server software* or *Start from a modpack*.
   - **Vanilla** – Mojang's own server. Datapacks only.
   - **Paper** – fast, plugin-ready; the best default. **Purpur** adds hundreds of gameplay options. **Folia** is multithreaded for huge player counts (Folia-aware plugins only).
   - **Fabric** / **Quilt** – lightweight mod loaders, first on new versions. **Forge** / **NeoForge** – the classic and modern big-modpack loaders.
   - **Modpack** – search Modrinth; the pack decides Minecraft version and loader.
2. **Version** – every version the software supports, newest first. Tick *Snapshots* for previews. The loader/build list defaults to *Latest recommended*.
3. **Server** – name, folder (automatic or your own; an existing server in that folder is kept), port, memory, game mode, difficulty, max players, seed, whitelist, PvP.
4. **Launch** – accept the Minecraft EULA (required by Mojang) and create. The software downloads in the background; the right Java is fetched automatically on first start.

**Import server** adopts an existing server folder (any software is detected) or a `.mcserver` pack exported from this app.

## 2. Running

- **Start / Stop / Restart** in the header. Stop saves the world (`stop`); *Force stop* kills it.
- **Countdown restarts** (… menu) warn players in chat (5 min, 1 min, 10 s by default) and can be cancelled from the header chip.
- The **Overview** shows how the server looks in the multiplayer list, the addresses to share (internet / LAN / this PC), live players, TPS, CPU and memory, and an activity feed (joins, chat, advancements).
- Servers keep running when you close the manager; on reopening it finds them again (commands then go over RCON).

## 3. Console

Live log with colour by level, filters (chat / warnings / errors), search, and older logs or crash reports from the drop-down. Type commands without a slash: **Tab** completes commands, players and gamerules, **↑/↓** walk the history. Quick buttons: Save, List, Day, Clear weather, TPS, Broadcast.

## 4. Players

Online players with actions (kick, op, whitelist, game mode, ban, IP ban), and tabs for whitelist, operators, bans, IP bans, everyone seen, **stats** and chat. While running, changes go through console commands; while stopped the JSON files are edited (names are looked up with Mojang for their UUID).

**Stats** is a leaderboard from the active world's statistics files (what players see under *Statistics* in-game): play time, last seen, deaths, mob and PvP kills, distance travelled, blocks mined and items crafted. Click a column to sort. The server writes these files when a player leaves and on autosave, so a player who is online right now may lag behind a few minutes.

## 5. Worlds & backups

- **Worlds** lists every world folder with seed, version, game mode, size and dimensions. *Use this world* switches `level-name` (next start). Import a `.zip` or a singleplayer save folder, export as `.zip`, rename, **reset** the whole world (optionally with a new seed) or just the **Nether** / **End**, manage **datapacks**. Nothing is deleted – old data goes to `.mcsm\quarantine` in the server folder.
- **Pre-generate the world** (top of *Worlds*) builds the terrain around spawn ahead of time with [Chunky](https://modrinth.com/plugin/chunky), so exploring new land never lags the server. *Install Chunky* if it isn't there (one restart), then with the server running pick Overworld / Nether / End and a radius (1,000–10,000 blocks) and press *Start*. A progress bar shows chunks, speed and time left; *Pause*, *Continue* and *Cancel* work any time. It uses a lot of CPU – best before people join.
- **Backups** zip the worlds plus `server.properties`, player lists and `config/` (optionally mods). Live backups pause saving for a consistent copy. Restores back up the current state first.

## 6. Mods, plugins & modpacks

- **Essentials** – hand-picked add-ons for this server's software, one click each: performance (Lithium, FerriteCore, ModernFix, Krypton, spark), world (Chunky, BlueMap), administration (LuckPerms, EssentialsX) and players (ViaVersion, Simple Voice Chat). Ports an add-on needs (BlueMap's web map on TCP 8100, voice chat on UDP 24454) are added to Port Forwarding for you.
- **Bedrock crossplay** (top of *Essentials*) – lets friends on phones, tablets, Xbox, PlayStation, Switch and Windows (Bedrock Edition) join. One click installs **Geyser** (translates Bedrock to Java) and **Floodgate** (Bedrock players don't need a Java account) and adds **UDP 19132** to the ports to forward. Bedrock players add the server with your address and port **19132**. Works on Paper, Purpur, Fabric, Quilt and NeoForge. Right after a new Minecraft release Geyser may not support it yet – the app tells you the newest version it supports instead of installing something that won't load.
- **Installed** – every jar with name, version, loader and source. Toggle to disable (renames to `.jar.disabled`), remove (moved to `.mcsm\removed`), **Identify** recognises hand-added jars on Modrinth, **Client pack** exports a `.mrpack` for players.
- **Browse** – Modrinth (no key), Hangar (Paper plugins), CurseForge (needs a free API key from console.curseforge.com, App settings → CurseForge API key). Results are filtered to your exact Minecraft version and loader; mods are installed with their required dependencies; client-only mods are skipped.
- **Updates** – newer compatible versions of Modrinth-tracked files, updated in one click. A file the running server holds open is swapped right before the next start.
- **Modpacks** – install a Modrinth modpack onto the server (switches its software/version; worlds stay). The previous mods are moved to `.mcsm\quarantine` first so they can't clash with the pack's. If the pack is for an **older** Minecraft version than your world, you're asked first – opening a world in an older version can damage it (a backup is made either way; a new server is the safer choice).
- Warnings appear for client-only mods (they crash dedicated servers) and mods for the wrong loader.

## 7. Configuration

- **Server Settings** – identity, EULA, the **MOTD designer** (colour/format codes with a live server-list preview), server icon (any image, scaled to 64×64), network (port, bind address, public address, RCON, query) and all documented `server.properties` keys with descriptions (*Show all* for the full list).
- **Version & Software** – what is installed, update checks, and switching software / Minecraft version / build (backup first; downgrades need confirmation).
- **Java & Memory** – memory slider with a recommendation based on your mod count, GC preset (Aikar's flags recommended), Java runtime (automatic or pick/download), JVM and server arguments, priority, CPU cores, and the exact launch command.
- **Automation** – start with the manager, crash restart + crash-loop guard, memory/uptime/empty-server restarts, schedules (restart, stop, start, backup, command, broadcast), restart warnings, backup interval/retention, automatic build updates.
- **Config Files** – edit any `.properties`, `.yml`, `.toml`, `.json` in the server (root, `config/`, `plugins/`) with highlighting and the last 30 versions kept.

## 8. Letting friends join (port forwarding)

Open **Port Forwarding**:

1. **What to forward** – Java Edition needs only TCP on the game port (25565 by default). Add plugin ports (Geyser 19132/UDP, BlueMap, Dynmap, Simple Voice Chat…) under *Extra ports* – Bedrock crossplay and the Essentials add-ons add theirs automatically.
2. **Automatic (UPnP)** – one click asks your router to forward the ports. Turn on *Re-apply every time the server starts* to keep them.
3. **Manual setup** – step-by-step, with where the setting lives on common routers.
4. **Windows Firewall** – *Allow through firewall* (one admin prompt) also removes “Block” rules Windows created for Java when its prompt was dismissed.
5. **Test from the internet** – asks mcsrvstat.us to ping your public address from outside.

If the app reports **CGNAT**, your ISP shares your IP and forwarding can't work – ask the ISP for a public IPv4 or use a tunnel such as playit.gg. **Double NAT** means another router sits in front: bridge it or forward on both.

## 9. Health

One-click checks for EULA, Java, memory, ports, firewall, offline mode, view distance, client-only / wrong-loader mods, disk space, recent crash reports and known fatal log lines – each with a fix button.

## 10. App settings

Default server folder, backup folder (another drive is safer), Java runtimes folder, CurseForge key, new-version announcements (and snapshots), tray mode, start with Windows, Discord/Slack webhook, refresh rate.

## 11. Troubleshooting

| Problem | What to do |
|---|---|
| **Friends can't join from outside** | Port Forwarding → *Test from the internet*. If it fails: forward the port (UPnP or manual), *Allow through firewall*, and check the app doesn't report **CGNAT** (then ask your ISP for a public IPv4 or use a tunnel such as playit.gg). |
| **“Failed to bind to port”** | Another program (or another server) uses the port. Change it under Server Settings → Network, or stop the other server. |
| **Server crashes right after start with mods** | Health shows client-only or wrong-loader mods – disable them on the Installed tab. The console names the mod that's missing a dependency. |
| **“requires version 21 of Java” / UnsupportedClassVersionError** | Java & Memory → set Java to *Automatic*. The app downloads the exact version the server needs. |
| **Lag spikes when people explore** | Worlds → *Pre-generate*. On modded servers add Lithium / FerriteCore / ModernFix from Essentials. Lower `view-distance` / `simulation-distance` under Server Settings. |
| **Bedrock players can't connect** | They must use port **19132** (not 25565). Forward **UDP** 19132 too (Port Forwarding → it is listed once crossplay is set up) and restart the server after installing. |
| **Out of memory / server uses too much RAM** | Java & Memory → the memory slider shows a recommendation for your mod count. Leave at least 2–3 GB for Windows. |
| **Something got deleted by mistake** | Nothing the app removes is gone: worlds go to `.mcsm\quarantine`, mods to `.mcsm\removed`, config edits keep 30 versions (Config Files → history), and restores back up first. |

Windows SmartScreen may warn on first launch because the app isn't code-signed yet: **More info → Run anyway**.
