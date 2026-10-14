import { useEffect, useMemo, useRef, useState } from 'react';
import { ExternalLink, FileSignature, Hash, ImagePlus, Network, Palette, Search, ShieldAlert, SlidersHorizontal, Sparkles, Trash2, Type } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { SERVER_COLORS } from '@/lib/colors';
import { MC_COLORS, ampToSection } from '@/lib/mc';
import { browse, pickFile } from '@/lib/shell';
import { NETWORK_KEYS, PROPERTIES, PROPERTY_GROUPS, type PropDef } from '@/data/properties';
import { useApp, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Badge, Callout, Card, PageHeader, Section } from '@/components/ui/Surface';
import { FieldRow, Input, NumberInput, SecretInput, Select, TextArea, Toggle } from '@/components/ui/Field';
import { ServerListCard } from '@/components/ui/Mc';
import { useDraft } from '../draft';

function PropControl({ def, value, onChange }: { def: PropDef; value: string; onChange: (v: string) => void }) {
  switch (def.type) {
    case 'bool':
      return <Toggle checked={value === 'true'} onChange={(v) => onChange(String(v))} />;
    case 'int':
      return <NumberInput value={Number(value || def.def) || 0} min={def.min} max={def.max} onChange={(v) => onChange(String(Math.round(v)))} />;
    case 'enum':
      return (
        <Select value={value || def.def} onChange={(e) => onChange(e.target.value)}>
          {def.options!.map((o) => (
            <option key={o} value={o}>
              {o.replace('minecraft:', '')}
            </option>
          ))}
        </Select>
      );
    default:
      return <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder={def.def || '(empty)'} mono={def.key !== 'motd'} />;
  }
}

/** Scale any picked image to the 64×64 PNG the server wants. */
async function toIconPng(file: string): Promise<string> {
  const src = await api.readImage(file);
  const img = new Image();
  img.crossOrigin = 'anonymous';
  await new Promise((ok, bad) => {
    img.onload = ok;
    img.onerror = () => bad('That image could not be read.');
    img.src = src;
  });
  const c = document.createElement('canvas');
  c.width = 64;
  c.height = 64;
  const ctx = c.getContext('2d')!;
  ctx.imageSmoothingEnabled = img.width > 64;
  const s = Math.min(img.width, img.height);
  ctx.drawImage(img, (img.width - s) / 2, (img.height - s) / 2, s, s, 0, 0, 64, 64);
  return c.toDataURL('image/png');
}

function MotdEditor({ value, onChange, name, icon }: { value: string; onChange: (v: string) => void; name: string; icon: string | null }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const insert = (code: string) => {
    const el = ref.current;
    if (!el) return onChange(value + code);
    const a = el.selectionStart;
    const b = el.selectionEnd;
    const next = value.slice(0, a) + code + value.slice(b);
    onChange(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(a + code.length, a + code.length);
    });
  };
  const lines = value.split('\n');
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-5 py-4">
      <div>
        <TextArea
          ref={ref}
          value={value}
          onChange={(e) => onChange(ampToSection(e.target.value).split('\n').slice(0, 2).join('\n'))}
          rows={2}
          className="min-h-0 font-mono text-[12.5px]"
          placeholder="A Minecraft Server"
        />
        <div className="mt-2 flex flex-wrap gap-1">
          {Object.entries(MC_COLORS).map(([k, c]) => (
            <button key={k} title={`${c.name} (§${k})`} onClick={() => insert(`§${k}`)} className="size-5 rounded border border-black/40 shadow-[inset_0_1px_0_rgb(255_255_255/0.25)]" style={{ background: c.hex }} />
          ))}
        </div>
        <div className="mt-1.5 flex gap-1">
          {[
            ['l', 'B', 'font-bold'],
            ['o', 'I', 'italic'],
            ['n', 'U', 'underline'],
            ['m', 'S', 'line-through'],
            ['k', '?', ''],
            ['r', 'Reset', ''],
          ].map(([k, l, c]) => (
            <button key={k} onClick={() => insert(`§${k}`)} className={cn('h-6 min-w-6 rounded border border-line-strong bg-panel-2 px-1.5 text-[11px] text-fg-2 hover:text-fg', c)} title={`§${k}`}>
              {l}
            </button>
          ))}
          <button onClick={() => insert('\n')} disabled={lines.length >= 2} className="h-6 rounded border border-line-strong bg-panel-2 px-1.5 text-[11px] text-fg-2 hover:text-fg disabled:opacity-40">
            New line
          </button>
        </div>
        <div className="mt-2 text-[11px] text-fg-4">Two lines, up to ~45 characters each. Type &amp;a style codes or click a colour.</div>
      </div>
      <div>
        <div className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">Preview</div>
        <div className="rounded-xl bg-[url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%2216%22 height=%2216%22><rect width=%228%22 height=%228%22 fill=%22%23362a1f%22/><rect x=%228%22 y=%228%22 width=%228%22 height=%228%22 fill=%22%23362a1f%22/><rect x=%228%22 width=%228%22 height=%228%22 fill=%22%232e2319%22/><rect y=%228%22 width=%228%22 height=%228%22 fill=%22%232e2319%22/></svg>')] p-3">
          <ServerListCard name={name} motd={value || 'A Minecraft Server'} icon={icon} players={3} max={20} ping={40} />
        </div>
      </div>
    </div>
  );
}

export function Settings({ id }: { id: string }) {
  const { draft, patch } = useDraft(id);
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [q, setQ] = useState('');
  const [showAll, setShowAll] = useState(false);
  const [icon, setIcon] = useState<string | null>(null);
  useEffect(() => {
    api.getServerIcon(id).then(setIcon).catch(() => {});
  }, [id]);
  const p = draft.properties;
  const n = draft.network;
  const setProp = (k: string, v: string) => patch((d) => void (d.properties[k] = v));
  const running = st?.state === 'running';

  const known = new Set(PROPERTIES.map((x) => x.key));
  const others = Object.keys(p).filter((k) => !known.has(k) && !NETWORK_KEYS.includes(k)).sort();
  const filtered = useMemo(() => {
    const s = q.toLowerCase();
    return PROPERTIES.filter((d) => d.key !== 'motd' && (s ? (d.key + d.label + d.desc).toLowerCase().includes(s) : showAll || d.hot));
  }, [q, showAll]);

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader
        title="Server Settings"
        description={running ? 'Changes are written to server.properties right away and apply on the next restart.' : 'Saved into server.properties (comments and unknown keys are kept).'}
      />

      {!draft.eulaAccepted && (
        <Card className="relative overflow-hidden border-warn/40">
          <div className="flex items-start gap-4 p-5">
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-warn/15 text-warn">
              <FileSignature className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-[14px] font-semibold text-fg">Accept the Minecraft EULA</div>
              <div className="mt-1 text-[12.5px] leading-relaxed text-fg-3">
                Every Minecraft server must agree to Mojang’s End User License Agreement before it starts. Read it, then tick the box – the manager writes <span className="font-mono">eula=true</span> for you.
              </div>
              <div className="mt-3 flex items-center gap-2">
                <Button variant="outline" size="sm" icon={<ExternalLink className="size-3.5" />} onClick={() => browse('https://aka.ms/MinecraftEULA')}>
                  Read the EULA
                </Button>
                <Button variant="primary" size="sm" onClick={() => patch((d) => void (d.eulaAccepted = true))}>
                  I agree to the EULA
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )}

      <Section title="Identity" icon={<Hash />} description="How the server appears in the manager.">
        <FieldRow label="Name">
          <Input value={draft.name} onChange={(e) => patch((d) => void (d.name = e.target.value))} />
        </FieldRow>
        <FieldRow label="Colour">
          <div className="flex flex-wrap justify-end gap-1.5">
            {Object.entries(SERVER_COLORS).map(([k, c]) => (
              <button
                key={k}
                title={c.label}
                onClick={() => patch((d) => void (d.color = k))}
                className={cn('size-6 rounded-full border-2 transition-transform', draft.color === k ? 'scale-110 border-white' : 'border-transparent')}
                style={{ background: c.hex }}
              />
            ))}
          </div>
        </FieldRow>
        <FieldRow label="Notes" description="Anything you want to remember about this server." stacked>
          <TextArea value={draft.notes} onChange={(e) => patch((d) => void (d.notes = e.target.value))} placeholder="Seed spot, rules, who has access…" />
        </FieldRow>
        {draft.eulaAccepted && (
          <FieldRow label="Minecraft EULA" description="Accepted – eula.txt is written on start.">
            <Toggle checked={draft.eulaAccepted} onChange={(v) => patch((d) => void (d.eulaAccepted = v))} />
          </FieldRow>
        )}
      </Section>

      <Section
        title="Server list look"
        icon={<Palette />}
        description="The message of the day and icon players see in Multiplayer."
        actions={
          <div className="flex gap-1.5">
            <Button
              size="sm"
              icon={<ImagePlus className="size-3.5" />}
              onClick={async () => {
                const f = await pickFile('Pick an image for the server icon', ['png', 'jpg', 'jpeg', 'gif', 'webp'], 'Images');
                if (!f) return;
                try {
                  const png = await toIconPng(f);
                  await api.setServerIcon(id, png);
                  setIcon(png);
                  toast('success', 'Server icon saved (64×64) – shows after the next start', id);
                } catch (e) {
                  toast('error', errMsg(e), id);
                }
              }}
            >
              Icon
            </Button>
            {icon && (
              <Button size="sm" variant="ghost" icon={<Trash2 className="size-3.5" />} onClick={() => api.setServerIcon(id, '').then(() => setIcon(null))}>
                Remove
              </Button>
            )}
          </div>
        }
      >
        <MotdEditor value={p['motd'] ?? ''} onChange={(v) => setProp('motd', v)} name={draft.name} icon={icon} />
      </Section>

      <Section title="Network" icon={<Network />} description="Ports and remote console. Forwarding and firewall live under Port Forwarding.">
        <FieldRow label="Server port (TCP)" description="Players connect here. Default 25565 – then they can leave the port off the address.">
          <NumberInput value={n.port} min={1} max={65535} onChange={(v) => patch((d) => void (d.network.port = v))} />
        </FieldRow>
        <FieldRow label="Bind address" description="Only listen on one network adapter (server-ip). Leave empty for all.">
          <Input mono value={n.bindIp} onChange={(e) => patch((d) => void (d.network.bindIp = e.target.value))} placeholder="all interfaces" />
        </FieldRow>
        <FieldRow label="Public address" description="A domain or IP you share with players – shown on the Overview. Empty = your detected public IP.">
          <Input mono value={n.publicAddress} onChange={(e) => patch((d) => void (d.network.publicAddress = e.target.value))} placeholder="play.example.com" />
        </FieldRow>
        <FieldRow
          label="RCON"
          badge={<Badge tone="accent">recommended</Badge>}
          description="Remote console the manager uses for player lists, TPS and commands when it didn’t launch the server itself. Never forward this port."
        >
          <Toggle checked={n.rconEnabled} onChange={(v) => patch((d) => void (d.network.rconEnabled = v))} />
        </FieldRow>
        {n.rconEnabled && (
          <>
            <FieldRow label="RCON port">
              <NumberInput value={n.rconPort} min={1} max={65535} onChange={(v) => patch((d) => void (d.network.rconPort = v))} />
            </FieldRow>
            <FieldRow label="RCON password">
              <SecretInput value={n.rconPassword} onChange={(v) => patch((d) => void (d.network.rconPassword = v))} generate invalid={!n.rconPassword} />
            </FieldRow>
          </>
        )}
        <FieldRow label="Query (GameSpy4)" description="UDP status protocol some server lists use.">
          <Toggle checked={n.queryEnabled} onChange={(v) => patch((d) => void (d.network.queryEnabled = v))} />
        </FieldRow>
        {n.queryEnabled && (
          <FieldRow label="Query port (UDP)">
            <NumberInput value={n.queryPort || n.port} min={1} max={65535} onChange={(v) => patch((d) => void (d.network.queryPort = v))} />
          </FieldRow>
        )}
      </Section>

      <Section
        title="server.properties"
        icon={<SlidersHorizontal />}
        description={showAll || q ? `All ${PROPERTIES.length} documented settings.` : 'The settings most servers change. Show everything for the full list.'}
        actions={
          <>
            <div className="relative w-52">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
              <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search settings…" className="pl-8" />
            </div>
            <Button size="sm" variant={showAll ? 'secondary' : 'outline'} onClick={() => setShowAll(!showAll)}>
              {showAll ? 'Essentials' : 'Show all'}
            </Button>
          </>
        }
      >
        {PROPERTY_GROUPS.map((g) => {
          const defs = filtered.filter((d) => d.group === g);
          if (!defs.length) return null;
          return (
            <div key={g} className="border-b border-line last:border-0">
              <div className="pt-4 text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">{g}</div>
              <div className="divide-y divide-line/60">
                {defs.map((d) => {
                  const v = p[d.key] ?? d.def;
                  const changed = p[d.key] != null && p[d.key] !== d.def;
                  return (
                    <FieldRow
                      key={d.key}
                      label={d.label}
                      badge={<span className="font-mono text-[10.5px] font-normal text-fg-4">{d.key}</span>}
                      description={
                        <>
                          {d.desc}
                          {changed && (
                            <button className="ml-1.5 text-accent hover:underline" onClick={() => setProp(d.key, d.def)}>
                              reset to {d.def || 'empty'}
                            </button>
                          )}
                        </>
                      }
                    >
                      <PropControl def={d} value={v} onChange={(x) => setProp(d.key, x)} />
                    </FieldRow>
                  );
                })}
              </div>
            </div>
          );
        })}
        {(showAll || q) && others.length > 0 && (
          <div>
            <div className="pt-4 text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">Other keys in the file</div>
            {others
              .filter((k) => !q || k.includes(q.toLowerCase()))
              .map((k) => (
                <FieldRow key={k} label={<span className="font-mono text-[12px]">{k}</span>}>
                  <Input mono value={p[k]} onChange={(e) => setProp(k, e.target.value)} />
                </FieldRow>
              ))}
          </div>
        )}
      </Section>

      {p['online-mode'] === 'false' && (
        <Callout tone="warn" title="Offline mode is on">
          <span className="flex items-center gap-1.5">
            <ShieldAlert className="size-3.5" /> Anyone can join as any name – including yours. Use it only behind Velocity/BungeeCord or on a LAN, and keep the whitelist on.
          </span>
        </Callout>
      )}
      <div className="flex items-center gap-2 text-[11.5px] text-fg-4">
        <Type className="size-3.5" /> Tip: gamerules (keepInventory, playersSleepingPercentage…) live in the world – set them from the Console with <span className="font-mono">gamerule</span>.
        <Sparkles className="ml-auto size-3.5" /> Ctrl+S saves.
      </div>
    </div>
  );
}
