// Shared network helpers: the ports a server needs and a short-lived cache for slow lookups.
import { api } from './ipc';
import type { NetInfo, ServerInstance } from './types';

export interface PortRow {
  port: number;
  protocol: 'TCP' | 'UDP';
  label: string;
  purpose: string;
  required: boolean;
}

/** Every port players need – what to forward on the router and allow through the firewall. */
export function serverPorts(inst: ServerInstance): PortRow[] {
  const n = inst.network;
  const rows: PortRow[] = [{ port: n.port, protocol: 'TCP', label: 'Minecraft', purpose: 'Player connections (Java Edition only uses TCP)', required: true }];
  if (n.queryEnabled) rows.push({ port: n.queryPort || n.port, protocol: 'UDP', label: 'Query', purpose: 'GameSpy4 query used by some server lists', required: false });
  for (const e of n.extraPorts) {
    if (!e.port) continue;
    const protos: ('TCP' | 'UDP')[] = e.protocol === 'both' ? ['TCP', 'UDP'] : [e.protocol === 'tcp' ? 'TCP' : 'UDP'];
    for (const p of protos) rows.push({ port: e.port, protocol: p, label: e.label || 'Extra', purpose: 'Plugin / mod port', required: true });
  }
  return rows;
}

/** Common add-on ports people forward next to the game port. */
export const EXTRA_PORT_PRESETS: { label: string; port: number; protocol: 'tcp' | 'udp' | 'both' }[] = [
  { label: 'Geyser (Bedrock players)', port: 19132, protocol: 'udp' },
  { label: 'Simple Voice Chat', port: 24454, protocol: 'udp' },
  { label: 'BlueMap web map', port: 8100, protocol: 'tcp' },
  { label: 'Dynmap web map', port: 8123, protocol: 'tcp' },
  { label: 'Squaremap web map', port: 8080, protocol: 'tcp' },
  { label: 'Plan analytics', port: 8804, protocol: 'tcp' },
  { label: 'Votifier', port: 8192, protocol: 'tcp' },
];

let netCache: { at: number; value: Promise<NetInfo> } | null = null;

/** Public IP + LAN addresses + gateway, cached for 5 minutes (the public IP lookup is an HTTP call). */
export function netInfo(force = false): Promise<NetInfo> {
  if (!force && netCache && Date.now() - netCache.at < 300_000) return netCache.value;
  const value = api.netInfo().catch((e) => {
    netCache = null;
    throw e;
  });
  netCache = { at: Date.now(), value };
  return value;
}

/** The LAN address the router should forward to. */
export function lanAddress(inst: ServerInstance, net: NetInfo | null): string | null {
  const b = inst.network.bindIp.trim();
  if (b && b !== '0.0.0.0') return b;
  return net?.local[0]?.ip ?? null;
}

/** "play.example.com" or "203.0.113.5:25570" – port omitted when it's the default. */
export function joinAddress(host: string, port: number): string {
  return port === 25565 ? host : `${host}:${port}`;
}
