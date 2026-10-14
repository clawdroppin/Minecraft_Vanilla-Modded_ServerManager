import { useEffect, useMemo, useState } from 'react';
import { Check, Loader2, RefreshCw, Search, Sparkles } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { SOFTWARE, loaderNoun, softwareInfo } from '@/lib/mc';
import type { LoaderVersion, VersionList } from '@/lib/types';
import { Badge } from '@/components/ui/Surface';
import { Checkbox, Input, Select } from '@/components/ui/Field';
import { IconButton } from '@/components/ui/Button';
import { SoftwareIcon } from '@/components/ui/Mc';

export interface Pick {
  software: string;
  mc: string;
  loader: string;
}

export function SoftwareGrid({ value, onChange, compact }: { value: string; onChange: (s: string) => void; compact?: boolean }) {
  return (
    <div className={cn('grid gap-2', compact ? 'grid-cols-4' : 'grid-cols-4')}>
      {SOFTWARE.map((s) => {
        const on = s.id === value;
        return (
          <button
            key={s.id}
            onClick={() => onChange(s.id)}
            className={cn(
              'group relative flex items-start gap-2.5 rounded-xl border p-2.5 text-left transition-[border,background,transform] duration-150',
              on ? 'border-accent/70 bg-accent/8 shadow-glow' : 'border-line bg-bg-raised hover:border-line-strong hover:bg-panel-2',
            )}
          >
            <SoftwareIcon software={s.id} size={compact ? 30 : 36} />
            <div className="min-w-0">
              <div className="text-[12.5px] font-semibold text-fg">{s.label}</div>
              <div className="truncate text-[11px] text-fg-3">{s.tagline}</div>
            </div>
            {on && <Check className="absolute top-2 right-2 size-3.5 text-accent" />}
          </button>
        );
      })}
    </div>
  );
}

/** Minecraft version list (filters + search) and loader/build select for one software. */
export function VersionPicker({ value, onChange, current }: { value: Pick; onChange: (p: Pick) => void; current?: string }) {
  const [list, setList] = useState<VersionList | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [snapshots, setSnapshots] = useState(false);
  const [old, setOld] = useState(false);
  const [q, setQ] = useState('');
  const [loaders, setLoaders] = useState<LoaderVersion[] | null>(null);
  const [showBeta, setShowBeta] = useState(false);
  const sw = value.software;

  const load = (refresh = false) => {
    setLoading(true);
    setErr(null);
    api
      .gameVersions(sw, refresh)
      .then((l) => {
        setList(l);
        if (!value.mc || !l.versions.some((v) => v.id === value.mc)) {
          const first = l.versions.find((v) => v.kind === 'release') ?? l.versions[0];
          if (first) onChange({ ...value, mc: first.id, loader: '' });
        }
      })
      .catch((e) => setErr(errMsg(e)))
      .finally(() => setLoading(false));
  };
  useEffect(() => {
    setList(null);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sw]);

  useEffect(() => {
    setLoaders(null);
    if (sw === 'vanilla' || !value.mc) return;
    api.loaderVersions(sw, value.mc).then(setLoaders).catch(() => setLoaders([]));
  }, [sw, value.mc]);

  const shown = useMemo(() => {
    const s = q.trim().toLowerCase();
    return (list?.versions ?? []).filter((v) => {
      if (s) return v.id.toLowerCase().includes(s);
      if (v.kind === 'snapshot') return snapshots;
      if (v.kind === 'old_beta' || v.kind === 'old_alpha') return old;
      return true;
    });
  }, [list, q, snapshots, old]);

  const loaderList = (loaders ?? []).filter((l) => showBeta || l.stable || l.tag === 'recommended' || l.tag === 'latest' || l.id === value.loader);

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_260px] gap-4">
      <div className="min-w-0">
        <div className="mb-2 flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Find a version (e.g. 1.20.1)" className="pl-8" />
          </div>
          <Checkbox checked={snapshots} onChange={setSnapshots} label="Snapshots" />
          {sw === 'vanilla' && <Checkbox checked={old} onChange={setOld} label="Beta/alpha" />}
          <IconButton label="Refresh from the official source" onClick={() => load(true)}>
            <RefreshCw className={loading ? 'animate-spin' : ''} />
          </IconButton>
        </div>
        {err && <div className="mb-2 rounded-lg border border-err/30 bg-err/8 px-3 py-2 text-[12px] text-err">{err}</div>}
        <div className="h-[260px] overflow-y-auto rounded-xl border border-line bg-bg-raised p-1.5">
          {!list && !err ? (
            <div className="flex h-full items-center justify-center gap-2 text-[12px] text-fg-3">
              <Loader2 className="size-4 animate-spin text-accent" /> Fetching {softwareInfo(sw).label} versions…
            </div>
          ) : (
            <div className="grid grid-cols-4 gap-1">
              {shown.map((v) => {
                const on = v.id === value.mc;
                return (
                  <button
                    key={v.id}
                    onClick={() => onChange({ ...value, mc: v.id, loader: '' })}
                    className={cn(
                      'flex h-8 items-center gap-1.5 rounded-lg px-2 text-left font-mono text-[12px] transition-colors',
                      on ? 'bg-accent-strong text-emerald-950' : 'text-fg-2 hover:bg-hover hover:text-fg',
                    )}
                  >
                    <span className="truncate">{v.id}</span>
                    {v.id === list?.latestRelease && <Sparkles className={cn('size-3 shrink-0', on ? '' : 'text-gold')} />}
                    {v.id === current && <span className={cn('ml-auto size-1.5 shrink-0 rounded-full', on ? 'bg-emerald-950' : 'bg-accent')} title="installed" />}
                    {v.kind !== 'release' && <span className={cn('ml-auto text-[9.5px]', on ? '' : 'text-fg-4')}>{v.kind === 'snapshot' ? 'snap' : 'old'}</span>}
                  </button>
                );
              })}
            </div>
          )}
        </div>
        {list && (
          <div className="mt-1.5 flex items-center gap-3 text-[11px] text-fg-4">
            <span>
              Latest release <span className="font-mono text-gold">{list.latestRelease}</span>
            </span>
            <span>
              snapshot <span className="font-mono text-fg-3">{list.latestSnapshot}</span>
            </span>
            <span className="ml-auto">{shown.length} shown</span>
          </div>
        )}
      </div>
      <div className="space-y-3">
        <div className="rounded-xl border border-line bg-bg-raised p-3">
          <div className="text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">Selected</div>
          <div className="mt-1.5 flex items-center gap-2">
            <SoftwareIcon software={sw} size={28} />
            <div>
              <div className="text-[13px] font-semibold text-fg">{softwareInfo(sw).label}</div>
              <div className="font-mono text-[12px] text-accent">{value.mc || '—'}</div>
            </div>
          </div>
        </div>
        {sw !== 'vanilla' && (
          <div>
            <div className="mb-1.5 flex items-center justify-between text-[11.5px] font-medium text-fg-3">
              {loaderNoun(sw)}
              <Checkbox checked={showBeta} onChange={setShowBeta} label={<span className="text-[11px]">beta</span>} />
            </div>
            <Select value={value.loader} onChange={(e) => onChange({ ...value, loader: e.target.value })} disabled={!loaders}>
              <option value="">{loaders == null ? 'Loading…' : loaders.length ? 'Latest recommended (auto-updates)' : 'None available for this version'}</option>
              {loaderList.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.id}
                  {l.tag ? ` · ${l.tag}` : ''}
                </option>
              ))}
            </Select>
            {loaders && loaders.length === 0 && <div className="mt-1.5 text-[11px] text-warn">{softwareInfo(sw).label} has nothing for {value.mc} yet.</div>}
            <div className="mt-1.5 text-[11px] leading-relaxed text-fg-4">
              {sw === 'paper' || sw === 'purpur' || sw === 'folia'
                ? 'Builds are bug-fix releases for the same Minecraft version.'
                : sw === 'fabric' || sw === 'quilt'
                  ? 'The loader version is independent of the Minecraft version.'
                  : 'Installed with the official installer (takes a minute).'}
            </div>
          </div>
        )}
        <div className="flex flex-wrap gap-1">
          {softwareInfo(sw).goodFor.map((g) => (
            <Badge key={g}>{g}</Badge>
          ))}
        </div>
      </div>
    </div>
  );
}
