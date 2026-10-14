import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowDown, Ban, BarChart3, Crown, Gavel, History, ListChecks, LogOut, MessageSquare, MoreHorizontal, Plus, RefreshCw, ShieldCheck, ShieldOff, Trash2, UserPlus, Users, Wifi } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { ago, clock, dateTime } from '@/lib/format';
import { stripFormatting } from '@/lib/mc';
import type { PlayerLists, PlayerStats } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Badge, Callout, Card, Empty, PageHeader } from '@/components/ui/Surface';
import { Input, Segmented, Toggle } from '@/components/ui/Field';
import { Menu } from '@/components/ui/Menu';
import { confirm } from '@/components/ui/Dialog';
import { PlayerHead } from '@/components/ui/Mc';
import { useDraft } from '../draft';

type Tab = 'online' | 'whitelist' | 'ops' | 'banned-players' | 'banned-ips' | 'known' | 'stats' | 'chat';

const STAT_COLS: { key: keyof PlayerStats; label: string; fmt: (s: PlayerStats) => string }[] = [
  { key: 'playTicks', label: 'Played', fmt: (s) => playtime(s.playTicks) },
  { key: 'lastSeen', label: 'Last seen', fmt: (s) => (s.lastSeen ? ago(s.lastSeen) : '–') },
  { key: 'deaths', label: 'Deaths', fmt: (s) => s.deaths.toLocaleString() },
  { key: 'mobKills', label: 'Mob kills', fmt: (s) => s.mobKills.toLocaleString() },
  { key: 'playerKills', label: 'PvP kills', fmt: (s) => s.playerKills.toLocaleString() },
  { key: 'distanceCm', label: 'Travelled', fmt: (s) => km(s.distanceCm) },
  { key: 'blocksMined', label: 'Mined', fmt: (s) => s.blocksMined.toLocaleString() },
  { key: 'itemsCrafted', label: 'Crafted', fmt: (s) => s.itemsCrafted.toLocaleString() },
];

function playtime(ticks: number) {
  const mins = Math.floor(ticks / 20 / 60);
  if (mins < 60) return `${mins} min`;
  const h = mins / 60;
  return h < 100 ? `${h.toFixed(1)} h` : `${Math.round(h).toLocaleString()} h`;
}

function km(cm: number) {
  const m = cm / 100;
  return m < 1000 ? `${Math.round(m)} m` : `${(m / 1000).toFixed(m < 100_000 ? 1 : 0)} km`;
}

/** Leaderboard from the world's stats files (what players see under Statistics in-game). */
function StatsTable({ id, live }: { id: string; live: boolean }) {
  const [rows, setRows] = useState<PlayerStats[] | null>(null);
  const [sort, setSort] = useState<keyof PlayerStats>('playTicks');
  const load = useCallback(() => {
    api
      .playerStats(id)
      .then(setRows)
      .catch(() => setRows([]));
  }, [id]);
  useEffect(() => {
    load();
  }, [load]);
  const sorted = useMemo(() => [...(rows ?? [])].sort((a, b) => Number(b[sort]) - Number(a[sort])), [rows, sort]);
  const total = (rows ?? []).reduce((n, r) => n + r.playTicks, 0);
  return (
    <Card>
      <div className="flex items-center gap-3 border-b border-line px-4 py-3">
        <div className="text-[13px] font-medium text-fg">{rows == null ? 'Reading stats…' : `${rows.length} player${rows.length === 1 ? '' : 's'} · ${playtime(total)} played in total`}</div>
        <div className="ml-auto text-[11.5px] text-fg-4">{live ? 'The server saves stats when players leave and on autosave.' : 'From the active world’s stats folder.'}</div>
        <IconButton label="Reload stats" onClick={load}>
          <RefreshCw />
        </IconButton>
      </div>
      {rows && rows.length === 0 ? (
        <Empty icon={<BarChart3 />} title="No stats yet" description="Stats appear once players have joined the active world." />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-[12.5px]">
            <thead>
              <tr className="border-b border-line text-left text-[11px] text-fg-4">
                <th className="px-4 py-2 font-medium">Player</th>
                {STAT_COLS.map((c) => (
                  <th key={c.key} className="px-3 py-2 text-right font-medium">
                    <button onClick={() => setSort(c.key)} className={cn('inline-flex items-center gap-1 hover:text-fg', sort === c.key && 'text-accent')}>
                      {c.label}
                      {sort === c.key && <ArrowDown className="size-3" />}
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {sorted.map((s, i) => (
                <tr key={s.uuid} className="hover:bg-panel-2">
                  <td className="px-4 py-2">
                    <div className="flex items-center gap-2.5">
                      <span className="w-5 text-right font-mono text-[11px] text-fg-4">{i + 1}</span>
                      <PlayerHead name={s.name || s.uuid} uuid={s.uuid} size={24} />
                      <span className="truncate font-medium text-fg" title={s.uuid}>
                        {s.name || s.uuid.slice(0, 8)}
                      </span>
                    </div>
                  </td>
                  {STAT_COLS.map((c) => (
                    <td key={c.key} className={cn('px-3 py-2 text-right font-mono tabular-nums', sort === c.key ? 'text-fg' : 'text-fg-3')}>
                      {c.fmt(s)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

export function Players({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const events = useApp((s) => s.players[id]) ?? [];
  const toast = useApp((s) => s.toast);
  const { draft, patch, save } = useDraft(id);
  const [tab, setTab] = useState<Tab>('online');
  const [lists, setLists] = useState<PlayerLists | null>(null);
  const [name, setName] = useState('');
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const live = st?.state === 'running';

  const load = useCallback(() => {
    api.playerLists(id).then(setLists).catch(() => {});
  }, [id]);
  useEffect(() => {
    load();
  }, [load, st?.players.length]);

  const act = async (list: string, add: boolean, target: string, why = '') => {
    setBusy(true);
    try {
      await api.playerAction(id, list, add, target, why);
      toast('success', `${target}: ${add ? 'added to' : 'removed from'} ${list.replace('-', ' ')}`, id);
      // The server writes the JSON a moment after the command.
      setTimeout(load, live ? 800 : 50);
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(false);
    }
  };
  const cmd = async (c: string) => {
    try {
      await api.sendCommand(id, c);
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const isOp = (n: string) => lists?.ops.some((o) => o.name?.toLowerCase() === n.toLowerCase());
  const isWl = (n: string) => lists?.whitelist.some((o) => o.name?.toLowerCase() === n.toLowerCase());
  const whitelistOn = draft.properties['white-list'] === 'true';
  const chats = events.filter((e) => e.kind === 'chat');

  const listFor: Record<string, Record<string, any>[]> = {
    whitelist: lists?.whitelist ?? [],
    ops: lists?.ops ?? [],
    'banned-players': lists?.bannedPlayers ?? [],
    'banned-ips': lists?.bannedIps ?? [],
    known: lists?.known ?? [],
  };

  return (
    <div className="mx-auto max-w-[1100px] p-6">
      <PageHeader
        title="Players"
        description={live ? 'Changes go through the server console and apply instantly.' : 'The server is off: lists are edited directly in its JSON files (names are checked against Mojang for their UUID).'}
        actions={
          <IconButton label="Reload lists" onClick={load}>
            <RefreshCw />
          </IconButton>
        }
      />
      <div className="mb-4 flex items-center justify-between gap-3">
        <Segmented
          value={tab}
          onChange={setTab}
          options={[
            { value: 'online', label: <>Online <span className="font-mono text-ok">{st?.players.length ?? 0}</span></>, icon: <Wifi className="size-3.5" /> },
            { value: 'whitelist', label: <>Whitelist <span className="font-mono text-fg-4">{lists?.whitelist.length ?? 0}</span></>, icon: <ListChecks className="size-3.5" /> },
            { value: 'ops', label: <>Operators <span className="font-mono text-fg-4">{lists?.ops.length ?? 0}</span></>, icon: <Crown className="size-3.5" /> },
            { value: 'banned-players', label: <>Bans <span className="font-mono text-fg-4">{lists?.bannedPlayers.length ?? 0}</span></>, icon: <Ban className="size-3.5" /> },
            { value: 'banned-ips', label: <>IP bans <span className="font-mono text-fg-4">{lists?.bannedIps.length ?? 0}</span></>, icon: <Gavel className="size-3.5" /> },
            { value: 'known', label: 'Seen', icon: <History className="size-3.5" /> },
            { value: 'stats', label: 'Stats', icon: <BarChart3 className="size-3.5" /> },
            { value: 'chat', label: <>Chat <span className="font-mono text-fg-4">{chats.length}</span></>, icon: <MessageSquare className="size-3.5" /> },
          ]}
        />
      </div>

      {tab === 'online' &&
        (st?.players.length ? (
          <div className="grid grid-cols-2 gap-3">
            {st.players.map((p) => (
              <Card key={p.name} className="flex items-center gap-3 p-3">
                <PlayerHead name={p.name} uuid={p.uuid} size={44} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13.5px] font-semibold text-fg">{p.name}</span>
                    {isOp(p.name) && (
                      <Badge tone="accent">
                        <Crown /> op
                      </Badge>
                    )}
                  </div>
                  <div className="selectable mt-0.5 truncate font-mono text-[10.5px] text-fg-4">{p.uuid ?? 'uuid unknown'}</div>
                  <div className="mt-0.5 text-[11px] text-fg-3">
                    online {ago(p.joinedAt).replace(' ago', '')}
                    {p.ip && <span className="font-mono"> · {p.ip}</span>}
                  </div>
                </div>
                <Menu
                  width={230}
                  trigger={() => (
                    <IconButton label="Actions" className="border border-line-strong">
                      <MoreHorizontal />
                    </IconButton>
                  )}
                  items={[
                    { label: 'Kick', icon: <LogOut />, onSelect: () => api.kickPlayer(id, p.name, 'Kicked by an operator').catch((e) => toast('error', errMsg(e), id)) },
                    { label: isOp(p.name) ? 'Remove operator' : 'Make operator', icon: <Crown />, onSelect: () => act('ops', !isOp(p.name), p.name) },
                    { label: isWl(p.name) ? 'Remove from whitelist' : 'Add to whitelist', icon: <ListChecks />, onSelect: () => act('whitelist', !isWl(p.name), p.name) },
                    { separator: true, label: '' },
                    { label: 'Survival', onSelect: () => cmd(`gamemode survival ${p.name}`) },
                    { label: 'Creative', onSelect: () => cmd(`gamemode creative ${p.name}`) },
                    { label: 'Spectator', onSelect: () => cmd(`gamemode spectator ${p.name}`) },
                    { label: 'Heal & feed', onSelect: () => cmd(`effect give ${p.name} minecraft:instant_health 1 10`).then(() => cmd(`effect give ${p.name} minecraft:saturation 1 10`)) },
                    { separator: true, label: '' },
                    {
                      label: 'Ban…',
                      icon: <Ban />,
                      danger: true,
                      onSelect: async () => {
                        const r = await confirm({ title: `Ban ${p.name}?`, message: 'They are kicked and can’t rejoin until pardoned.', confirm: 'Ban', danger: true });
                        if (r.ok) act('banned-players', true, p.name, 'Banned by an operator');
                      },
                    },
                    ...(p.ip ? [{ label: `Ban IP ${p.ip}`, icon: <Gavel />, danger: true, onSelect: () => act('banned-ips', true, p.ip!, 'Banned by an operator') }] : []),
                  ]}
                />
              </Card>
            ))}
          </div>
        ) : (
          <Card>
            <Empty icon={<Users />} title={live ? 'Nobody is online' : 'The server is offline'} description={live ? 'Players appear here the moment they join.' : 'Start the server to see who is playing.'} />
          </Card>
        ))}

      {tab === 'whitelist' && (
        <Card className="mb-4 flex items-center gap-4 px-5 py-3.5">
          <ShieldCheck className="size-5 text-accent" />
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-medium text-fg">Whitelist {whitelistOn ? 'on' : 'off'}</div>
            <div className="text-[12px] text-fg-3">{whitelistOn ? 'Only players below can join.' : 'Anyone can join – turn it on for a private server.'}</div>
          </div>
          <Toggle
            checked={whitelistOn}
            onChange={async (v) => {
              patch((d) => {
                d.properties['white-list'] = String(v);
                d.properties['enforce-whitelist'] = String(v);
              });
              if (live) await cmd(`whitelist ${v ? 'on' : 'off'}`);
              setTimeout(save, 0);
            }}
          />
        </Card>
      )}

      {['whitelist', 'ops', 'banned-players', 'banned-ips'].includes(tab) && (
        <Card>
          <div className="flex items-center gap-2 border-b border-line px-4 py-3">
            <UserPlus className="size-4 text-fg-3" />
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={tab === 'banned-ips' ? 'IP address, e.g. 203.0.113.7' : 'Player name'}
              className="w-60"
              onKeyDown={(e) => e.key === 'Enter' && name.trim() && act(tab, true, name.trim(), reason).then(() => setName(''))}
            />
            {tab.startsWith('banned') && <Input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Reason (optional)" className="flex-1" />}
            <Button variant="primary" icon={<Plus className="size-3.5" />} disabled={!name.trim()} loading={busy} onClick={() => act(tab, true, name.trim(), reason).then(() => setName(''))}>
              {tab === 'ops' ? 'Make operator' : tab.startsWith('banned') ? 'Ban' : 'Add'}
            </Button>
          </div>
          {tab === 'ops' && (
            <div className="border-b border-line px-4 py-2 text-[11.5px] text-fg-4">New operators get level {inst.properties['op-permission-level'] ?? '4'} (op-permission-level).</div>
          )}
          {listFor[tab].length === 0 ? (
            <Empty title="Nobody here yet" className="py-10" />
          ) : (
            <div className="divide-y divide-line">
              {listFor[tab].map((e, i) => (
                <div key={i} className="flex items-center gap-3 px-4 py-2.5">
                  {tab !== 'banned-ips' ? <PlayerHead name={e.name} uuid={e.uuid} size={28} /> : <Gavel className="size-5 text-fg-4" />}
                  <div className="min-w-0 flex-1">
                    <div className="text-[12.5px] font-medium text-fg">{e.name ?? e.ip}</div>
                    <div className="selectable truncate font-mono text-[10.5px] text-fg-4">
                      {e.uuid ?? ''}
                      {e.reason ? ` · ${e.reason}` : ''}
                      {e.created ? ` · ${e.created}` : ''}
                    </div>
                  </div>
                  {tab === 'ops' && <Badge tone="accent">level {e.level ?? 4}</Badge>}
                  <IconButton label="Remove" onClick={() => act(tab, false, e.name ?? e.ip)}>
                    {tab.startsWith('banned') ? <ShieldOff /> : <Trash2 />}
                  </IconButton>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}

      {tab === 'known' && (
        <Card>
          {listFor.known.length === 0 ? (
            <Empty icon={<History />} title="No players seen yet" description="Everyone who has joined is remembered here (usercache.json)." />
          ) : (
            <div className="divide-y divide-line">
              {listFor.known.map((e, i) => (
                <div key={i} className="flex items-center gap-3 px-4 py-2.5">
                  <PlayerHead name={e.name} uuid={e.uuid} size={28} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[12.5px] font-medium text-fg">{e.name}</div>
                    <div className="selectable font-mono text-[10.5px] text-fg-4">{e.uuid}</div>
                  </div>
                  {isOp(e.name) && <Badge tone="accent">op</Badge>}
                  {isWl(e.name) && <Badge tone="ok">whitelisted</Badge>}
                  <Button size="xs" variant="outline" onClick={() => act('whitelist', !isWl(e.name), e.name)}>
                    {isWl(e.name) ? 'Unwhitelist' : 'Whitelist'}
                  </Button>
                  <Button size="xs" variant="outline" onClick={() => act('ops', !isOp(e.name), e.name)}>
                    {isOp(e.name) ? 'Deop' : 'Op'}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}

      {tab === 'stats' && <StatsTable id={id} live={live} />}

      {tab === 'chat' && (
        <Card>
          {chats.length === 0 ? (
            <Empty icon={<MessageSquare />} title="No chat yet" description="Chat messages since the manager opened appear here." />
          ) : (
            <div className="max-h-[560px] space-y-0.5 overflow-y-auto p-3">
              {chats.map((c, i) => (
                <div key={i} className="flex items-start gap-2.5 rounded-lg px-2 py-1.5 hover:bg-panel-2">
                  <PlayerHead name={c.name} size={20} className="mt-0.5" />
                  <div className="min-w-0 flex-1 text-[12.5px]">
                    <span className="font-medium text-fg">{c.name}</span> <span className="selectable text-fg-2">{stripFormatting(c.text)}</span>
                  </div>
                  <span className="font-mono text-[10.5px] text-fg-4" title={dateTime(c.ts)}>
                    {clock(c.ts)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
      {!inst.properties['online-mode'] || inst.properties['online-mode'] !== 'false' ? null : (
        <Callout tone="warn" className="mt-4" title="Offline mode">
          Accounts aren’t verified, so names can be impersonated. UUIDs here are derived from names (offline UUIDs).
        </Callout>
      )}
    </div>
  );
}
