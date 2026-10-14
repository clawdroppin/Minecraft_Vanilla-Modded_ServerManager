import { useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { ArrowLeft, ArrowRight, Check, Download, ExternalLink, FolderOpen, Layers, Loader2, PackageOpen, Rocket, Search, Server, Sparkles } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes } from '@/lib/format';
import { SERVER_COLORS } from '@/lib/colors';
import { browse, pickFolder } from '@/lib/shell';
import { DIFFICULTIES, GAMEMODES, softwareInfo } from '@/lib/mc';
import type { FolderProbe, ProjectVersion, SearchHit } from '@/lib/types';
import { useApp } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { Checkbox, Input, Label, NumberInput, Select, Slider, Toggle } from '@/components/ui/Field';
import { Badge, Callout } from '@/components/ui/Surface';
import { SoftwareIcon } from '@/components/ui/Mc';
import { SoftwareGrid, VersionPicker, type Pick } from '@/features/server/VersionPicker';

const STEPS = [
  { title: 'Type', icon: Layers },
  { title: 'Version', icon: Sparkles },
  { title: 'Server', icon: Server },
  { title: 'Launch', icon: Rocket },
];

type Mode = 'software' | 'modpack';

function ModpackSearch({ value, onPick }: { value: SearchHit | null; onPick: (h: SearchHit) => void }) {
  const [q, setQ] = useState('');
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    const t = setTimeout(
      () =>
        api
          .searchContent({ source: 'modrinth', query: q, projectType: 'modpack', sort: q ? 'relevance' : 'downloads', offset: 0 })
          .then((p) => {
            setHits(p.hits);
            setErr(null);
          })
          .catch((e) => setErr(errMsg(e))),
      q ? 350 : 0,
    );
    return () => clearTimeout(t);
  }, [q]);
  return (
    <div>
      <div className="relative mb-2">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
        <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search Modrinth modpacks (Cobblemon, Fabulously Optimized, All the Mods…)" className="pl-8" autoFocus />
      </div>
      {err && <Callout tone="err">{err}</Callout>}
      <div className="grid h-[300px] grid-cols-2 content-start gap-2 overflow-y-auto pr-1">
        {!hits && !err && (
          <div className="col-span-2 flex items-center justify-center gap-2 py-10 text-[12px] text-fg-3">
            <Loader2 className="size-4 animate-spin" /> Loading modpacks…
          </div>
        )}
        {hits?.map((h) => (
          <button
            key={h.id}
            onClick={() => onPick(h)}
            className={cn('flex gap-2.5 rounded-xl border p-2.5 text-left transition-colors', value?.id === h.id ? 'border-accent/70 bg-accent/8' : 'border-line hover:border-line-strong')}
          >
            {h.iconUrl ? <img src={h.iconUrl} alt="" className="size-10 shrink-0 rounded-lg" /> : <PackageOpen className="size-10 shrink-0 text-fg-4" />}
            <div className="min-w-0">
              <div className="truncate text-[12.5px] font-semibold text-fg">{h.title}</div>
              <div className="line-clamp-2 text-[11px] leading-snug text-fg-3">{h.description}</div>
              <div className="mt-0.5 text-[10.5px] text-fg-4">
                {(h.downloads / 1000).toFixed(0)}k downloads · {h.categories.slice(0, 2).join(', ')}
              </div>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

export function NewServerDialog() {
  const dialog = useApp((s) => s.dialog);
  const setDialog = useApp((s) => s.setDialog);
  const instances = useApp((s) => s.instances);
  const info = useApp((s) => s.info);
  const open = dialog?.kind === 'new';
  const [step, setStep] = useState(0);
  const [mode, setMode] = useState<Mode>('software');
  const [pick, setPick] = useState<Pick>({ software: 'paper', mc: '', loader: '' });
  const [pack, setPack] = useState<SearchHit | null>(null);
  const [packVersions, setPackVersions] = useState<ProjectVersion[] | null>(null);
  const [packVersion, setPackVersion] = useState<ProjectVersion | null>(null);
  const [name, setName] = useState('');
  const [color, setColor] = useState('grass');
  const [dir, setDir] = useState('');
  const [port, setPort] = useState(25565);
  const [mem, setMem] = useState(4096);
  const [props, setProps] = useState<Record<string, string>>({});
  const [eula, setEula] = useState(false);
  const [installNow, setInstallNow] = useState(true);
  const [template, setTemplate] = useState('');
  const [probe, setProbe] = useState<FolderProbe | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const totalMb = Math.floor((info?.totalMemory ?? 16 * 1073741824) / 1048576);
  const maxMem = Math.max(2048, Math.floor((totalMb * 0.85) / 512) * 512);

  useEffect(() => {
    if (!open) return;
    const n = instances.length + 1;
    setStep(0);
    setMode('software');
    setPick({ software: 'paper', mc: '', loader: '' });
    setPack(null);
    setPackVersion(null);
    setName(n === 1 ? 'My Minecraft Server' : `Minecraft Server ${n}`);
    setColor(Object.keys(SERVER_COLORS)[instances.length % Object.keys(SERVER_COLORS).length]);
    setDir('');
    setTemplate(dialog?.kind === 'new' ? (dialog.templateId ?? '') : '');
    setProps({ gamemode: 'survival', difficulty: 'normal', 'max-players': '20', 'white-list': 'false', 'level-seed': '', pvp: 'true', 'online-mode': 'true' });
    setEula(false);
    setInstallNow(true);
    setError(null);
    setMem(Math.min(4096, maxMem));
    api.suggestPorts().then(([p]) => setPort(p)).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    if (!dir) return setProbe(null);
    const t = setTimeout(() => api.probeFolder(dir).then(setProbe).catch(() => setProbe(null)), 250);
    return () => clearTimeout(t);
  }, [dir]);

  useEffect(() => {
    setPackVersions(null);
    setPackVersion(null);
    if (!pack) return;
    api
      .projectVersions(null, 'modrinth', pack.id, 'modpack')
      .then((l) => {
        setPackVersions(l);
        setPackVersion(l.find((v) => v.versionType === 'release') ?? l[0] ?? null);
      })
      .catch(() => setPackVersions([]));
  }, [pack]);

  // Modpacks want more memory.
  useEffect(() => {
    if (mode === 'modpack') setMem((m) => Math.max(m, Math.min(6144, maxMem)));
  }, [mode, maxMem]);

  const portClash = instances.find((i) => i.network.port === port || (i.network.rconEnabled && i.network.rconPort === port));
  const canNext =
    step === 0 ? (mode === 'software' ? !!pick.software : !!pack) : step === 1 ? (mode === 'software' ? !!pick.mc : !!packVersion) : step === 2 ? name.trim().length > 0 && !portClash : eula;
  const setProp = (k: string, v: string) => setProps((p) => ({ ...p, [k]: v }));

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      const inst = await api.createInstance({
        name: name.trim(),
        installDir: dir || null,
        color,
        software: mode === 'software' ? pick.software : 'fabric',
        mcVersion: mode === 'software' ? pick.mc : (packVersion?.gameVersions[0] ?? ''),
        loaderVersion: mode === 'software' ? pick.loader : '',
        port,
        maxMemMb: mem,
        eulaAccepted: eula,
        installNow,
        templateId: template || null,
        properties: Object.fromEntries(Object.entries(props).filter(([k, v]) => v !== '' || k === 'level-seed')),
        modpack:
          mode === 'modpack' && pack && packVersion
            ? { source: 'modrinth', projectId: pack.id, versionId: packVersion.id, title: pack.title, versionName: packVersion.versionNumber, iconUrl: pack.iconUrl }
            : null,
      });
      useApp.getState().upsertInstance(inst);
      setDialog(null);
      useApp.getState().openServer(inst.id, 'overview');
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const sw = softwareInfo(pick.software);

  return (
    <Dialog
      open={open}
      onClose={() => setDialog(null)}
      title="Create a Minecraft server"
      description="Any official version, any loader – or a complete modpack."
      icon={<Sparkles />}
      width={900}
      footer={
        <>
          {step > 0 && (
            <Button variant="ghost" icon={<ArrowLeft className="size-3.5" />} onClick={() => setStep(step - 1)} className="mr-auto">
              Back
            </Button>
          )}
          <Button variant="ghost" onClick={() => setDialog(null)}>
            Cancel
          </Button>
          {step < 3 ? (
            <Button variant="primary" disabled={!canNext} iconRight={<ArrowRight className="size-3.5" />} onClick={() => setStep(step + 1)}>
              Continue
            </Button>
          ) : (
            <Button variant="primary" disabled={!canNext} loading={busy} icon={<Rocket className="size-3.5" />} onClick={create}>
              {installNow ? 'Create & install' : 'Create'}
            </Button>
          )}
        </>
      }
    >
      <div className="mb-5 flex items-center gap-2">
        {STEPS.map((s, i) => (
          <div key={s.title} className="flex flex-1 items-center gap-2">
            <div
              className={cn(
                'flex size-7 items-center justify-center rounded-full border text-[12px] font-semibold transition-colors',
                i < step ? 'border-ok/50 bg-ok/12 text-ok' : i === step ? 'border-accent bg-accent/15 text-accent' : 'border-line-strong text-fg-4',
              )}
            >
              {i < step ? <Check className="size-3.5" /> : i + 1}
            </div>
            <span className={cn('text-[12.5px] font-medium', i === step ? 'text-fg' : 'text-fg-4')}>{s.title}</span>
            {i < STEPS.length - 1 && <div className="h-px flex-1 bg-line" />}
          </div>
        ))}
      </div>

      <AnimatePresence mode="wait">
        <motion.div key={step} initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -8 }} transition={{ duration: 0.14 }}>
          {step === 0 && (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-2">
                {(
                  [
                    ['software', 'Pick server software', 'Vanilla, Paper, Fabric, Forge… then add mods/plugins whenever you like.', Layers],
                    ['modpack', 'Start from a modpack', 'Install a Modrinth modpack’s server files, loader and version in one go.', PackageOpen],
                  ] as const
                ).map(([m, t, d, Icon]) => (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    className={cn('flex items-start gap-3 rounded-xl border p-3.5 text-left', mode === m ? 'border-accent/70 bg-accent/8 shadow-glow' : 'border-line hover:border-line-strong')}
                  >
                    <Icon className={cn('mt-0.5 size-5', mode === m ? 'text-accent' : 'text-fg-4')} />
                    <div>
                      <div className="text-[13px] font-semibold text-fg">{t}</div>
                      <div className="mt-0.5 text-[11.5px] leading-snug text-fg-3">{d}</div>
                    </div>
                  </button>
                ))}
              </div>
              {mode === 'software' ? (
                <>
                  <SoftwareGrid value={pick.software} onChange={(s) => setPick({ software: s, mc: '', loader: '' })} />
                  <div className="rounded-xl border border-line bg-panel-2/40 px-3.5 py-2.5 text-[12px] leading-relaxed text-fg-3">
                    <span className="font-medium text-fg">{sw.label}: </span>
                    {sw.description}
                  </div>
                </>
              ) : (
                <ModpackSearch value={pack} onPick={setPack} />
              )}
            </div>
          )}

          {step === 1 &&
            (mode === 'software' ? (
              <VersionPicker value={pick} onChange={setPick} />
            ) : (
              <div className="space-y-3">
                <div className="flex items-center gap-3">
                  {pack?.iconUrl && <img src={pack.iconUrl} alt="" className="size-12 rounded-xl" />}
                  <div className="min-w-0 flex-1">
                    <div className="text-[14px] font-semibold text-fg">{pack?.title}</div>
                    <div className="text-[12px] text-fg-3">by {pack?.author}</div>
                  </div>
                  <Button size="sm" variant="ghost" icon={<ExternalLink className="size-3.5" />} onClick={() => pack && browse(pack.url)}>
                    Modrinth
                  </Button>
                </div>
                <div className="h-[300px] space-y-1 overflow-y-auto rounded-xl border border-line bg-bg-raised p-1.5">
                  {!packVersions && (
                    <div className="flex items-center justify-center gap-2 py-10 text-[12px] text-fg-3">
                      <Loader2 className="size-4 animate-spin" /> Loading versions…
                    </div>
                  )}
                  {packVersions?.map((v) => (
                    <button
                      key={v.id}
                      onClick={() => setPackVersion(v)}
                      className={cn('flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left', packVersion?.id === v.id ? 'bg-accent/12' : 'hover:bg-hover')}
                    >
                      <div className="min-w-0 flex-1">
                        <div className="text-[12.5px] font-medium text-fg">{v.versionNumber || v.name}</div>
                        <div className="text-[11px] text-fg-4">
                          Minecraft {v.gameVersions.join(', ')} · {v.loaders.join(', ')} · {bytes(v.size)}
                        </div>
                      </div>
                      {v.versionType !== 'release' && <Badge tone="warn">{v.versionType}</Badge>}
                      {packVersion?.id === v.id && <Check className="size-4 text-accent" />}
                    </button>
                  ))}
                </div>
                <div className="text-[11.5px] text-fg-4">Only the pack’s server-side files are installed. Players install the same pack in their launcher (Modrinth App, Prism, CurseForge app…).</div>
              </div>
            ))}

          {step === 2 && (
            <div className="grid grid-cols-2 gap-x-6 gap-y-4">
              <div>
                <Label>Name</Label>
                <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
              </div>
              <div>
                <Label>Colour</Label>
                <div className="flex h-8 items-center gap-1.5">
                  {Object.entries(SERVER_COLORS).map(([k, c]) => (
                    <button key={k} title={c.label} onClick={() => setColor(k)} className={cn('size-6 rounded-full border-2 transition-transform', color === k ? 'scale-110 border-white' : 'border-transparent')} style={{ background: c.hex }} />
                  ))}
                </div>
              </div>
              <div className="col-span-2">
                <Label>Folder</Label>
                <div className="flex gap-2">
                  <Input mono value={dir} onChange={(e) => setDir(e.target.value)} placeholder="Automatic (inside the manager’s data folder)" />
                  <Button
                    icon={<FolderOpen className="size-3.5" />}
                    onClick={async () => {
                      const p = await pickFolder('Server folder');
                      if (p) setDir(p);
                    }}
                  >
                    Browse
                  </Button>
                </div>
                {probe?.detected && (
                  <div className="mt-1.5 text-[11.5px] text-info">
                    Found an existing {softwareInfo(probe.detected.software).label} {probe.detected.mcVersion} server there – its worlds and settings are kept.
                  </div>
                )}
                {probe && !probe.writable && <div className="mt-1.5 text-[11.5px] text-err">That folder isn’t writable.</div>}
                {probe?.freeBytes != null && <div className="mt-1 text-[11px] text-fg-4">{bytes(probe.freeBytes)} free on that drive</div>}
              </div>
              <div>
                <Label>Port</Label>
                <NumberInput value={port} min={1} max={65535} onChange={setPort} />
                {portClash && <div className="mt-1 text-[11.5px] text-err">Used by {portClash.name}.</div>}
              </div>
              <div>
                <Label>Memory · {mem >= 1024 ? `${(mem / 1024).toFixed(1)} GB` : `${mem} MB`}</Label>
                <Slider value={mem} min={1024} max={maxMem} step={512} onChange={setMem} warnAbove={totalMb * 0.75} />
              </div>
              <div>
                <Label>Game mode</Label>
                <Select value={props.gamemode} onChange={(e) => setProp('gamemode', e.target.value)}>
                  {GAMEMODES.map((g) => (
                    <option key={g} value={g} className="capitalize">
                      {g}
                    </option>
                  ))}
                </Select>
              </div>
              <div>
                <Label>Difficulty</Label>
                <Select value={props.difficulty} onChange={(e) => setProp('difficulty', e.target.value)}>
                  {DIFFICULTIES.map((g) => (
                    <option key={g} value={g}>
                      {g}
                    </option>
                  ))}
                </Select>
              </div>
              <div>
                <Label>Max players</Label>
                <NumberInput value={Number(props['max-players'])} min={1} max={500} onChange={(v) => setProp('max-players', String(v))} />
              </div>
              <div>
                <Label>Seed (optional)</Label>
                <Input mono value={props['level-seed']} onChange={(e) => setProp('level-seed', e.target.value)} placeholder="random" />
              </div>
              <label className="flex items-center justify-between rounded-xl border border-line px-3 py-2 text-[12.5px] text-fg-2">
                Whitelist (friends only)
                <Toggle size="sm" checked={props['white-list'] === 'true'} onChange={(v) => setProps((p) => ({ ...p, 'white-list': String(v), 'enforce-whitelist': String(v) }))} />
              </label>
              <label className="flex items-center justify-between rounded-xl border border-line px-3 py-2 text-[12.5px] text-fg-2">
                PvP
                <Toggle size="sm" checked={props.pvp === 'true'} onChange={(v) => setProp('pvp', String(v))} />
              </label>
              {instances.length > 0 && mode === 'software' && (
                <div className="col-span-2">
                  <Label>Copy settings from</Label>
                  <Select value={template} onChange={(e) => setTemplate(e.target.value)}>
                    <option value="">Nothing – fresh defaults</option>
                    {instances.map((i) => (
                      <option key={i.id} value={i.id}>
                        {i.name} (Java, automation, properties)
                      </option>
                    ))}
                  </Select>
                </div>
              )}
            </div>
          )}

          {step === 3 && (
            <div className="space-y-4">
              <div className="flex items-center gap-4 rounded-xl border border-line bg-bg-raised p-4">
                {mode === 'software' ? <SoftwareIcon software={pick.software} size={52} className="rounded-xl" /> : pack?.iconUrl ? <img src={pack.iconUrl} alt="" className="size-[52px] rounded-xl" /> : null}
                <div className="min-w-0 flex-1">
                  <div className="text-[15px] font-semibold text-fg">{name}</div>
                  <div className="text-[12.5px] text-fg-3">
                    {mode === 'software' ? `${sw.label} ${pick.mc}${pick.loader ? ` · ${pick.loader}` : ''}` : `${pack?.title} ${packVersion?.versionNumber} (Minecraft ${packVersion?.gameVersions[0]})`} · port {port} · {(mem / 1024).toFixed(1)} GB
                  </div>
                  <div className="text-[11.5px] text-fg-4">
                    {props.gamemode} · {props.difficulty} · up to {props['max-players']} players{props['white-list'] === 'true' ? ' · whitelist' : ''}
                  </div>
                </div>
              </div>
              <div className={cn('rounded-xl border p-4', eula ? 'border-ok/40 bg-ok/5' : 'border-warn/40 bg-warn/5')}>
                <Checkbox
                  checked={eula}
                  onChange={setEula}
                  label={
                    <span>
                      I agree to the{' '}
                      <button className="text-accent underline" onClick={() => browse('https://aka.ms/MinecraftEULA')}>
                        Minecraft End User License Agreement
                      </button>{' '}
                      (required to run any Minecraft server)
                    </span>
                  }
                />
              </div>
              <Checkbox checked={installNow} onChange={setInstallNow} label={<span className="flex items-center gap-1.5"><Download className="size-3.5" /> Download and install now (Java is fetched automatically if needed)</span>} />
              {error && <Callout tone="err">{error}</Callout>}
            </div>
          )}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}
