import { useEffect, useState, type ReactNode } from 'react';
import { Box, Feather, Flame, Gem, Grid3x3, Hammer, Leaf, Scissors, User } from 'lucide-react';
import { cn } from '@/lib/cn';
import { headUrl, parseFormatted, softwareInfo } from '@/lib/mc';

/** Isometric grass block – the app's logo. */
export function GrassBlock({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 64" className={className} shapeRendering="crispEdges">
      <path d="M32 4 58 17 32 30 6 17Z" fill="#5fbf3a" />
      <path d="M32 4 45 10.5 32 17 19 10.5Z" fill="#79d14c" />
      <path d="M6 17 32 30V60L6 47Z" fill="#8b5a2b" />
      <path d="M58 17 32 30V60L58 47Z" fill="#6e4520" />
      <path d="M6 17 32 30V36L6 23Z" fill="#4a9a2c" />
      <path d="M58 17 32 30V36L58 23Z" fill="#3c8024" />
      <path d="M10 28h4v4h-4zM18 36h4v4h-4zM12 42h4v4h-4zM24 46h4v4h-4z" fill="#a26d3a" opacity=".8" />
      <path d="M42 34h4v4h-4zM50 30h4v4h-4zM38 46h4v4h-4zM48 42h4v4h-4z" fill="#5a3818" opacity=".8" />
    </svg>
  );
}

const SW_ICON: Record<string, typeof Box> = {
  vanilla: Box,
  paper: Feather,
  purpur: Gem,
  folia: Leaf,
  fabric: Scissors,
  quilt: Grid3x3,
  forge: Hammer,
  neoforge: Flame,
};

/** Square software tile (Vanilla grass, Paper, Fabric…). */
export function SoftwareIcon({ software, size = 32, className }: { software: string; size?: number; className?: string }) {
  const s = softwareInfo(software);
  const Icon = SW_ICON[s.id] ?? Box;
  return (
    <span
      className={cn('inline-flex shrink-0 items-center justify-center rounded-lg shadow-[inset_0_1px_0_rgb(255_255_255/0.12),0_0_0_1px_rgb(0_0_0/0.4)]', className)}
      style={{ width: size, height: size, background: s.bg }}
    >
      {s.id === 'vanilla' ? <GrassBlock className="size-[70%]" /> : <Icon style={{ color: s.color, width: size * 0.52, height: size * 0.52 }} />}
    </span>
  );
}

export function SoftwareBadge({ software, version, loader }: { software: string; version?: string; loader?: string }) {
  const s = softwareInfo(software);
  return (
    <span className="inline-flex h-6 items-center gap-1.5 rounded-md border border-line-strong bg-panel-2 pr-2 pl-0.5 text-[11.5px] font-medium text-fg-2">
      <SoftwareIcon software={software} size={20} className="rounded-[5px]" />
      {s.label}
      {version && <span className="font-mono text-fg">{version}</span>}
      {loader && <span className="font-mono text-fg-4">· {loader}</span>}
    </span>
  );
}

/** Render §-coded text like the game does. Obfuscated text shimmers. */
export function FormattedText({ text, base, className }: { text: string; base?: string; className?: string }) {
  const runs = parseFormatted(text, base);
  const [tick, setTick] = useState(0);
  const obf = runs.some((r) => r.obf);
  useEffect(() => {
    if (!obf) return;
    const t = setInterval(() => setTick((x) => x + 1), 90);
    return () => clearInterval(t);
  }, [obf]);
  const scramble = (s: string) => s.replace(/\S/g, () => String.fromCharCode(33 + Math.floor(Math.random() * 90)));
  return (
    <span className={cn('mc-text whitespace-pre-wrap', className)} data-tick={tick}>
      {runs.map((r, i) => (
        <span
          key={i}
          style={{
            color: r.color,
            fontWeight: r.bold ? 700 : undefined,
            fontStyle: r.italic ? 'italic' : undefined,
            textDecoration: [r.underline && 'underline', r.strike && 'line-through'].filter(Boolean).join(' ') || undefined,
          }}
        >
          {r.obf ? scramble(r.text) : r.text}
        </span>
      ))}
    </span>
  );
}

/** A server-list entry, the way players see it in Multiplayer. */
export function ServerListCard({ name, motd, icon, players, max, ping, version }: { name: string; motd: string; icon?: string | null; players?: number; max?: number; ping?: number | null; version?: string }) {
  const lines = motd.split('\n').slice(0, 2);
  const bars = ping == null ? 0 : ping < 150 ? 5 : ping < 300 ? 4 : ping < 600 ? 3 : ping < 1000 ? 2 : 1;
  return (
    <div className="flex items-start gap-3 rounded-lg border border-black/40 bg-black/45 p-2 shadow-[inset_0_0_0_1px_rgb(255_255_255/0.04)]">
      {icon ? (
        <img src={icon} alt="" className="size-16 shrink-0 [image-rendering:pixelated]" />
      ) : (
        <div className="flex size-16 shrink-0 items-center justify-center bg-[#2b2b2b]">
          <GrassBlock className="size-11" />
        </div>
      )}
      <div className="min-w-0 flex-1 leading-[18px]">
        <div className="flex items-center gap-2">
          <span className="mc-text truncate text-[13px] text-white">{name}</span>
          <span className="mc-text ml-auto shrink-0 text-[12px] text-[#AAAAAA]">
            {players ?? 0}
            <span className="text-[#555555]">/</span>
            {max ?? 20}
          </span>
          <span className="flex h-3 shrink-0 items-end gap-[1px]" title={ping != null ? `${ping} ms` : 'Offline'}>
            {[1, 2, 3, 4, 5].map((b) => (
              <span key={b} className="w-[2px]" style={{ height: b * 2 + 2, background: b <= bars ? (bars >= 4 ? '#55FF55' : bars >= 3 ? '#FFFF55' : '#FF5555') : '#333' }} />
            ))}
          </span>
        </div>
        {lines.map((l, i) => (
          <div key={i} className="truncate text-[13px]">
            <FormattedText text={l} />
          </div>
        ))}
        {version && <div className="mc-text truncate text-[11px] text-[#555555]">{version}</div>}
      </div>
    </div>
  );
}

export function PlayerHead({ name, uuid, size = 28, className }: { name: string; uuid?: string | null; size?: number; className?: string }) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return (
      <span className={cn('inline-flex shrink-0 items-center justify-center rounded-md bg-panel-2 text-fg-4', className)} style={{ width: size, height: size }}>
        <User style={{ width: size * 0.6, height: size * 0.6 }} />
      </span>
    );
  }
  return (
    <img
      src={headUrl(uuid || name, size * 2)}
      alt=""
      onError={() => setFailed(true)}
      className={cn('shrink-0 rounded-md [image-rendering:pixelated] shadow-[0_0_0_1px_rgb(0_0_0/0.4)]', className)}
      style={{ width: size, height: size }}
    />
  );
}

/** Small labelled metric inside headers and cards. */
export function Pill({ icon, children, className }: { icon?: ReactNode; children: ReactNode; className?: string }) {
  return <span className={cn('inline-flex items-center gap-1 text-[11.5px] text-fg-3 [&_svg]:size-3', className)}>{icon}{children}</span>;
}

/** TPS colour: green ≥ 19, yellow ≥ 15, red below. */
export function tpsTone(tps: number | null | undefined): string {
  if (tps == null) return 'text-fg-4';
  if (tps >= 19) return 'text-ok';
  if (tps >= 15) return 'text-warn';
  return 'text-err';
}
