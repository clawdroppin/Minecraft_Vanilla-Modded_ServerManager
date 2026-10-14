import { useEffect, useMemo, useState } from 'react';
import { Check, Coffee, Copy, Cpu, Download, FolderSearch, Gauge, MemoryStick, RefreshCw, TerminalSquare, Zap } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { copy, pickFile } from '@/lib/shell';
import type { JavaRuntime } from '@/lib/types';
import { useApp, useInstance } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Badge, Callout, PageHeader, Section } from '@/components/ui/Surface';
import { FieldRow, Input, NumberInput, Select, Slider } from '@/components/ui/Field';
import { useDraft } from '../draft';

const GC = [
  { id: 'aikar', label: "Aikar's flags", desc: 'Tuned G1 – the community standard for Minecraft servers. Best for almost everyone.', badge: 'recommended' },
  { id: 'g1', label: 'G1 (basic)', desc: 'Java’s default collector with a 200 ms pause goal.', badge: '' },
  { id: 'zgc', label: 'Generational ZGC', desc: 'Near-zero pauses; needs Java 17+, lots of RAM (12 GB+) and spare cores.', badge: 'big servers' },
  { id: 'none', label: 'No GC flags', desc: 'Only your own JVM arguments.', badge: '' },
];

function gb(mb: number) {
  return mb >= 1024 ? `${(mb / 1024).toFixed(mb % 1024 ? 1 : 0)} GB` : `${mb} MB`;
}

export function JavaView({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const { draft, patch } = useDraft(id);
  const info = useApp((s) => s.info);
  const toast = useApp((s) => s.toast);
  const j = draft.java;
  const [runtimes, setRuntimes] = useState<JavaRuntime[] | null>(null);
  const [line, setLine] = useState('');
  const [mods, setMods] = useState(0);
  const required = inst.installed?.javaMajor ?? 21;
  const totalMb = Math.floor((info?.totalMemory ?? 16 * 1073741824) / 1048576);
  const maxSlider = Math.max(2048, Math.floor((totalMb * 0.9) / 512) * 512);

  const loadJava = () => api.listJava().then(setRuntimes).catch(() => setRuntimes([]));
  useEffect(() => {
    loadJava();
    api.listContent(id).then((c) => setMods(c.items.filter((i) => i.enabled).length)).catch(() => {});
  }, [id]);
  useEffect(() => {
    const t = setTimeout(() => api.previewLaunchLine(draft).then(setLine).catch(() => {}), 250);
    return () => clearTimeout(t);
  }, [draft]);

  const kind = inst.installed && ['fabric', 'quilt', 'forge', 'neoforge'].includes(inst.installed.software) ? 'mods' : 'plugins';
  const recommended = kind === 'mods' ? (mods > 200 ? 10240 : mods > 100 ? 8192 : mods > 30 ? 6144 : 4096) : 3072;
  const auto = useMemo(() => {
    if (!runtimes) return null;
    const exact = runtimes.filter((r) => r.major === required);
    if (exact.length) return exact.find((r) => r.managed) ?? exact[0];
    if (required <= 8) return null;
    return runtimes.filter((r) => r.major > required).sort((a, b) => a.major - b.major)[0] ?? null;
  }, [runtimes, required]);
  const feature = [8, 11, 17, 21, 25].find((f) => f >= required) ?? required;
  const cores = info?.cores ?? 8;

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader title="Java & Memory" description="How the server’s Java process is started. Changes apply on the next start." />

      <Section title="Memory" icon={<MemoryStick />} description={`This PC has ${gb(totalMb)} of RAM. Leave at least 2–3 GB for Windows.`}>
        <div className="py-4">
          <div className="mb-2 flex items-end justify-between">
            <div>
              <div className="text-[12px] text-fg-3">Maximum heap (-Xmx)</div>
              <div className="font-mono text-[26px] font-semibold text-fg">{gb(j.maxMemMb)}</div>
            </div>
            <div className="text-right text-[11.5px] text-fg-3">
              Recommended for {kind === 'mods' ? `${mods} mods` : 'this server'}: <span className="font-mono text-accent">{gb(recommended)}</span>
              {j.maxMemMb !== recommended && (
                <button className="ml-2 text-accent hover:underline" onClick={() => patch((d) => void (d.java.maxMemMb = Math.min(recommended, maxSlider)))}>
                  use it
                </button>
              )}
            </div>
          </div>
          <Slider
            value={Math.min(j.maxMemMb, maxSlider)}
            min={1024}
            max={maxSlider}
            step={512}
            warnAbove={totalMb * 0.75}
            onChange={(v) => patch((d) => {
              d.java.maxMemMb = v;
              if (d.java.minMemMb > v) d.java.minMemMb = v;
            })}
            marks={[1, Math.round(maxSlider / 1024 / 4), Math.round(maxSlider / 1024 / 2), Math.round((maxSlider / 1024) * 0.75), Math.floor(maxSlider / 1024)].map((x) => x)}
          />
          {j.maxMemMb > totalMb * 0.75 && <div className="mt-2 text-[11.5px] text-warn">That leaves little for Windows – the server may start swapping.</div>}
        </div>
        <FieldRow label="Initial heap (-Xms)" description="Aikar recommends the same as the maximum so Java grabs it up front (pairs with AlwaysPreTouch).">
          <div className="flex items-center gap-2">
            <NumberInput value={j.minMemMb} min={256} max={j.maxMemMb} step={256} suffix="MB" onChange={(v) => patch((d) => void (d.java.minMemMb = v))} />
            <Button size="sm" variant="outline" onClick={() => patch((d) => void (d.java.minMemMb = d.java.maxMemMb))}>
              = max
            </Button>
          </div>
        </FieldRow>
      </Section>

      <Section title="Garbage collector" icon={<Gauge />} description="JVM flags that keep lag spikes from garbage collection low.">
        <div className="grid grid-cols-2 gap-2 py-4">
          {GC.map((g) => (
            <button
              key={g.id}
              onClick={() => patch((d) => void (d.java.gcPreset = g.id))}
              className={cn('rounded-xl border p-3 text-left transition-colors', j.gcPreset === g.id ? 'border-accent/60 bg-accent/8' : 'border-line hover:border-line-strong')}
            >
              <div className="flex items-center gap-2 text-[12.5px] font-semibold text-fg">
                {j.gcPreset === g.id ? <Check className="size-3.5 text-accent" /> : <Zap className="size-3.5 text-fg-4" />}
                {g.label}
                {g.badge && <Badge tone={g.badge === 'recommended' ? 'ok' : 'info'}>{g.badge}</Badge>}
              </div>
              <div className="mt-1 text-[11.5px] leading-relaxed text-fg-3">{g.desc}</div>
            </button>
          ))}
        </div>
      </Section>

      <Section
        title="Java runtime"
        icon={<Coffee />}
        description={`Minecraft ${inst.installed?.mcVersion ?? inst.mcVersion} needs Java ${required}${required > 8 ? ' or newer' : ' exactly'}.`}
        actions={
          <IconButton label="Rescan" onClick={loadJava}>
            <RefreshCw />
          </IconButton>
        }
      >
        <div className="space-y-1.5 py-4">
          <button
            onClick={() => patch((d) => void (d.java.path = ''))}
            className={cn('flex w-full items-center gap-3 rounded-xl border px-3.5 py-2.5 text-left', !j.path ? 'border-accent/60 bg-accent/8' : 'border-line hover:border-line-strong')}
          >
            <Coffee className="size-4 text-accent" />
            <div className="min-w-0 flex-1">
              <div className="text-[12.5px] font-medium text-fg">Automatic</div>
              <div className="truncate text-[11.5px] text-fg-3">
                {auto ? `Uses Java ${auto.version} (${auto.vendor || auto.source}) – ${auto.path}` : `Downloads Eclipse Temurin ${feature} on first start`}
              </div>
            </div>
            <Badge tone="ok">recommended</Badge>
          </button>
          {(runtimes ?? []).map((r) => {
            const ok = r.major >= required && !(required <= 8 && r.major > 8);
            return (
              <button
                key={r.path}
                onClick={() => patch((d) => void (d.java.path = r.path))}
                className={cn('flex w-full items-center gap-3 rounded-xl border px-3.5 py-2 text-left', j.path === r.path ? 'border-accent/60 bg-accent/8' : 'border-line hover:border-line-strong')}
              >
                <span className={cn('w-12 shrink-0 text-center font-mono text-[15px] font-semibold', ok ? 'text-fg' : 'text-fg-4')}>{r.major}</span>
                <div className="min-w-0 flex-1">
                  <div className="text-[12px] text-fg">
                    {r.version} <span className="text-fg-4">· {r.vendor || 'unknown vendor'}</span>
                  </div>
                  <div className="truncate font-mono text-[10.5px] text-fg-4">{r.path}</div>
                </div>
                {r.managed && <Badge tone="accent">managed</Badge>}
                {r.source === 'minecraft-launcher' && <Badge>launcher</Badge>}
                {!ok && <Badge tone="warn">too {r.major < required ? 'old' : 'new'}</Badge>}
              </button>
            );
          })}
          <div className="flex gap-2 pt-1">
            <Button
              size="sm"
              icon={<FolderSearch className="size-3.5" />}
              onClick={async () => {
                const p = await pickFile('Pick java.exe', ['exe'], 'Java');
                if (!p) return;
                try {
                  const r = await api.probeJava(p);
                  patch((d) => void (d.java.path = r.path));
                  toast('success', `Java ${r.version} selected`, id);
                } catch (e) {
                  toast('error', errMsg(e), id);
                }
              }}
            >
              Browse for java.exe
            </Button>
            {[8, 17, 21, 25].map((f) => (
              <Button key={f} size="sm" variant="outline" icon={<Download className="size-3.5" />} onClick={() => api.installJava(f).then(() => setTimeout(loadJava, 1000))}>
                Temurin {f}
              </Button>
            ))}
          </div>
        </div>
      </Section>

      <Section title="Arguments & process" icon={<TerminalSquare />}>
        <FieldRow label="Extra JVM arguments" description="Added before -jar, e.g. -Dpaper.disableChannelLimit=true." stacked>
          <Input mono value={j.jvmArgs} onChange={(e) => patch((d) => void (d.java.jvmArgs = e.target.value))} placeholder="-Dsome.property=value" />
        </FieldRow>
        <FieldRow label="Server arguments" description="After the jar. nogui keeps the server’s own window away (the manager is your console).">
          <Input mono value={j.serverArgs} onChange={(e) => patch((d) => void (d.java.serverArgs = e.target.value))} />
        </FieldRow>
        <FieldRow label="Process priority" description="Applies live.">
          <Select value={j.priority} onChange={(e) => patch((d) => void (d.java.priority = e.target.value))}>
            <option value="idle">Low</option>
            <option value="belowNormal">Below normal</option>
            <option value="normal">Normal</option>
            <option value="aboveNormal">Above normal</option>
            <option value="high">High</option>
          </Select>
        </FieldRow>
        <FieldRow label={<span className="flex items-center gap-1.5"><Cpu className="size-3.5 text-fg-3" /> CPU cores</span>} description="Pin the server to some cores (all when none are picked). Applies live." stacked>
          <div className="flex flex-wrap gap-1">
            {Array.from({ length: cores }, (_, c) => {
              const on = j.affinity.includes(c);
              return (
                <button
                  key={c}
                  onClick={() => patch((d) => void (d.java.affinity = on ? d.java.affinity.filter((x) => x !== c) : [...d.java.affinity, c].sort((a, b) => a - b)))}
                  className={cn('h-7 w-9 rounded-md border font-mono text-[11px]', on ? 'border-accent-strong bg-accent-strong text-emerald-950' : 'border-line-strong text-fg-3 hover:text-fg')}
                >
                  {c}
                </button>
              );
            })}
            {j.affinity.length > 0 && (
              <Button size="sm" variant="ghost" onClick={() => patch((d) => void (d.java.affinity = []))}>
                All cores
              </Button>
            )}
          </div>
        </FieldRow>
      </Section>

      <Section
        title="Launch command"
        icon={<TerminalSquare />}
        description="Exactly what the manager runs."
        actions={
          <IconButton label="Copy" onClick={() => copy(line).then(() => toast('success', 'Copied'))}>
            <Copy />
          </IconButton>
        }
      >
        <pre className="selectable my-4 max-h-48 overflow-auto rounded-xl border border-line bg-bg p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-fg-2">{line || '…'}</pre>
      </Section>
      {!inst.installed && <Callout tone="info">The launch target (jar / args file) appears once the server software is installed.</Callout>}
    </div>
  );
}
