// Mirrors src-tauri/src/models.rs and the command payloads (serde camelCase).

export type Software = 'vanilla' | 'paper' | 'purpur' | 'folia' | 'fabric' | 'quilt' | 'forge' | 'neoforge';

export interface AppSettings {
  defaultInstallRoot: string | null;
  backupDir: string | null;
  javaDir: string | null;
  lastDataDir?: string | null;
  telemetryIntervalMs: number;
  confirmDestructive: boolean;
  reduceMotion: boolean;
  versionCheckMins: number;
  notifySnapshots: boolean;
  lastSeenRelease: string;
  lastSeenSnapshot: string;
  accent: string;
  closeToTray: boolean;
  webhookUrl: string;
  webhookLevels: string[];
  curseforgeKey: string;
}

export interface LaunchTarget {
  kind: 'jar' | 'args' | string;
  path: string;
}

export interface InstallState {
  software: Software | string;
  mcVersion: string;
  loaderVersion: string;
  target: LaunchTarget;
  javaMajor: number;
  installedAt: number;
  managedFiles: string[];
}

export interface JavaSettings {
  path: string;
  minMemMb: number;
  maxMemMb: number;
  gcPreset: 'aikar' | 'g1' | 'zgc' | 'none' | string;
  jvmArgs: string;
  serverArgs: string;
  priority: 'idle' | 'belowNormal' | 'normal' | 'aboveNormal' | 'high' | string;
  affinity: number[];
}

export interface ExtraPort {
  port: number;
  protocol: 'tcp' | 'udp' | 'both' | string;
  label: string;
}

export interface NetworkSettings {
  port: number;
  bindIp: string;
  rconEnabled: boolean;
  rconPort: number;
  rconPassword: string;
  queryEnabled: boolean;
  queryPort: number;
  upnp: boolean;
  extraPorts: ExtraPort[];
  publicAddress: string;
}

export type ScheduleAction = 'restart' | 'backup' | 'stop' | 'start' | 'command' | 'say';

export interface ScheduleEntry {
  enabled: boolean;
  time: string;
  days: number[];
  action: ScheduleAction | string;
  command: string;
}

export interface AutomationSettings {
  autoStart: boolean;
  autoRestart: boolean;
  restartDelaySecs: number;
  crashLimit: number;
  stopTimeoutSecs: number;
  schedules: ScheduleEntry[];
  restartWarnings: number[];
  warningMessage: string;
  backupIntervalMins: number;
  backupKeep: number;
  backupOnlyIfPlayers: boolean;
  backupOnStop: boolean;
  backupBeforeUpdate: boolean;
  backupMods: boolean;
  updateCheckMins: number;
  autoUpdateBuilds: boolean;
  memoryLimitMb: number;
  maxUptimeHours: number;
  emptyRestartMins: number;
}

export interface ModpackRef {
  source: string;
  projectId: string;
  versionId: string;
  title: string;
  versionName: string;
  iconUrl: string;
}

export interface ServerInstance {
  id: string;
  name: string;
  color: string;
  installDir: string;
  createdAt: number;
  notes: string;
  software: Software | string;
  mcVersion: string;
  loaderVersion: string;
  installed: InstallState | null;
  eulaAccepted: boolean;
  java: JavaSettings;
  network: NetworkSettings;
  properties: Record<string, string>;
  automation: AutomationSettings;
  modpack: ModpackRef | null;
}

export type ServerState = 'notInstalled' | 'stopped' | 'installing' | 'updating' | 'starting' | 'running' | 'stopping' | 'restarting' | 'crashed';

export interface OnlinePlayer {
  name: string;
  uuid: string | null;
  ip: string | null;
  joinedAt: number;
}

export interface NextSchedule {
  at: number;
  action: string;
}

export interface UpdateInfo {
  kind: 'build' | 'release' | string;
  current: string;
  latest: string;
  label: string;
}

export interface ServerStatus {
  id: string;
  state: ServerState;
  pid: number | null;
  startedAt: number | null;
  readyAt: number | null;
  bootSecs: number | null;
  runningVersion: string | null;
  crashCount: number;
  crashLoop: boolean;
  pendingRestart: boolean;
  players: OnlinePlayer[];
  maxPlayers: number | null;
  tps: number | null;
  mspt: number | null;
  nextSchedule: NextSchedule | null;
  lastBackup: number | null;
  lastExitCode: number | null;
  busy: string | null;
  update: UpdateInfo | null;
  canCommand: boolean;
  countdown: NextSchedule | null;
}

export interface ProcSample {
  id: string;
  cpu: number;
  mem: number;
  virt: number;
  readBps: number;
  writeBps: number;
  uptime: number;
  players: number;
  tps: number | null;
}

export interface SystemSample {
  cpu: number;
  memUsed: number;
  memTotal: number;
  rxBps: number;
  txBps: number;
  cores: number;
  perCore: number[];
}

export interface TelemetryEvent {
  ts: number;
  system: SystemSample;
  servers: ProcSample[];
}

export interface TaskEvent {
  taskId: string;
  instanceId: string | null;
  kind: string;
  title: string;
  message: string;
  progress: number | null;
  done: boolean;
  error: string | null;
  silent?: boolean;
}

export interface LogBatch {
  id: string;
  source: string;
  lines: string[];
  ts: number;
}

export interface ServerEvent {
  id: string;
  level: 'info' | 'success' | 'warn' | 'error';
  message: string;
  ts: number;
}

export interface PlayerEvent {
  id: string;
  name: string;
  kind: 'join' | 'leave' | 'chat' | 'advancement' | string;
  text: string;
  ts: number;
}

export interface BackupInfo {
  path: string;
  name: string;
  size: number;
  createdAt: number;
  label: string;
  kind: 'manual' | 'auto' | string;
  files: number;
  mcVersion: string;
  worlds: string[];
}

export interface ConfigFile {
  path: string;
  exists: boolean;
  text: string;
  modified: number | null;
}

export interface ConfigEntry {
  rel: string;
  size: number;
  modified: number;
}

export interface LogFile {
  path: string;
  name: string;
  kind: 'log' | 'crash' | 'console' | string;
  size: number;
  modified: number;
}

export interface PackManifest {
  format: number;
  appVersion: string;
  exportedAt: number;
  instance: ServerInstance;
  hasWorlds: boolean;
  hasConfigs: boolean;
  hasMods: boolean;
  includesSecrets: boolean;
}

export interface DiagCheck {
  id: string;
  title: string;
  level: 'ok' | 'warn' | 'error' | 'info';
  detail: string;
  fix: string;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  portable: boolean;
  cores: number;
  totalMemory: number;
  os: string;
  hostname: string;
}

export interface FolderProbe {
  exists: boolean;
  writable: boolean;
  empty: boolean;
  detected: InstallState | null;
  freeBytes: number | null;
}

export interface McVersion {
  id: string;
  kind: 'release' | 'snapshot' | 'old_beta' | 'old_alpha' | string;
  releaseTime: string;
}

export interface VersionList {
  latestRelease: string;
  latestSnapshot: string;
  versions: McVersion[];
}

export interface LoaderVersion {
  id: string;
  stable: boolean;
  tag: string;
}

export interface JavaRuntime {
  path: string;
  major: number;
  version: string;
  vendor: string;
  managed: boolean;
  source: string;
}

export interface WorldInfo {
  name: string;
  path: string;
  active: boolean;
  size: number;
  modified: number;
  levelName: string | null;
  version: string | null;
  dataVersion: number | null;
  seed: string | null;
  gameType: number | null;
  difficulty: number | null;
  hardcore: boolean;
  lastPlayed: number | null;
  dayTime: number | null;
  dimensions: string[];
  hasNether: boolean;
  hasEnd: boolean;
  datapacks: number;
}

export interface Datapack {
  name: string;
  size: number;
  isZip: boolean;
  description: string | null;
}

export interface PlayerLists {
  whitelist: Record<string, any>[];
  ops: Record<string, any>[];
  bannedPlayers: Record<string, any>[];
  bannedIps: Record<string, any>[];
  known: Record<string, any>[];
}

export interface PlayerStats {
  uuid: string;
  name: string;
  /** Ticks (20 per second). */
  playTicks: number;
  deaths: number;
  mobKills: number;
  playerKills: number;
  distanceCm: number;
  jumps: number;
  blocksMined: number;
  itemsCrafted: number;
  lastSeen: number;
}

export interface SourceInfo {
  source: string;
  projectId: string;
  versionId: string;
  slug: string;
  title: string;
  versionNumber: string;
  iconUrl: string;
  clientSide: string;
  serverSide: string;
  installedAt: number;
  dependency: boolean;
}

export interface JarMeta {
  modId: string | null;
  name: string | null;
  version: string | null;
  description: string | null;
  authors: string[];
  icon: string | null;
  environment: string | null;
  loader: string | null;
}

export interface ContentItem {
  fileName: string;
  enabled: boolean;
  size: number;
  modified: number;
  sha1: string;
  meta: JarMeta;
  source: SourceInfo | null;
}

export interface ContentState {
  kind: 'mods' | 'plugins' | 'none';
  dir: string | null;
  items: ContentItem[];
}

export interface SearchHit {
  source: 'modrinth' | 'curseforge' | 'hangar' | string;
  id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  follows: number;
  iconUrl: string;
  categories: string[];
  clientSide: string;
  serverSide: string;
  updated: string;
  projectType: string;
  url: string;
}

export interface SearchPage {
  hits: SearchHit[];
  total: number;
  offset: number;
}

export interface ProjectVersion {
  id: string;
  name: string;
  versionNumber: string;
  gameVersions: string[];
  loaders: string[];
  versionType: string;
  published: string;
  downloads: number;
  fileName: string;
  size: number;
}

export interface Installed {
  title: string;
  fileName: string;
  dependency: boolean;
  skipped: string | null;
}

export interface UpdateCandidate {
  fileName: string;
  title: string;
  projectId: string;
  current: string;
  latest: string;
  versionId: string;
  source: string;
}

export interface FirewallState {
  ruleExists: boolean;
  portsOk: boolean;
  rulePorts: string;
  programRule: boolean;
  blockedBy: string[];
  java: string | null;
}

export interface LocalIp {
  interface: string;
  ip: string;
}

export interface NetInfo {
  publicIp: string | null;
  local: LocalIp[];
  gateway: string | null;
}

export interface UpnpMapping {
  protocol: string;
  externalPort: number;
  internalPort: number;
  internalClient: string;
  description: string;
  enabled: boolean;
}

export interface UpnpInfo {
  found: boolean;
  gateway: string | null;
  routerName: string | null;
  manufacturer: string | null;
  model: string | null;
  externalIp: string | null;
  wanKind: 'public' | 'private' | 'cgnat' | 'unknown';
  mappings: UpnpMapping[];
  error: string | null;
}

export interface PingResult {
  online: boolean;
  address: string;
  latencyMs: number | null;
  version: string;
  protocol: number;
  playersOnline: number;
  playersMax: number;
  sample: string[];
  motd: string;
  motdRaw: string;
  favicon: string | null;
  error: string | null;
}

export interface DiskUsage {
  total: number;
  worlds: number;
  content: number;
  libraries: number;
  logs: number;
  backups: number;
}
