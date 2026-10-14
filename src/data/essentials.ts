// Hand-picked server add-ons, installed from Modrinth in one click. Each entry lists the server
// software it works on (checked against the project's Modrinth loaders, October 2026).
import type { ExtraPort } from '@/lib/types';

export interface Essential {
  /** Modrinth project id (matches `SourceInfo.projectId` once installed). */
  id: string;
  slug: string;
  title: string;
  blurb: string;
  icon: string;
  category: 'performance' | 'admin' | 'world' | 'social';
  software: string[];
  /** Port the add-on opens – added to the forwarded ports on install. */
  port?: ExtraPort;
  /** One-line hint shown after installing. */
  after?: string;
}

const MODS = ['fabric', 'quilt', 'forge', 'neoforge'];
const PLUGINS = ['paper', 'purpur', 'folia'];
const icon = (id: string, file: string) => `https://cdn.modrinth.com/data/${id}/${file}`;

export const ESSENTIALS: Essential[] = [
  {
    id: 'gvQqBUqZ',
    slug: 'lithium',
    title: 'Lithium',
    blurb: 'Faster game logic – mobs, redstone, chunk ticking. Vanilla behaviour, fewer lag spikes.',
    icon: icon('gvQqBUqZ', 'bcc8686c13af0143adf4285d741256af824f70b7_96.webp'),
    category: 'performance',
    software: ['fabric', 'quilt', 'neoforge'],
  },
  {
    id: 'uXXizFIs',
    slug: 'ferrite-core',
    title: 'FerriteCore',
    blurb: 'Cuts memory use – a big help for modpacks on smaller PCs.',
    icon: icon('uXXizFIs', '222a126f26f8f9ae1eb339f3b767677f18bff31f_96.webp'),
    category: 'performance',
    software: MODS,
  },
  {
    id: 'nmDcB62a',
    slug: 'modernfix',
    title: 'ModernFix',
    blurb: 'Faster startup, less memory and dozens of bug fixes for modded servers.',
    icon: icon('nmDcB62a', '2af94de5e08ae54567ee86b968fc7ce076d9fee5_96.webp'),
    category: 'performance',
    software: MODS,
  },
  {
    id: 'fQEb0iXm',
    slug: 'krypton',
    title: 'Krypton',
    blurb: 'Optimised networking stack – less bandwidth and CPU per player.',
    icon: icon('fQEb0iXm', '3ea60899d060a9286e03b87bfa9e71d0cbe2dde7_96.webp'),
    category: 'performance',
    software: ['fabric', 'quilt'],
  },
  {
    id: 'l6YH9Als',
    slug: 'spark',
    title: 'spark',
    blurb: 'Profiler: find what causes lag with /spark profiler. (Paper and Purpur have it built in.)',
    icon: icon('l6YH9Als', '61a777dd08a8447ac93e8b6372e6d27d48cd1e1a_96.webp'),
    category: 'performance',
    software: MODS,
  },
  {
    id: 'fALzjamp',
    slug: 'chunky',
    title: 'Chunky',
    blurb: 'Pre-generates the world so exploring never lags. Start it from Worlds → Pre-generate.',
    icon: icon('fALzjamp', 'e1954413665e57b7bae1feef44eda530270c7d47_96.webp'),
    category: 'world',
    software: [...MODS, ...PLUGINS],
    after: 'Restart, then use Worlds → Pre-generate.',
  },
  {
    id: 'swbUV1cr',
    slug: 'bluemap',
    title: 'BlueMap',
    blurb: '3D web map of your world in the browser at http://localhost:8100.',
    icon: icon('swbUV1cr', 'b9a17ab8c850046101a47e62037225c9b007a2ca_96.webp'),
    category: 'world',
    software: [...MODS, ...PLUGINS],
    port: { port: 8100, protocol: 'tcp', label: 'BlueMap web map' },
    after: 'After the first start set accept-download: true in BlueMap’s core.conf (Config Files).',
  },
  {
    id: 'Vebnzrzj',
    slug: 'luckperms',
    title: 'LuckPerms',
    blurb: 'Permissions and ranks – who may use which command. Web editor with /lp editor.',
    icon: icon('Vebnzrzj', '90943902cc650e95b167265b8f2d7c893f82c8f4_96.webp'),
    category: 'admin',
    software: [...MODS, ...PLUGINS],
  },
  {
    id: 'hXiIvTyT',
    slug: 'essentialsx',
    title: 'EssentialsX',
    blurb: 'Homes, warps, /tpa, kits, nicknames, economy – the classic server toolbox.',
    icon: icon('hXiIvTyT', 'e621675be1d0421b43b65ab8082507532d937009_96.webp'),
    category: 'admin',
    software: ['paper', 'purpur'],
  },
  {
    id: 'P1OZGk5p',
    slug: 'viaversion',
    title: 'ViaVersion',
    blurb: 'Lets players on newer Minecraft versions join without waiting for an update.',
    icon: icon('P1OZGk5p', 'ad14260a7308dc9e4c3385f3f6b5bdabfe17f295_96.webp'),
    category: 'social',
    software: PLUGINS,
  },
  {
    id: '9eGKb6K1',
    slug: 'simple-voice-chat',
    title: 'Simple Voice Chat',
    blurb: 'Proximity voice chat. Players install the same mod on their client.',
    icon: icon('9eGKb6K1', 'icon.png'),
    category: 'social',
    software: [...MODS, ...PLUGINS],
    port: { port: 24454, protocol: 'udp', label: 'Simple Voice Chat' },
  },
];

export const ESSENTIAL_CATEGORIES: { id: Essential['category']; label: string }[] = [
  { id: 'performance', label: 'Performance' },
  { id: 'world', label: 'World & maps' },
  { id: 'admin', label: 'Administration' },
  { id: 'social', label: 'Players' },
];

/** Bedrock crossplay (Geyser + Floodgate) works on these. */
export const BEDROCK_SOFTWARE = ['paper', 'purpur', 'fabric', 'quilt', 'neoforge'];
