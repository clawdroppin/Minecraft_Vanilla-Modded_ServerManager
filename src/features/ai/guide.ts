// Reference the assistant can look up with the `app_guide` tool. Kept out of the system prompt
// so small local models (8k context) have room for logs and files.
import { VIEWS } from '@/features/server/views';

export const GUIDE: Record<string, string> = {
  profile: `Server profile fields (change with update_server, read with get_server):
- java: minMemMb, maxMemMb, gcPreset (aikar | g1 | zgc | none – aikar recommended), jvmArgs, serverArgs ("nogui"), priority (normal | belowNormal | aboveNormal | high | idle), affinity (CPU core list, [] = all), path ("" = automatic Java – recommended)
- network: port, bindIp, rconEnabled, rconPort, queryEnabled, queryPort, upnp (re-forward on every start), extraPorts [{port, protocol "tcp"|"udp"|"both", label}], publicAddress
- properties: every server.properties key as a string: "view-distance", "simulation-distance", "max-players", "difficulty" (peaceful|easy|normal|hard), "gamemode", "online-mode", "white-list", "enforce-whitelist", "pvp", "motd" (§ colour codes), "level-seed", "level-name", "spawn-protection", "allow-flight", "hardcore"…
- automation: autoStart (start with the app), autoRestart (after crashes), restartDelaySecs, crashLimit (crash-loop guard per 15 min), stopTimeoutSecs, schedules, restartWarnings (seconds before, e.g. [300,60,10]), warningMessage ("{time}" placeholder), backupIntervalMins (0 = off), backupKeep, backupOnlyIfPlayers, backupOnStop, backupBeforeUpdate, backupMods, updateCheckMins, autoUpdateBuilds, memoryLimitMb, maxUptimeHours, emptyRestartMins (0 = off for the restart triggers)
- schedules: [{enabled, time "HH:MM" local, days [0=Sun..6=Sat] ([] = every day), action restart|backup|stop|start|command|say, command}] – the array is replaced whole, so include existing entries you want to keep.
Example: {"automation":{"schedules":[{"enabled":true,"time":"04:00","days":[],"action":"restart","command":""}],"restartWarnings":[300,60,10],"backupIntervalMins":120,"backupKeep":12}}`,

  java: `Java: each Minecraft version needs an EXACT Java major: 26.x → 25, 1.20.5–1.21.x → 21, 1.18–1.20.4 → 17, 1.17 → 16, older → 8. Mods like Cobblemon refuse newer Java. java.path "" = automatic (the app downloads Eclipse Temurin). "UnsupportedClassVersionError … class file version N" means Java N-44 is needed; "requires version N of … (java)" means exactly Java N.
Memory: vanilla/Paper 2–4 GB for a few friends, 6–8 GB with 100+ mods, 10–12 GB for huge packs. Leave 3 GB+ for Windows; more than needed makes GC pauses worse. Aikar's flags (gcPreset aikar) suit almost everyone; zgc for 16 GB+ heaps.`,

  fixes: `Common problems:
- Crash on start with mods: read_console; the error names the mod. Client-only mods (shaders, minimaps, Sodium, Iris, OptiFine, Oculus, JEI-only addons…) crash dedicated servers → set_content_enabled false. "requires X" / missing dependency → search_content + install_content. Mixin errors = wrong mod version or loader.
- "Failed to bind to port": another server/program uses the port → change network.port.
- Lag (TPS < 18 or MSPT > 50): lower view-distance (8) / simulation-distance (6), pre-generate with Chunky (Worlds page), performance mods (Fabric/Quilt: Lithium, FerriteCore, ModernFix, Krypton; NeoForge: ModernFix, FerriteCore), spark to profile ("spark profiler start"), Paper: config/paper-world-defaults.yml.
- Friends can't join: network_status (CGNAT = forwarding can't work → public IPv4 from the ISP or a tunnel like playit.gg), forward_ports, allow_firewall, test_from_internet. Bedrock players use UDP 19132.
- "You are not whitelisted": player_action whitelist add. online-mode=false is insecure – warn.
- Out of memory: raise java.maxMemMb (restart). "Could not reserve enough space": lower it or use 64-bit Java.`,

  app: `How the app works:
- Software: vanilla (Mojang; datapacks), paper (fast, plugins – best default), purpur (Paper + options), folia (multithreaded; Folia plugins only), fabric / quilt (light mod loaders), forge (classic modpacks), neoforge (modern Forge), Modrinth modpacks. Plugins live in plugins/, mods in mods/, datapacks in <world>/datapacks.
- Versions: Mojang uses year versions since 2026 (26.1, 26.2, 26.3, snapshots 26.4-snapshot-3); before that 1.21.x. 26.x worlds keep player data in world/players/.
- The profile owns server.properties (written on start); editing server.properties with a file tool flows back into the profile. Most property/Java changes need a restart.
- Ports: game TCP 25565 by default; RCON = game port + 10, localhost only (never forward it). Commands go over stdin or RCON.
- Servers keep running when the app closes and are re-adopted. Stop saves the world; force_stop kills it.
- .mcsm/ in each server folder: console.log, history/ (config versions), quarantine/ (removed worlds), removed/ (removed mods). Nothing is deleted for good.
- Pages: ${VIEWS.map((v) => `${v.id} (${v.label})`).join(', ')}. Mods & Plugins has Installed / Browse / Updates / Modpacks / Essentials (curated add-ons + Bedrock crossplay). Worlds has Chunky pre-generation.
- The user's own decisions: accepting the Minecraft EULA (Server Settings checkbox), deleting servers, app settings.`,
};
