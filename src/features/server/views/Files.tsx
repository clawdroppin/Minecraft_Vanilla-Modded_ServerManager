import { useEffect, useState } from 'react';
import { Archive, Bomb, Download, FileCode2, FolderOpen, Globe2, HardDrive, Library, Puzzle, ScrollText, Trash2, Upload } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { ago, bytes, dateTime } from '@/lib/format';
import { pickFolder, reveal } from '@/lib/shell';
import type { DiskUsage, LogFile } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { FieldRow, Input } from '@/components/ui/Field';
import { Badge, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';

export function Files({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const { draft, patch } = useDraft(id);
  const toast = useApp((s) => s.toast);
  const setDialog = useApp((s) => s.setDialog);
  const [usage, setUsage] = useState<DiskUsage | null>(null);
  const [backupDir, setBackupDir] = useState('');
  const [logs, setLogs] = useState<LogFile[]>([]);
  const [viewer, setViewer] = useState<{ name: string; text: string } | null>(null);
  const live = st?.state === 'running' || st?.state === 'starting' || st?.state === 'stopping';

  useEffect(() => {
    api.diskUsage(id).then(setUsage).catch(() => {});
    api.backupDir(id).then(setBackupDir).catch(() => {});
    api.logFiles(id).then(setLogs).catch(() => {});
  }, [id, st?.state]);

  const d = inst.installDir;
  const contentDir = inst.installed && ['paper', 'purpur', 'folia'].includes(inst.installed.software) ? 'plugins' : 'mods';
  const folders = [
    { label: 'Server folder', path: d, icon: HardDrive },
    { label: 'Active world', path: `${d}\\${inst.properties['level-name'] || 'world'}`, icon: Globe2 },
    { label: contentDir === 'plugins' ? 'Plugins' : 'Mods', path: `${d}\\${contentDir}`, icon: Puzzle },
    { label: 'Config', path: `${d}\\config`, icon: FileCode2 },
    { label: 'Logs', path: `${d}\\logs`, icon: ScrollText },
    { label: 'Crash reports', path: `${d}\\crash-reports`, icon: Bomb },
    { label: 'Backups', path: backupDir, icon: Archive },
  ];
  const parts = usage
    ? [
        { label: 'Worlds', v: usage.worlds, c: '#16a34a' },
        { label: contentDir === 'plugins' ? 'Plugins' : 'Mods', v: usage.content, c: '#a78bfa' },
        { label: 'Libraries', v: usage.libraries, c: '#64748b' },
        { label: 'Logs & crashes', v: usage.logs, c: '#facc15' },
        { label: 'Other', v: Math.max(0, usage.total - usage.worlds - usage.content - usage.libraries - usage.logs), c: '#334155' },
        { label: 'Backups', v: usage.backups, c: '#3b82f6' },
      ]
    : [];
  const total = parts.reduce((s, p) => s + p.v, 0);

  const openLog = async (f: LogFile) => {
    try {
      setViewer({ name: f.name, text: await api.readTextFile(id, f.path) });
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader title="Files & Logs" description="Where everything lives, what takes space, and every log and crash report." />

      <Section title="Location" icon={<HardDrive />} bodyClassName="divide-y divide-line">
        <FieldRow
          label="Server folder"
          description="Moving a server: stop it, move the folder in Explorer, then point this at the new place."
          error={live && draft.installDir !== inst.installDir ? 'Stop the server first.' : undefined}
        >
          <div className="flex gap-2">
            <Input mono value={draft.installDir} onChange={(e) => patch((x) => void (x.installDir = e.target.value))} />
            <Button
              disabled={live}
              onClick={async () => {
                const p = await pickFolder('Server folder', draft.installDir);
                if (p) patch((x) => void (x.installDir = p));
              }}
            >
              Browse
            </Button>
          </div>
        </FieldRow>
      </Section>

      <div className="grid grid-cols-2 gap-4">
        <Section title="Folders" icon={<FolderOpen />} bodyClassName="py-2">
          {folders.map((f) => (
            <button key={f.label} onClick={() => f.path && reveal(f.path)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-panel-2">
              <f.icon className="size-4 text-fg-4" />
              <div className="min-w-0 flex-1">
                <div className="text-[12.5px] text-fg">{f.label}</div>
                <div className="truncate font-mono text-[10.5px] text-fg-4">{f.path}</div>
              </div>
              <FolderOpen className="size-3.5 text-fg-4" />
            </button>
          ))}
        </Section>
        <Section title="Disk usage" icon={<Library />} bodyClassName="py-4 space-y-3">
          {usage == null ? (
            <div className="text-[12px] text-fg-4">Measuring…</div>
          ) : (
            <>
              <div className="flex h-2.5 overflow-hidden rounded-full bg-panel-2">
                {parts.map((s, i) => (
                  <div key={i} style={{ width: `${(s.v / Math.max(1, total)) * 100}%`, background: s.c }} className="h-full border-r-2 border-panel last:border-0" />
                ))}
              </div>
              {parts.map((r) => (
                <div key={r.label} className="flex items-center gap-2 text-[12.5px]">
                  <span className="size-2 rounded-full" style={{ background: r.c }} />
                  <span className="text-fg-3">{r.label}</span>
                  <span className="ml-auto font-mono text-fg">{bytes(r.v)}</span>
                </div>
              ))}
              <div className="flex items-center gap-2 border-t border-line pt-2 text-[12.5px]">
                <span className="text-fg-2">Total</span>
                <span className="ml-auto font-mono font-semibold text-fg">{bytes(total)}</span>
              </div>
            </>
          )}
        </Section>
      </div>

      <Section title="Logs & crash reports" icon={<ScrollText />} description="Rotated logs (.log.gz) open too." bodyClassName="py-2">
        {logs.length === 0 ? (
          <div className="py-4 text-[12px] text-fg-4">No logs yet.</div>
        ) : (
          <div className="max-h-[320px] overflow-y-auto">
            {logs.slice(0, 80).map((f) => (
              <button key={f.path} onClick={() => openLog(f)} className="flex w-full items-center gap-3 rounded-lg px-2 py-1.5 text-left hover:bg-panel-2">
                {f.kind === 'crash' ? <Bomb className="size-4 text-err" /> : <ScrollText className="size-4 text-fg-4" />}
                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-fg-2">{f.name}</span>
                {f.kind === 'crash' && <Badge tone="err">crash</Badge>}
                {f.kind === 'console' && <Badge>console</Badge>}
                <span className="w-20 text-right font-mono text-[11px] text-fg-4">{bytes(f.size)}</span>
                <span className="w-24 text-right text-[11px] text-fg-4" title={dateTime(f.modified)}>
                  {ago(f.modified)}
                </span>
              </button>
            ))}
          </div>
        )}
      </Section>

      <Section title="Share & manage" icon={<Upload />} bodyClassName="py-4 flex flex-wrap gap-2">
        <Button icon={<Upload className="size-3.5" />} onClick={() => setDialog({ kind: 'export', id })}>
          Export server pack
        </Button>
        <Button icon={<Download className="size-3.5" />} onClick={() => setDialog({ kind: 'clone', id })}>
          Duplicate server
        </Button>
        <Button variant="danger" icon={<Trash2 className="size-3.5" />} className="ml-auto" onClick={() => setDialog({ kind: 'delete', id })}>
          Delete server
        </Button>
      </Section>

      <Dialog open={!!viewer} onClose={() => setViewer(null)} title={viewer?.name ?? ''} icon={<ScrollText />} width={1000}>
        <pre className="selectable max-h-[65vh] overflow-auto rounded-xl border border-line bg-bg p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-fg-2">{viewer?.text}</pre>
      </Dialog>
    </div>
  );
}
