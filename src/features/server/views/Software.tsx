import { useState } from 'react';
import { ArrowUpCircle, Coffee, Download, ExternalLink, FileBox, Layers, RefreshCw, TriangleAlert } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { dateTime } from '@/lib/format';
import { softwareInfo } from '@/lib/mc';
import { browse } from '@/lib/shell';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Callout, Card, PageHeader, Section } from '@/components/ui/Surface';
import { confirm } from '@/components/ui/Dialog';
import { SoftwareIcon } from '@/components/ui/Mc';
import { SoftwareGrid, VersionPicker, type Pick } from '../VersionPicker';

export function Software({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const cur = inst.installed;
  const [pick, setPick] = useState<Pick>({ software: inst.software, mc: inst.mcVersion, loader: inst.loaderVersion });
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const busyState = st?.state === 'installing' || st?.state === 'updating';
  const info = softwareInfo(cur?.software ?? inst.software);
  const target = softwareInfo(pick.software);
  const familyChange = cur && softwareInfo(cur.software).content !== target.content;
  const same = cur && cur.software === pick.software && cur.mcVersion === pick.mc && (!pick.loader || pick.loader === cur.loaderVersion);

  const apply = async (p: Pick, allowDowngrade = false) => {
    if (!allowDowngrade) {
      const words = cur ? `switch ${inst.name} from ${softwareInfo(cur.software).label} ${cur.mcVersion} to ${softwareInfo(p.software).label} ${p.mc}` : `install ${softwareInfo(p.software).label} ${p.mc}`;
      const r = await confirm({
        title: cur ? 'Change version?' : 'Install server?',
        message: (
          <>
            This will {words}
            {p.loader ? ` (${p.loader})` : ''}.{' '}
            {cur && st?.state === 'running' ? 'The server is stopped, backed up, updated and started again. ' : ''}
            {cur ? 'Worlds are upgraded when the new version first loads them – that can’t be undone, which is why a backup is made first.' : ''}
          </>
        ),
        confirm: cur ? 'Switch version' : 'Install',
      });
      if (!r.ok) return;
    }
    setBusy(true);
    try {
      await api.changeVersion(id, p.software, p.mc, p.loader, allowDowngrade);
    } catch (e) {
      const msg = errMsg(e);
      if (msg.includes('older than')) {
        const r = await confirm({ title: 'Downgrade the world?', message: msg, confirm: 'Downgrade anyway', danger: true });
        if (r.ok) await apply(p, true);
      } else toast('error', msg, id);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-[1060px] space-y-5 p-6 pb-24">
      <PageHeader title="Version & Software" description="Every official Minecraft release and snapshot, plus Paper, Purpur, Folia, Fabric, Quilt, Forge and NeoForge – pulled live from their official sources, so new versions appear the day they ship." />

      <Card className="relative overflow-hidden">
        <div className="pixel-dirt pointer-events-none absolute inset-0" />
        <div className="relative flex items-center gap-5 p-5">
          <SoftwareIcon software={cur?.software ?? inst.software} size={64} className="rounded-2xl" />
          <div className="min-w-0 flex-1">
            <div className="text-[10.5px] font-semibold tracking-[0.12em] text-fg-4 uppercase">{cur ? 'Installed' : 'Not installed yet'}</div>
            <div className="mt-0.5 flex items-baseline gap-2">
              <span className="text-[22px] font-semibold tracking-tight text-fg">{info.label}</span>
              <span className="font-mono text-[20px] text-accent">{cur?.mcVersion ?? inst.mcVersion}</span>
              {cur?.loaderVersion && <span className="font-mono text-[13px] text-fg-3">{['paper', 'purpur', 'folia'].includes(cur.software) ? `build ${cur.loaderVersion}` : cur.loaderVersion}</span>}
            </div>
            {cur && (
              <div className="mt-1 flex flex-wrap gap-x-4 text-[11.5px] text-fg-3">
                <span className="flex items-center gap-1">
                  <Coffee className="size-3" /> Java {cur.javaMajor}
                </span>
                <span className="flex items-center gap-1">
                  <FileBox className="size-3" /> <span className="font-mono">{cur.target.path}</span>
                </span>
                {cur.installedAt > 0 && <span>installed {dateTime(cur.installedAt)}</span>}
              </div>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <Button
              size="sm"
              icon={<RefreshCw className="size-3.5" />}
              loading={checking}
              disabled={!cur}
              onClick={async () => {
                setChecking(true);
                try {
                  const u = await api.checkUpdate(id);
                  toast(u ? 'info' : 'success', u ? u.label : `${info.label} ${cur?.mcVersion} is up to date`, id);
                } catch (e) {
                  toast('error', errMsg(e), id);
                } finally {
                  setChecking(false);
                }
              }}
            >
              Check for updates
            </Button>
            <Button size="sm" variant="ghost" icon={<ExternalLink className="size-3.5" />} onClick={() => browse(info.site)}>
              {info.label} website
            </Button>
          </div>
        </div>
        {st?.update && (
          <div className="relative flex items-center gap-3 border-t border-line bg-info/6 px-5 py-3">
            <ArrowUpCircle className="size-4 text-info" />
            <div className="flex-1 text-[12.5px] text-fg-2">{st.update.label}</div>
            <Button
              size="sm"
              variant="primary"
              loading={busy || busyState}
              onClick={() =>
                apply(
                  st.update!.kind === 'build'
                    ? { software: cur!.software, mc: cur!.mcVersion, loader: st.update!.latest }
                    : { software: cur!.software, mc: st.update!.latest, loader: '' },
                )
              }
            >
              {st.update.kind === 'build' ? 'Install update' : `Upgrade to ${st.update.latest}`}
            </Button>
          </div>
        )}
      </Card>

      <Section title={cur ? 'Change software or version' : 'Choose what to install'} icon={<Layers />} description="Pick a server type, then the Minecraft version and loader/build.">
        <div className="space-y-4 py-4">
          <SoftwareGrid value={pick.software} onChange={(s) => setPick({ software: s, mc: pick.mc, loader: '' })} />
          <div className="rounded-xl border border-line bg-panel-2/40 px-3.5 py-2.5 text-[12px] leading-relaxed text-fg-3">{target.description}</div>
          <VersionPicker value={pick} onChange={setPick} current={cur?.software === pick.software ? cur.mcVersion : undefined} />
          {familyChange && (
            <Callout tone="warn" title={`Switching from ${softwareInfo(cur!.software).label} to ${target.label}`}>
              {target.content === 'none'
                ? 'Vanilla ignores mods and plugins – they stay on disk but won’t load.'
                : `${target.label} loads ${target.content}, not ${softwareInfo(cur!.software).content}. Your current ${softwareInfo(cur!.software).content} stay on disk but won’t load.`}{' '}
              Worlds carry over.
            </Callout>
          )}
          <div className="flex items-center justify-end gap-2">
            {same && <span className="mr-auto flex items-center gap-1.5 text-[12px] text-fg-4">This is what’s installed – reinstalling repairs missing files.</span>}
            {!same && cur && pick.mc && cur.mcVersion !== pick.mc && (
              <span className="mr-auto flex items-center gap-1.5 text-[12px] text-warn">
                <TriangleAlert className="size-3.5" /> Make sure your mods/plugins support {pick.mc}.
              </span>
            )}
            <Button variant="primary" size="lg" icon={<Download className="size-4" />} loading={busy || busyState} disabled={!pick.mc} onClick={() => apply(pick)}>
              {!cur ? 'Install' : same ? 'Reinstall' : 'Switch version'}
            </Button>
          </div>
        </div>
      </Section>
    </div>
  );
}
