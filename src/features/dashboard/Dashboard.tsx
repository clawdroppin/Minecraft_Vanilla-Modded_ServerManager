import { useEffect, useState } from 'react';
import { motion } from 'motion/react';
import { ArrowUpCircle, ArrowUpRight, Cpu, Download, ExternalLink, FlaskConical, Gauge, MemoryStick, Network, Play, Plus, Server, Sparkles, Square, Users } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { bytes, pct } from '@/lib/format';
import { serverColor } from '@/lib/colors';
import { browse } from '@/lib/shell';
import { ago } from '@/lib/format';
import { useServerHistory, useSystemHistory } from '@/lib/live';
import type { ServerInstance, VersionList } from '@/lib/types';
import { useApp } from '@/store/app';
import { Sparkline, SERIES } from '@/components/charts/Charts';
import { Button } from '@/components/ui/Button';
import { confirm } from '@/components/ui/Dialog';
import { Card, StatusPill } from '@/components/ui/Surface';
import { ServerControls } from '@/features/server/ServerControls';
import { GrassBlock, SoftwareBadge, SoftwareIcon, tpsTone } from '@/components/ui/Mc';

function ServerCard({ inst, i }: { inst: ServerInstance; i: number }) {
  const st = useApp((s) => s.statuses[inst.id]);
  const openServer = useApp((s) => s.openServer);
  const hist = useServerHistory(inst.id);
  const win = hist.slice(-90);
  const last = hist[hist.length - 1];
  const live = st?.state === 'running' || st?.state === 'starting';
  return (
    <motion.div initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.18, delay: Math.min(i * 0.03, 0.2) }}>
      <Card interactive className="group relative flex h-full flex-col overflow-hidden p-0">
        <div className="absolute inset-x-0 top-0 h-[2px]" style={{ background: serverColor(inst.color), opacity: live ? 1 : 0.4 }} />
        <button onClick={() => openServer(inst.id)} className="flex flex-1 flex-col p-4 text-left">
          <div className="flex items-start gap-3">
            <SoftwareIcon software={inst.installed?.software ?? inst.software} size={38} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5 truncate text-[14px] font-semibold text-fg">
                {inst.name}
                <ArrowUpRight className="size-3.5 text-fg-4 opacity-0 transition-opacity group-hover:opacity-100" />
              </div>
              <div className="mt-0.5 truncate font-mono text-[11px] text-fg-4">
                {inst.installed ? `${inst.installed.mcVersion}${inst.installed.loaderVersion ? ` · ${inst.installed.loaderVersion}` : ''}` : `${inst.mcVersion || 'latest'} (not installed)`} · :{inst.network.port}
              </div>
            </div>
            <StatusPill state={st?.state} busy={st?.busy} />
          </div>
          <div className="mt-4 grid grid-cols-4 gap-3">
            <div>
              <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">CPU</div>
              <div className="font-mono text-[14px] text-fg">{live && last ? pct(last.cpu, 1) : '—'}</div>
            </div>
            <div>
              <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">Memory</div>
              <div className="font-mono text-[14px] text-fg">{live && last ? bytes(last.mem) : '—'}</div>
            </div>
            <div>
              <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">Players</div>
              <div className="font-mono text-[14px] text-fg">{live ? `${st?.players.length ?? 0}/${st?.maxPlayers ?? inst.properties['max-players'] ?? 20}` : '—'}</div>
            </div>
            <div>
              <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">TPS</div>
              <div className={`font-mono text-[14px] ${tpsTone(st?.tps)}`}>{live && st?.tps != null ? st.tps.toFixed(1) : '—'}</div>
            </div>
          </div>
          <Sparkline className="mt-3" height={36} values={win.map((p) => p.cpu)} max={Math.max(10, ...win.map((p) => p.cpu)) * 1.2} color={serverColor(inst.color)} />
        </button>
        <div className="flex items-center justify-between border-t border-line bg-bg-raised/50 px-4 py-2.5">
          <span className="text-[11.5px] text-fg-4">
            {st?.update ? (
              <span className="flex items-center gap-1 text-info">
                <ArrowUpCircle className="size-3" /> {st.update.kind === 'build' ? 'Update available' : `${st.update.latest} available`}
              </span>
            ) : st?.state === 'notInstalled' ? (
              'Not installed'
            ) : live && st?.players.length ? (
              st.players.map((p) => p.name).join(', ')
            ) : (
              <SoftwareBadge software={inst.installed?.software ?? inst.software} />
            )}
          </span>
          <ServerControls id={inst.id} size="sm" compact />
        </div>
      </Card>
    </motion.div>
  );
}

function LatestMinecraft() {
  const [v, setV] = useState<VersionList | null>(null);
  const instances = useApp((s) => s.instances);
  useEffect(() => {
    api.gameVersions('vanilla').then(setV).catch(() => {});
    const t = setInterval(() => api.gameVersions('vanilla').then(setV).catch(() => {}), 30 * 60_000);
    return () => clearInterval(t);
  }, []);
  if (!v) return null;
  const rel = v.versions.find((x) => x.id === v.latestRelease);
  const snap = v.versions.find((x) => x.id === v.latestSnapshot);
  const behind = instances.filter((i) => i.installed && i.installed.mcVersion !== v.latestRelease).length;
  return (
    <Card className="relative overflow-hidden p-4">
      <div className="pixel-dirt pointer-events-none absolute inset-0" />
      <div className="relative flex items-center gap-4">
        <GrassBlock className="size-11" />
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-medium tracking-wide text-fg-3 uppercase">Latest Minecraft</div>
          <div className="mt-0.5 flex items-baseline gap-3">
            <span className="flex items-center gap-1.5 font-mono text-[20px] font-semibold text-fg">
              <Sparkles className="size-4 text-gold" />
              {v.latestRelease}
            </span>
            <span className="text-[11.5px] text-fg-4">released {rel ? ago(Date.parse(rel.releaseTime)) : ''}</span>
            {v.latestSnapshot !== v.latestRelease && (
              <span className="flex items-center gap-1 text-[11.5px] text-fg-3">
                <FlaskConical className="size-3.5 text-violet" /> snapshot <span className="font-mono text-fg-2">{v.latestSnapshot}</span> {snap ? ago(Date.parse(snap.releaseTime)) : ''}
              </span>
            )}
          </div>
          <div className="mt-0.5 text-[11.5px] text-fg-4">
            {v.versions.filter((x) => x.kind === 'release').length} releases and {v.versions.filter((x) => x.kind === 'snapshot').length} snapshots available
            {behind > 0 ? ` · ${behind} of your servers run an older version` : ''}
          </div>
        </div>
        <Button size="sm" variant="ghost" icon={<ExternalLink className="size-3.5" />} onClick={() => browse('https://www.minecraft.net/en-us/articles')}>
          Changelog
        </Button>
      </div>
    </Card>
  );
}

function SystemStrip() {
  const sys = useSystemHistory();
  const info = useApp((s) => s.info);
  const win = sys.slice(-90);
  const last = sys[sys.length - 1];
  const tiles = [
    { label: 'CPU', icon: Cpu, value: last ? pct(last.cpu) : '—', sub: `${info?.cores ?? '?'} logical cores`, values: win.map((p) => p.cpu), max: 100 },
    { label: 'Memory', icon: MemoryStick, value: last ? bytes(last.memUsed) : '—', sub: last ? `of ${bytes(last.memTotal)}` : '', values: win.map((p) => p.memUsed), fit: true },
    { label: 'Network in', icon: Network, value: last ? `${bytes(last.rxBps)}/s` : '—', sub: 'whole PC', values: win.map((p) => p.rxBps), color: SERIES[1] },
    { label: 'Network out', icon: Network, value: last ? `${bytes(last.txBps)}/s` : '—', sub: 'whole PC', values: win.map((p) => p.txBps), color: SERIES[0] },
  ];
  return (
    <div className="grid grid-cols-4 gap-4">
      {tiles.map((t) => (
        <Card key={t.label} className="p-4">
          <div className="flex items-center gap-2 text-[11px] font-medium tracking-wide text-fg-3 uppercase">
            <t.icon className="size-3.5" />
            {t.label}
          </div>
          <div className="mt-1.5 flex items-baseline gap-2">
            <span className="font-mono text-[19px] font-semibold text-fg tabular-nums">{t.value}</span>
            <span className="text-[11px] text-fg-4">{t.sub}</span>
          </div>
          <Sparkline className="mt-2" height={28} values={t.values} max={t.max} color={t.color} fit={t.fit} />
        </Card>
      ))}
    </div>
  );
}

export function Dashboard() {
  const instances = useApp((s) => s.instances);
  const statuses = useApp((s) => s.statuses);
  const info = useApp((s) => s.info);
  const setDialog = useApp((s) => s.setDialog);
  const toast = useApp((s) => s.toast);
  const [bulk, setBulk] = useState<'start' | 'stop' | null>(null);
  const online = instances.filter((i) => statuses[i.id]?.state === 'running');
  const players = online.reduce((n, i) => n + (statuses[i.id]?.players.length ?? 0), 0);
  const startable = instances.filter((i) => ['stopped', 'crashed'].includes(statuses[i.id]?.state ?? ''));
  const hour = new Date().getHours();
  const greet = hour < 5 ? 'The creepers are out' : hour < 12 ? 'Good morning, crafter' : hour < 18 ? 'Good afternoon, crafter' : 'Good evening, crafter';

  const bulkRun = async (a: 'start' | 'stop') => {
    if (a === 'stop') {
      const r = await confirm({ title: `Stop ${online.length} server${online.length === 1 ? '' : 's'}?`, message: 'Each server saves and shuts down cleanly.', confirm: 'Stop all' });
      if (!r.ok) return;
    }
    setBulk(a);
    for (const i of a === 'start' ? startable : online) {
      try {
        if (a === 'start') await api.start(i.id);
        else await api.stop(i.id);
      } catch (e) {
        toast('error', `${i.name}: ${errMsg(e)}`, i.id);
      }
    }
    setBulk(null);
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="grass-grid relative border-b border-line">
        <div className="mx-auto max-w-[1280px] px-8 pt-8 pb-7">
          <div className="flex items-end justify-between gap-6">
            <div>
              <div className="flex items-center gap-2 text-[12px] font-medium text-accent">
                <Gauge className="size-3.5" /> {info?.hostname ?? 'This PC'}
              </div>
              <h1 className="mt-1.5 text-[26px] font-semibold tracking-tight text-fg">{greet}</h1>
              <p className="mt-1 text-[13px] text-fg-3">
                {instances.length === 0
                  ? 'Create your first Minecraft server – vanilla, Paper, Fabric, Forge, NeoForge or a whole modpack – in about a minute.'
                  : `${online.length} of ${instances.length} server${instances.length === 1 ? '' : 's'} online · ${players} player${players === 1 ? '' : 's'} connected`}
              </p>
            </div>
            <div className="flex gap-2">
              {instances.length > 0 && (
                <>
                  <Button icon={<Play className="size-3.5" />} disabled={!startable.length} loading={bulk === 'start'} onClick={() => bulkRun('start')}>
                    Start all
                  </Button>
                  <Button icon={<Square className="size-3.5" />} disabled={!online.length} loading={bulk === 'stop'} onClick={() => bulkRun('stop')}>
                    Stop all
                  </Button>
                </>
              )}
              <Button icon={<Download className="size-3.5" />} onClick={() => setDialog({ kind: 'import' })}>
                Import
              </Button>
              <Button variant="primary" size="md" icon={<Plus className="size-4" />} onClick={() => setDialog({ kind: 'new' })}>
                New server
              </Button>
            </div>
          </div>
        </div>
      </div>
      <div className="mx-auto max-w-[1280px] space-y-6 px-8 py-6">
        <LatestMinecraft />
        <SystemStrip />
        <div>
          <div className="mb-3 flex items-center gap-2 text-[12px] font-semibold tracking-[0.1em] text-fg-3 uppercase">
            <Server className="size-3.5" /> Servers
            {players > 0 && (
              <span className="ml-2 flex items-center gap-1 font-mono text-[11px] tracking-normal text-ok normal-case">
                <Users className="size-3" /> {players} online
              </span>
            )}
          </div>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
            {instances.map((i, idx) => (
              <ServerCard key={i.id} inst={i} i={idx} />
            ))}
            <button
              onClick={() => setDialog({ kind: 'new' })}
              className="group flex min-h-[210px] flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-line-strong text-fg-3 transition-colors hover:border-accent/60 hover:bg-accent/[0.03] hover:text-fg"
            >
              <div className="flex size-11 items-center justify-center rounded-xl border border-line bg-panel-2 transition-colors group-hover:border-accent/40 group-hover:text-accent">
                <Plus className="size-5" />
              </div>
              <div className="text-[13px] font-medium">Create a server</div>
              <div className="text-[11.5px] text-fg-4">Any version, any loader, or a modpack</div>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
