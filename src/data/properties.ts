// server.properties catalog (Minecraft wiki). Network keys (server-port, rcon, query, server-ip)
// are owned by the profile's Network section and not listed here.

export type PropType = 'bool' | 'int' | 'string' | 'enum' | 'text';

export interface PropDef {
  key: string;
  label: string;
  type: PropType;
  def: string;
  group: 'General' | 'Gameplay' | 'World' | 'Players & security' | 'Performance' | 'Resource pack' | 'Advanced';
  desc: string;
  options?: string[];
  min?: number;
  max?: number;
  /** Shown as recommended/important */
  hot?: boolean;
}

export const PROPERTIES: PropDef[] = [
  // General
  { key: 'motd', label: 'MOTD', type: 'text', def: 'A Minecraft Server', group: 'General', desc: 'Message shown under the server name in the multiplayer list. Supports colour codes (§ / &).', hot: true },
  { key: 'max-players', label: 'Max players', type: 'int', def: '20', min: 1, max: 1000, group: 'General', desc: 'Most players online at once (operators can bypass it if configured).', hot: true },
  { key: 'hide-online-players', label: 'Hide online players', type: 'bool', def: 'false', group: 'General', desc: 'Don’t send the player list in the server list status.' },
  { key: 'enable-status', label: 'Show in server list', type: 'bool', def: 'true', group: 'General', desc: 'Off makes the server appear offline in the multiplayer list (it still accepts players).' },
  { key: 'bug-report-link', label: 'Bug report link', type: 'string', def: '', group: 'General', desc: 'URL shown in the game’s pause/disconnect screens for reporting problems.' },

  // Gameplay
  { key: 'gamemode', label: 'Default game mode', type: 'enum', def: 'survival', options: ['survival', 'creative', 'adventure', 'spectator'], group: 'Gameplay', desc: 'Game mode new players start in.', hot: true },
  { key: 'force-gamemode', label: 'Force game mode', type: 'bool', def: 'false', group: 'Gameplay', desc: 'Put players back into the default game mode every time they join.' },
  { key: 'difficulty', label: 'Difficulty', type: 'enum', def: 'easy', options: ['peaceful', 'easy', 'normal', 'hard'], group: 'Gameplay', desc: 'Mob damage, hunger, and which mobs spawn.', hot: true },
  { key: 'hardcore', label: 'Hardcore', type: 'bool', def: 'false', group: 'Gameplay', desc: 'Hard difficulty and players become spectators after death. Set before generating the world.' },
  { key: 'pvp', label: 'PvP', type: 'bool', def: 'true', group: 'Gameplay', desc: 'Players can damage each other. (26.x moves this to the pvp gamerule.)', hot: true },
  { key: 'allow-flight', label: 'Allow flight', type: 'bool', def: 'false', group: 'Gameplay', desc: 'Don’t kick players who fly in survival – turn on for mods/plugins with flight (jetpacks, elytra mods) to avoid false kicks.' },
  { key: 'spawn-monsters', label: 'Spawn monsters', type: 'bool', def: 'true', group: 'Gameplay', desc: 'Hostile mobs spawn. Prefer the doMobSpawning gamerule on new versions.' },
  { key: 'enable-command-block', label: 'Command blocks', type: 'bool', def: 'false', group: 'Gameplay', desc: 'Allow command blocks to run.' },
  { key: 'spawn-protection', label: 'Spawn protection radius', type: 'int', def: '16', min: 0, max: 1000, group: 'Gameplay', desc: 'Blocks around spawn non-operators can’t change. 0 turns it off (it’s also off when no operators exist).' },
  { key: 'player-idle-timeout', label: 'Kick idle players (minutes)', type: 'int', def: '0', min: 0, max: 1440, group: 'Gameplay', desc: 'Kick players who are idle this long. 0 = never.' },

  // World
  { key: 'level-name', label: 'World folder', type: 'string', def: 'world', group: 'World', desc: 'Folder of the world the server loads (switch worlds under Worlds).', hot: true },
  { key: 'level-seed', label: 'Seed', type: 'string', def: '', group: 'World', desc: 'Seed for new worlds. Empty = random. Only used when the world is first generated.' },
  { key: 'level-type', label: 'World type', type: 'enum', def: 'minecraft:normal', options: ['minecraft:normal', 'minecraft:flat', 'minecraft:large_biomes', 'minecraft:amplified', 'minecraft:single_biome_surface'], group: 'World', desc: 'Generator preset for new worlds.' },
  { key: 'generator-settings', label: 'Generator settings', type: 'string', def: '{}', group: 'World', desc: 'JSON for flat / single-biome worlds (advanced).' },
  { key: 'generate-structures', label: 'Generate structures', type: 'bool', def: 'true', group: 'World', desc: 'Villages, temples, strongholds… in new chunks.' },
  { key: 'allow-nether', label: 'Allow Nether', type: 'bool', def: 'true', group: 'World', desc: 'Players can travel to the Nether.' },
  { key: 'max-world-size', label: 'World border radius', type: 'int', def: '29999984', min: 1, max: 29999984, group: 'World', desc: 'Maximum radius of the world border in blocks.' },
  { key: 'initial-enabled-packs', label: 'Initial enabled packs', type: 'string', def: 'vanilla', group: 'World', desc: 'Datapacks / experiments enabled when the world is created (comma separated), e.g. vanilla,minecart_improvements.' },
  { key: 'initial-disabled-packs', label: 'Initial disabled packs', type: 'string', def: '', group: 'World', desc: 'Datapacks not auto-enabled when the world is created.' },
  { key: 'region-file-compression', label: 'Region compression', type: 'enum', def: 'deflate', options: ['deflate', 'lz4', 'none'], group: 'World', desc: 'Chunk storage compression: lz4 is faster but larger.' },

  // Players & security
  { key: 'online-mode', label: 'Online mode (verify accounts)', type: 'bool', def: 'true', group: 'Players & security', desc: 'Check every player against Mojang/Microsoft. Turn off only behind a proxy (Velocity/BungeeCord) – otherwise anyone can join as anyone.', hot: true },
  { key: 'white-list', label: 'Whitelist', type: 'bool', def: 'false', group: 'Players & security', desc: 'Only players on the whitelist may join (manage it under Players).', hot: true },
  { key: 'enforce-whitelist', label: 'Enforce whitelist', type: 'bool', def: 'false', group: 'Players & security', desc: 'Kick online players who aren’t whitelisted when the whitelist reloads.' },
  { key: 'enforce-secure-profile', label: 'Require signed chat', type: 'bool', def: 'true', group: 'Players & security', desc: 'Players must have a Mojang-signed public key (secure chat).' },
  { key: 'op-permission-level', label: 'Operator level', type: 'enum', def: '4', options: ['1', '2', '3', '4'], group: 'Players & security', desc: 'Default permission level for /op: 1 bypass spawn protection, 2 cheats & command blocks, 3 multiplayer management, 4 everything.' },
  { key: 'function-permission-level', label: 'Function permission level', type: 'enum', def: '2', options: ['1', '2', '3', '4'], group: 'Players & security', desc: 'Permission level of datapack functions.' },
  { key: 'prevent-proxy-connections', label: 'Block VPN / proxies', type: 'bool', def: 'false', group: 'Players & security', desc: 'Kick players whose ISP differs from the one Mojang sees (blocks some VPNs).' },
  { key: 'broadcast-console-to-ops', label: 'Broadcast console to ops', type: 'bool', def: 'true', group: 'Players & security', desc: 'Show console command output to online operators.' },
  { key: 'log-ips', label: 'Log player IPs', type: 'bool', def: 'true', group: 'Players & security', desc: 'Write player IP addresses into the log (the manager uses them for IP bans).' },
  { key: 'accepts-transfers', label: 'Accept transfers', type: 'bool', def: 'false', group: 'Players & security', desc: 'Accept players sent here with /transfer from another server.' },
  { key: 'rate-limit', label: 'Packet rate limit', type: 'int', def: '0', min: 0, max: 100000, group: 'Players & security', desc: 'Kick clients sending more packets per second than this. 0 = off.' },
  { key: 'text-filtering-config', label: 'Text filtering config', type: 'string', def: '', group: 'Players & security', desc: 'Chat filtering service configuration (Realms feature).' },

  // Performance
  { key: 'view-distance', label: 'View distance', type: 'int', def: '10', min: 3, max: 32, group: 'Performance', desc: 'Chunks sent to players (radius). 8–12 suits most servers.', hot: true },
  { key: 'simulation-distance', label: 'Simulation distance', type: 'int', def: '10', min: 3, max: 32, group: 'Performance', desc: 'Chunks around players that tick (mobs, farms, crops). The biggest lever for TPS.', hot: true },
  { key: 'entity-broadcast-range-percentage', label: 'Entity broadcast range %', type: 'int', def: '100', min: 10, max: 1000, group: 'Performance', desc: 'How far away entities are sent to clients, as % of the default.' },
  { key: 'network-compression-threshold', label: 'Compression threshold', type: 'int', def: '256', min: -1, max: 65535, group: 'Performance', desc: 'Packets larger than this (bytes) are compressed. -1 disables compression (LAN only).' },
  { key: 'max-tick-time', label: 'Watchdog (ms)', type: 'int', def: '60000', min: -1, max: 600000, group: 'Performance', desc: 'Stop the server if one tick takes longer than this. -1 disables the watchdog (useful for big modpacks during world-gen).' },
  { key: 'sync-chunk-writes', label: 'Synchronous chunk writes', type: 'bool', def: 'true', group: 'Performance', desc: 'Safer saving at some performance cost. Paper defaults this off.' },
  { key: 'use-native-transport', label: 'Native transport', type: 'bool', def: 'true', group: 'Performance', desc: 'Linux-only networking optimisation (no effect on Windows).' },
  { key: 'max-chained-neighbor-updates', label: 'Max chained neighbour updates', type: 'int', def: '1000000', min: -1, max: 100000000, group: 'Performance', desc: 'Limit on consecutive block updates (stops lag machines).' },
  { key: 'pause-when-empty-seconds', label: 'Pause when empty (s)', type: 'int', def: '60', min: -1, max: 86400, group: 'Performance', desc: 'Stop ticking this many seconds after the last player leaves (1.21.2+). -1 never pauses.' },

  // Resource pack
  { key: 'resource-pack', label: 'Resource pack URL', type: 'string', def: '', group: 'Resource pack', desc: 'Direct download link to a .zip resource pack offered to players.' },
  { key: 'resource-pack-sha1', label: 'Resource pack SHA-1', type: 'string', def: '', group: 'Resource pack', desc: 'Hash of the pack so clients can cache it.' },
  { key: 'resource-pack-id', label: 'Resource pack UUID', type: 'string', def: '', group: 'Resource pack', desc: 'Optional id for the pack.' },
  { key: 'require-resource-pack', label: 'Require the pack', type: 'bool', def: 'false', group: 'Resource pack', desc: 'Kick players who decline the resource pack.' },
  { key: 'resource-pack-prompt', label: 'Prompt text', type: 'string', def: '', group: 'Resource pack', desc: 'Message shown when the pack is offered (JSON text).' },

  // Advanced
  { key: 'enable-jmx-monitoring', label: 'JMX monitoring', type: 'bool', def: 'false', group: 'Advanced', desc: 'Expose tick-time MBeans for JMX tools.' },
  { key: 'debug', label: 'Debug', type: 'bool', def: 'false', group: 'Advanced', desc: 'Extra debug logging.' },
];

export const PROPERTY_GROUPS = ['General', 'Gameplay', 'World', 'Players & security', 'Performance', 'Resource pack', 'Advanced'] as const;

/** Keys the Network section owns (never edited as plain properties). */
export const NETWORK_KEYS = ['server-port', 'server-ip', 'enable-rcon', 'rcon.port', 'rcon.password', 'broadcast-rcon-to-ops', 'enable-query', 'query.port'];

export function propDef(key: string) {
  return PROPERTIES.find((p) => p.key === key);
}
