# AGENTS.md – hand-off guide for AI assistants and contributors

This is the hand-off document for **Minecraft Server Manager**, a Windows desktop app that creates, mods, runs and automates Minecraft **Java Edition** servers (Vanilla, Paper, Purpur, Folia, Fabric, Quilt, Forge, NeoForge, and Modrinth modpacks). Read it fully before changing code. `CLAUDE.md` points here. User-facing documentation lives in [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md); keep it in sync when behaviour changes.

It shares its architecture with the sibling managers in `O:\ServerManagers` (Dragonwilds, ARK, Dune).

Repository: **https://github.com/clawdroppin/Minecraft_Vanilla-Modded_ServerManager** (branch `main`; releases are built by GitHub Actions from `v*` tags).

## 0. Quick start for AI agents

1. Read this file, then the area of `src-tauri/src/core/` or `src/features/` you're changing. Section 3 holds facts already verified against real servers – trust them instead of re-researching.
2. Every new backend command touches four places: `commands.rs` (+ `lib.rs` handler list), `src/lib/ipc.ts`, `src/lib/types.ts` (mirror of `models.rs`, camelCase) and `src/lib/devMock.ts`.
3. Verify against **real servers** (section 1, “Testing the real app”), not just unit tests – the 26.x world layout, Geyser's version support and Windows file locking were all found that way. Clean up test servers and `%APPDATA%\com.minecraftservermanager.app` afterwards.
4. Before finishing: `cargo check` warning-free, `cargo test --lib`, `npx tsc -p tsconfig.json --noEmit`; update `docs/USER_GUIDE.md`, this file and `CHANGELOG.md` when behaviour changes; regenerate screenshots if the UI changed.
5. Don't publish (push, tag, release) unless the owner asks – see section 5.

## 1. Stack & commands

| Layer | Tech |
|---|---|
| Shell | Tauri 2: main window created in code, tray icon, single-instance and autostart plugins |
| Backend | Rust 2021, tokio, reqwest, sysinfo, zip, flate2, regex, igd-next (UPnP), sha1/sha2/md5, windows-sys |
| Frontend | React 19, TypeScript 7, Vite 8 (dev port **1430**), Tailwind CSS 4, Motion, Zustand, CodeMirror 6, cmdk, lucide-react |

```bash
npm install
npm run tauri dev                      # full app, hot reload
npm run dev                            # UI only in a browser with the MOCK backend (src/lib/devMock.ts)
npx tsc -p tsconfig.json --noEmit      # typecheck
cd src-tauri && cargo check            # must stay warning-free (CI uses -D warnings)
cd src-tauri && cargo test --lib       # unit tests
npm run build:all                      # NSIS + MSI + portable zip (local)
node scripts/screenshots.mjs           # regenerate docs/screenshots (needs `npm run dev` running)
```

**Testing the real app.** `cargo build` in `src-tauri`, keep `npm run dev` running (the debug exe loads `devUrl`), then run the debug exe with `MCSM_CDP_PORT=9444` and drive the WebView over the Chrome DevTools Protocol: `window.__TAURI_INTERNALS__.invoke('command', args)`. The CDP hook only exists in debug builds. Don't test with `tauri dev` – its rebuilds kill the process tree, including servers.

**Real servers** for testing go in a git-ignored `.test-server/` folder. Delete them afterwards (`delete_instance(deleteFiles=true)`), plus `%APPDATA%\com.minecraftservermanager.app` if you want a clean first run (it also holds downloaded Java runtimes).

## 2. Repository map

```
src-tauri/src/
  main.rs / lib.rs   entry, window, tray, plugins, command list
  commands.rs        every #[tauri::command] (thin; add new ones to lib.rs + src/lib/ipc.ts + devMock.ts)
  state.rs           AppState, per-server Runtime, stdin pipes, cached_json() (TTL + disk fallback), TaskReporter, notify()/webhook
  models.rs          persisted & event types (camelCase), ServerInstance helpers (ports, dirs)
  paths.rs           installed vs portable data dir, path rebasing
  core/
    versions.rs      catalogs: Mojang manifest, Fill (Paper/Folia), Purpur, Fabric/Quilt meta, Forge & NeoForge maven
    install.rs       installs/switches software (direct jars, headless Quilt/Forge/NeoForge installers), detect_existing()
    java.rs          Java detection (system, JAVA_HOME, Minecraft launcher runtimes) + Adoptium Temurin downloads
    server.rs        JVM args (Aikar flags), server.properties/eula sync, start/stop/restart, commands, countdowns, install_job
    supervisor.rs    single loop: java process discovery (-Dmcsm.id marker), crash/adoption, console tail + player/chat parsing,
                     RCON metrics (list + TPS), countdown warnings, schedules, backups, build-update checks, new-release watcher
    mcproto.rs       RCON client, list/TPS parsers, Server List Ping
    props.rs         lossless server.properties document (\uXXXX escapes, comments kept)
    worlds.rs, nbt.rs  world discovery from level.dat (+ 26.x world_gen_settings.dat), import/export/reset/rename, datapacks
    players.rs       whitelist/ops/bans JSON, Mojang UUID lookup, offline UUIDs
    mods.rs          installed jars: metadata (fabric/quilt/forge/neoforge/bukkit/paper), enable/disable, source index
    sources.rs       Modrinth / CurseForge / Hangar search + install (+ deps), identify, updates, .mrpack install, client pack export
    backup.rs        zip backups of worlds + configs (+mods), restore (quarantine first), retention, .mcserver packs
    network.rs, upnp.rs  firewall rules (ports + java.exe, removes Block rules), IPs, UPnP mapping, mcsrvstat.us external check
    diagnostics.rs   health checks with fix ids
src/
  store/app.ts  lib/ipc.ts (typed commands)  lib/types.ts (mirrors models.rs)  lib/live.ts (ring buffers)
  lib/mc.ts     software catalog, § formatting, commands for autocomplete     data/properties.ts (server.properties catalog)
  components/ui/Mc.tsx  grass-block logo, SoftwareIcon, FormattedText (MOTD), ServerListCard, PlayerHead
  data/essentials.ts  curated one-click add-ons (Modrinth ids, supported software, ports they open) + Bedrock-capable software
  features/server/  Workspace, VersionPicker, draft.ts, views/* (one file per nav item; Worlds hosts Chunky pre-generation,
                    Players the stats leaderboard, Mods the Essentials tab with Bedrock crossplay)
  features/dialogs/ NewServerDialog (software or modpack wizard), Dialogs (import/export/clone/delete/app settings)
```

## 3. Verified facts (tested against real servers, October 2026)

1. **Versions.** Mojang moved to year-based versions in 2026 (`26.1`, `26.3`, snapshots `26.4-snapshot-3`). The manifest still lists everything back to `rd-132211`; servers exist from 1.2.5. `javaVersion.majorVersion` in each version JSON is authoritative (26.x → 25, 1.20.5–1.21.x → 21, 1.18–1.20.4 → 17, 1.17 → 16, older → 8).
2. **Java policy: exact major.** Cobblemon (and other mods) refuse to run on a newer Java (`requires version 21 of 'OpenJDK 64-Bit Server VM' (java), but only the wrong version is present: 25`). `java::pick` only accepts the exact major; it downloads that Temurin LTS when missing. Only non-LTS requirements (Java 16) take the closest newer runtime.
3. **Launch.** Every server gets `-Dmcsm.id=<profile id>`; the supervisor finds servers by scanning `java.exe` command lines, so a server started before the manager (re)opened is adopted. stdout/stderr go to `<server>\.mcsm\console.log` (previous run → `console.prev.log`), which is tailed. stdin stays a pipe for commands; after a manager restart commands go over RCON instead (tested).
4. **Stop** = `stop` on stdin, else over RCON, then kill after the timeout. A `Stopping the server` log line marks the exit as intentional (an op typing `stop` is not a crash).
5. **RCON** is on by default (port = game port + 10, random 24-char password, `broadcast-rcon-to-ops=false`). One persistent connection polls `list` and TPS every 10 s: Paper/Purpur/Folia `tps`, vanilla/Fabric/Quilt `tick query` (1.20.3+; older vanilla has none), NeoForge/Forge `neoforge tps` / `forge tps` (`Overall: 20.000 TPS (0.120 ms/tick)`).
6. **Console formats** differ: vanilla/Fabric `[12:00:00] [Server thread/INFO]: …`, Paper `[12:00:00 INFO]: …` (command feedback prefixed `System chat:` on 26.x), Forge/NeoForge `[12:00:00] [Server thread/INFO] [minecraft/MinecraftServer]: …`. All parsers anchor on `]: ` and are unit-tested in `supervisor.rs`.
7. **Installs (all tested end to end):** Paper 26.3 (Fill v3, sha256-verified), Fabric 1.21.8 (Mojang server.jar + Fabric's server launcher jar), Quilt 1.21.1 (installer → `quilt-server-launch.jar`), Forge 1.20.1 and NeoForge 1.21.1 (installer `--installServer`, launched via `@libraries/…/win_args.txt`), Vanilla 1.12.2 (Java 8), Cobblemon modpack (.mrpack → Fabric 1.21.1 + 113 mods).
8. **NeoForge versions → Minecraft:** `21.1.200` → 1.21.1, `21.0.x` → 1.21, `26.3.0.76` → 26.3, `26.1.2.x` → 26.1.2 (`versions::neoforge_mc`).
9. **26.x world layout:** dimensions live in `world/dimensions/minecraft/{overworld,the_nether,the_end}` (also on Paper – no more `world_nether` folders); the seed is in `dimensions/minecraft/overworld/data/minecraft/world_gen_settings.dat` (`data.seed`); difficulty is a string in `Data.difficulty_settings.difficulty`. Older worlds use `Data.WorldGenSettings.seed` / `RandomSeed`, `DIM-1` / `DIM1` and Bukkit `_nether` / `_the_end` folders – all handled.
10. **Windows locks loaded jars.** Replacing/removing a mod while the server runs can't move the file; `mods::remove_or_defer` queues it in `.mcsm/pending-removals.json` and `server::start` applies it before launch (otherwise Fabric crashes on duplicate mods).
11. **Live backups** use `save-off` → `save-all flush` (wait for “Saved the game”) → zip → `save-on`. 26.x worlds back up as one folder; `session.lock` is skipped.
12. **Whitelist while running** goes through commands (`whitelist add Notch` → server resolves the UUID); while stopped the JSON is edited directly with a Mojang API lookup (`069a79f4-44e9-4726-a5be-fca90e38aaf5` for Notch) or the offline UUID (`UUID.nameUUIDFromBytes("OfflinePlayer:"+name)`).
13. **Modrinth** search facets: `project_type`, `categories:<loader>` (Quilt also searches Fabric), `versions:<mc>`, `server_side:required|optional` for mods. Client-only projects (`server_side: unsupported`) are never installed on servers. CurseForge needs a user-supplied API key; some files have no `downloadUrl` (author opted out) – the UI says so.
14. **Bedrock crossplay (Geyser + Floodgate).** Geyser's newest build *fails to enable* on a Minecraft release it doesn't support yet (tested: Geyser 2.11.3-b1252 on Paper 26.3 → `Error occurred while enabling Geyser-Spigot`); Modrinth lists the supported `game_versions` per build (Paper: 1.20.5 … 26.2 at the time), so `sources::install_bedrock` checks Modrinth first and refuses with the newest supported version. Geyser comes from Modrinth for every platform; Floodgate for Paper/Purpur only exists on GeyserMC's download API (`download.geysermc.org/v2/projects/floodgate/versions/latest/builds/latest` → `downloads.spigot.{name,sha256}`), Floodgate for Fabric/NeoForge is on Modrinth (pulls in Fabric API). Tested: Paper 26.2 and Fabric 26.2 both log `Started Geyser on UDP port 19132`. Geyser's own `Done (1.41s)! Run /geyser help` line must not count as server ready (READY_RE requires `For help, type`).
15. **Chunky** (pre-generation) accepts Bukkit world names on Paper (`world`, `world_nether`, `world_the_end` – still valid on 26.x) and dimension ids on mod loaders (`minecraft:overworld`). Progress lines (identical on both): `[Chunky] Task running for world_nether. Processed: 583 chunks (38.33%), ETA: 0:00:01, Rate: 499.6 cps, Current: 9, -11` and `Task finished for … Processed: 1521 chunks (100.00%), Total time: 0:00:02`. The UI parses them from the console stream (`Worlds.tsx`, `CHUNKY_RE`).
16. **Player data on 26.x** lives in `world/players/` (`players/data` exists after the first start); stats are read from `world/players/stats` and the classic `world/stats`. Formats: 1.13+ `stats."minecraft:custom"."minecraft:play_time"` (ticks), pre-1.13 flat `stat.playOneMinute` (both unit-tested in `players.rs`).
17. **Renaming a whole folder fails on Windows** while any file in it is held open (seen when swapping modpacks – fresh jars being scanned); `backup::quarantine_contents` moves files one by one with retries and a copy fallback. `std::fs::rename` across drives also fails – `delete_instance` copies the backups out instead (tested: server on `O:`, app data on `C:`).
18. **Auto build updates** on a running server go through `begin_countdown("update")`; with nobody online that must call `install_job` (stop → install → start), not `restart` (tested: Paper 26.2 build 131 → 133 while running).
19. **Modpack downloads** run six at a time (`buffer_unordered(6)`) with three attempts each (`download::download_retry`); the HTTP client has a 60 s read timeout so a stalled transfer fails instead of hanging. Vanilla Perfected (62 server files) installs in ~11 s.

## 4. Invariants & gotchas

- **The EULA is the user's decision.** `eula.txt` is only written when `eulaAccepted` is set by the user in the UI.
- **server.properties is lossless** – always go through `props.rs`. The profile owns `properties` (everything) plus `network` keys overlaid on every sync; the server's defaults are absorbed into the profile after each successful start.
- **Lifecycle operations hold `AppState::op_lock(id)`**; the supervisor uses `try_lock`. Don't add per-server loops – extend the supervisor tick.
- **Never lose user data.** World resets/deletes, restores and mod removals move files into `<server>\.mcsm\quarantine` or `.mcsm\removed`; restores and version switches back up first.
- **Downgrades** of an existing world need explicit consent (`change_version(allowDowngrade)`).
- **Downloads verify hashes** (sha1 from Modrinth/Mojang, sha256 from Fill/GeyserMC) and never leave half files (`.part` + rename).
- **Only the user's actions change their router or firewall** (buttons, or the per-server “re-apply UPnP on start” opt-in). RCON is never forwarded.
- **Mock backend only in browser dev** (`!isTauri()`); extend `devMock.ts` whenever you add a command.
- **Charts:** series colours `#16a34a` / `#3b82f6` were validated for the dark surface (dataviz validator). One y-axis per chart.

## 5. Releasing

1. Bump the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`; add a `CHANGELOG.md` entry.
2. `cargo check` (warning-free), `cargo test --lib`, `npx tsc --noEmit`, `npm run build:all` must pass.
3. Commit, `git tag vX.Y.Z && git push origin main --tags` – `.github/workflows/release.yml` (Windows runner) builds and publishes `Minecraft-Server-Manager-Setup.exe`, `.msi`, `-Portable.zip` and `SHA256SUMS.txt` to https://github.com/clawdroppin/Minecraft_Vanilla-Modded_ServerManager/releases. Tags with a `-` (e.g. `v1.2.0-beta`) become pre-releases. Never move a released tag.
4. The `gh` CLI isn't installed on the owner's PC; git pushes use Git Credential Manager. The repo description/topics are set through the GitHub REST API.

## 6. Ideas / not yet done

- Join/leave/chat parsing verified only from server-side commands, not with a real client connection; restart-countdown warnings likewise.
- Bedrock Dedicated Server, Velocity/BungeeCord proxies, Spigot via BuildTools, Mohist/Arclight hybrids.
- Geyser on Folia, Geyser/Floodgate update checks (GeyserMC-sourced Floodgate isn't covered by Modrinth update checks), Bedrock reachability test (RakNet unconnected ping).
- Chunky progress in the backend (today the UI parses console lines – progress is only visible while the app is open).
- Hangar dependency resolution, CurseForge modpacks, Spiget.
- Code signing, app auto-update, Linux host support, vitest suite for TS helpers.
