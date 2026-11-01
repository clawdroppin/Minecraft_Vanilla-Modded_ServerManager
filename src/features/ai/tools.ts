// What the AI assistant can do. Every tool goes through the same commands the UI uses, so the
// backend's rules (paths stay inside the server folder, EULA only from the checkbox, backups
// before version switches…) apply to the assistant too.
import { api } from '@/lib/ipc';
import { useApp, type ViewId } from '@/store/app';
import { VIEWS } from '@/features/server/views';
import { softwareInfo } from '@/lib/mc';
import { GUIDE } from './guide';
import type { ServerInstance } from '@/lib/types';

type Args = Record<string, any>;

export interface Tool {
  name: string;
  description: string;
  /** JSON Schema properties; `required` lists the mandatory ones. */
  params: Record<string, { type: string; description: string; enum?: string[]; items?: object }>;
  required?: string[];
  /** Changes something – needs the user's approval in "ask" mode. */
  change?: boolean;
  /** One line for the approval card / activity log. */
  label: (a: Args) => string;
  run: (a: Args) => Promise<unknown>;
}

const MAX_RESULT = 6_000;

/** Tool results go back to the model as text, trimmed so small context windows survive. */
export function resultText(value: unknown): string {
  const s = typeof value === 'string' ? value : JSON.stringify(value ?? 'OK');
  return s.length > MAX_RESULT ? `${s.slice(0, MAX_RESULT)}\n…(cut at ${MAX_RESULT} characters – ask for a smaller part)` : s;
}

/** Finds a server by id or (case-insensitive, partial) name; defaults to the open tab. */
export function resolveServer(ref: unknown): ServerInstance {
  const s = useApp.getState();
  const want = String(ref ?? '').trim().toLowerCase();
  if (!want) {
    const open = s.instances.find((i) => i.id === s.active);
    if (open) return open;
    if (s.instances.length === 1) return s.instances[0];
    throw `Say which server (one of: ${s.instances.map((i) => i.name).join(', ') || 'none yet'}).`;
  }
  const hit =
    s.instances.find((i) => i.id.toLowerCase() === want) ??
    s.instances.find((i) => i.name.toLowerCase() === want) ??
    s.instances.find((i) => i.name.toLowerCase().includes(want));
  if (!hit) throw `No server called "${ref}". Servers: ${s.instances.map((i) => i.name).join(', ') || 'none'}.`;
  return hit;
}

/** Strips secrets before a profile goes to the model. */
function redact(i: ServerInstance) {
  return { ...i, network: { ...i.network, rconPassword: i.network.rconPassword ? '(set)' : '' } };
}

function summary(i: ServerInstance) {
  const st = useApp.getState().statuses[i.id];
  return {
    id: i.id,
    name: i.name,
    software: i.installed?.software ?? i.software,
    minecraft: i.installed?.mcVersion ?? i.mcVersion,
    loader: i.installed?.loaderVersion ?? i.loaderVersion,
    installed: !!i.installed,
    state: st?.state ?? 'stopped',
    players: st?.players.map((p) => p.name) ?? [],
    tps: st?.tps ?? null,
    port: i.network.port,
    memoryMb: i.java.maxMemMb,
    eulaAccepted: i.eulaAccepted,
    folder: i.installDir,
    modpack: i.modpack?.title ?? null,
  };
}

/** Content type for searches/installs on this server's software. */
function contentType(i: ServerInstance, wanted?: string): string {
  if (wanted) return wanted;
  const c = softwareInfo(i.installed?.software ?? i.software).content;
  return c === 'mods' ? 'mod' : c === 'plugins' ? 'plugin' : 'datapack';
}

/** Merges `patch` into `base`: objects recursively, everything else replaced. */
function merge(base: any, patch: any): any {
  if (patch === null || typeof patch !== 'object' || Array.isArray(patch) || typeof base !== 'object' || base === null || Array.isArray(base)) return patch;
  const out = { ...base };
  for (const [k, v] of Object.entries(patch)) out[k] = merge(base[k], v);
  return out;
}

const LOCKED = ['id', 'installDir', 'installed', 'eulaAccepted', 'createdAt'];
const server = { type: 'string', description: 'Server name (optional: defaults to the open server)' };

export const TOOLS: Tool[] = [
  // ---- Looking around (always allowed) ----
  {
    name: 'list_servers',
    description: 'All servers: software, version, state, players, TPS, port, memory, folder.',
    params: {},
    label: () => 'List servers',
    run: async () => useApp.getState().instances.map(summary),
  },
  {
    name: 'app_guide',
    description: 'Detailed reference: profile (every settings field, schedule format), java (versions, memory), fixes (common problems), app (how the app works).',
    params: { topic: { type: 'string', description: 'Topic.', enum: Object.keys(GUIDE) } },
    required: ['topic'],
    label: (a) => `Look up the ${a.topic} guide`,
    run: async (a) => GUIDE[String(a.topic)] ?? `Topics: ${Object.keys(GUIDE).join(', ')}`,
  },
  {
    name: 'get_server',
    description: 'Full profile of a server (java, network, server.properties values, automation) + live status.',
    params: { server },
    label: (a) => `Read the profile of ${resolveServer(a.server).name}`,
    run: async (a) => {
      const i = resolveServer(a.server);
      return { profile: redact(i), status: useApp.getState().statuses[i.id] ?? null };
    },
  },
  {
    name: 'read_console',
    description: 'Newest console lines. First step for crashes/errors.',
    params: { server, lines: { type: 'integer', description: 'How many lines (default 150, max 600).' } },
    label: (a) => `Read the console of ${resolveServer(a.server).name}`,
    run: async (a) => (await api.logTail(resolveServer(a.server).id, Math.min(600, Number(a.lines) || 150))).join('\n'),
  },
  {
    name: 'list_logs',
    description: 'Older logs and crash reports (read one with read_log).',
    params: { server },
    label: (a) => `List logs of ${resolveServer(a.server).name}`,
    run: async (a) => (await api.logFiles(resolveServer(a.server).id)).slice(0, 40),
  },
  {
    name: 'read_log',
    description: 'Reads a log/crash report by the full path from list_logs.',
    params: { server, path: { type: 'string', description: 'Full path from list_logs.' } },
    required: ['path'],
    label: (a) => `Read ${String(a.path).split(/[\\/]/).pop()}`,
    run: async (a) => {
      const t = await api.readTextFile(resolveServer(a.server).id, String(a.path));
      return t.length > MAX_RESULT ? '…' + t.slice(-MAX_RESULT + 200) : t;
    },
  },
  {
    name: 'list_config_files',
    description: 'Editable config files of a server (relative paths).',
    params: { server },
    label: (a) => `List config files of ${resolveServer(a.server).name}`,
    run: async (a) => (await api.configFiles(resolveServer(a.server).id)).map((c) => `${c.rel} (${c.size} B)`).join('\n'),
  },
  {
    name: 'list_folder',
    description: 'Lists a folder of the server (relative path, "" = root).',
    params: { server, path: { type: 'string', description: 'Relative folder, e.g. "config" or "plugins/Geyser-Spigot".' } },
    label: (a) => `Look in ${a.path || 'the server folder'}`,
    run: async (a) => (await api.listDir(resolveServer(a.server).id, String(a.path ?? ''))).map((e) => (e.isDir ? `${e.name}/` : `${e.name} (${e.size} B)`)).join('\n'),
  },
  {
    name: 'read_file',
    description: 'Reads a text file in the server folder, e.g. "server.properties", "config/paper-global.yml".',
    params: { server, path: { type: 'string', description: 'Relative path.' } },
    required: ['path'],
    label: (a) => `Read ${a.path}`,
    run: async (a) => {
      const f = await api.readConfig(resolveServer(a.server).id, String(a.path));
      return f.exists ? f.text : `${a.path} doesn't exist yet (write_file creates it).`;
    },
  },
  {
    name: 'list_content',
    description: 'Installed mods/plugins with version, enabled state and warnings (client-only, wrong loader).',
    params: { server },
    label: (a) => `List mods/plugins of ${resolveServer(a.server).name}`,
    run: async (a) => api.listContent(resolveServer(a.server).id),
  },
  {
    name: 'search_content',
    description: 'Searches Modrinth/Hangar/CurseForge for content compatible with the server\'s version and loader.',
    params: {
      server,
      query: { type: 'string', description: 'Search words.' },
      source: { type: 'string', description: 'modrinth (default), hangar (Paper plugins) or curseforge (needs an API key).', enum: ['modrinth', 'hangar', 'curseforge'] },
      type: { type: 'string', description: 'mod, plugin, datapack or modpack. Default: what the server loads.', enum: ['mod', 'plugin', 'datapack', 'modpack'] },
    },
    required: ['query'],
    label: (a) => `Search ${a.source ?? 'modrinth'} for "${a.query}"`,
    run: async (a) => {
      const i = resolveServer(a.server);
      const type = contentType(i, a.type);
      const page = await api.searchContent({ id: type === 'modpack' ? null : i.id, source: a.source ?? 'modrinth', query: String(a.query), projectType: type, sort: 'relevance', offset: 0 });
      return page.hits.slice(0, 10).map((h) => ({ project: h.source === 'hangar' ? h.slug : h.id, title: h.title, description: h.description, downloads: h.downloads, serverSide: h.serverSide, clientSide: h.clientSide, source: h.source }));
    },
  },
  {
    name: 'check_content_updates',
    description: 'Available updates of installed mods/plugins.',
    params: { server },
    label: (a) => `Check mod updates of ${resolveServer(a.server).name}`,
    run: async (a) => api.contentUpdates(resolveServer(a.server).id),
  },
  {
    name: 'run_health_check',
    description: 'Health checks: EULA, Java, memory, ports, firewall, bad mods, disk, crash reports, fatal log lines.',
    params: { server },
    label: (a) => `Run the health check on ${resolveServer(a.server).name}`,
    run: async (a) => api.diagnostics(resolveServer(a.server).id),
  },
  {
    name: 'list_worlds',
    description: 'Worlds with seed, version, size; which is active.',
    params: { server },
    label: (a) => `List worlds of ${resolveServer(a.server).name}`,
    run: async (a) => api.listWorlds(resolveServer(a.server).id),
  },
  {
    name: 'list_backups',
    description: 'Backups (path, label, size, time).',
    params: { server },
    label: (a) => `List backups of ${resolveServer(a.server).name}`,
    run: async (a) => (await api.listBackups(resolveServer(a.server).id)).slice(0, 30),
  },
  {
    name: 'get_players',
    description: 'Whitelist, ops, bans, known players and play stats.',
    params: { server },
    label: (a) => `Read players of ${resolveServer(a.server).name}`,
    run: async (a) => {
      const id = resolveServer(a.server).id;
      const [lists, stats] = await Promise.all([api.playerLists(id), api.playerStats(id).catch(() => [])]);
      return { lists, stats: stats.slice(0, 25) };
    },
  },
  {
    name: 'network_status',
    description: 'IPs, CGNAT/double NAT, UPnP and firewall state.',
    params: { server },
    label: () => 'Check the network',
    run: async (a) => {
      const id = resolveServer(a.server).id;
      const [net, fw, upnp] = await Promise.all([api.netInfo(), api.firewallState(id).catch((e) => String(e)), api.upnpProbe().catch((e) => String(e))]);
      return { net, firewall: fw, upnp };
    },
  },
  {
    name: 'test_from_internet',
    description: 'Checks from the internet whether the server is reachable.',
    params: { server },
    label: (a) => `Test ${resolveServer(a.server).name} from the internet`,
    run: async (a) => {
      const i = resolveServer(a.server);
      const net = await api.netInfo();
      const host = i.network.publicAddress || net.publicIp;
      if (!host) throw 'The public address is unknown.';
      return api.externalCheck(host, i.network.port);
    },
  },
  {
    name: 'list_versions',
    description: 'Minecraft versions a software offers (newest first).',
    params: { software: { type: 'string', description: 'vanilla, paper, purpur, folia, fabric, quilt, forge or neoforge.' } },
    required: ['software'],
    label: (a) => `List ${a.software} versions`,
    run: async (a) => {
      const v = await api.gameVersions(String(a.software));
      return { latestRelease: v.latestRelease, latestSnapshot: v.latestSnapshot, versions: v.versions.filter((x) => x.kind === 'release').slice(0, 40).map((x) => x.id) };
    },
  },
  {
    name: 'check_server_update',
    description: 'Is a newer build/loader available?',
    params: { server },
    label: (a) => `Check for updates of ${resolveServer(a.server).name}`,
    run: async (a) => (await api.checkUpdate(resolveServer(a.server).id)) ?? 'Up to date.',
  },
  {
    name: 'open_view',
    description: 'Shows the user a page of the app.',
    params: { server, view: { type: 'string', description: 'Page id, or "dashboard".', enum: ['dashboard', ...VIEWS.map((v) => v.id)] } },
    required: ['view'],
    label: (a) => `Open ${a.view}`,
    run: async (a) => {
      const s = useApp.getState();
      if (a.view === 'dashboard') return s.setActive('home');
      s.openServer(resolveServer(a.server).id, a.view as ViewId);
      return 'Opened.';
    },
  },

  // ---- Changes (need approval in "ask" mode) ----
  {
    name: 'write_file',
    description: 'Writes a whole text file in the server folder (old version kept). Send the complete content. Not eula.txt.',
    params: { server, path: { type: 'string', description: 'Relative path.' }, content: { type: 'string', description: 'The complete new file content.' } },
    required: ['path', 'content'],
    change: true,
    label: (a) => `Write ${a.path} (${String(a.content ?? '').split('\n').length} lines) on ${resolveServer(a.server).name}`,
    run: async (a) => {
      const r = await api.writeConfig(resolveServer(a.server).id, String(a.path), String(a.content));
      useApp.getState().upsertInstance(r.instance);
      return r.restartNeeded ? 'Saved. Restart the server for it to take effect.' : 'Saved.';
    },
  },
  {
    name: 'edit_file',
    description: 'Replaces exact text (appearing once) in a file - best for small edits.',
    params: { server, path: { type: 'string', description: 'Relative path.' }, find: { type: 'string', description: 'Exact text to replace.' }, replace: { type: 'string', description: 'New text.' } },
    required: ['path', 'find', 'replace'],
    change: true,
    label: (a) => `Edit ${a.path} on ${resolveServer(a.server).name}: "${String(a.find).slice(0, 60)}" → "${String(a.replace).slice(0, 60)}"`,
    run: async (a) => {
      const id = resolveServer(a.server).id;
      const f = await api.readConfig(id, String(a.path));
      const n = f.text.split(String(a.find)).length - 1;
      if (n !== 1) throw n === 0 ? 'That text is not in the file – read_file it again and copy the exact text.' : `That text appears ${n} times – include more surrounding lines.`;
      const r = await api.writeConfig(id, String(a.path), f.text.replace(String(a.find), () => String(a.replace)));
      useApp.getState().upsertInstance(r.instance);
      return r.restartNeeded ? 'Edited. Restart the server for it to take effect.' : 'Edited.';
    },
  },
  {
    name: 'update_server',
    description:
      'Changes a server profile by merging `changes` (same shape as the get_server profile), e.g. {"properties":{"view-distance":"8"}} or {"java":{"maxMemMb":6144}}. Arrays are replaced whole. See app_guide profile.',
    // A JSON string, not an object: some local models (Gemma on Ollama) can't emit free-form object arguments.
    params: { server, changes: { type: 'string', description: 'JSON object with the fields to change, e.g. {"properties":{"view-distance":"8"}}' } },
    required: ['changes'],
    change: true,
    label: (a) => `Change ${resolveServer(a.server).name}: ${(typeof a.changes === 'string' ? a.changes : JSON.stringify(a.changes)).slice(0, 160)}`,
    run: async (a) => {
      const i = resolveServer(a.server);
      let changes: any;
      try {
        changes = typeof a.changes === 'string' ? JSON.parse(a.changes) : a.changes;
      } catch {
        throw '`changes` must be a JSON object, e.g. {"properties":{"view-distance":"8"}}.';
      }
      if (!changes || typeof changes !== 'object' || Array.isArray(changes)) throw '`changes` must be a JSON object.';
      const bad = Object.keys(changes ?? {}).filter((k) => LOCKED.includes(k));
      if (bad.length) throw `${bad.join(', ')} can't be changed by the assistant.`;
      if (changes.properties) changes.properties = Object.fromEntries(Object.entries(changes.properties).map(([k, v]) => [k, String(v)]));
      const next = await api.updateInstance(merge(i, changes));
      useApp.getState().upsertInstance(next);
      return 'Saved. Settings marked "restart needed" apply after the next restart.';
    },
  },
  {
    name: 'send_command',
    description: 'Runs a console command on the running server (no slash), e.g. "gamerule keepInventory true".',
    params: { server, command: { type: 'string', description: 'The command.' } },
    required: ['command'],
    change: true,
    label: (a) => `Run "${a.command}" on ${resolveServer(a.server).name}`,
    run: async (a) => (await api.sendCommand(resolveServer(a.server).id, String(a.command).replace(/^\//, ''))) || 'Sent (check read_console for the output).',
  },
  {
    name: 'server_power',
    description: 'Start/stop/restart; *_countdown warns players first; force_stop kills (last resort).',
    params: {
      server,
      action: { type: 'string', description: 'What to do.', enum: ['start', 'stop', 'restart', 'restart_countdown', 'stop_countdown', 'force_stop'] },
      seconds: { type: 'integer', description: 'Countdown length (default 300).' },
    },
    required: ['action'],
    change: true,
    label: (a) => `${String(a.action).replace('_', ' ')} ${resolveServer(a.server).name}`,
    run: async (a) => {
      const id = resolveServer(a.server).id;
      const secs = Number(a.seconds) || 300;
      switch (a.action) {
        case 'start':
          await api.start(id);
          break;
        case 'stop':
          await api.stop(id);
          break;
        case 'force_stop':
          await api.stop(id, true);
          break;
        case 'restart':
          await api.restart(id);
          break;
        case 'restart_countdown':
          await api.startCountdown(id, 'restart', secs);
          break;
        case 'stop_countdown':
          await api.startCountdown(id, 'stop', secs);
          break;
        default:
          throw `Unknown action ${a.action}.`;
      }
      return 'Done. The state updates within a few seconds (list_servers).';
    },
  },
  {
    name: 'install_content',
    description: 'Installs a project from search_content (with dependencies).',
    params: {
      server,
      project: { type: 'string', description: 'The `project` value from search_content.' },
      title: { type: 'string', description: 'Its title.' },
      source: { type: 'string', description: 'modrinth (default), hangar or curseforge.', enum: ['modrinth', 'hangar', 'curseforge'] },
      type: { type: 'string', description: 'mod, plugin or datapack (default: what the server loads).', enum: ['mod', 'plugin', 'datapack'] },
    },
    required: ['project'],
    change: true,
    label: (a) => `Install ${a.title || a.project} on ${resolveServer(a.server).name}`,
    run: async (a) => {
      const i = resolveServer(a.server);
      return api.installContent(i.id, a.source ?? 'modrinth', String(a.project), null, contentType(i, a.type), String(a.title || a.project));
    },
  },
  {
    name: 'set_content_enabled',
    description: 'Enables/disables an installed mod/plugin by file name.',
    params: { server, file: { type: 'string', description: 'File name from list_content.' }, enabled: { type: 'boolean', description: 'true = enable.' } },
    required: ['file', 'enabled'],
    change: true,
    label: (a) => `${a.enabled ? 'Enable' : 'Disable'} ${a.file}`,
    run: async (a) => api.setContentEnabled(resolveServer(a.server).id, String(a.file), !!a.enabled),
  },
  {
    name: 'remove_content',
    description: 'Removes a mod/plugin by file name (kept in .mcsm/removed).',
    params: { server, file: { type: 'string', description: 'File name from list_content.' } },
    required: ['file'],
    change: true,
    label: (a) => `Remove ${a.file}`,
    run: async (a) => api.removeContent(resolveServer(a.server).id, String(a.file)),
  },
  {
    name: 'update_all_content',
    description: 'Updates every mod/plugin with an update.',
    params: { server },
    change: true,
    label: (a) => `Update all mods/plugins on ${resolveServer(a.server).name}`,
    run: async (a) => {
      const id = resolveServer(a.server).id;
      const items = await api.contentUpdates(id);
      return items.length ? `${await api.updateContent(id, items)} updated.` : 'Nothing to update.';
    },
  },
  {
    name: 'create_backup',
    description: 'Backs up worlds and configs now.',
    params: { server, label: { type: 'string', description: 'Optional label.' } },
    change: true,
    label: (a) => `Back up ${resolveServer(a.server).name}`,
    run: async (a) => {
      await api.createBackup(resolveServer(a.server).id, String(a.label ?? 'assistant'));
      return 'Backup started.';
    },
  },
  {
    name: 'restore_backup',
    description: 'Restores a backup (path from list_backups); server must be stopped.',
    params: { server, path: { type: 'string', description: 'Backup path.' }, include_config: { type: 'boolean', description: 'Also restore configs (default true).' } },
    required: ['path'],
    change: true,
    label: (a) => `Restore ${String(a.path).split(/[\\/]/).pop()} on ${resolveServer(a.server).name}`,
    run: async (a) => `${await api.restoreBackup(resolveServer(a.server).id, String(a.path), a.include_config !== false)} files restored.`,
  },
  {
    name: 'player_action',
    description: 'Adds/removes a player on whitelist, ops, bans or IP bans.',
    params: {
      server,
      list: { type: 'string', description: 'Which list.', enum: ['whitelist', 'ops', 'banned-players', 'banned-ips'] },
      add: { type: 'boolean', description: 'true = add, false = remove.' },
      player: { type: 'string', description: 'Player name (or IP for banned-ips).' },
      reason: { type: 'string', description: 'Ban reason.' },
    },
    required: ['list', 'add', 'player'],
    change: true,
    label: (a) => `${a.add ? 'Add' : 'Remove'} ${a.player} ${a.add ? 'to' : 'from'} ${a.list}`,
    run: async (a) => api.playerAction(resolveServer(a.server).id, String(a.list), !!a.add, String(a.player), String(a.reason ?? '')),
  },
  {
    name: 'change_version',
    description: 'Switches software/Minecraft version (backup first).',
    params: {
      server,
      software: { type: 'string', description: 'vanilla, paper, purpur, folia, fabric, quilt, forge, neoforge.' },
      minecraft: { type: 'string', description: 'Minecraft version (list_versions).' },
      loader: { type: 'string', description: 'Build/loader version, empty = latest.' },
    },
    required: ['software', 'minecraft'],
    change: true,
    label: (a) => `Switch ${resolveServer(a.server).name} to ${a.software} ${a.minecraft}`,
    run: async (a) => {
      await api.changeVersion(resolveServer(a.server).id, String(a.software), String(a.minecraft), String(a.loader ?? ''));
      return 'Installing in the background (backup first).';
    },
  },
  {
    name: 'setup_bedrock',
    description: 'Installs Geyser + Floodgate so Bedrock players can join (UDP 19132).',
    params: { server },
    change: true,
    label: (a) => `Set up Bedrock crossplay on ${resolveServer(a.server).name}`,
    run: async (a) => api.setupBedrock(resolveServer(a.server).id),
  },
  {
    name: 'forward_ports',
    description: 'Forwards the server\'s ports on the router (UPnP).',
    params: { server },
    change: true,
    label: (a) => `Forward ports of ${resolveServer(a.server).name} with UPnP`,
    run: async (a) => api.upnpMap(resolveServer(a.server).id),
  },
  {
    name: 'allow_firewall',
    description: 'Adds Windows Firewall rules (the user gets an admin prompt).',
    params: { server },
    change: true,
    label: (a) => `Allow ${resolveServer(a.server).name} through the Windows Firewall`,
    run: async (a) => {
      await api.addFirewallRules(resolveServer(a.server).id);
      return 'Firewall rules added.';
    },
  },
  {
    name: 'create_server',
    description: 'Creates and installs a new server (the user accepts the EULA later).',
    params: {
      name: { type: 'string', description: 'Server name.' },
      software: { type: 'string', description: 'vanilla, paper, purpur, folia, fabric, quilt, forge, neoforge.' },
      minecraft: { type: 'string', description: 'Minecraft version (list_versions).' },
      memory_mb: { type: 'integer', description: 'Max memory (default 4096).' },
    },
    required: ['name', 'software', 'minecraft'],
    change: true,
    label: (a) => `Create server "${a.name}" (${a.software} ${a.minecraft})`,
    run: async (a) => {
      const inst = await api.createInstance({ name: String(a.name), software: String(a.software), mcVersion: String(a.minecraft), loaderVersion: '', maxMemMb: Number(a.memory_mb) || 4096, eulaAccepted: false, installNow: true });
      useApp.getState().upsertInstance(inst);
      return { created: summary(inst), note: 'Installing. The user must accept the EULA under Server Settings before starting.' };
    },
  },
];

export const TOOL_MAP = new Map(TOOLS.map((t) => [t.name, t]));

/** OpenAI-format tool definitions. */
export function toolSpecs() {
  return TOOLS.map((t) => ({
    type: 'function',
    function: { name: t.name, description: t.description, parameters: { type: 'object', properties: t.params, required: t.required ?? [] } },
  }));
}

/** Parses a tool call's arguments; small models sometimes wrap them in code fences. */
export function parseArgs(raw: string): Args {
  const t = (raw || '{}').trim().replace(/^```(?:json)?\s*|\s*```$/g, '');
  try {
    const v = JSON.parse(t || '{}');
    return typeof v === 'string' ? JSON.parse(v) : (v ?? {});
  } catch {
    throw `The arguments weren't valid JSON: ${t.slice(0, 200)}`;
  }
}
