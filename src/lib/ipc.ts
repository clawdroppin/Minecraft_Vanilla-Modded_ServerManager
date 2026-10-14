import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import type * as T from './types';

type Handler = (args: Record<string, unknown>) => unknown | Promise<unknown>;
type Mock = { handlers: Record<string, Handler>; listeners: Map<string, Set<(p: unknown) => void>> };

let mock: Mock | null = null;

/** Browser preview only (`npm run dev` outside Tauri) – see devMock.ts. */
export function installMock(handlers: Record<string, Handler>) {
  mock = { handlers, listeners: new Map() };
}
export function mockEmit(event: string, payload: unknown) {
  mock?.listeners.get(event)?.forEach((fn) => fn(payload));
}

async function invoke<R>(cmd: string, args: Record<string, unknown> = {}): Promise<R> {
  if (mock) {
    const h = mock.handlers[cmd];
    if (!h) throw `Mock: command ${cmd} not implemented`;
    await new Promise((r) => setTimeout(r, 40));
    return (await h(args)) as R;
  }
  return tauriInvoke<R>(cmd, args);
}

export async function listen<P>(event: string, fn: (payload: P) => void): Promise<() => void> {
  if (mock) {
    const set = mock.listeners.get(event) ?? new Set();
    mock.listeners.set(event, set);
    const cb = (p: unknown) => fn(p as P);
    set.add(cb);
    return () => set.delete(cb);
  }
  return tauriListen<P>(event, (e) => fn(e.payload));
}

export const inTauri = () => isTauri();

/** Typed wrappers over every Rust command. Errors arrive as plain strings. */
export const api = {
  appInfo: () => invoke<T.AppInfo>('get_app_info'),
  getSettings: () => invoke<T.AppSettings>('get_settings'),
  saveSettings: (settings: T.AppSettings) => invoke<void>('save_settings', { settings }),
  cancelTask: (taskId: string) => invoke<void>('cancel_task', { taskId }),
  testWebhook: (url: string) => invoke<void>('test_webhook', { url }),
  quit: () => invoke<void>('app_quit'),
  hideWindow: () => invoke<void>('window_hide'),
  probeFolder: (path: string) => invoke<T.FolderProbe>('probe_folder', { path }),

  gameVersions: (software: string, refresh = false) => invoke<T.VersionList>('game_versions', { software, refresh }),
  loaderVersions: (software: string, mc: string) => invoke<T.LoaderVersion[]>('loader_versions', { software, mc }),

  listInstances: () => invoke<T.ServerInstance[]>('list_instances'),
  getStatuses: () => invoke<T.ServerStatus[]>('get_statuses'),
  suggestPorts: (excludeId?: string) => invoke<[number, number]>('suggest_ports', { excludeId: excludeId ?? null }),
  createInstance: (input: {
    name: string;
    installDir?: string | null;
    color?: string;
    software: string;
    mcVersion: string;
    loaderVersion: string;
    port?: number | null;
    maxMemMb?: number;
    eulaAccepted: boolean;
    installNow: boolean;
    templateId?: string | null;
    properties?: Record<string, string>;
    modpack?: T.ModpackRef | null;
  }) => invoke<T.ServerInstance>('create_instance', { input }),
  updateInstance: (instance: T.ServerInstance) => invoke<T.ServerInstance>('update_instance', { instance }),
  reorderInstances: (ids: string[]) => invoke<void>('reorder_instances', { ids }),
  deleteInstance: (id: string, deleteFiles: boolean) => invoke<void>('delete_instance', { id, deleteFiles }),
  cloneInstance: (id: string, name: string, copyWorlds: boolean) => invoke<T.ServerInstance>('clone_instance', { id, name, copyWorlds }),
  importExisting: (path: string, name?: string) => invoke<T.ServerInstance>('import_existing', { path, name: name ?? null }),

  installServer: (id: string) => invoke<void>('install_server', { id }),
  changeVersion: (id: string, software: string, mcVersion: string, loaderVersion: string, allowDowngrade = false) =>
    invoke<void>('change_version', { id, software, mcVersion, loaderVersion, allowDowngrade }),
  checkUpdate: (id: string) => invoke<T.UpdateInfo | null>('check_update', { id }),
  start: (id: string) => invoke<void>('start_server', { id }),
  stop: (id: string, force = false) => invoke<void>('stop_server', { id, force }),
  restart: (id: string) => invoke<void>('restart_server', { id }),
  startCountdown: (id: string, action: 'restart' | 'stop', secs: number) => invoke<void>('start_countdown', { id, action, secs }),
  cancelCountdown: (id: string) => invoke<void>('cancel_countdown', { id }),
  sendCommand: (id: string, command: string) => invoke<string | null>('send_command', { id, command }),
  launchLine: (id: string) => invoke<string>('get_launch_line', { id }),
  previewLaunchLine: (instance: T.ServerInstance) => invoke<string>('preview_launch_line', { instance }),
  clearCrashState: (id: string) => invoke<void>('clear_crash_state', { id }),

  listJava: () => invoke<T.JavaRuntime[]>('list_java'),
  probeJava: (path: string) => invoke<T.JavaRuntime>('probe_java', { path }),
  installJava: (feature: number) => invoke<void>('install_java', { feature }),

  configFiles: (id: string) => invoke<T.ConfigEntry[]>('list_config_files', { id }),
  readConfig: (id: string, rel: string) => invoke<T.ConfigFile>('read_config', { id, rel }),
  writeConfig: (id: string, rel: string, text: string) => invoke<{ restartNeeded: boolean; instance: T.ServerInstance }>('write_config', { id, rel, text }),
  configHistory: (id: string, rel: string) => invoke<[string, number][]>('list_config_history', { id, rel }),
  readTextFile: (id: string, path: string) => invoke<string>('read_text_file', { id, path }),
  getServerIcon: (id: string) => invoke<string | null>('get_server_icon', { id }),
  setServerIcon: (id: string, pngBase64: string) => invoke<void>('set_server_icon', { id, pngBase64 }),
  readImage: (path: string) => invoke<string>('read_image', { path }),

  logTail: (id: string, maxLines: number) => invoke<string[]>('read_log_tail', { id, maxLines }),
  logFiles: (id: string) => invoke<T.LogFile[]>('list_log_files', { id }),

  listWorlds: (id: string) => invoke<T.WorldInfo[]>('list_worlds', { id }),
  activateWorld: (id: string, name: string) => invoke<T.ServerInstance>('activate_world', { id, name }),
  importWorld: (id: string, path: string, name?: string) => invoke<string>('import_world', { id, path, name: name ?? null }),
  exportWorld: (id: string, name: string, dest: string) => invoke<void>('export_world', { id, name, dest }),
  deleteWorld: (id: string, name: string) => invoke<void>('delete_world', { id, name }),
  resetWorld: (id: string, name: string, what: 'all' | 'nether' | 'end', seed?: string | null) => invoke<number>('reset_world', { id, name, what, seed: seed ?? null }),
  renameWorld: (id: string, name: string, newName: string) => invoke<void>('rename_world', { id, name, newName }),
  listDatapacks: (id: string, world: string) => invoke<T.Datapack[]>('list_datapacks', { id, world }),
  addDatapacks: (id: string, world: string, paths: string[]) => invoke<string[]>('add_datapacks', { id, world, paths }),
  removeDatapack: (id: string, world: string, name: string) => invoke<void>('remove_datapack', { id, world, name }),

  playerLists: (id: string) => invoke<T.PlayerLists>('player_lists', { id }),
  playerAction: (id: string, list: string, add: boolean, target: string, reason = '') => invoke<void>('player_action', { id, list, add, target, reason }),
  kickPlayer: (id: string, name: string, reason = '') => invoke<void>('kick_player', { id, name, reason }),
  playerStats: (id: string) => invoke<T.PlayerStats[]>('player_stats', { id }),
  setupBedrock: (id: string) => invoke<T.Installed[]>('setup_bedrock', { id }),

  listContent: (id: string) => invoke<T.ContentState>('list_content', { id }),
  addContentFiles: (id: string, paths: string[]) => invoke<string[]>('add_content_files', { id, paths }),
  setContentEnabled: (id: string, name: string, enabled: boolean) => invoke<string>('set_content_enabled', { id, name, enabled }),
  removeContent: (id: string, name: string) => invoke<boolean>('remove_content', { id, name }),
  identifyContent: (id: string) => invoke<number>('identify_content', { id }),
  contentUpdates: (id: string) => invoke<T.UpdateCandidate[]>('content_updates', { id }),
  searchContent: (a: { id?: string | null; source: string; query: string; projectType: string; sort: string; offset: number; mc?: string | null }) =>
    invoke<T.SearchPage>('search_content', { id: a.id ?? null, source: a.source, query: a.query, projectType: a.projectType, sort: a.sort, offset: a.offset, mc: a.mc ?? null }),
  projectVersions: (id: string | null, source: string, project: string, projectType: string) => invoke<T.ProjectVersion[]>('project_versions', { id, source, project, projectType }),
  installContent: (id: string, source: string, project: string, version: string | null, projectType: string, title: string) =>
    invoke<T.Installed[]>('install_content', { id, source, project, version, projectType, title }),
  updateContent: (id: string, items: T.UpdateCandidate[]) => invoke<number>('update_content', { id, items }),
  exportClientPack: (id: string, dest: string, includeUnknown: boolean) => invoke<[number, number]>('export_client_pack', { id, dest, includeUnknown }),
  installModpack: (id: string, modpack: T.ModpackRef, allowDowngrade = false) => invoke<void>('install_modpack', { id, modpack, allowDowngrade }),

  createBackup: (id: string, label: string) => invoke<void>('create_backup', { id, label }),
  listBackups: (id: string) => invoke<T.BackupInfo[]>('list_backups', { id }),
  backupDir: (id: string) => invoke<string>('get_backup_dir', { id }),
  restoreBackup: (id: string, path: string, includeConfig: boolean) => invoke<number>('restore_backup', { id, path, includeConfig }),
  deleteBackup: (id: string, path: string) => invoke<void>('delete_backup', { id, path }),

  exportPack: (id: string, dest: string, options: { includeWorlds: boolean; includeSecrets: boolean; includeMods: boolean }) => invoke<void>('export_pack', { id, dest, options }),
  readPack: (path: string) => invoke<T.PackManifest>('read_pack', { path }),
  importPack: (path: string, name: string, installDir: string | null, installNow: boolean) => invoke<T.ServerInstance>('import_pack', { path, name, installDir, installNow }),

  firewallState: (id: string) => invoke<T.FirewallState>('firewall_state', { id }),
  addFirewallRules: (id: string) => invoke<void>('add_firewall_rules', { id }),
  removeFirewallRules: (id: string) => invoke<void>('remove_firewall_rules', { id }),
  netInfo: () => invoke<T.NetInfo>('net_info'),
  upnpProbe: () => invoke<T.UpnpInfo>('upnp_probe'),
  upnpMap: (id: string) => invoke<string[]>('upnp_map', { id }),
  upnpUnmap: (id: string) => invoke<void>('upnp_unmap', { id }),
  pingServer: (host: string, port: number) => invoke<T.PingResult>('ping_server', { host, port }),
  externalCheck: (host: string, port: number) => invoke<any>('external_check', { host, port }),

  diagnostics: (id: string) => invoke<T.DiagCheck[]>('run_diagnostics', { id }),
  diskUsage: (id: string) => invoke<T.DiskUsage>('disk_usage', { id }),
};

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
