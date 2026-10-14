# CLAUDE.md

All project guidance for AI assistants lives in **[AGENTS.md](AGENTS.md)** – read it before making changes. User docs: [docs/USER_GUIDE.md](docs/USER_GUIDE.md).
Repository: https://github.com/clawdroppin/Minecraft_Vanilla-Modded_ServerManager – releases are published by pushing a `vX.Y.Z` tag (see AGENTS.md §5).

Quick reminders:
- Minecraft **Java Edition** servers: Vanilla, Paper, Purpur, Folia, Fabric, Quilt, Forge, NeoForge, Modrinth modpacks.
- Default ports: TCP **25565** game, RCON = game + 10 (never forwarded). Servers are tagged `-Dmcsm.id=<id>` and re-adopted after a manager restart.
- Java must be the **exact** major Minecraft asks for (mods like Cobblemon refuse newer Java) – Temurin is downloaded automatically.
- Only write `eula=true` when the user accepted the EULA in the UI.
- Keep `cargo check` warning-free, `cargo test --lib` and `npx tsc -p tsconfig.json --noEmit` passing.
- The mock backend (`src/lib/devMock.ts`) must only load in browser dev (`!isTauri()`).
