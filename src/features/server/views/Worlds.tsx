import { useCallback, useEffect, useMemo, useState } from 'react';
import { Copy, Download, FolderOpen, Globe2, Map as MapIcon, MoreHorizontal, Package, Pause, Pencil, Play, Plus, RefreshCw, RotateCcw, Skull, Sparkles, Trash2, Upload, X } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { useLogLines } from '@/lib/live';
import { ago, bytes, dateTime } from '@/lib/format';
import { DIFFICULTY_LABEL, GAME_TYPE_LABEL } from '@/lib/mc';
import { copy, pickFile, pickFiles, pickFolder, pickSave, reveal } from '@/lib/shell';
import type { Datapack, WorldInfo } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Badge, Card, Empty, PageHeader } from '@/components/ui/Surface';
import { Dialog, confirm } from '@/components/ui/Dialog';
import { Checkbox, FieldRow, Input, Segmented } from '@/components/ui/Field';
import { Menu } from '@/components/ui/Menu';

function NewWorldDialog({ id, open, onClose, onDone }: { id: string; open: boolean; onClose: () => void; onDone: () => void }) {
  const inst = useInstance(id)!;
  const toast = useApp((s) => s.toast);
  const [name, setName] = useState('world2');
  const [seed, setSeed] = useState('');
  const [type, setType] = useState('minecraft:normal');
  const go = async () => {
    try {
      const d = structuredClone(inst);
      d.properties['level-name'] = name.trim();
      d.properties['level-seed'] = seed.trim();
      d.properties['level-type'] = type;
      const out = await api.updateInstance(d);
      useApp.getState().upsertInstance(out);
      toast('success', `“${name}” will be generated on the next start`, id);
      onDone();
      onClose();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="New world"
      description="The server generates it on its next start. Your current world stays where it is."
      icon={<Sparkles />}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" disabled={!name.trim()} onClick={go}>
            Use this world
          </Button>
        </>
      }
    >
      <FieldRow label="Folder name">
        <Input value={name} onChange={(e) => setName(e.target.value)} />
      </FieldRow>
      <FieldRow label="Seed" description="Empty = random.">
        <Input mono value={seed} onChange={(e) => setSeed(e.target.value)} placeholder="random" />
      </FieldRow>
      <FieldRow label="World type">
        <select value={type} onChange={(e) => setType(e.target.value)} className="h-8 w-full rounded-lg border border-line-strong bg-bg-raised px-2 text-[12.5px] text-fg">
          {['minecraft:normal', 'minecraft:flat', 'minecraft:large_biomes', 'minecraft:amplified', 'minecraft:single_biome_surface'].map((t) => (
            <option key={t} value={t}>
              {t.replace('minecraft:', '').replace(/_/g, ' ')}
            </option>
          ))}
        </select>
      </FieldRow>
    </Dialog>
  );
}

function ResetDialog({ id, world, onClose, onDone }: { id: string; world: WorldInfo | null; onClose: () => void; onDone: () => void }) {
  const toast = useApp((s) => s.toast);
  const [what, setWhat] = useState<'all' | 'nether' | 'end'>('all');
  const [newSeed, setNewSeed] = useState(false);
  const [seed, setSeed] = useState('');
  const go = async () => {
    if (!world) return;
    try {
      const n = await api.resetWorld(id, world.name, what, what === 'all' && newSeed ? seed : null);
      toast('success', n ? `Reset ${what === 'all' ? world.name : `the ${what === 'nether' ? 'Nether' : 'End'}`} – regenerates on the next start (old files kept in .mcsm/quarantine)` : 'Nothing to reset', id);
      onDone();
      onClose();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };
  return (
    <Dialog
      open={!!world}
      onClose={onClose}
      title={`Reset ${world?.name}`}
      description="A backup is taken first and the old files are moved to quarantine – nothing is deleted."
      icon={<RotateCcw />}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" onClick={go}>
            Reset
          </Button>
        </>
      }
    >
      <div className="space-y-2">
        {(
          [
            ['all', 'Whole world', 'Overworld, Nether and End are generated fresh.'],
            ['nether', 'Nether only', 'Great for a fresh Nether after an update.'],
            ['end', 'End only', 'Respawns the dragon and the outer islands.'],
          ] as const
        ).map(([v, l, d]) => (
          <button key={v} onClick={() => setWhat(v)} className={`flex w-full items-start gap-3 rounded-xl border px-3.5 py-2.5 text-left ${what === v ? 'border-accent/60 bg-accent/8' : 'border-line hover:border-line-strong'}`}>
            <span className={`mt-1 size-3 rounded-full border-2 ${what === v ? 'border-accent bg-accent' : 'border-fg-4'}`} />
            <span>
              <span className="block text-[12.5px] font-medium text-fg">{l}</span>
              <span className="text-[11.5px] text-fg-3">{d}</span>
            </span>
          </button>
        ))}
      </div>
      {what === 'all' && (
        <div className="mt-4 space-y-2">
          <Checkbox checked={newSeed} onChange={setNewSeed} label="Use a new seed" />
          {newSeed && <Input mono value={seed} onChange={(e) => setSeed(e.target.value)} placeholder="Empty = random" />}
        </div>
      )}
    </Dialog>
  );
}

function Datapacks({ id, world }: { id: string; world: string }) {
  const toast = useApp((s) => s.toast);
  const [packs, setPacks] = useState<Datapack[]>([]);
  const load = useCallback(() => api.listDatapacks(id, world).then(setPacks).catch(() => {}), [id, world]);
  useEffect(() => {
    load();
  }, [load]);
  return (
    <div className="border-t border-line bg-bg-raised/40 px-5 py-3">
      <div className="mb-2 flex items-center gap-2">
        <Package className="size-4 text-fg-3" />
        <span className="text-[12.5px] font-medium text-fg">Datapacks</span>
        <span className="text-[11.5px] text-fg-4">· applied on world load (/reload while running)</span>
        <div className="ml-auto flex gap-1.5">
          <Button size="xs" variant="outline" onClick={() => useApp.getState().setView(id, 'mods')}>
            Browse Modrinth
          </Button>
          <Button
            size="xs"
            icon={<Plus className="size-3" />}
            onClick={async () => {
              const paths = await pickFiles('Add datapacks', ['zip'], 'Datapacks');
              if (!paths.length) return;
              try {
                await api.addDatapacks(id, world, paths);
                toast('success', `Added ${paths.length} datapack${paths.length === 1 ? '' : 's'}`, id);
                load();
              } catch (e) {
                toast('error', errMsg(e), id);
              }
            }}
          >
            Add .zip
          </Button>
        </div>
      </div>
      {packs.length === 0 ? (
        <div className="py-2 text-[12px] text-fg-4">No datapacks in this world.</div>
      ) : (
        <div className="grid grid-cols-2 gap-1.5">
          {packs.map((p) => (
            <div key={p.name} className="flex items-center gap-2 rounded-lg border border-line bg-panel px-2.5 py-1.5">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12px] text-fg">{p.name}</div>
                <div className="truncate text-[11px] text-fg-4">{p.description ?? (p.isZip ? 'zip' : 'folder')} · {bytes(p.size)}</div>
              </div>
              <IconButton
                label="Remove"
                onClick={() =>
                  api
                    .removeDatapack(id, world, p.name)
                    .then(load)
                    .catch((e) => toast('error', errMsg(e), id))
                }
              >
                <Trash2 />
              </IconButton>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------- Pre-generation

const RADII = [1000, 2500, 5000, 10000];
// "Task running for minecraft:overworld. Processed: 1024 chunks (5.12%), ETA: 0:04:12, Rate: 83.2 cps, Current: 12, -5"
// "Task finished for minecraft:overworld. Processed: 20000 chunks (100.00%), Total time: 0:04:12"
const CHUNKY_RE = /Task (running|finished|paused|cancelled|stopped) for ([^.\s]+)\.(?: Processed: ([\d,]+) chunks \(([\d.]+)%\))?(?:, ETA: ([\d:]+))?(?:, Rate: ([\d.]+) cps)?(?:, Total time: ([\d:]+))?/i;

interface PregenState {
  status: string;
  world: string;
  chunks?: string;
  pct?: number;
  eta?: string;
  rate?: string;
  total?: string;
}

function lastPregen(lines: { text: string }[]): PregenState | null {
  for (let i = lines.length - 1; i >= Math.max(0, lines.length - 400); i--) {
    const m = CHUNKY_RE.exec(lines[i].text);
    if (m) return { status: m[1].toLowerCase(), world: m[2], chunks: m[3], pct: m[4] ? Number(m[4]) : undefined, eta: m[5], rate: m[6], total: m[7] };
  }
  return null;
}

/** World pre-generation with Chunky: generate the area around spawn ahead of time so exploring never lags. */
function Pregen({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const lines = useLogLines(id);
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [dim, setDim] = useState<'overworld' | 'the_nether' | 'the_end'>('overworld');
  const [radius, setRadius] = useState(2500);
  const [busy, setBusy] = useState(false);
  const software = inst.installed?.software ?? inst.software;
  const plugins = ['paper', 'purpur', 'folia'].includes(software);
  const running = st?.state === 'running';
  const progress = useMemo(() => lastPregen(lines), [lines]);

  useEffect(() => {
    if (software === 'vanilla') return setInstalled(false);
    api
      .listContent(id)
      .then((c) => setInstalled(c.items.some((i) => i.enabled && (i.meta.modId?.toLowerCase() === 'chunky' || i.source?.slug === 'chunky' || /^chunky[-_]/i.test(i.fileName)))))
      .catch(() => setInstalled(false));
  }, [id, software, st?.state]);

  if (software === 'vanilla') return null;

  const level = inst.properties['level-name'] || 'world';
  // Chunky names worlds the way the platform does: Bukkit world names on Paper, dimension ids on mod loaders.
  const worldArg = plugins ? { overworld: level, the_nether: `${level}_nether`, the_end: `${level}_the_end` }[dim] : `minecraft:${dim}`;
  const send = async (cmds: string[]) => {
    setBusy(true);
    try {
      for (const c of cmds) await api.sendCommand(id, c);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(false);
    }
  };
  const active = progress && (progress.status === 'running' || progress.status === 'paused');
  const chunks = Math.pow(Math.ceil((radius * 2) / 16), 2);

  return (
    <Card className="mb-4 px-5 py-4">
      <div className="flex items-start gap-4">
        <div className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-accent/15 text-accent">
          <MapIcon className="size-6" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold text-fg">Pre-generate the world</div>
          <div className="mt-0.5 text-[12.5px] text-fg-3">
            Generates terrain around spawn ahead of time with Chunky, so players exploring new land don’t cause lag spikes. Best run before people join – it uses a lot of CPU while it works.
          </div>
        </div>
      </div>
      {installed === false ? (
        <div className="mt-3 flex items-center gap-3 rounded-lg border border-line bg-panel-2 px-3 py-2.5 text-[12.5px] text-fg-3">
          Chunky isn’t installed on this server.
          <Button
            size="sm"
            variant="primary"
            className="ml-auto"
            icon={<Download className="size-3.5" />}
            loading={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await api.installContent(id, 'modrinth', 'chunky', null, plugins ? 'plugin' : 'mod', 'Chunky');
                setInstalled(true);
                toast('success', running ? 'Chunky installed – restart the server to load it.' : 'Chunky installed – start the server, then pre-generate.', id);
              } catch (e) {
                toast('error', errMsg(e), id);
              } finally {
                setBusy(false);
              }
            }}
          >
            Install Chunky
          </Button>
        </div>
      ) : installed ? (
        <>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Segmented
              value={dim}
              onChange={setDim}
              options={[
                { value: 'overworld', label: 'Overworld' },
                { value: 'the_nether', label: 'Nether' },
                { value: 'the_end', label: 'End' },
              ]}
            />
            <Segmented value={String(radius)} onChange={(v) => setRadius(Number(v))} options={RADII.map((r) => ({ value: String(r), label: `${r.toLocaleString()} blocks` }))} />
            <span className="text-[11.5px] text-fg-4">≈ {chunks.toLocaleString()} chunks around spawn</span>
            <div className="ml-auto flex gap-1.5">
              {active ? (
                <>
                  {progress!.status === 'paused' ? (
                    <Button size="sm" icon={<Play className="size-3.5" />} disabled={!running || busy} onClick={() => send(['chunky continue'])}>
                      Continue
                    </Button>
                  ) : (
                    <Button size="sm" icon={<Pause className="size-3.5" />} disabled={!running || busy} onClick={() => send(['chunky pause'])}>
                      Pause
                    </Button>
                  )}
                  <Button size="sm" variant="outline" icon={<X className="size-3.5" />} disabled={!running || busy} onClick={() => send(['chunky cancel', 'chunky confirm'])}>
                    Cancel
                  </Button>
                </>
              ) : (
                <Button size="sm" variant="primary" icon={<Play className="size-3.5" />} disabled={!running} loading={busy} onClick={() => send([`chunky world ${worldArg}`, 'chunky spawn', 'chunky shape square', `chunky radius ${radius}`, 'chunky start'])}>
                  Start
                </Button>
              )}
            </div>
          </div>
          {!running && <div className="mt-2 text-[11.5px] text-fg-4">Start the server to pre-generate.</div>}
          {progress && (
            <div className="mt-3">
              <div className="mb-1 flex items-center gap-2 text-[11.5px] text-fg-3">
                <span className="font-mono text-fg-2">{progress.world}</span>
                <span className="capitalize">{progress.status}</span>
                {progress.chunks && <span>· {progress.chunks} chunks</span>}
                {progress.rate && <span>· {progress.rate} chunks/s</span>}
                <span className="ml-auto font-mono">{progress.status === 'finished' ? `done in ${progress.total ?? '?'}` : progress.eta ? `ETA ${progress.eta}` : ''}</span>
              </div>
              <div className="h-2 overflow-hidden rounded-full bg-panel-2">
                <div className={cn('h-full rounded-full transition-[width] duration-500', progress.status === 'finished' ? 'bg-ok' : 'bg-accent')} style={{ width: `${Math.min(100, progress.pct ?? (progress.status === 'finished' ? 100 : 0))}%` }} />
              </div>
            </div>
          )}
        </>
      ) : null}
    </Card>
  );
}

export function Worlds({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [worlds, setWorlds] = useState<WorldInfo[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [newOpen, setNewOpen] = useState(false);
  const [reset, setReset] = useState<WorldInfo | null>(null);
  const running = st?.state === 'running' || st?.state === 'starting';
  const load = useCallback(() => api.listWorlds(id).then(setWorlds).catch((e) => toast('error', errMsg(e), id)), [id, toast]);
  useEffect(() => {
    load();
  }, [load, inst.properties['level-name'], st?.state]);

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      toast('success', ok, id);
      load();
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const importWorld = async (kind: 'zip' | 'folder') => {
    const p = kind === 'zip' ? await pickFile('Import a world (.zip)', ['zip'], 'World archive') : await pickFolder('Pick a world folder (e.g. from .minecraft\\saves)');
    if (!p) return;
    await run(async () => {
      const name = await api.importWorld(id, p);
      const r = await confirm({ title: `Imported “${name}”`, message: 'Load this world on the next start?', confirm: 'Use it' });
      if (r.ok) useApp.getState().upsertInstance(await api.activateWorld(id, name));
    }, 'World imported');
  };

  return (
    <div className="mx-auto max-w-[1100px] p-6">
      <PageHeader
        title="Worlds"
        description="Every world folder in the server. The active one is what players join; switching takes effect on the next start."
        actions={
          <>
            <IconButton label="Refresh" onClick={load}>
              <RefreshCw />
            </IconButton>
            <Menu
              width={240}
              trigger={() => <Button icon={<Upload className="size-3.5" />}>Import world</Button>}
              items={[
                { label: 'From a .zip archive…', icon: <Package />, onSelect: () => importWorld('zip') },
                { label: 'From a folder (singleplayer save)…', icon: <FolderOpen />, onSelect: () => importWorld('folder') },
              ]}
            />
            <Button variant="primary" icon={<Plus className="size-3.5" />} onClick={() => setNewOpen(true)}>
              New world
            </Button>
          </>
        }
      />
      <Pregen id={id} />
      {!worlds ? null : worlds.length === 0 ? (
        <Card>
          <Empty icon={<Globe2 />} title="No worlds yet" description="The server creates one the first time it starts." />
        </Card>
      ) : (
        <div className="space-y-3">
          {worlds.map((w) => (
            <Card key={w.name} className={w.active ? 'border-accent/40 shadow-[0_0_0_1px_rgb(34_197_94/0.15)]' : ''}>
              <div className="flex items-center gap-4 px-5 py-4">
                <div className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-emerald-700 to-emerald-950 text-emerald-200">
                  {w.hardcore ? <Skull className="size-6 text-red-300" /> : <Globe2 className="size-6" />}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[14px] font-semibold text-fg">{w.name}</span>
                    {w.active && <Badge tone="ok">active</Badge>}
                    {w.hardcore && <Badge tone="err">hardcore</Badge>}
                    {w.version && <Badge>{w.version}</Badge>}
                    {!w.size && w.active && <Badge tone="info">generates on start</Badge>}
                  </div>
                  <div className="mt-1 flex flex-wrap gap-x-4 gap-y-0.5 text-[11.5px] text-fg-3">
                    {w.seed && (
                      <button className="font-mono hover:text-fg" title="Copy seed" onClick={() => copy(w.seed!).then(() => toast('success', 'Seed copied'))}>
                        seed {w.seed} <Copy className="inline size-3" />
                      </button>
                    )}
                    {w.gameType != null && <span>{GAME_TYPE_LABEL[w.gameType] ?? w.gameType}</span>}
                    {w.difficulty != null && <span>{DIFFICULTY_LABEL[w.difficulty] ?? w.difficulty}</span>}
                    {w.size > 0 && <span>{bytes(w.size)}</span>}
                    {w.lastPlayed ? <span title={dateTime(w.lastPlayed)}>played {ago(w.lastPlayed)}</span> : null}
                    {w.dayTime != null && <span>day {Math.floor(w.dayTime / 24000)}</span>}
                    <span>{[w.hasNether && 'Nether', w.hasEnd && 'End'].filter(Boolean).join(' · ') || 'Overworld only'}</span>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {!w.active && (
                    <Button
                      size="sm"
                      icon={<Play className="size-3.5" />}
                      onClick={() => run(async () => useApp.getState().upsertInstance(await api.activateWorld(id, w.name)), running ? `${w.name} loads after a restart` : `${w.name} is now active`)}
                    >
                      Use this world
                    </Button>
                  )}
                  <Button size="sm" variant="outline" icon={<Package className="size-3.5" />} onClick={() => setOpen(open === w.name ? null : w.name)}>
                    Datapacks {w.datapacks > 0 && <span className="font-mono text-fg-4">{w.datapacks}</span>}
                  </Button>
                  <Menu
                    width={230}
                    trigger={() => (
                      <IconButton label="World actions" className="border border-line-strong">
                        <MoreHorizontal />
                      </IconButton>
                    )}
                    items={[
                      {
                        label: 'Export as .zip…',
                        icon: <Download />,
                        onSelect: async () => {
                          const dest = await pickSave('Export world', `${w.name}.zip`, ['zip'], 'Zip');
                          if (dest) run(() => api.exportWorld(id, w.name, dest), `Exported ${w.name}`);
                        },
                      },
                      { label: 'Open folder', icon: <FolderOpen />, onSelect: () => reveal(w.path) },
                      {
                        label: 'Rename…',
                        icon: <Pencil />,
                        onSelect: () => {
                          const n = window.prompt('New folder name', w.name);
                          if (n && n !== w.name) run(() => api.renameWorld(id, w.name, n), 'Renamed');
                        },
                      },
                      { separator: true, label: '' },
                      { label: 'Reset / regenerate…', icon: <RotateCcw />, disabled: w.active && running, onSelect: () => setReset(w) },
                      {
                        label: 'Delete…',
                        icon: <Trash2 />,
                        danger: true,
                        disabled: w.active && running,
                        onSelect: async () => {
                          const r = await confirm({ title: `Delete ${w.name}?`, message: 'The folder is moved to .mcsm\\quarantine inside the server folder – delete it there to free the space.', confirm: 'Delete', danger: true });
                          if (r.ok) run(() => api.deleteWorld(id, w.name), `${w.name} moved to quarantine`);
                        },
                      },
                    ]}
                  />
                </div>
              </div>
              {open === w.name && <Datapacks id={id} world={w.name} />}
            </Card>
          ))}
        </div>
      )}
      <NewWorldDialog id={id} open={newOpen} onClose={() => setNewOpen(false)} onDone={load} />
      <ResetDialog id={id} world={reset} onClose={() => setReset(null)} onDone={load} />
    </div>
  );
}
