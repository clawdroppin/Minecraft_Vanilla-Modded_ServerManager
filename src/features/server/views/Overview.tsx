import { useEffect, useState } from 'react';
import { Archive, ArrowRight, ArrowUpCircle, CalendarClock, Check, CircleDashed, Copy, Cpu, Gauge, Globe2, Home, LogIn, LogOut, MemoryStick, MessageSquare, Monitor, Network, Terminal, Trophy, Users } from 'lucide-react';
import { api } from '@/lib/ipc';
import { cn } from '@/lib/cn';
import { ago, bytes, clock, duration, until } from '@/lib/format';
import { lineLevel, useLogLines, useServerHistory } from '@/lib/live';
import { copy } from '@/lib/shell';
import { joinAddress, netInfo } from '@/lib/net';
import { stripFormatting } from '@/lib/mc';
import type { NetInfo } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Sparkline } from '@/components/charts/Charts';
import { Button } from '@/components/ui/Button';
import { Callout, Card, Empty, Stat } from '@/components/ui/Surface';
import { PlayerHead, ServerListCard, tpsTone } from '@/components/ui/Mc';
import { ServerControls } from '../ServerControls';

function CopyChip({ label, value, icon, hint }: { label: string; value: string; icon: React.ReactNode; hint?: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      title={hint}
      onClick={() => {
        copy(value);
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
      className="group flex min-w-0 flex-1 items-center gap-3 rounded-xl border border-line bg-bg-raised px-3.5 py-2.5 text-left transition-colors hover:border-line-strong"
    >
      <div className="text-fg-4 [&_svg]:size-4">{icon}</div>
      <div className="min-w-0 flex-1">
        <div className="text-[10.5px] font-medium tracking-wide text-fg-4 uppercase">{label}</div>
        <div className="selectable truncate font-mono text-[13px] text-fg">{value || '—'}</div>
      </div>
      {done ? <Check className="size-3.5 text-ok" /> : <Copy className="size-3.5 text-fg-4 opacity-0 transition-opacity group-hover:opacity-100" />}
    </button>
  );
}

export function Overview({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const hist = useServerHistory(id);
  const lines = useLogLines(id);
  const events = useApp((s) => s.players[id]) ?? [];
  const setView = useApp((s) => s.setView);
  const [net, setNet] = useState<NetInfo | null>(null);
  const [icon, setIcon] = useState<string | null>(null);
  const [fwOk, setFwOk] = useState<boolean | null>(null);
  useEffect(() => {
    netInfo().then(setNet).catch(() => {});
    api.getServerIcon(id).then(setIcon).catch(() => {});
    api.firewallState(id).then((f) => setFwOk((f.portsOk || f.programRule) && f.blockedBy.length === 0)).catch(() => setFwOk(null));
  }, [id, st?.state]);

  const win = hist.slice(-120);
  const last = hist[hist.length - 1];
  const live = st?.state === 'running' || st?.state === 'starting';
  const state = st?.state ?? 'stopped';
  const port = inst.network.port;
  const publicHost = inst.network.publicAddress.trim() || net?.publicIp || '';
  const lan = net?.local[0]?.ip;
  const maxPlayers = st?.maxPlayers ?? Number(inst.properties['max-players'] ?? 20);
  const xmx = inst.java.maxMemMb * 1048576;

  const checklist = [
    { done: state !== 'notInstalled', label: `Install ${inst.software === 'vanilla' ? 'the Minecraft server' : 'the server software'}`, go: 'software' as const },
    { done: inst.eulaAccepted, label: 'Accept the Minecraft EULA', go: 'settings' as const },
    { done: fwOk !== false, label: 'Allow the server through Windows Firewall', go: 'ports' as const },
    { done: inst.network.upnp || !!inst.network.publicAddress, label: 'Forward the port so friends can join from the internet', go: 'ports' as const },
    { done: live, label: 'Start the server', go: null },
  ];
  const setupDone = checklist.every((c) => c.done);
  const recentEvents = events.slice(-14).reverse();
  const recentLines = lines.slice(-8);

  return (
    <div className="mx-auto max-w-[1240px] space-y-5 p-6">
      {st?.state === 'crashed' && (
        <Callout
          tone="err"
          title={st.crashLoop ? 'The server keeps crashing' : 'The server stopped unexpectedly'}
          action={
            <div className="flex gap-2">
              <Button size="sm" icon={<Terminal className="size-3.5" />} onClick={() => setView(id, 'console')}>
                Console
              </Button>
              <Button size="sm" onClick={() => setView(id, 'health')}>
                Health check
              </Button>
            </div>
          }
        >
          {st.crashLoop
            ? `It crashed ${st.crashCount} times in 15 minutes, so automatic restarts are paused.`
            : `Exit code ${st.lastExitCode ?? '?'}. Automatic restart is ${inst.automation.autoRestart ? 'on' : 'off'}.`}{' '}
          Crash reports and errors are listed under Health.
        </Callout>
      )}
      {st?.update && (
        <Callout
          tone="info"
          title={st.update.kind === 'build' ? 'Update available' : 'New Minecraft version'}
          action={
            <Button size="sm" icon={<ArrowUpCircle className="size-3.5" />} onClick={() => setView(id, 'software')}>
              Review
            </Button>
          }
        >
          {st.update.label}
        </Callout>
      )}

      <Card className="relative overflow-hidden">
        <div className="pixel-dirt pointer-events-none absolute inset-0" />
        <div className="relative grid grid-cols-[minmax(0,1fr)_auto] gap-6 p-5">
          <div className="min-w-0 space-y-4">
            <div className="max-w-[560px]">
              <div className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">In the multiplayer list</div>
              <ServerListCard
                name={inst.name}
                motd={inst.properties['motd'] ?? 'A Minecraft Server'}
                icon={icon}
                players={st?.players.length ?? 0}
                max={maxPlayers}
                ping={live ? 18 : null}
                version={live ? (st?.runningVersion ?? undefined) : 'Offline'}
              />
            </div>
            <div className="flex gap-2">
              <CopyChip label="Internet" icon={<Globe2 />} value={publicHost ? joinAddress(publicHost, port) : ''} hint="Share this with friends (needs port forwarding)" />
              <CopyChip label="Same network (LAN)" icon={<Network />} value={lan ? joinAddress(lan, port) : ''} />
              <CopyChip label="This PC" icon={<Monitor />} value={joinAddress('localhost', port)} />
            </div>
          </div>
          <div className="flex flex-col items-end justify-between gap-4">
            {!live && <ServerControls id={id} size="lg" />}
            {st?.startedAt && live && (
              <div className="text-right">
                <div className="text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">Uptime</div>
                <div className="font-mono text-[20px] font-semibold text-fg">{duration((Date.now() - st.startedAt) / 1000)}</div>
                {st.bootSecs != null && <div className="text-[11px] text-fg-4">booted in {st.bootSecs.toFixed(1)}s</div>}
              </div>
            )}
          </div>
        </div>
      </Card>

      {!setupDone && (
        <Card className="p-5">
          <div className="text-[14px] font-semibold text-fg">Get {inst.name} online</div>
          <div className="mt-0.5 text-[12.5px] text-fg-3">A few steps and your world is ready for friends.</div>
          <div className="mt-3 grid grid-cols-2 gap-x-8 gap-y-2">
            {checklist.map((c, i) => (
              <button
                key={i}
                disabled={c.done || !c.go}
                onClick={() => c.go && setView(id, c.go)}
                className={cn('flex items-center gap-2.5 text-left text-[12.5px]', c.done ? 'text-fg-4 line-through' : 'text-fg-2 hover:text-fg')}
              >
                {c.done ? <Check className="size-4 text-ok" /> : <CircleDashed className="size-4 text-accent" />}
                {c.label}
                {!c.done && c.go && <ArrowRight className="size-3.5 text-fg-4" />}
              </button>
            ))}
          </div>
        </Card>
      )}

      <div className="grid grid-cols-4 gap-4">
        <Stat label="Players" icon={<Users />} value={`${st?.players.length ?? 0}/${maxPlayers}`} sub={live ? (st?.players.length ? st.players.map((p) => p.name).join(', ') : 'Nobody online') : 'Server offline'}>
          <Sparkline values={win.map((p) => p.players)} className="mt-2" />
        </Stat>
        <Stat
          label="TPS"
          icon={<Gauge />}
          value={<span className={tpsTone(st?.tps)}>{st?.tps != null ? st.tps.toFixed(1) : '–'}</span>}
          sub={st?.mspt != null ? `${st.mspt.toFixed(1)} ms per tick` : live ? (inst.network.rconEnabled ? 'Measuring…' : 'Needs RCON') : '20 is perfect'}
        >
          <Sparkline values={win.map((p) => p.tps ?? 0)} className="mt-2" />
        </Stat>
        <Stat label="CPU" icon={<Cpu />} value={live && last ? `${last.cpu.toFixed(0)}%` : '–'} sub="of all cores">
          <Sparkline values={win.map((p) => p.cpu)} className="mt-2" />
        </Stat>
        <Stat label="Memory" icon={<MemoryStick />} value={live && last ? bytes(last.mem) : '–'} sub={`limit ${bytes(xmx, 0)} (-Xmx)`}>
          <Sparkline values={win.map((p) => p.mem)} max={xmx * 1.15} className="mt-2" />
        </Stat>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
        <Card className="flex min-h-[300px] flex-col">
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <div className="text-[13px] font-semibold text-fg">Online now</div>
            <Button variant="ghost" size="sm" onClick={() => setView(id, 'players')} iconRight={<ArrowRight className="size-3.5" />}>
              Players
            </Button>
          </div>
          {st?.players.length ? (
            <div className="grid grid-cols-2 gap-2 p-4">
              {st.players.map((p) => (
                <div key={p.name} className="flex items-center gap-2.5 rounded-lg border border-line bg-bg-raised px-2.5 py-2">
                  <PlayerHead name={p.name} uuid={p.uuid} size={30} />
                  <div className="min-w-0">
                    <div className="truncate text-[12.5px] font-medium text-fg">{p.name}</div>
                    <div className="text-[11px] text-fg-4">online {ago(p.joinedAt).replace(' ago', '')}</div>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <Empty icon={<Users />} title={live ? 'Nobody online yet' : 'Server offline'} description={live ? 'Share the address above with your friends.' : 'Start the server to let players in.'} className="flex-1" />
          )}
        </Card>
        <Card className="flex min-h-[300px] flex-col">
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <div className="text-[13px] font-semibold text-fg">Activity</div>
            <div className="flex items-center gap-3 text-[11.5px] text-fg-3">
              <span className="flex items-center gap-1">
                <Archive className="size-3.5" /> backup {ago(st?.lastBackup)}
              </span>
              {st?.nextSchedule && (
                <span className="flex items-center gap-1">
                  <CalendarClock className="size-3.5" /> {st.nextSchedule.action} {until(st.nextSchedule.at)}
                </span>
              )}
            </div>
          </div>
          {recentEvents.length ? (
            <div className="min-h-0 flex-1 space-y-1 overflow-y-auto p-3">
              {recentEvents.map((e, i) => {
                const Icon = { join: LogIn, leave: LogOut, chat: MessageSquare, advancement: Trophy }[e.kind] ?? Home;
                return (
                  <div key={i} className="flex items-start gap-2.5 rounded-lg px-2 py-1.5 text-[12.5px] hover:bg-panel-2">
                    <Icon className={cn('mt-0.5 size-3.5 shrink-0', e.kind === 'join' ? 'text-ok' : e.kind === 'leave' ? 'text-fg-4' : e.kind === 'advancement' ? 'text-gold' : 'text-info')} />
                    <span className="min-w-0 flex-1">
                      <span className="font-medium text-fg">{e.name}</span>{' '}
                      <span className="text-fg-3">
                        {e.kind === 'join' ? 'joined' : e.kind === 'leave' ? 'left' : e.kind === 'advancement' ? 'earned' : 'said'}
                      </span>{' '}
                      {e.text && <span className={e.kind === 'advancement' ? 'text-gold' : 'text-fg-2'}>{e.kind === 'advancement' ? `[${e.text}]` : stripFormatting(e.text)}</span>}
                    </span>
                    <span className="shrink-0 font-mono text-[10.5px] text-fg-4">{clock(e.ts)}</span>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="min-h-0 flex-1 overflow-hidden p-3 font-mono text-[11.5px] leading-relaxed">
              {recentLines.length ? (
                recentLines.map((l) => (
                  <div key={l.n} className={cn('truncate', l.source === 'manager' ? 'text-accent' : { error: 'text-err', warn: 'text-warn', chat: 'text-info', debug: 'text-fg-4', info: 'text-fg-3' }[lineLevel(l.text)])}>
                    {l.text}
                  </div>
                ))
              ) : (
                <Empty icon={<Terminal />} title="Quiet so far" description="Joins, chat and advancements show up here." />
              )}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
