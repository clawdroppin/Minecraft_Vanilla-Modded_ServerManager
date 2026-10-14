import { useCallback, useEffect, useMemo, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { StreamLanguage, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { json } from '@codemirror/legacy-modes/mode/javascript';
import { EditorView } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { ChevronRight, FileCode2, FolderOpen, History, RefreshCw, RotateCcw, Save, Search } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { ago, bytes, dateTime } from '@/lib/format';
import { reveal } from '@/lib/shell';
import type { ConfigEntry, ConfigFile } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Input, Select } from '@/components/ui/Field';
import { Callout, Empty } from '@/components/ui/Surface';
import { confirm } from '@/components/ui/Dialog';

const theme = EditorView.theme(
  {
    '&': { backgroundColor: 'transparent', color: '#e3ece7' },
    '.cm-content': { caretColor: '#4ade80', padding: '12px 0' },
    '.cm-gutters': { backgroundColor: 'transparent', color: '#53625c', border: 'none' },
    '.cm-activeLine': { backgroundColor: 'rgb(220 252 231 / 0.03)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: '#a9b8b1' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'rgb(34 197 94 / 0.25) !important' },
    '.cm-cursor': { borderLeftColor: '#4ade80' },
    '.cm-searchMatch': { backgroundColor: 'rgb(250 204 21 / 0.25)' },
    '.cm-panels': { backgroundColor: '#161d1a', color: '#e3ece7' },
  },
  { dark: true },
);

const highlight = HighlightStyle.define([
  { tag: t.comment, color: '#53625c', fontStyle: 'italic' },
  { tag: t.heading, color: '#4ade80', fontWeight: '600' },
  { tag: [t.definition(t.propertyName), t.propertyName, t.variableName, t.attributeName], color: '#86efac' },
  { tag: t.string, color: '#d6e2dc' },
  { tag: [t.atom, t.bool, t.number], color: '#60a5fa' },
  { tag: t.keyword, color: '#fbbf24' },
  { tag: t.meta, color: '#a78bfa' },
]);

function langFor(rel: string) {
  const ext = rel.split('.').pop()?.toLowerCase();
  const mode = ext === 'yml' || ext === 'yaml' ? yaml : ext === 'toml' ? toml : ext === 'json' || ext === 'json5' || ext === 'mcmeta' ? json : properties;
  return [StreamLanguage.define(mode), syntaxHighlighting(highlight), theme, EditorView.lineWrapping];
}

/** Root files first, then folders. */
function group(entries: ConfigEntry[]) {
  const out = new Map<string, ConfigEntry[]>();
  for (const e of entries) {
    const parts = e.rel.split('/');
    const dir = parts.length === 1 ? '' : parts.slice(0, Math.min(2, parts.length - 1)).join('/');
    out.set(dir, [...(out.get(dir) ?? []), e]);
  }
  return [...out.entries()].sort((a, b) => (a[0] === '' ? -1 : b[0] === '' ? 1 : a[0].localeCompare(b[0])));
}

export function ConfigFiles({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const upsert = useApp((s) => s.upsertInstance);
  const [entries, setEntries] = useState<ConfigEntry[] | null>(null);
  const [file, setFile] = useState('server.properties');
  const [loaded, setLoaded] = useState<ConfigFile | null>(null);
  const [text, setText] = useState('');
  const [saving, setSaving] = useState(false);
  const [history, setHistory] = useState<[string, number][]>([]);
  const [q, setQ] = useState('');
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const dirty = !!loaded && text !== loaded.text;

  const loadList = useCallback(() => api.configFiles(id).then(setEntries).catch(() => setEntries([])), [id]);
  useEffect(() => {
    loadList();
  }, [loadList]);

  const open = useCallback(
    async (rel: string) => {
      try {
        const c = await api.readConfig(id, rel);
        setFile(rel);
        setLoaded(c);
        setText(c.text);
        api.configHistory(id, rel).then(setHistory).catch(() => setHistory([]));
      } catch (e) {
        toast('error', errMsg(e));
      }
    },
    [id, toast],
  );
  useEffect(() => {
    open('server.properties');
  }, [open]);

  const pick = async (rel: string) => {
    if (dirty) {
      const r = await confirm({ title: 'Discard changes?', message: `${file} has unsaved changes.`, confirm: 'Discard', danger: true });
      if (!r.ok) return;
    }
    open(rel);
  };

  const save = async () => {
    setSaving(true);
    try {
      const r = await api.writeConfig(id, file, text);
      upsert(r.instance);
      toast('success', r.restartNeeded ? `${file} saved – restart to apply` : `${file} saved`, id);
      await open(file);
    } catch (e) {
      toast('error', errMsg(e));
    } finally {
      setSaving(false);
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 's' && dirty) {
        e.preventDefault();
        save();
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  });

  const groups = useMemo(() => group((entries ?? []).filter((e) => !q || e.rel.toLowerCase().includes(q.toLowerCase()))), [entries, q]);
  const ext = useMemo(() => langFor(file), [file]);

  return (
    <div className="flex h-full">
      <aside className="flex w-[280px] shrink-0 flex-col border-r border-line bg-bg-raised/40">
        <div className="flex items-center gap-1.5 border-b border-line p-2.5">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
            <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder={`${entries?.length ?? 0} config files`} className="pl-8" />
          </div>
          <IconButton label="Rescan" onClick={loadList}>
            <RefreshCw />
          </IconButton>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {groups.map(([dir, files]) => (
            <div key={dir || 'root'} className="mb-1">
              {dir && (
                <button
                  onClick={() => {
                    const n = new Set(closed);
                    if (n.has(dir)) n.delete(dir);
                    else n.add(dir);
                    setClosed(n);
                  }}
                  className="flex h-7 w-full items-center gap-1 px-1.5 text-left text-[11px] font-semibold tracking-wide text-fg-4 uppercase hover:text-fg-3"
                >
                  <ChevronRight className={cn('size-3 transition-transform', !closed.has(dir) && 'rotate-90')} />
                  {dir}
                </button>
              )}
              {!closed.has(dir) &&
                files.map((f) => {
                  const name = dir ? f.rel.slice(dir.length + 1) : f.rel;
                  return (
                    <button
                      key={f.rel}
                      onClick={() => pick(f.rel)}
                      title={`${f.rel} · ${bytes(f.size)} · ${dateTime(f.modified)}`}
                      className={cn('flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[12px]', file === f.rel ? 'bg-hover text-fg' : 'text-fg-3 hover:bg-panel-2 hover:text-fg-2', dir && 'pl-5')}
                    >
                      <FileCode2 className={cn('size-3.5 shrink-0', file === f.rel ? 'text-accent' : 'text-fg-4')} />
                      <span className="truncate">{name}</span>
                    </button>
                  );
                })}
            </div>
          ))}
          {entries && entries.length === 0 && <Empty title="No config files yet" description="They appear after the server’s first start." />}
        </div>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-2 border-b border-line px-4 py-2.5">
          <div className="min-w-0 flex-1">
            <div className="truncate font-mono text-[12.5px] text-fg">{file}</div>
            <div className="text-[11px] text-fg-4">{loaded?.exists ? `modified ${ago(loaded.modified)}` : 'new file – created on save'}</div>
          </div>
          {history.length > 0 && (
            <Select
              value=""
              onChange={async (e) => {
                if (!e.target.value) return;
                setText(await api.readTextFile(id, e.target.value));
              }}
              className="w-56"
            >
              <option value="">Restore a previous version…</option>
              {history.map(([p, ts]) => (
                <option key={p} value={p}>
                  {dateTime(ts)}
                </option>
              ))}
            </Select>
          )}
          {history.length > 0 && <History className="size-4 text-fg-4" />}
          <IconButton label="Open folder" onClick={() => reveal(`${inst.installDir}\\${file.includes('/') ? file.slice(0, file.lastIndexOf('/')).replace(/\//g, '\\') : ''}`)}>
            <FolderOpen />
          </IconButton>
          <Button variant="ghost" size="sm" icon={<RotateCcw className="size-3.5" />} disabled={!dirty} onClick={() => loaded && setText(loaded.text)}>
            Revert
          </Button>
          <Button variant="primary" size="sm" icon={<Save className="size-3.5" />} disabled={!dirty} loading={saving} onClick={save}>
            Save
          </Button>
        </div>
        {st?.state === 'running' && (
          <Callout tone="info" className="m-3 mb-0">
            Most configs are read at start – save now, restart to apply. Player lists (whitelist.json, ops.json…) are owned by the running server: change them on the Players page instead.
          </Callout>
        )}
        <div className="min-h-0 flex-1 overflow-hidden">
          <CodeMirror value={text} onChange={setText} extensions={ext} theme="none" height="100%" basicSetup={{ foldGutter: false, highlightActiveLine: true }} />
        </div>
      </div>
    </div>
  );
}
