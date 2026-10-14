import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { AlertTriangle, Check, CheckCircle2, ChevronDown, Copy, ExternalLink, Globe2, Loader2, Plug, Plus, Radar, Router, ShieldCheck, Trash2, Wand2, Wifi, XCircle } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { EXTRA_PORT_PRESETS, joinAddress, lanAddress, netInfo, serverPorts } from '@/lib/net';
import { browse, copy } from '@/lib/shell';
import type { FirewallState, NetInfo, PingResult, UpnpInfo } from '@/lib/types';
import { useApp, useInstance } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Input, NumberInput, Select, Toggle } from '@/components/ui/Field';
import { Badge, Callout, Card, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';
import { ServerListCard } from '@/components/ui/Mc';

/** Where the setting usually lives. Menus differ between firmware versions, so this is a hint. */
const ROUTERS: { brand: string; path: string }[] = [
  { brand: 'TP-Link', path: 'Advanced → NAT Forwarding → Virtual Servers → Add' },
  { brand: 'Netgear / Orbi', path: 'Advanced → Advanced Setup → Port Forwarding / Port Triggering → Add Custom Service' },
  { brand: 'ASUS', path: 'WAN → Virtual Server / Port Forwarding → Add profile' },
  { brand: 'Linksys', path: 'Security → Apps and Gaming → Single Port Forwarding' },
  { brand: 'Google / Nest Wifi', path: 'Google Home app → Wi-Fi → Settings → Advanced networking → Port management' },
  { brand: 'eero', path: 'eero app → Settings → Network settings → Reservations & port forwarding' },
  { brand: 'Xfinity / Comcast', path: 'Xfinity app → WiFi → View WiFi equipment → Advanced settings → Port forwarding' },
  { brand: 'AT&T', path: 'http://192.168.1.254 → Firewall → NAT/Gaming → Custom Services' },
  { brand: 'Spectrum', path: 'My Spectrum app → Services → Router → Advanced settings → Port forwarding' },
  { brand: 'BT Smart Hub', path: 'Advanced settings → Firewall → Port forwarding' },
  { brand: 'Virgin Media Hub', path: 'Advanced settings → Security → Port forwarding' },
  { brand: 'FRITZ!Box', path: 'Internet → Permit Access → Port Sharing → Add Device for Sharing' },
];

function CopyCell({ value }: { value: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      onClick={() => {
        copy(value);
        setDone(true);
        setTimeout(() => setDone(false), 1100);
      }}
      className="group inline-flex items-center gap-1.5 rounded px-1 font-mono text-fg hover:bg-hover"
      title="Copy"
    >
      {value}
      {done ? <Check className="size-3 text-ok" /> : <Copy className="size-3 text-fg-4 opacity-0 group-hover:opacity-100" />}
    </button>
  );
}

function Step({ n, title, done, children }: { n: number; title: ReactNode; done?: boolean | null; children: ReactNode }) {
  return (
    <div className="flex gap-4 py-4">
      <div
        className={cn(
          'flex size-7 shrink-0 items-center justify-center rounded-full border text-[12px] font-semibold',
          done ? 'border-ok/40 bg-ok/10 text-ok' : 'border-accent/40 bg-accent/10 text-accent',
        )}
      >
        {done ? <Check className="size-3.5" /> : n}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-semibold text-fg">{title}</div>
        <div className="mt-1.5 text-[12.5px] leading-relaxed text-fg-2">{children}</div>
      </div>
    </div>
  );
}

function ReachabilityTest({ id, host, port, lan }: { id: string; host: string; port: number; lan: string | null }) {
  const toast = useApp((s) => s.toast);
  const [ext, setExt] = useState<any | null>(null);
  const [local, setLocal] = useState<PingResult | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const runExt = async () => {
    if (!host) return toast('warn', 'No public address known yet.', id);
    setBusy('ext');
    setExt(null);
    try {
      setExt(await api.externalCheck(host, port));
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
    }
  };
  const runLocal = async () => {
    setBusy('local');
    try {
      setLocal(await api.pingServer(lan ?? '127.0.0.1', port));
    } finally {
      setBusy(null);
    }
  };
  return (
    <Section title="Test reachability" icon={<Radar />} description="Checks whether a player can actually reach the server." bodyClassName="py-4 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="primary" icon={<Globe2 className="size-3.5" />} loading={busy === 'ext'} onClick={runExt}>
          Test from the internet
        </Button>
        <Button icon={<Wifi className="size-3.5" />} loading={busy === 'local'} onClick={runLocal}>
          Test on the LAN
        </Button>
        <span className="text-[11.5px] text-fg-4">The internet test asks mcsrvstat.us to ping {host ? joinAddress(host, port) : 'your public address'} from outside (it sees your IP).</span>
      </div>
      {local && (
        <Callout tone={local.online ? 'ok' : 'err'} title={local.online ? `LAN OK – ${local.latencyMs} ms` : 'Not reachable on the LAN'}>
          {local.online ? `${local.address} answers: ${local.version}, ${local.playersOnline}/${local.playersMax} players.` : `${local.address}: ${local.error}. Is the server running?`}
        </Callout>
      )}
      {ext &&
        (ext.online ? (
          <div className="space-y-2">
            <Callout tone="ok" title="Reachable from the internet">
              Players can join with <span className="font-mono text-fg">{joinAddress(host, port)}</span>.
            </Callout>
            <div className="max-w-[520px]">
              <ServerListCard name={host} motd={(ext.motd?.raw ?? []).join('\n')} icon={ext.icon ?? null} players={ext.players?.online} max={ext.players?.max} ping={50} version={ext.version} />
            </div>
          </div>
        ) : (
          <Callout tone="err" title="Not reachable from the internet">
            mcsrvstat.us couldn’t reach {joinAddress(host, port)}
            {ext.debug?.error?.ping ? ` (${ext.debug.error.ping})` : ''}. Make sure the server is running, the port is forwarded to this PC, Windows Firewall allows it, and your ISP isn’t using CGNAT (see below). Results are cached by the service for ~1 minute.
          </Callout>
        ))}
    </Section>
  );
}

export function PortForwarding({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const toast = useApp((s) => s.toast);
  const setView = useApp((s) => s.setView);
  const { draft, patch } = useDraft(id);
  const [net, setNet] = useState<NetInfo | null>(null);
  const [upnp, setUpnp] = useState<UpnpInfo | null>(null);
  const [fw, setFw] = useState<FirewallState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [routersOpen, setRoutersOpen] = useState(false);

  const ports = serverPorts(inst);
  const lan = lanAddress(inst, net);

  const load = useCallback(async () => {
    netInfo().then(setNet).catch(() => {});
    api.firewallState(id).then(setFw).catch(() => {});
    setUpnp(null);
    api.upnpProbe().then(setUpnp).catch((e) => setUpnp({ found: false, gateway: null, routerName: null, manufacturer: null, model: null, externalIp: null, wanKind: 'unknown', mappings: [], error: errMsg(e) }));
  }, [id]);
  useEffect(() => {
    load();
  }, [load]);

  const act = async (what: 'map' | 'unmap' | 'firewall') => {
    setBusy(what);
    try {
      if (what === 'map') {
        const lines = await api.upnpMap(id);
        toast('success', lines.join(' · '), id);
      } else if (what === 'unmap') {
        await api.upnpUnmap(id);
        toast('success', 'Router port forwards removed', id);
      } else {
        await api.addFirewallRules(id);
        toast('success', 'Windows Firewall now allows the server', id);
      }
      await load();
    } catch (e) {
      toast('error', errMsg(e), id);
    } finally {
      setBusy(null);
    }
  };

  const mapped = (port: number, proto: string) => upnp?.mappings.find((m) => m.externalPort === port && m.protocol === proto);
  const allMappedHere = !!upnp?.found && ports.filter((p) => p.required).every((p) => mapped(p.port, p.protocol)?.internalClient === lan);
  const fwOk = fw ? (fw.portsOk || fw.programRule) && fw.blockedBy.length === 0 : null;
  const gateway = upnp?.gateway ?? net?.gateway ?? null;

  // NAT diagnosis: the router's WAN address vs the address the internet sees.
  let nat: { tone: 'ok' | 'warn' | 'err'; title: string; text: ReactNode } | null = null;
  if (upnp?.found && upnp.externalIp) {
    if (upnp.wanKind === 'cgnat') {
      nat = {
        tone: 'err',
        title: 'Your ISP uses carrier-grade NAT (CGNAT)',
        text: (
          <>
            Your router’s internet address <span className="font-mono">{upnp.externalIp}</span> is shared by the ISP, so port forwarding on your router can’t reach you. Ask your ISP for a public (or static) IPv4 address – many do it for free – or use a tunnel such as playit.gg (free Minecraft tunnels, no forwarding needed) or a VPS.
          </>
        ),
      };
    } else if (upnp.wanKind === 'private') {
      nat = {
        tone: 'warn',
        title: 'Double NAT – there is another router in front of this one',
        text: (
          <>
            This router’s internet address <span className="font-mono">{upnp.externalIp}</span> is a private address, so another device (often the ISP modem) sits in front of it. Either put the ISP modem in <b className="text-fg">bridge / modem mode</b>, or forward the same ports on <b className="text-fg">both</b> devices (the outer one to this router’s address).
          </>
        ),
      };
    } else if (net?.publicIp && upnp.externalIp !== net.publicIp) {
      nat = {
        tone: 'warn',
        title: 'Your public address differs from the router’s',
        text: (
          <>
            The router reports <span className="font-mono">{upnp.externalIp}</span> but the internet sees <span className="font-mono">{net.publicIp}</span> – there is NAT beyond your router (or a VPN is active on this PC). Forwarding may not be enough; check with your ISP and turn off VPNs while hosting.
          </>
        ),
      };
    } else {
      nat = { tone: 'ok', title: 'Direct internet connection', text: <>Your router holds the public address <span className="font-mono">{upnp.externalIp}</span> – port forwarding will work.</> };
    }
  }

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader
        title="Port Forwarding"
        description="Players outside your home network can only join if your router passes the Minecraft port (TCP) to this PC. Friends on the same Wi-Fi don’t need it."
        actions={
          <Button size="sm" icon={busy ? <Loader2 className="size-3.5 animate-spin" /> : <Wifi className="size-3.5" />} onClick={load}>
            Re-check
          </Button>
        }
      />

      <Section title="What to forward" icon={<Globe2 />} description="Java Edition only needs one TCP port. A second server on this PC uses the next one (25566, …). Never forward the RCON port." bodyClassName="py-2">
        <table className="w-full text-left text-[12.5px]">
          <thead className="text-[11px] tracking-wide text-fg-4 uppercase">
            <tr>
              <th className="py-2 font-medium">Service</th>
              <th className="py-2 font-medium">Protocol</th>
              <th className="py-2 font-medium">External & internal port</th>
              <th className="py-2 font-medium">Forward to (this PC)</th>
              <th className="py-2 text-right font-medium">Router status</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {ports.map((p) => {
              const m = mapped(p.port, p.protocol);
              return (
                <tr key={`${p.port}-${p.protocol}`}>
                  <td className="py-3">
                    <div className="font-medium text-fg">
                      {p.label} {!p.required && <span className="text-fg-4">(optional)</span>}
                    </div>
                    <div className="text-[11.5px] text-fg-4">{p.purpose}</div>
                  </td>
                  <td className="py-3">
                    <Badge tone={p.protocol === 'TCP' ? 'accent' : 'info'}>{p.protocol}</Badge>
                  </td>
                  <td className="py-3">
                    <CopyCell value={String(p.port)} />
                  </td>
                  <td className="py-3">{lan ? <CopyCell value={lan} /> : <span className="text-fg-4">detecting…</span>}</td>
                  <td className="py-3 text-right">
                    {!upnp ? (
                      <span className="text-fg-4">…</span>
                    ) : !upnp.found ? (
                      <span className="text-[11.5px] text-fg-4">unknown (no UPnP)</span>
                    ) : m ? (
                      m.internalClient === lan ? (
                        <Badge tone="ok">
                          <CheckCircle2 /> Forwarded here
                        </Badge>
                      ) : (
                        <Badge tone="err" className="max-w-full">
                          <XCircle /> Points to {m.internalClient}
                        </Badge>
                      )
                    ) : (
                      <Badge tone="warn">Not forwarded</Badge>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <div className="border-t border-line py-2.5 text-[11.5px] text-fg-4">
          Use the same number for external and internal port. RCON ({inst.network.rconPort}) stays private – never forward it.
        </div>
      </Section>

      {nat && (
        <Callout tone={nat.tone === 'ok' ? 'ok' : nat.tone === 'warn' ? 'warn' : 'err'} title={nat.title}>
          {nat.text}
        </Callout>
      )}

      <Section
        title={
          <span className="flex items-center gap-2">
            Automatic (UPnP)
            {upnp?.found && <Badge tone="ok">Router found</Badge>}
            {upnp && !upnp.found && <Badge tone="warn">Not available</Badge>}
          </span>
        }
        description="Most home routers can open ports on request. One click, no router login."
        icon={<Wand2 />}
        bodyClassName="py-4 space-y-4"
      >
        {!upnp ? (
          <div className="flex items-center gap-2 text-[12.5px] text-fg-3">
            <Loader2 className="size-4 animate-spin" /> Looking for your router…
          </div>
        ) : upnp.found ? (
          <>
            <div className="grid grid-cols-3 gap-3 text-[12.5px]">
              <div className="rounded-lg border border-line bg-bg-raised px-3 py-2">
                <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">Router</div>
                <div className="truncate text-fg">{[upnp.manufacturer, upnp.routerName ?? upnp.model].filter(Boolean).join(' ') || 'Unknown model'}</div>
              </div>
              <div className="rounded-lg border border-line bg-bg-raised px-3 py-2">
                <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">Router address</div>
                <div className="font-mono text-fg">{upnp.gateway}</div>
              </div>
              <div className="rounded-lg border border-line bg-bg-raised px-3 py-2">
                <div className="text-[10.5px] tracking-wide text-fg-4 uppercase">Internet address</div>
                <div className="font-mono text-fg">{upnp.externalIp ?? '—'}</div>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="primary" icon={<Wand2 className="size-3.5" />} loading={busy === 'map'} onClick={() => act('map')}>
                {allMappedHere ? 'Refresh port forwards' : 'Forward ports automatically'}
              </Button>
              {ports.some((p) => mapped(p.port, p.protocol)) && (
                <Button variant="ghost" loading={busy === 'unmap'} onClick={() => act('unmap')}>
                  Remove forwards
                </Button>
              )}
              <label className="ml-auto flex items-center gap-2.5 text-[12.5px] text-fg-2">
                Re-apply every time the server starts
                <Toggle checked={draft.network.upnp} onChange={(v) => patch((d) => void (d.network.upnp = v))} />
              </label>
            </div>
            {draft.network.upnp !== inst.network.upnp && <div className="text-[11.5px] text-accent">Save (Ctrl+S) to keep this setting.</div>}
          </>
        ) : (
          <Callout tone="warn" title="Your router didn’t answer UPnP">
            {upnp.error ?? 'UPnP may be turned off.'} You can switch UPnP on in the router settings (often under Advanced → UPnP), or forward the ports by hand with the steps below.
          </Callout>
        )}
      </Section>

      <Section title="Manual setup" icon={<Router />} description="If UPnP isn’t available – or you prefer a permanent rule – set it up once in your router." bodyClassName="divide-y divide-line">
        <Step n={1} title="Give this PC a fixed LAN address">
          In the router’s <b className="text-fg">DHCP / LAN</b> settings, reserve <span className="font-mono text-fg">{lan ?? 'this PC’s address'}</span> for this computer (often called “Address reservation” or “Static lease”). Otherwise the forward breaks when the address changes.
        </Step>
        <Step n={2} title="Open your router’s settings">
          <div className="flex flex-wrap items-center gap-2">
            {gateway ? (
              <>
                Go to
                <Button size="sm" icon={<ExternalLink className="size-3.5" />} onClick={() => browse(`http://${gateway}`)}>
                  http://{gateway}
                </Button>
                and sign in (the password is often on a sticker on the router, or use the router’s phone app).
              </>
            ) : (
              <>Open your router’s admin page (usually http://192.168.0.1 or http://192.168.1.1) and sign in.</>
            )}
          </div>
        </Step>
        <Step n={3} title="Add a port-forward rule for each port">
          Look for <b className="text-fg">Port Forwarding</b>, <b className="text-fg">Virtual Server</b>, <b className="text-fg">NAT</b> or <b className="text-fg">Gaming</b>. Create:
          <div className="mt-2 overflow-hidden rounded-lg border border-line">
            <table className="w-full text-[12px]">
              <thead className="bg-bg-raised/60 text-[10.5px] tracking-wide text-fg-4 uppercase">
                <tr>
                  <th className="px-3 py-1.5 text-left font-medium">Name</th>
                  <th className="px-3 py-1.5 text-left font-medium">Protocol</th>
                  <th className="px-3 py-1.5 text-left font-medium">External port</th>
                  <th className="px-3 py-1.5 text-left font-medium">Internal port</th>
                  <th className="px-3 py-1.5 text-left font-medium">Device / IP</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {ports
                  .filter((p) => p.required)
                  .map((p) => (
                    <tr key={`${p.port}-${p.protocol}`}>
                      <td className="px-3 py-1.5">
                        <CopyCell value={`Minecraft ${p.label}`} />
                      </td>
                      <td className="px-3 py-1.5 font-mono">{p.protocol}</td>
                      <td className="px-3 py-1.5">
                        <CopyCell value={String(p.port)} />
                      </td>
                      <td className="px-3 py-1.5">
                        <CopyCell value={String(p.port)} />
                      </td>
                      <td className="px-3 py-1.5">{lan ? <CopyCell value={lan} /> : '—'}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
          <button onClick={() => setRoutersOpen(!routersOpen)} className="mt-3 flex items-center gap-1.5 text-[12px] text-accent hover:underline">
            <ChevronDown className={cn('size-3.5 transition-transform', routersOpen && 'rotate-180')} /> Where is it on my router?
          </button>
          {routersOpen && (
            <div className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1.5 rounded-lg border border-line bg-bg-raised/60 p-3 text-[12px]">
              {ROUTERS.map((r) => (
                <div key={r.brand}>
                  <span className="font-medium text-fg">{r.brand}:</span> <span className="text-fg-3">{r.path}</span>
                </div>
              ))}
              <div className="col-span-2 pt-1 text-[11px] text-fg-4">Menus vary by firmware – if it isn’t there, search your router model + “port forwarding”.</div>
            </div>
          )}
        </Step>
        <Step n={4} title="Allow the server through Windows Firewall" done={fwOk}>
          {fwOk ? (
            'An inbound rule already allows the server.'
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              {fw?.blockedBy.length ? <span className="text-err">A Windows Block rule is stopping the server.</span> : 'No inbound rule yet.'}
              <Button size="sm" variant="primary" icon={<ShieldCheck className="size-3.5" />} loading={busy === 'firewall'} onClick={() => act('firewall')}>
                {fw?.blockedBy.length ? 'Unblock & allow' : 'Allow through firewall'}
              </Button>
            </div>
          )}
        </Step>
        <Step n={5} title="Test it">
          Start the server, then use <b className="text-fg">Test from the internet</b> below, or ask a friend on a <b className="text-fg">different</b> network to add <span className="font-mono text-fg">{net?.publicIp ? joinAddress(inst.network.publicAddress || net.publicIp, inst.network.port) : 'your public IP'}</span> under Multiplayer → Add Server.
          <div className="mt-2 flex items-center gap-1.5 text-[11.5px] text-fg-4">
            <AlertTriangle className="size-3.5" /> Joining your own public address from this PC often fails even when forwarding works (many routers don’t support “NAT loopback”) – use localhost here.
          </div>
        </Step>
      </Section>

      <ReachabilityTest id={id} host={inst.network.publicAddress || net?.publicIp || ''} port={inst.network.port} lan={lan} />

      <Section
        title="Extra ports"
        icon={<Plug />}
        description="Plugins and mods that listen on their own port (Bedrock via Geyser, web maps, voice chat). They are forwarded and allowed together with the game port."
        actions={
          <Select
            value=""
            onChange={(e) => {
              const pr = EXTRA_PORT_PRESETS.find((x) => x.label === e.target.value);
              if (pr) patch((d) => void d.network.extraPorts.push({ port: pr.port, protocol: pr.protocol, label: pr.label }));
            }}
            className="w-56"
          >
            <option value="">+ Add a common port…</option>
            {EXTRA_PORT_PRESETS.map((p) => (
              <option key={p.label} value={p.label}>
                {p.label} ({p.protocol.toUpperCase()} {p.port})
              </option>
            ))}
          </Select>
        }
        bodyClassName="py-3 space-y-2"
      >
        {draft.network.extraPorts.length === 0 && <div className="py-2 text-[12px] text-fg-4">None – Java players only need the game port.</div>}
        {draft.network.extraPorts.map((e, i) => (
          <div key={i} className="flex items-center gap-2">
            <Input value={e.label} onChange={(x) => patch((d) => void (d.network.extraPorts[i].label = x.target.value))} className="flex-1" />
            <NumberInput value={e.port} min={1} max={65535} onChange={(v) => patch((d) => void (d.network.extraPorts[i].port = v))} className="w-36" />
            <Select value={e.protocol} onChange={(x) => patch((d) => void (d.network.extraPorts[i].protocol = x.target.value))} className="w-28">
              <option value="tcp">TCP</option>
              <option value="udp">UDP</option>
              <option value="both">Both</option>
            </Select>
            <IconButton label="Remove" onClick={() => patch((d) => void d.network.extraPorts.splice(i, 1))}>
              <Trash2 />
            </IconButton>
          </div>
        ))}
        <Button size="sm" variant="ghost" icon={<Plus className="size-3.5" />} onClick={() => patch((d) => void d.network.extraPorts.push({ port: 8100, protocol: 'tcp', label: 'Custom' }))}>
          Custom port
        </Button>
        {JSON.stringify(draft.network.extraPorts) !== JSON.stringify(inst.network.extraPorts) && (
          <div className="text-[11.5px] text-accent">Save (Ctrl+S), then re-run “Forward ports” and “Allow through firewall” so the new ports are included.</div>
        )}
      </Section>

      <Card className="flex items-center justify-between gap-4 p-4 text-[12.5px] text-fg-3">
        Changing the game port or RCON? That’s under Server Settings → Network.
        <Button size="sm" onClick={() => setView(id, 'settings')}>
          Server Settings
        </Button>
      </Card>
    </div>
  );
}
