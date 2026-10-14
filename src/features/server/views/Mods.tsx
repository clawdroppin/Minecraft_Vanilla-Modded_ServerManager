import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ArrowUpCircle,
  BadgeCheck,
  Boxes,
  Check,
  Download,
  ExternalLink,
  FileUp,
  FolderOpen,
  Heart,
  Loader2,
  MonitorSmartphone,
  MoreHorizontal,
  Package,
  PackageOpen,
  Puzzle,
  RefreshCw,
  ScanSearch,
  Search,
  Server,
  Smartphone,
  Sparkles,
  Trash2,
  TriangleAlert,
} from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { ago, bytes } from '@/lib/format';
import { softwareInfo } from '@/lib/mc';
import { browse, pickFiles, pickSave, reveal } from '@/lib/shell';
import type { ContentItem, ContentState, ProjectVersion, SearchHit, SearchPage, UpdateCandidate } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Badge, Callout, Card, Empty, PageHeader } from '@/components/ui/Surface';
import { Checkbox, Input, Segmented, Select, Toggle } from '@/components/ui/Field';
import { Menu } from '@/components/ui/Menu';
import { Dialog, confirm } from '@/components/ui/Dialog';
import { SoftwareIcon } from '@/components/ui/Mc';
import { BEDROCK_SOFTWARE, ESSENTIALS, ESSENTIAL_CATEGORIES, type Essential } from '@/data/essentials';

type Tab = 'installed' | 'essentials' | 'browse' | 'updates' | 'modpacks';

function compact(n: number) {
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : String(n);
}

function ProjectIcon({ src, size = 40 }: { src?: string | null; size?: number }) {
  const [bad, setBad] = useState(false);
  if (!src || bad) {
    return (
      <span className="flex shrink-0 items-center justify-center rounded-lg bg-panel-2 text-fg-4" style={{ width: size, height: size }}>
        <Package style={{ width: size * 0.5, height: size * 0.5 }} />
      </span>
    );
  }
  return <img src={src} alt="" loading="lazy" onError={() => setBad(true)} className="shrink-0 rounded-lg bg-panel-2 object-cover" style={{ width: size, height: size }} />;
}

const SOURCE_LABEL: Record<string, string> = { modrinth: 'Modrinth', curseforge: 'CurseForge', hangar: 'Hangar', geysermc: 'GeyserMC' };

function sideBadges(client: string, server: string) {
  const out = [];
  if (server === 'unsupported') out.push(<Badge key="s" tone="err"><MonitorSmartphone /> client only</Badge>);
  else if (client === 'unsupported') out.push(<Badge key="c" tone="violet"><Server /> server only</Badge>);
  else if (client === 'required' && server === 'required') out.push(<Badge key="b" tone="info">client + server</Badge>);
  return out;
}

// ---------------------------------------------------------------------------- Installed

function Installed({ id, state, reload, onBrowse }: { id: string; state: ContentState; reload: () => void; onBrowse: () => void }) {
  const toast = useApp((s) => s.toast);
  const inst = useInstance(id)!;
  const [q, setQ] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const noun = state.kind === 'plugins' ? 'plugin' : 'mod';
  const items = useMemo(() => {
    const l = q.toLowerCase();
    return state.items.filter((i) => !l || [i.fileName, i.meta.name, i.source?.title, i.meta.modId].some((x) => x?.toLowerCase().includes(l)));
  }, [state.items, q]);
  const enabled = state.items.filter((i) => i.enabled).length;
  const clientOnly = state.items.filter((i) => i.enabled && (i.meta.environment === 'client' || i.source?.serverSide === 'unsupported'));

  const toggle = async (it: ContentItem, v: boolean) => {
    try {
      await api.setContentEnabled(id, it.fileName, v);
      reload();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  return (
    <>
      {clientOnly.length > 0 && (
        <Callout tone="err" className="mb-4" title={`${clientOnly.length} client-only ${noun}${clientOnly.length === 1 ? '' : 's'}`}>
          {clientOnly.map((c) => c.source?.title ?? c.meta.name ?? c.fileName).join(', ')} only run{clientOnly.length === 1 ? 's' : ''} in the game client and can crash a dedicated server. Disable them here – players
          still install them in their own launcher (see “Client pack”).
        </Callout>
      )}
      <Card>
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="relative w-64">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Filter ${state.items.length} ${noun}s…`} className="pl-8" />
          </div>
          <span className="text-[11.5px] text-fg-4">
            {enabled} enabled · {state.items.length - enabled} disabled
          </span>
          <div className="ml-auto flex gap-1.5">
            <Button
              size="sm"
              variant="outline"
              icon={<ScanSearch className="size-3.5" />}
              loading={busy === 'identify'}
              onClick={async () => {
                setBusy('identify');
                try {
                  const n = await api.identifyContent(id);
                  toast('success', n ? `Recognised ${n} file${n === 1 ? '' : 's'} on Modrinth` : 'Nothing new to recognise', id);
                  reload();
                } catch (e) {
                  toast('error', errMsg(e), id);
                } finally {
                  setBusy(null);
                }
              }}
            >
              Identify
            </Button>
            {state.kind === 'mods' && (
              <Button
                size="sm"
                variant="outline"
                icon={<MonitorSmartphone className="size-3.5" />}
                loading={busy === 'pack'}
                onClick={async () => {
                  const dest = await pickSave('Save client pack', `${inst.name.replace(/[^\w-]+/g, '-')}-client.mrpack`, ['mrpack'], 'Modrinth pack');
                  if (!dest) return;
                  const r = await confirm({
                    title: 'Include mods that aren’t on Modrinth?',
                    message: 'Mods Modrinth doesn’t know can be bundled into the pack file itself. Only share such packs if the mod licences allow redistribution.',
                    confirm: 'Export',
                    checkbox: 'Bundle unknown mod files',
                  });
                  if (!r.ok) return;
                  setBusy('pack');
                  try {
                    const [n, unknown] = await api.exportClientPack(id, dest, r.checked);
                    toast('success', `Client pack saved – ${n} mods${unknown ? `, ${unknown} not on Modrinth${r.checked ? ' (bundled)' : ' (skipped)'}` : ''}. Players import it in Modrinth App / Prism Launcher.`, id);
                    reveal(dest.replace(/[\\/][^\\/]+$/, ''));
                  } catch (e) {
                    toast('error', errMsg(e), id);
                  } finally {
                    setBusy(null);
                  }
                }}
              >
                Client pack
              </Button>
            )}
            <Button
              size="sm"
              icon={<FileUp className="size-3.5" />}
              onClick={async () => {
                const paths = await pickFiles(`Add ${noun} jars`, ['jar'], 'Java archives');
                if (!paths.length) return;
                try {
                  await api.addContentFiles(id, paths);
                  toast('success', `Added ${paths.length} file${paths.length === 1 ? '' : 's'}`, id);
                  reload();
                } catch (e) {
                  toast('error', errMsg(e), id);
                }
              }}
            >
              Add files
            </Button>
            {state.dir && (
              <IconButton label="Open folder" onClick={() => reveal(state.dir!)}>
                <FolderOpen />
              </IconButton>
            )}
          </div>
        </div>
        {state.items.length === 0 ? (
          <Empty
            icon={<Puzzle />}
            title={`No ${noun}s yet`}
            description={`Find ${noun}s for ${softwareInfo(inst.installed?.software ?? inst.software).label} ${inst.installed?.mcVersion ?? ''} on Modrinth${state.kind === 'plugins' ? ', Hangar' : ''} or CurseForge – dependencies are installed automatically.`}
            action={
              <Button variant="primary" icon={<Search className="size-3.5" />} onClick={onBrowse}>
                Browse {noun}s
              </Button>
            }
          />
        ) : (
          <div className="divide-y divide-line">
            {items.map((it) => {
              const title = it.source?.title || it.meta.name || it.fileName;
              const co = it.meta.environment === 'client' || it.source?.serverSide === 'unsupported';
              const wrongLoader =
                it.meta.loader &&
                ((state.kind === 'plugins' && !['bukkit', 'paper'].includes(it.meta.loader)) ||
                  (state.kind === 'mods' && ['bukkit', 'paper'].includes(it.meta.loader)) ||
                  (inst.installed?.software === 'fabric' && ['forge', 'neoforge'].includes(it.meta.loader)) ||
                  (['forge', 'neoforge'].includes(inst.installed?.software ?? '') && ['fabric', 'quilt'].includes(it.meta.loader)));
              return (
                <div key={it.fileName} className={cn('flex items-center gap-3 px-4 py-2.5', !it.enabled && 'opacity-55')}>
                  <ProjectIcon src={it.source?.iconUrl || it.meta.icon} size={36} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-[13px] font-medium text-fg">{title}</span>
                      <span className="truncate font-mono text-[11px] text-fg-4">{it.source?.versionNumber || it.meta.version}</span>
                      {co && it.enabled && (
                        <Badge tone="err">
                          <TriangleAlert /> client only
                        </Badge>
                      )}
                      {wrongLoader && it.enabled && (
                        <Badge tone="err">
                          <TriangleAlert /> {it.meta.loader} {noun}
                        </Badge>
                      )}
                      {it.source?.dependency && <Badge>dependency</Badge>}
                    </div>
                    <div className="mt-0.5 truncate text-[11.5px] text-fg-3">{it.meta.description || it.fileName}</div>
                  </div>
                  <div className="flex shrink-0 items-center gap-3">
                    <span className="hidden text-right text-[11px] text-fg-4 xl:block">
                      {it.source ? (
                        <span className="flex items-center gap-1">
                          <BadgeCheck className="size-3 text-accent" /> {SOURCE_LABEL[it.source.source] ?? it.source.source}
                        </span>
                      ) : (
                        'manual'
                      )}
                      <span className="block font-mono">{bytes(it.size)}</span>
                    </span>
                    <Toggle checked={it.enabled} onChange={(v) => toggle(it, v)} />
                    <Menu
                      width={210}
                      trigger={() => (
                        <IconButton label="More">
                          <MoreHorizontal />
                        </IconButton>
                      )}
                      items={[
                        ...(it.source?.slug && it.source.source === 'modrinth' ? [{ label: 'Open on Modrinth', icon: <ExternalLink />, onSelect: () => browse(`https://modrinth.com/project/${it.source!.slug}`) }] : []),
                        ...(it.source?.source === 'curseforge' ? [{ label: 'Open on CurseForge', icon: <ExternalLink />, onSelect: () => browse(`https://www.curseforge.com/minecraft/mc-mods/${it.source!.slug}`) }] : []),
                        ...(it.source?.source === 'hangar' ? [{ label: 'Open on Hangar', icon: <ExternalLink />, onSelect: () => browse(`https://hangar.papermc.io/search?query=${encodeURIComponent(it.source!.slug)}`) }] : []),
                        {
                          label: 'Remove',
                          icon: <Trash2 />,
                          danger: true,
                          onSelect: async () => {
                            const r = await confirm({ title: `Remove ${title}?`, message: 'The file moves to .mcsm\\removed – you can bring it back from there.', confirm: 'Remove', danger: true });
                            if (!r.ok) return;
                            try {
                              const now = await api.removeContent(id, it.fileName);
                              if (!now) toast('info', `${title} is in use – it’s removed automatically before the next start.`, id);
                              reload();
                            } catch (e) {
                              toast('error', errMsg(e), id);
                            }
                          },
                        },
                      ]}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>
    </>
  );
}

// ---------------------------------------------------------------------------- Browse

function VersionsDialog({ id, hit, projectType, onClose, onInstall }: { id: string | null; hit: SearchHit | null; projectType: string; onClose: () => void; onInstall: (v: ProjectVersion) => void }) {
  const [list, setList] = useState<ProjectVersion[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    setList(null);
    setErr(null);
    if (hit) api.projectVersions(id, hit.source, hit.id, projectType).then(setList).catch((e) => setErr(errMsg(e)));
  }, [hit, id, projectType]);
  return (
    <Dialog open={!!hit} onClose={onClose} title={hit?.title ?? ''} description="Versions compatible with this server, newest first." icon={<Package />} width={640}>
      {err && <Callout tone="err">{err}</Callout>}
      {!list && !err && (
        <div className="flex items-center gap-2 py-6 text-[12.5px] text-fg-3">
          <Loader2 className="size-4 animate-spin" /> Loading versions…
        </div>
      )}
      {list && list.length === 0 && <Empty title="No compatible versions" description="Nothing published for this Minecraft version / loader." />}
      <div className="space-y-1">
        {list?.map((v) => (
          <div key={v.id} className="flex items-center gap-3 rounded-lg px-2.5 py-2 hover:bg-panel-2">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-[12.5px] font-medium text-fg">{v.versionNumber || v.name}</span>
                {v.versionType !== 'release' && <Badge tone="warn">{v.versionType}</Badge>}
              </div>
              <div className="truncate text-[11px] text-fg-4">
                {v.gameVersions.slice(-4).join(', ')} · {v.loaders.join(', ')} · {v.published ? ago(Date.parse(v.published)) : ''} {v.size ? `· ${bytes(v.size)}` : ''}
              </div>
            </div>
            <Button size="sm" onClick={() => onInstall(v)}>
              Install
            </Button>
          </div>
        ))}
      </div>
    </Dialog>
  );
}

function Browse({ id, kind, installedIds, reload, modpacks }: { id: string; kind: ContentState['kind']; installedIds: Set<string>; reload: () => void; modpacks?: boolean }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const settings = useApp((s) => s.settings);
  const types = modpacks ? ['modpack'] : kind === 'mods' ? ['mod', 'datapack'] : kind === 'plugins' ? ['plugin', 'datapack'] : ['datapack'];
  const [projectType, setProjectType] = useState(types[0]);
  const [source, setSource] = useState('modrinth');
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState('relevance');
  const [page, setPage] = useState<SearchPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [versionsFor, setVersionsFor] = useState<SearchHit | null>(null);
  const [justInstalled, setJustInstalled] = useState<Set<string>>(new Set());

  const sources = modpacks ? ['modrinth'] : projectType === 'plugin' ? ['modrinth', 'hangar', 'curseforge'] : ['modrinth', 'curseforge'];
  const src = sources.includes(source) ? source : 'modrinth';

  const search = useCallback(
    async (offset = 0) => {
      setLoading(true);
      setErr(null);
      try {
        const r = await api.searchContent({ id: modpacks ? null : id, source: src, query, projectType, sort, offset, mc: modpacks ? (inst.installed?.mcVersion ?? null) : null });
        setPage((p) => (offset > 0 && p ? { ...r, hits: [...p.hits, ...r.hits] } : r));
      } catch (e) {
        setErr(errMsg(e));
        if (offset === 0) setPage(null);
      } finally {
        setLoading(false);
      }
    },
    [id, src, query, projectType, sort, modpacks, inst.installed?.mcVersion],
  );
  useEffect(() => {
    const t = setTimeout(() => search(0), query ? 350 : 0);
    return () => clearTimeout(t);
  }, [search, query]);

  const install = async (hit: SearchHit, version: ProjectVersion | null) => {
    setVersionsFor(null);
    if (modpacks) {
      if (!version) return setVersionsFor(hit);
      const r = await confirm({
        title: `Install ${hit.title} on ${inst.name}?`,
        message: 'The server switches to the pack’s Minecraft version and loader, then the pack’s server files are installed. A backup is made first; your worlds stay.',
        confirm: 'Install modpack',
      });
      if (!r.ok) return;
      const ref = { source: 'modrinth', projectId: hit.id, versionId: version.id, title: hit.title, versionName: version.versionNumber, iconUrl: hit.iconUrl };
      try {
        await api.installModpack(id, ref);
      } catch (e) {
        const msg = errMsg(e);
        if (!msg.includes('older than')) return toast('error', msg, id);
        const d = await confirm({ title: 'Downgrade the world?', message: msg, confirm: 'Downgrade anyway', danger: true });
        if (d.ok) await api.installModpack(id, ref, true).catch((e2) => toast('error', errMsg(e2), id));
      }
      return;
    }
    setInstalling(hit.id);
    try {
      const out = await api.installContent(id, hit.source, hit.source === 'hangar' ? hit.slug : hit.id, version?.id ?? null, projectType, hit.title);
      const skipped = out.filter((o) => o.skipped);
      if (skipped.length) toast('warn', skipped.map((s) => `${s.title}: ${s.skipped}`).join(' · '), id);
      setJustInstalled((s) => new Set([...s, hit.id]));
      reload();
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setInstalling(null);
    }
  };

  return (
    <div>
      <Card className="mb-4 flex flex-wrap items-center gap-2 px-4 py-3">
        <div className="relative min-w-[260px] flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={`Search ${modpacks ? 'modpacks' : projectType + 's'}…`} className="h-9 pl-8 text-[13px]" autoFocus />
        </div>
        {types.length > 1 && (
          <Segmented
            value={projectType}
            onChange={(v) => {
              setProjectType(v);
              setPage(null);
            }}
            options={types.map((t) => ({ value: t, label: t === 'mod' ? 'Mods' : t === 'plugin' ? 'Plugins' : t === 'datapack' ? 'Datapacks' : 'Modpacks' }))}
          />
        )}
        {sources.length > 1 && (
          <Segmented value={src} onChange={setSource} options={sources.map((s) => ({ value: s, label: SOURCE_LABEL[s] }))} />
        )}
        <Select value={sort} onChange={(e) => setSort(e.target.value)} className="w-36">
          <option value="relevance">Relevance</option>
          <option value="downloads">Downloads</option>
          <option value="follows">Popularity</option>
          <option value="updated">Recently updated</option>
          <option value="newest">Newest</option>
        </Select>
      </Card>
      <div className="mb-3 flex items-center gap-2 text-[11.5px] text-fg-4">
        <SoftwareIcon software={inst.installed?.software ?? inst.software} size={16} className="rounded" />
        {modpacks
          ? `Modrinth modpacks${inst.installed?.mcVersion ? ` for Minecraft ${inst.installed.mcVersion}` : ''} – installing one switches this server to the pack’s loader and version.`
          : `Showing ${projectType}s for ${softwareInfo(inst.installed?.software ?? inst.software).label} ${inst.installed?.mcVersion ?? inst.mcVersion}${projectType === 'mod' ? ' that work on servers' : ''}.`}
        {page && <span className="ml-auto font-mono">{page.total.toLocaleString()} results</span>}
      </div>
      {src === 'curseforge' && !settings?.curseforgeKey && (
        <Callout tone="info" className="mb-4" title="CurseForge needs an API key" action={<Button size="sm" onClick={() => useApp.getState().setDialog({ kind: 'settings' })}>App settings</Button>}>
          CurseForge only allows apps with a key. Get a free one at console.curseforge.com and paste it under App settings → Content sources.
        </Callout>
      )}
      {err && !(src === 'curseforge' && !settings?.curseforgeKey) && <Callout tone="err" className="mb-4">{err}</Callout>}
      {st?.state === 'running' && !modpacks && <div className="mb-3 text-[11.5px] text-warn">New {kind} load after a restart.</div>}
      <div className="grid grid-cols-2 gap-3">
        {page?.hits.map((h) => {
          const have = installedIds.has(h.id) || justInstalled.has(h.id);
          return (
            <Card key={`${h.source}-${h.id}`} className="flex gap-3 p-3.5">
              <ProjectIcon src={h.iconUrl} size={52} />
              <div className="min-w-0 flex-1">
                <div className="flex items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <button onClick={() => h.url && browse(h.url)} className="block truncate text-left text-[13.5px] font-semibold text-fg hover:text-accent">
                      {h.title}
                    </button>
                    <div className="truncate text-[11px] text-fg-4">by {h.author}</div>
                  </div>
                  <div className="flex shrink-0 gap-1">
                    {have ? (
                      <Badge tone="ok" className="h-7 px-2">
                        <Check /> Installed
                      </Badge>
                    ) : (
                      <>
                        <Button size="sm" variant="primary" icon={<Download className="size-3.5" />} loading={installing === h.id} onClick={() => install(h, null)}>
                          {modpacks ? 'Pick version' : 'Install'}
                        </Button>
                        {!modpacks && (
                          <IconButton label="Choose a version" className="border border-line-strong" onClick={() => setVersionsFor(h)}>
                            <MoreHorizontal />
                          </IconButton>
                        )}
                      </>
                    )}
                  </div>
                </div>
                <div className="mt-1.5 line-clamp-2 text-[12px] leading-snug text-fg-3">{h.description}</div>
                <div className="mt-2 flex flex-wrap items-center gap-1.5 text-[11px] text-fg-4">
                  <span className="flex items-center gap-1">
                    <Download className="size-3" /> {compact(h.downloads)}
                  </span>
                  <span className="flex items-center gap-1">
                    <Heart className="size-3" /> {compact(h.follows)}
                  </span>
                  {h.updated && <span>· {ago(Date.parse(h.updated))}</span>}
                  {sideBadges(h.clientSide, h.serverSide)}
                  {h.categories.slice(0, 3).map((c) => (
                    <Badge key={c} className="capitalize">
                      {c}
                    </Badge>
                  ))}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      {loading && (
        <div className="flex justify-center py-6">
          <Loader2 className="size-5 animate-spin text-accent" />
        </div>
      )}
      {page && !loading && page.hits.length < page.total && (
        <div className="flex justify-center py-5">
          <Button onClick={() => search(page.hits.length)}>Load more</Button>
        </div>
      )}
      {page && !loading && page.hits.length === 0 && <Empty icon={<Search />} title="Nothing found" description="Try other words, or another source." />}
      <VersionsDialog id={modpacks ? null : id} hit={versionsFor} projectType={projectType} onClose={() => setVersionsFor(null)} onInstall={(v) => versionsFor && install(versionsFor, v)} />
    </div>
  );
}

// ---------------------------------------------------------------------------- Essentials

function Essentials({ id, installedIds, reload }: { id: string; installedIds: Set<string>; reload: () => void }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());
  const software = inst.installed?.software ?? inst.software;
  const kind = ['paper', 'purpur', 'folia'].includes(software) ? 'plugin' : 'mod';
  const fits = ESSENTIALS.filter((e) => e.software.includes(software));
  const bedrockOk = BEDROCK_SOFTWARE.includes(software);
  const hasBedrock = installedIds.has('geyser') || installedIds.has('wKkoqHrH') || done.has('bedrock');

  /** Add a port the add-on needs to the forwarded / firewall-allowed list. */
  const addPort = async (e: Essential) => {
    if (!e.port || inst.network.extraPorts.some((p) => p.port === e.port!.port)) return;
    const d = structuredClone(inst);
    d.network.extraPorts.push(e.port);
    useApp.getState().upsertInstance(await api.updateInstance(d));
  };

  const install = async (e: Essential) => {
    setBusy(e.id);
    try {
      const out = await api.installContent(id, 'modrinth', e.slug, null, kind, e.title);
      const skipped = out.filter((o) => o.skipped);
      if (skipped.length) toast('warn', skipped.map((s) => `${s.title}: ${s.skipped}`).join(' · '), id);
      await addPort(e);
      setDone((s) => new Set([...s, e.id]));
      if (e.after) toast('info', `${e.title}: ${e.after}`, id);
      reload();
    } catch (err) {
      toast('error', errMsg(err), id);
    } finally {
      setBusy(null);
    }
  };

  if (!inst.installed) return null;
  return (
    <div className="space-y-5">
      <Card className="flex items-start gap-4 p-4">
        <div className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-info/15 text-info">
          <Smartphone className="size-6" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[14px] font-semibold text-fg">
            Bedrock crossplay
            {hasBedrock && (
              <Badge tone="ok">
                <Check /> Installed
              </Badge>
            )}
          </div>
          <div className="mt-1 text-[12.5px] leading-relaxed text-fg-3">
            Let friends on phones, tablets, Xbox, PlayStation, Switch and Windows (Bedrock Edition) join this Java server. Installs <b className="text-fg-2">Geyser</b> (translates Bedrock) and{' '}
            <b className="text-fg-2">Floodgate</b> (no Java account needed) and adds <span className="font-mono">UDP 19132</span> to the ports to forward. Bedrock players add the server with your address and
            port 19132.
          </div>
          {!bedrockOk && <div className="mt-2 text-[12px] text-warn">Needs Paper, Purpur, Fabric, Quilt or NeoForge – switch under Version &amp; Software.</div>}
        </div>
        <Button
          variant={hasBedrock ? 'outline' : 'primary'}
          icon={<Download className="size-3.5" />}
          disabled={!bedrockOk}
          loading={busy === 'bedrock'}
          onClick={async () => {
            setBusy('bedrock');
            try {
              const out = await api.setupBedrock(id);
              const skipped = out.filter((o) => o.skipped && o.skipped !== 'already up to date');
              if (skipped.length) toast('warn', skipped.map((s) => `${s.title}: ${s.skipped}`).join(' · '), id);
              setDone((s) => new Set([...s, 'bedrock']));
              toast('success', st?.state === 'running' ? 'Bedrock crossplay installed – restart the server to turn it on.' : 'Bedrock crossplay installed – it starts with the server.', id);
              reload();
            } catch (e) {
              toast('error', errMsg(e), id);
            } finally {
              setBusy(null);
            }
          }}
        >
          {hasBedrock ? 'Update' : 'Set up'}
        </Button>
      </Card>

      {fits.length === 0 ? (
        <Empty icon={<Sparkles />} title="No picks for this software" description="Vanilla servers run datapacks only – browse them on the Browse tab." />
      ) : (
        ESSENTIAL_CATEGORIES.map((c) => {
          const list = fits.filter((e) => e.category === c.id);
          if (!list.length) return null;
          return (
            <div key={c.id}>
              <div className="mb-2 text-[11px] font-semibold tracking-wider text-fg-4 uppercase">{c.label}</div>
              <div className="grid grid-cols-2 gap-3">
                {list.map((e) => {
                  const have = installedIds.has(e.id) || done.has(e.id);
                  return (
                    <Card key={e.id} className="flex gap-3 p-3.5">
                      <ProjectIcon src={e.icon} size={44} />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-start gap-2">
                          <button onClick={() => browse(`https://modrinth.com/project/${e.slug}`)} className="min-w-0 flex-1 truncate text-left text-[13.5px] font-semibold text-fg hover:text-accent">
                            {e.title}
                          </button>
                          {have ? (
                            <Badge tone="ok" className="h-7 px-2">
                              <Check /> Installed
                            </Badge>
                          ) : (
                            <Button size="sm" variant="primary" icon={<Download className="size-3.5" />} loading={busy === e.id} disabled={busy != null && busy !== e.id} onClick={() => install(e)}>
                              Install
                            </Button>
                          )}
                        </div>
                        <div className="mt-1 text-[12px] leading-snug text-fg-3">{e.blurb}</div>
                        {e.port && (
                          <div className="mt-1.5 font-mono text-[11px] text-fg-4">
                            opens {e.port.protocol.toUpperCase()} {e.port.port}
                          </div>
                        )}
                      </div>
                    </Card>
                  );
                })}
              </div>
            </div>
          );
        })
      )}
      {st?.state === 'running' && <div className="text-[11.5px] text-warn">New {kind}s load after a restart.</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------- Updates

function Updates({ id, reload }: { id: string; reload: () => void }) {
  const toast = useApp((s) => s.toast);
  const [list, setList] = useState<UpdateCandidate[] | null>(null);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const check = useCallback(() => {
    setList(null);
    api
      .contentUpdates(id)
      .then((l) => {
        setList(l);
        setSel(new Set(l.map((x) => x.fileName)));
      })
      .catch((e) => {
        toast('error', errMsg(e), id);
        setList([]);
      });
  }, [id, toast]);
  useEffect(() => {
    check();
  }, [check]);
  return (
    <Card>
      <div className="flex items-center gap-2 border-b border-line px-4 py-3">
        <div className="text-[13px] font-medium text-fg">{list == null ? 'Checking Modrinth…' : list.length ? `${list.length} update${list.length === 1 ? '' : 's'} available` : 'Everything is up to date'}</div>
        <div className="ml-auto flex gap-1.5">
          <IconButton label="Check again" onClick={check}>
            <RefreshCw />
          </IconButton>
          <Button
            variant="primary"
            icon={<ArrowUpCircle className="size-3.5" />}
            disabled={!list?.length || sel.size === 0}
            loading={busy}
            onClick={async () => {
              setBusy(true);
              try {
                const n = await api.updateContent(
                  id,
                  list!.filter((x) => sel.has(x.fileName)),
                );
                toast('success', `Updated ${n}`, id);
                reload();
                check();
              } catch (e) {
                toast('error', errMsg(e), id);
              } finally {
                setBusy(false);
              }
            }}
          >
            Update {sel.size || ''}
          </Button>
        </div>
      </div>
      {list == null ? (
        <div className="flex justify-center py-10">
          <Loader2 className="size-5 animate-spin text-accent" />
        </div>
      ) : list.length === 0 ? (
        <Empty icon={<BadgeCheck />} title="No updates" description="Only files Modrinth recognises can be checked – use Identify on the Installed tab for jars you added by hand." />
      ) : (
        <div className="divide-y divide-line">
          {list.map((u) => (
            <div key={u.fileName} className="flex items-center gap-3 px-4 py-2.5">
              <Checkbox
                checked={sel.has(u.fileName)}
                onChange={(v) => {
                  const n = new Set(sel);
                  if (v) n.add(u.fileName);
                  else n.delete(u.fileName);
                  setSel(n);
                }}
              />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12.5px] font-medium text-fg">{u.title}</div>
                <div className="font-mono text-[11px] text-fg-4">
                  {u.current} → <span className="text-ok">{u.latest}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------------------- View

export function Mods({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const [tab, setTab] = useState<Tab>('installed');
  const [state, setState] = useState<ContentState | null>(null);
  const toast = useApp((s) => s.toast);
  const reload = useCallback(() => api.listContent(id).then(setState).catch((e) => toast('error', errMsg(e), id)), [id, toast]);
  useEffect(() => {
    reload();
  }, [reload, inst.installed?.software]);
  const kind = state?.kind ?? 'none';
  const installedIds = useMemo(() => new Set((state?.items ?? []).flatMap((i) => [i.source?.projectId ?? '', i.source?.slug ?? '']).filter(Boolean)), [state]);
  const sw = softwareInfo(inst.installed?.software ?? inst.software);
  const title = kind === 'plugins' ? 'Plugins' : kind === 'mods' ? 'Mods' : 'Datapacks';

  return (
    <div className="mx-auto max-w-[1180px] p-6">
      <PageHeader
        title={title}
        description={
          kind === 'none'
            ? 'Vanilla servers run datapacks only. Switch to Fabric, Quilt, Forge or NeoForge for mods – or Paper / Purpur for plugins – under Version & Software.'
            : `${sw.label} loads ${kind} from the ${kind}/ folder. Installs pull in required dependencies; client-only mods are skipped automatically.`
        }
        actions={
          <Segmented
            value={tab}
            onChange={setTab}
            options={[
              ...(kind !== 'none' ? [{ value: 'installed' as Tab, label: <>Installed <span className="font-mono text-fg-4">{state?.items.length ?? 0}</span></>, icon: <Boxes className="size-3.5" /> }] : []),
              ...(kind !== 'none' ? [{ value: 'essentials' as Tab, label: 'Essentials', icon: <Sparkles className="size-3.5" /> }] : []),
              { value: 'browse' as Tab, label: 'Browse', icon: <Search className="size-3.5" /> },
              ...(kind !== 'none' ? [{ value: 'updates' as Tab, label: 'Updates', icon: <ArrowUpCircle className="size-3.5" /> }] : []),
              { value: 'modpacks' as Tab, label: 'Modpacks', icon: <PackageOpen className="size-3.5" /> },
            ]}
          />
        }
      />
      {inst.modpack && (
        <Card className="mb-4 flex items-center gap-3 px-4 py-3">
          <ProjectIcon src={inst.modpack.iconUrl} size={36} />
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-medium text-fg">{inst.modpack.title}</div>
            <div className="text-[11.5px] text-fg-3">Modpack {inst.modpack.versionName} · players need the same pack (Modrinth App, Prism, CurseForge app…)</div>
          </div>
          <Button size="sm" variant="outline" icon={<ExternalLink className="size-3.5" />} onClick={() => browse(`https://modrinth.com/modpack/${inst.modpack!.projectId}`)}>
            Pack page
          </Button>
        </Card>
      )}
      {!inst.installed && (
        <Callout tone="info" className="mb-4">
          Install the server first – mods and plugins are matched to its exact Minecraft version and loader.
        </Callout>
      )}
      {tab === 'installed' && kind !== 'none' && state && <Installed id={id} state={state} reload={reload} onBrowse={() => setTab('browse')} />}
      {(tab === 'browse' || (tab === 'installed' && kind === 'none')) && <Browse id={id} kind={kind} installedIds={installedIds} reload={reload} />}
      {tab === 'essentials' && kind !== 'none' && <Essentials id={id} installedIds={installedIds} reload={reload} />}
      {tab === 'updates' && <Updates id={id} reload={reload} />}
      {tab === 'modpacks' && <Browse id={id} kind={kind} installedIds={installedIds} reload={reload} modpacks />}
    </div>
  );
}
