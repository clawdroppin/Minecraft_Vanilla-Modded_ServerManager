// Fake backend for `npm run dev` in a plain browser. Never loaded inside Tauri (see main.tsx).
import { installMock, mockEmit } from './ipc';
import type { AppSettings, BackupInfo, ContentItem, ServerInstance, ServerStatus, VersionList } from './types';

const now = Date.now();

function makeInstance(id: string, name: string, port: number, color: string, software: string, mc: string, loader: string, extra: Partial<ServerInstance> = {}): ServerInstance {
  return {
    id,
    name,
    color,
    installDir: `D:\\MinecraftServers\\${name.replace(/\W+/g, '-')}`,
    createdAt: now - 86400000 * 3,
    notes: '',
    software,
    mcVersion: mc,
    loaderVersion: '',
    installed: { software, mcVersion: mc, loaderVersion: loader, target: { kind: 'jar', path: `${software}-${mc}.jar` }, javaMajor: 25, installedAt: now - 86400000 * 2, managedFiles: [] },
    eulaAccepted: true,
    java: { path: '', minMemMb: 4096, maxMemMb: 6144, gcPreset: 'aikar', jvmArgs: '', serverArgs: 'nogui', priority: 'normal', affinity: [] },
    network: { port, bindIp: '', rconEnabled: true, rconPort: port + 10, rconPassword: 'k3QmZp8vXwT2nR7s', queryEnabled: false, queryPort: port, upnp: false, extraPorts: [], publicAddress: '' },
    properties: {
      motd: '§a§lEmerald Isles §7– §fsurvival SMP\n§eNew: §bthe deep dark update is live!',
      'max-players': '20',
      difficulty: 'normal',
      gamemode: 'survival',
      'view-distance': '10',
      'simulation-distance': '8',
      'online-mode': 'true',
      'white-list': 'true',
      pvp: 'true',
      'level-name': 'world',
      'spawn-protection': '16',
    },
    automation: {
      autoStart: false,
      autoRestart: true,
      restartDelaySecs: 10,
      crashLimit: 3,
      stopTimeoutSecs: 120,
      schedules: [{ enabled: true, time: '05:00', days: [], action: 'restart', command: '' }],
      restartWarnings: [300, 60, 10],
      warningMessage: 'Server restarting in {time}!',
      backupIntervalMins: 60,
      backupKeep: 24,
      backupOnlyIfPlayers: true,
      backupOnStop: true,
      backupBeforeUpdate: true,
      backupMods: false,
      updateCheckMins: 120,
      autoUpdateBuilds: false,
      memoryLimitMb: 0,
      maxUptimeHours: 0,
      emptyRestartMins: 0,
    },
    modpack: null,
    ...extra,
  };
}

let instances: ServerInstance[] = [
  makeInstance('a1b2c3d4-0001', 'Emerald Isles SMP', 25565, 'grass', 'paper', '26.3', '42'),
  makeInstance('a1b2c3d4-0002', 'Create Modpack', 25566, 'copper', 'neoforge', '1.21.1', '21.1.200'),
];

const players = [
  { name: 'Steve', uuid: '8667ba71-b85a-4004-af54-457a9734eed7', ip: '192.168.1.23', joinedAt: now - 3600_000 },
  { name: 'Alex', uuid: 'ec561538-f3fd-461d-aff5-086b22154bce', ip: '82.11.4.9', joinedAt: now - 900_000 },
  { name: 'jeb_', uuid: '853c80ef-3c37-49fd-aa49-938b674adae6', ip: '91.7.22.1', joinedAt: now - 300_000 },
];

const statuses: Record<string, ServerStatus> = {};
function status(id: string, patch: Partial<ServerStatus> = {}): ServerStatus {
  const s: ServerStatus = statuses[id] ?? {
    id,
    state: 'stopped',
    pid: null,
    startedAt: null,
    readyAt: null,
    bootSecs: null,
    runningVersion: null,
    crashCount: 0,
    crashLoop: false,
    pendingRestart: false,
    players: [],
    maxPlayers: 20,
    tps: null,
    mspt: null,
    nextSchedule: { at: now + 3600_000 * 5, action: 'restart' },
    lastBackup: now - 1800_000,
    lastExitCode: null,
    busy: null,
    update: null,
    canCommand: false,
    countdown: null,
  };
  statuses[id] = { ...s, ...patch };
  return statuses[id];
}
status('a1b2c3d4-0001', { state: 'running', pid: 4242, startedAt: now - 7200_000, readyAt: now - 7180_000, bootSecs: 6.4, runningVersion: '26.3', players, tps: 19.9, mspt: 11.2, canCommand: true });
status('a1b2c3d4-0002', { update: { kind: 'build', current: '21.1.200', latest: '21.1.209', label: 'NeoForge 21.1.200 → 21.1.209 for Minecraft 1.21.1' } });

let settings: AppSettings = {
  defaultInstallRoot: null,
  backupDir: null,
  javaDir: null,
  telemetryIntervalMs: 1000,
  confirmDestructive: true,
  reduceMotion: false,
  versionCheckMins: 30,
  notifySnapshots: false,
  lastSeenRelease: '26.3',
  lastSeenSnapshot: '26.4-snapshot-3',
  accent: 'grass',
  closeToTray: true,
  webhookUrl: '',
  webhookLevels: ['success', 'error'],
  curseforgeKey: '',
  ai: { baseUrl: 'http://localhost:11434/v1', model: '', apiKey: '', temperature: 0.3, approval: 'ask', tools: true },
};

const RELEASES = ['26.3', '26.2', '26.1.2', '26.1.1', '26.1', '1.21.11', '1.21.10', '1.21.8', '1.21.4', '1.21.1', '1.21', '1.20.6', '1.20.4', '1.20.1', '1.19.4', '1.19.2', '1.18.2', '1.17.1', '1.16.5', '1.15.2', '1.14.4', '1.12.2', '1.8.9', '1.7.10'];
const versionList = (sw: string): VersionList => ({
  latestRelease: '26.3',
  latestSnapshot: '26.4-snapshot-3',
  versions: [
    ...(sw === 'vanilla' || sw === 'fabric' ? [{ id: '26.4-snapshot-3', kind: 'snapshot', releaseTime: new Date(now - 7 * 86400000).toISOString() }] : []),
    ...RELEASES.filter((v) => (sw === 'neoforge' ? v.startsWith('26') || v.startsWith('1.21') || v === '1.20.4' : sw === 'folia' ? v.startsWith('26') || v.startsWith('1.21') : true)).map((id, i) => ({
      id,
      kind: 'release',
      releaseTime: new Date(now - (i + 1) * 40 * 86400000).toISOString(),
    })),
  ],
});

const mods: ContentItem[] = [
  ['Create', 'create-1.21.1-6.0.6.jar', '6.0.6', 'Aesthetic technology and building mod', 'https://cdn.modrinth.com/data/LNytGWDc/icon.png'],
  ['Farmer’s Delight', 'FarmersDelight-1.21.1-1.2.7.jar', '1.2.7', 'Expands farming and cooking', 'https://cdn.modrinth.com/data/R2OftAxM/icon.png'],
  ['Sodium', 'sodium-neoforge-0.6.13.jar', '0.6.13', 'Rendering engine replacement', 'https://cdn.modrinth.com/data/AANobbMI/icon.png'],
  ['Jade', 'Jade-1.21.1-NeoForge-15.10.2.jar', '15.10.2', 'Shows what you are looking at', 'https://cdn.modrinth.com/data/nvQzSEkH/icon.png'],
].map(([title, file, ver, desc, icon], i) => ({
  fileName: file,
  enabled: true,
  size: 1_200_000 + i * 900_000,
  modified: now - i * 86400000,
  sha1: `sha${i}`,
  meta: { modId: title.toLowerCase(), name: title, version: ver, description: desc, authors: [], icon: null, environment: i === 2 ? 'client' : '*', loader: 'neoforge' },
  source: { source: 'modrinth', projectId: `p${i}`, versionId: `v${i}`, slug: title.toLowerCase(), title, versionNumber: ver, iconUrl: icon, clientSide: 'required', serverSide: i === 2 ? 'unsupported' : 'required', installedAt: now, dependency: false },
}));

const backups: BackupInfo[] = [
  { path: 'D:\\b\\1.zip', name: '2026-10-13_09-00-00_auto.zip', size: 184_000_000, createdAt: now - 1800_000, label: 'Interval', kind: 'auto', files: 4120, mcVersion: '26.3', worlds: ['world', 'world_nether', 'world_the_end'] },
  { path: 'D:\\b\\2.zip', name: '2026-10-12_21-14-02_manual.zip', size: 179_500_000, createdAt: now - 86400000 / 2, label: 'Before the Ender Dragon', kind: 'manual', files: 4002, mcVersion: '26.3', worlds: ['world', 'world_nether', 'world_the_end'] },
];

const LOG = [
  '[12:00:01] [Server thread/INFO]: Starting minecraft server version 26.3',
  '[12:00:01] [Server thread/INFO]: Loading properties',
  '[12:00:02] [Server thread/INFO]: Default game type: SURVIVAL',
  '[12:00:02] [Server thread/INFO]: Starting Minecraft server on *:25565',
  '[12:00:05] [Server thread/INFO]: Preparing level "world"',
  '[12:00:07] [Server thread/INFO]: Done (6.412s)! For help, type "help"',
  '[12:00:07] [Server thread/INFO]: RCON running on 0.0.0.0:25575',
  '[12:04:51] [User Authenticator #1/INFO]: UUID of player Steve is 8667ba71-b85a-4004-af54-457a9734eed7',
  '[12:04:51] [Server thread/INFO]: Steve joined the game',
  '[12:05:30] [Server thread/INFO]: <Steve> anyone up for the end?',
  '[12:07:12] [Server thread/WARN]: Can\'t keep up! Is the server overloaded? Running 2154ms or 43 ticks behind',
  '[12:09:44] [Server thread/INFO]: Alex has made the advancement [Stone Age]',
];

export function installDevMock() {
  const find = (id: unknown) => instances.find((i) => i.id === id)!;
  installMock({
    get_app_info: () => ({ version: '1.2.0', dataDir: 'C:\\Users\\me\\AppData\\Roaming\\com.minecraftservermanager.app', portable: false, cores: 16, totalMemory: 34359738368, os: 'Windows 11 Pro', hostname: 'GAMING-PC' }),
    get_settings: () => settings,
    save_settings: ({ settings: s }) => void (settings = s as AppSettings),
    list_instances: () => instances,
    get_statuses: () => instances.map((i) => status(i.id)),
    suggest_ports: () => [25567, 25577],
    game_versions: ({ software }) => versionList(String(software)),
    loader_versions: ({ software }) =>
      String(software) === 'paper'
        ? [42, 41, 40, 38].map((n, i) => ({ id: String(n), stable: i > 0, tag: i === 0 ? 'beta' : '' }))
        : [
            { id: '21.1.209', stable: true, tag: 'recommended' },
            { id: '21.1.205', stable: true, tag: '' },
            { id: '21.1.200', stable: true, tag: '' },
          ],
    create_instance: ({ input }) => {
      const i = input as { name: string; software: string; mcVersion: string; color: string; port: number };
      const inst = makeInstance(crypto.randomUUID(), i.name, i.port ?? 25567, i.color ?? 'grass', i.software, i.mcVersion || '26.3', '');
      instances = [...instances, inst];
      status(inst.id);
      return inst;
    },
    update_instance: ({ instance }) => {
      const i = instance as ServerInstance;
      instances = instances.map((x) => (x.id === i.id ? i : x));
      return i;
    },
    reorder_instances: () => undefined,
    delete_instance: ({ id }) => void (instances = instances.filter((i) => i.id !== id)),
    clone_instance: ({ id, name }) => {
      const c = { ...structuredClone(find(id)), id: crypto.randomUUID(), name: String(name) };
      instances = [...instances, c];
      return c;
    },
    install_server: () => undefined,
    change_version: () => undefined,
    check_update: ({ id }) => status(String(id)).update,
    start_server: ({ id }) => {
      status(String(id), { state: 'starting', startedAt: Date.now() });
      mockEmit('server-status', statuses[String(id)]);
      setTimeout(() => mockEmit('server-status', status(String(id), { state: 'running', readyAt: Date.now(), tps: 20, canCommand: true })), 1500);
    },
    stop_server: ({ id }) => {
      mockEmit('server-status', status(String(id), { state: 'stopped', players: [], tps: null, canCommand: false }));
    },
    restart_server: () => undefined,
    start_countdown: ({ id, action, secs }) => mockEmit('server-status', status(String(id), { countdown: { at: Date.now() + Number(secs) * 1000, action: String(action) } })),
    cancel_countdown: ({ id }) => mockEmit('server-status', status(String(id), { countdown: null })),
    send_command: ({ id, command }) => {
      mockEmit('log-lines', { id, source: 'server', lines: [`[12:10:00] [Server thread/INFO]: > ${command}`], ts: Date.now() });
      return null;
    },
    get_launch_line: () => '"C:\\java\\bin\\java.exe" -Xms4096M -Xmx6144M -XX:+UseG1GC … -jar paper-26.3-42.jar nogui',
    preview_launch_line: ({ instance }) => {
      const i = instance as ServerInstance;
      return `"C:\\Users\\me\\AppData\\Roaming\\com.minecraftservermanager.app\\java\\temurin-25\\jdk-25.0.4+1-jre\\bin\\java.exe" -Xms${i.java.minMemMb}M -Xmx${i.java.maxMemMb}M ${i.java.gcPreset === 'aikar' ? '-XX:+IgnoreUnrecognizedVMOptions -XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 … -Daikars.new.flags=true ' : ''}-Dmcsm.id=${i.id} -Dfile.encoding=UTF-8 ${i.java.jvmArgs} -jar ${i.installed?.target.path ?? 'server.jar'} ${i.java.serverArgs}`;
    },
    clear_crash_state: () => undefined,
    list_java: () => [
      { path: 'C:\\Users\\me\\AppData\\Roaming\\com.minecraftservermanager.app\\java\\temurin-25\\jdk-25.0.4+1-jre\\bin\\java.exe', major: 25, version: '25.0.4', vendor: 'Eclipse Adoptium', managed: true, source: 'managed' },
      { path: 'C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.8.9-hotspot\\bin\\java.exe', major: 21, version: '21.0.8', vendor: 'Eclipse Adoptium', managed: false, source: 'system' },
      { path: 'C:\\Users\\me\\AppData\\Roaming\\.minecraft\\runtime\\java-runtime-delta\\windows-x64\\java-runtime-delta\\bin\\java.exe', major: 21, version: '21.0.7', vendor: 'Microsoft', managed: false, source: 'minecraft-launcher' },
      { path: 'C:\\Program Files\\Java\\jre1.8.0_461\\bin\\java.exe', major: 8, version: '1.8.0_461', vendor: 'Oracle Corporation', managed: false, source: 'system' },
    ],
    probe_java: () => ({ path: 'C:\\java\\bin\\java.exe', major: 21, version: '21.0.8', vendor: 'Custom', managed: false, source: 'custom' }),
    install_java: () => undefined,
    list_config_files: () =>
      ['server.properties', 'bukkit.yml', 'spigot.yml', 'ops.json', 'config/paper-global.yml', 'config/paper-world-defaults.yml', 'plugins/EssentialsX/config.yml', 'plugins/LuckPerms/config.yml'].map((rel, i) => ({ rel, size: 1200 + i * 300, modified: now - i * 3600_000 })),
    read_config: ({ rel }) => ({
      path: `D:\\MinecraftServers\\${rel}`,
      exists: true,
      modified: now - 3600_000,
      text: String(rel).endsWith('.properties') ? '#Minecraft server properties\nmotd=\\u00A7aEmerald Isles\nmax-players=20\ndifficulty=normal\nview-distance=10\n' : '# Paper configuration\nchunk-loading-basic:\n  autoconfig-send-distance: true\n  player-max-chunk-load-rate: 100.0\n',
    }),
    write_config: ({ id }) => ({ restartNeeded: false, instance: find(id) }),
    list_config_history: () => [],
    read_text_file: () => LOG.join('\n'),
    get_server_icon: () => null,
    set_server_icon: () => undefined,
    read_image: () => '',
    read_log_tail: () => LOG,
    list_log_files: () => [
      { path: 'D:\\x\\.mcsm\\console.log', name: 'console.log', kind: 'console', size: 48_000, modified: now },
      { path: 'D:\\x\\logs\\2026-10-12-1.log.gz', name: '2026-10-12-1.log.gz', kind: 'log', size: 12_400, modified: now - 86400000 },
      { path: 'D:\\x\\crash-reports\\crash-2026-10-11.txt', name: 'crash-2026-10-11_21.04.12-server.txt', kind: 'crash', size: 9_400, modified: now - 2 * 86400000 },
    ],
    list_worlds: () => [
      { name: 'world', path: 'D:\\x\\world', active: true, size: 412_000_000, modified: now - 60_000, levelName: 'Emerald Isles', version: '26.3', dataVersion: 4700, seed: '-4172144997902289642', gameType: 0, difficulty: 2, hardcore: false, lastPlayed: now - 60_000, dayTime: 912_000, dimensions: ['world_nether', 'world_the_end'], hasNether: true, hasEnd: true, datapacks: 2 },
      { name: 'creative_build', path: 'D:\\x\\creative_build', active: false, size: 38_000_000, modified: now - 9 * 86400000, levelName: 'Build World', version: '1.21.8', dataVersion: 4440, seed: '12345', gameType: 1, difficulty: 0, hardcore: false, lastPlayed: now - 9 * 86400000, dayTime: 6000, dimensions: [], hasNether: false, hasEnd: false, datapacks: 0 },
    ],
    activate_world: ({ id }) => find(id),
    import_world: () => 'imported',
    export_world: () => undefined,
    delete_world: () => undefined,
    reset_world: () => 1,
    rename_world: () => undefined,
    list_datapacks: () => [
      { name: 'Terralith.zip', size: 4_100_000, isZip: true, description: 'Overworld reimagined' },
      { name: 'graves', size: 80_000, isZip: false, description: 'Keep your items safe' },
    ],
    add_datapacks: () => [],
    remove_datapack: () => undefined,
    player_lists: () => ({
      whitelist: players.map((p) => ({ uuid: p.uuid, name: p.name })),
      ops: [{ uuid: players[0].uuid, name: 'Steve', level: 4, bypassesPlayerLimit: false }],
      bannedPlayers: [{ uuid: '00000000-0000-0000-0000-000000000001', name: 'Griefer42', created: '2026-10-01 18:22:01 +0200', source: 'Steve', expires: 'forever', reason: 'TNT in spawn' }],
      bannedIps: [],
      known: players.map((p) => ({ name: p.name, uuid: p.uuid, expiresOn: '2026-11-13 12:00:00 +0200' })),
    }),
    player_action: () => undefined,
    list_dir: () => [
      { name: 'config', isDir: true, size: 0, modified: now },
      { name: 'mods', isDir: true, size: 0, modified: now },
      { name: 'world', isDir: true, size: 0, modified: now },
      { name: 'server.properties', isDir: false, size: 1480, modified: now },
    ],
    ai_models: () => ['qwen3:8b', 'llama3.1:8b', 'nomic-embed-text'],
    ai_chat: ({ messages, onEvent }) => {
      // Fake assistant: reads the console and health checks, explains, then proposes a change.
      const emit = onEvent as (e: unknown) => void;
      const tools = (messages as { role: string }[]).filter((m) => m.role === 'tool').length;
      const done = (toolCalls: unknown[]) => emit({ kind: 'done', toolCalls, finishReason: toolCalls.length ? 'tool_calls' : 'stop', model: 'qwen3:14b' });
      if (tools === 0) {
        setTimeout(() => done([{ id: 'c1', name: 'read_console', arguments: '{"lines":150}' }, { id: 'c2', name: 'run_health_check', arguments: '{}' }]), 300);
        return 1;
      }
      const text =
        tools === 2
          ? 'Found it – the lag spikes come from **world generation**:\n\n- TPS drops to ~14 whenever someone explores new land\n- `view-distance` is **12**, which loads 625 chunks per player\n- Memory (6 GB) and Java 21 are fine\n\nI suggest lowering the view distance to **8** and pre-generating the world with Chunky. Changing the view distance now:'
          : 'Done – `view-distance` is now **8**. It applies after the next restart; want me to schedule a restart with a 5-minute warning for your players?';
      // Two pieces is enough to show streaming (timers are throttled in headless screenshots).
      const half = text.indexOf(' ', text.length / 2);
      setTimeout(() => emit({ kind: 'delta', text: text.slice(0, half) }), 150);
      setTimeout(() => {
        emit({ kind: 'delta', text: text.slice(half) });
        done(tools === 2 ? [{ id: 'c3', name: 'update_server', arguments: '{"changes":"{\\"properties\\":{\\"view-distance\\":\\"8\\"}}"}' }] : []);
      }, 400);
      return 1;
    },
    ai_stop: () => undefined,
    kick_player: () => undefined,
    player_stats: () =>
      players.map((p, i) => ({ uuid: p.uuid, name: p.name, playTicks: [1_512_000, 640_000, 92_000][i], deaths: [14, 6, 1][i], mobKills: [812, 230, 12][i], playerKills: [3, 1, 0][i], distanceCm: [91_000_000, 30_400_000, 2_100_000][i], jumps: [8200, 3100, 400][i], blocksMined: [24_100, 9_800, 640][i], itemsCrafted: [3_400, 1_200, 90][i], lastSeen: now - i * 7200_000 })),
    setup_bedrock: () => [
      { title: 'Geyser', fileName: 'Geyser-Spigot.jar', dependency: false, skipped: null },
      { title: 'Floodgate', fileName: 'floodgate-spigot.jar', dependency: true, skipped: null },
    ],
    list_content: ({ id }) => (find(id).installed?.software === 'neoforge' ? { kind: 'mods', dir: 'D:\\x\\mods', items: mods } : { kind: 'plugins', dir: 'D:\\x\\plugins', items: [] }),
    add_content_files: () => [],
    set_content_enabled: ({ name }) => name,
    remove_content: () => undefined,
    identify_content: () => 0,
    content_updates: () => [{ fileName: mods[1].fileName, title: 'Farmer’s Delight', projectId: 'p1', current: '1.2.7', latest: '1.2.9', versionId: 'v9', source: 'modrinth' }],
    search_content: ({ projectType }) => ({
      total: 2,
      offset: 0,
      hits: [
        { source: 'modrinth', id: 'P7dR8mSH', slug: 'fabric-api', title: projectType === 'modpack' ? 'Cobblemon Official Modpack' : projectType === 'plugin' ? 'LuckPerms' : 'Lithium', description: 'No-compromises game logic and server optimization mod.', author: 'jellysquid3', downloads: 136_000_000, follows: 23_000, iconUrl: 'https://cdn.modrinth.com/data/gvQqBUqZ/icon.png', categories: ['optimization'], clientSide: 'optional', serverSide: 'optional', updated: new Date(now - 86400000 * 3).toISOString(), projectType: String(projectType), url: 'https://modrinth.com' },
        { source: 'modrinth', id: 'AANobbMI', slug: 'sodium', title: 'Sodium', description: 'The fastest rendering optimization mod for Minecraft.', author: 'jellysquid3', downloads: 120_000_000, follows: 30_000, iconUrl: 'https://cdn.modrinth.com/data/AANobbMI/icon.png', categories: ['optimization'], clientSide: 'required', serverSide: 'unsupported', updated: new Date(now - 86400000 * 9).toISOString(), projectType: String(projectType), url: 'https://modrinth.com' },
      ],
    }),
    project_versions: () => [{ id: 'v1', name: 'Release', versionNumber: '0.26.2', gameVersions: ['26.3'], loaders: ['fabric'], versionType: 'release', published: new Date(now).toISOString(), downloads: 1000, fileName: 'x.jar', size: 900_000 }],
    install_content: ({ title }) => [{ title, fileName: 'x.jar', dependency: false, skipped: null }],
    update_content: () => 1,
    export_client_pack: () => [12, 1],
    install_modpack: () => undefined,
    create_backup: () => undefined,
    list_backups: () => backups,
    get_backup_dir: () => 'D:\\MinecraftServers\\Emerald-Isles-SMP\\backups',
    restore_backup: () => 4120,
    delete_backup: () => undefined,
    export_pack: () => undefined,
    read_pack: () => ({ format: 1, appVersion: '1.0.0', exportedAt: now, instance: instances[0], hasWorlds: true, hasConfigs: true, hasMods: false, includesSecrets: false }),
    import_pack: () => instances[0],
    import_existing: () => instances[0],
    probe_folder: () => ({ exists: true, writable: true, empty: true, detected: null, freeBytes: 812_000_000_000 }),
    firewall_state: () => ({ ruleExists: true, portsOk: true, rulePorts: 'TCP 25565', programRule: true, blockedBy: [], java: 'C:\\java\\bin\\java.exe' }),
    add_firewall_rules: () => undefined,
    remove_firewall_rules: () => undefined,
    net_info: () => ({ publicIp: '203.0.113.42', local: [{ interface: 'Ethernet', ip: '192.168.1.50' }], gateway: '192.168.1.1' }),
    upnp_probe: () => ({
      found: true,
      gateway: '192.168.1.1',
      routerName: 'Archer AX55',
      manufacturer: 'TP-Link',
      model: 'AX3000',
      externalIp: '203.0.113.42',
      wanKind: 'public',
      mappings: [{ protocol: 'TCP', externalPort: 25565, internalPort: 25565, internalClient: '192.168.1.50', description: 'MC Emerald Isles', enabled: true }],
      error: null,
    }),
    upnp_map: () => ['TCP 25565 → 192.168.1.50:25565 forwarded'],
    upnp_unmap: () => undefined,
    ping_server: ({ host, port }) => ({ online: true, address: `${host}:${port}`, latencyMs: 2, version: 'Paper 26.3', protocol: 790, playersOnline: 3, playersMax: 20, sample: [], motd: 'Emerald Isles', motdRaw: '§aEmerald Isles', favicon: null, error: null }),
    external_check: () => ({ online: true, version: 'Paper 26.3', players: { online: 3, max: 20 }, motd: { raw: ['§a§lEmerald Isles §7– §fsurvival SMP'] } }),
    run_diagnostics: () => [
      { id: 'install', title: 'Server software', level: 'ok', detail: 'Paper 26.3 (42) – paper-26.3-42.jar', fix: '' },
      { id: 'eula', title: 'Minecraft EULA', level: 'ok', detail: 'Accepted – written to eula.txt on every start.', fix: '' },
      { id: 'java', title: 'Java', level: 'ok', detail: 'Java 25+ found', fix: '' },
      { id: 'memory', title: 'Memory', level: 'ok', detail: '6144 MB of 32768 MB system memory allocated.', fix: '' },
      { id: 'clientmods', title: 'Client-only mods', level: 'error', detail: '1 only works in the game client and can crash a dedicated server: Sodium.', fix: 'mods' },
      { id: 'firewall', title: 'Windows Firewall', level: 'ok', detail: 'An inbound rule allows the server.', fix: '' },
    ],
    disk_usage: () => ({ total: 1_400_000_000, worlds: 450_000_000, content: 120_000_000, libraries: 310_000_000, logs: 12_000_000, backups: 2_900_000_000 }),
    test_webhook: () => undefined,
    cancel_task: () => undefined,
    app_quit: () => undefined,
    window_hide: () => undefined,
  });

  // Live telemetry so charts move.
  let t = 0;
  setInterval(() => {
    t++;
    mockEmit('telemetry', {
      ts: Date.now(),
      system: { cpu: 18 + Math.sin(t / 7) * 6, memUsed: 14e9 + Math.sin(t / 30) * 4e8, memTotal: 34359738368, rxBps: 120_000 + Math.random() * 60_000, txBps: 340_000 + Math.random() * 90_000, cores: 16, perCore: Array.from({ length: 16 }, (_, i) => 10 + ((i * 13 + t * 7) % 50)) },
      servers: instances
        .filter((i) => statuses[i.id]?.state === 'running')
        .map((i) => ({ id: i.id, cpu: 9 + Math.sin(t / 5) * 4 + Math.random() * 2, mem: 4.1e9 + Math.sin(t / 40) * 2e8, virt: 7e9, readBps: 20_000, writeBps: 80_000 + Math.random() * 40_000, uptime: 7200 + t, players: statuses[i.id].players.length, tps: 19.6 + Math.random() * 0.4 })),
    });
  }, 1000);
  setTimeout(() => {
    mockEmit('log-lines', { id: 'a1b2c3d4-0001', source: 'manager', lines: ['Server is online on port 25565 – started in 6.4s.'], ts: Date.now() });
    for (const [kind, name, text] of [
      ['join', 'Steve', ''],
      ['chat', 'Steve', 'anyone up for the end?'],
      ['join', 'Alex', ''],
      ['advancement', 'Alex', 'Stone Age'],
      ['join', 'jeb_', ''],
      ['chat', 'Alex', 'bring ender pearls!'],
    ])
      mockEmit('player-event', { id: 'a1b2c3d4-0001', name, kind, text, ts: Date.now() - 60_000 });
  }, 400);
}
