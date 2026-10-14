import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDownToLine, ChevronRight, CloudSun, Copy, Eraser, FileText, FolderOpen, Gauge, List, Megaphone, Save, Search, Send, Sun, X } from 'lucide-react';
import { cn } from '@/lib/cn';
import { api, errMsg } from '@/lib/ipc';
import { bytes, clock, dateTime } from '@/lib/format';
import { clearLines, lineLevel, seedLines, useLogLines, type LogLine } from '@/lib/live';
import { COMMANDS, GAMERULES, stripFormatting } from '@/lib/mc';
import { copy, reveal } from '@/lib/shell';
import type { LogFile } from '@/lib/types';
import { useApp, useInstance, useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { Input, Segmented, Select } from '@/components/ui/Field';

const ROW = 20;

type Level = 'all' | 'warn' | 'error' | 'chat';

const levelClass = { error: 'text-err', warn: 'text-warn', info: 'text-fg-2', debug: 'text-fg-4', chat: 'text-info' } as const;
const sourceClass: Record<string, string> = { manager: 'text-accent', install: 'text-diamond', rcon: 'text-violet', java: 'text-diamond', content: 'text-diamond', modpack: 'text-diamond' };

function VirtualLines({ lines, follow, onUserScroll, query }: { lines: LogLine[]; follow: boolean; onUserScroll: (atBottom: boolean) => void; query: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState(0);
  const [height, setHeight] = useState(600);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    if (follow && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines.length, follow]);
  const start = Math.max(0, Math.floor(scroll / ROW) - 20);
  const end = Math.min(lines.length, Math.ceil((scroll + height) / ROW) + 20);
  const q = query.toLowerCase();
  return (
    <div
      ref={ref}
      className="selectable relative min-h-0 flex-1 overflow-auto bg-bg/70 font-mono text-[11.5px]"
      onScroll={(e) => {
        const el = e.currentTarget;
        setScroll(el.scrollTop);
        onUserScroll(el.scrollHeight - el.scrollTop - el.clientHeight < ROW * 2);
      }}
    >
      <div style={{ height: lines.length * ROW, position: 'relative' }}>
        {lines.slice(start, end).map((l, i) => {
          const text = stripFormatting(l.text);
          const idx = q ? text.toLowerCase().indexOf(q) : -1;
          return (
            <div
              key={l.n}
              className={cn('absolute right-0 left-0 flex gap-3 px-4 whitespace-pre hover:bg-panel/60', sourceClass[l.source] ?? levelClass[lineLevel(l.text)])}
              style={{ top: (start + i) * ROW, height: ROW, lineHeight: `${ROW}px` }}
            >
              <span className="w-[54px] shrink-0 text-fg-4/70">{l.source === 'server' ? '' : l.source}</span>
              {l.source !== 'server' && <span className="-ml-2 shrink-0 text-fg-4/60">{clock(l.ts)}</span>}
              <span>
                {idx >= 0 ? (
                  <>
                    {text.slice(0, idx)}
                    <mark className="rounded-sm bg-accent/30 text-fg">{text.slice(idx, idx + q.length)}</mark>
                    {text.slice(idx + q.length)}
                  </>
                ) : (
                  text
                )}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

const HISTORY_KEY = 'mcsm.cmdHistory';

function CommandBar({ id }: { id: string }) {
  const st = useStatus(id);
  const toast = useApp((s) => s.toast);
  const [cmd, setCmd] = useState('');
  const [hist, setHist] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]');
    } catch {
      return [];
    }
  });
  const [hi, setHi] = useState(-1);
  const [sel, setSel] = useState(0);
  const enabled = !!st?.canCommand;
  const players = st?.players.map((p) => p.name) ?? [];

  const suggestions = useMemo(() => {
    const t = cmd.replace(/^\//, '');
    if (!t) return [];
    const parts = t.split(' ');
    const lastTok = parts[parts.length - 1].toLowerCase();
    let pool: string[];
    if (parts.length === 1) pool = COMMANDS;
    else if (parts[0] === 'gamerule' && parts.length === 2) pool = GAMERULES;
    else if (['time'].includes(parts[0]) && parts.length === 2) pool = ['set', 'add', 'query'];
    else if (parts[0] === 'time' && parts[1] === 'set') pool = ['day', 'noon', 'night', 'midnight'];
    else if (parts[0] === 'weather') pool = ['clear', 'rain', 'thunder'];
    else if (parts[0] === 'gamemode' || parts[0] === 'defaultgamemode') pool = parts.length === 2 ? ['survival', 'creative', 'adventure', 'spectator'] : players;
    else if (parts[0] === 'difficulty') pool = ['peaceful', 'easy', 'normal', 'hard'];
    else if (parts[0] === 'whitelist' && parts.length === 2) pool = ['add', 'remove', 'list', 'on', 'off', 'reload'];
    else pool = [...players, '@a', '@p', '@r', '@s', '@e'];
    if (!lastTok && parts.length === 1) return [];
    return pool.filter((c) => c.toLowerCase().startsWith(lastTok) && c.toLowerCase() !== lastTok).slice(0, 8);
  }, [cmd, players]);

  const complete = (s: string) => {
    const parts = cmd.replace(/^\//, '').split(' ');
    parts[parts.length - 1] = s;
    setCmd(parts.join(' ') + ' ');
    setSel(0);
  };

  const send = async (c = cmd) => {
    const text = c.trim();
    if (!text) return;
    try {
      await api.sendCommand(id, text);
      const next = [text, ...hist.filter((h) => h !== text)].slice(0, 100);
      setHist(next);
      try {
        localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
      } catch {
        /* storage unavailable */
      }
      if (c === cmd) setCmd('');
      setHi(-1);
    } catch (e) {
      toast('error', errMsg(e), id);
    }
  };

  const quick: { label: string; icon: React.ReactNode; cmd: string }[] = [
    { label: 'Save', icon: <Save />, cmd: 'save-all' },
    { label: 'List', icon: <List />, cmd: 'list' },
    { label: 'Day', icon: <Sun />, cmd: 'time set day' },
    { label: 'Clear weather', icon: <CloudSun />, cmd: 'weather clear' },
    { label: 'TPS', icon: <Gauge />, cmd: ['paper', 'purpur', 'folia'].includes(useApp.getState().instances.find((i) => i.id === id)?.installed?.software ?? '') ? 'tps' : 'tick query' },
  ];

  return (
    <div className="relative shrink-0 border-t border-line bg-bg-raised/70 px-4 py-2.5">
      {suggestions.length > 0 && (
        <div className="absolute bottom-full left-4 mb-1 w-72 rounded-xl border border-line-strong bg-panel-2 p-1 shadow-pop">
          {suggestions.map((s, i) => (
            <button
              key={s}
              onMouseDown={(e) => {
                e.preventDefault();
                complete(s);
              }}
              className={cn('flex h-7 w-full items-center rounded-lg px-2.5 text-left font-mono text-[12px]', i === sel ? 'bg-hover text-fg' : 'text-fg-2')}
            >
              {s}
              {i === sel && <span className="ml-auto text-[10px] text-fg-4">Tab</span>}
            </button>
          ))}
        </div>
      )}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <ChevronRight className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-accent" />
          <Input
            mono
            disabled={!enabled}
            value={cmd}
            placeholder={enabled ? 'Type a command – no slash needed (Tab completes, ↑ history)' : 'Start the server to send commands'}
            onChange={(e) => {
              setCmd(e.target.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Tab' && suggestions.length) {
                e.preventDefault();
                complete(suggestions[sel] ?? suggestions[0]);
              } else if (e.key === 'Enter') {
                send();
              } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                if (suggestions.length && cmd) setSel((sel - 1 + suggestions.length) % suggestions.length);
                else {
                  const n = Math.min(hi + 1, hist.length - 1);
                  if (n >= 0) {
                    setHi(n);
                    setCmd(hist[n]);
                  }
                }
              } else if (e.key === 'ArrowDown') {
                e.preventDefault();
                if (suggestions.length && cmd && hi < 0) setSel((sel + 1) % suggestions.length);
                else {
                  const n = hi - 1;
                  setHi(Math.max(-1, n));
                  setCmd(n >= 0 ? hist[n] : '');
                }
              } else if (e.key === 'Escape') setCmd('');
            }}
            className="h-9 pl-8 text-[12.5px]"
          />
        </div>
        <Button variant="primary" disabled={!enabled || !cmd.trim()} icon={<Send className="size-3.5" />} onClick={() => send()}>
          Send
        </Button>
        <Button
          disabled={!enabled}
          icon={<Megaphone className="size-3.5" />}
          onClick={() => {
            const m = window.prompt('Broadcast to everyone in-game:');
            if (m) send(`say ${m}`);
          }}
        >
          Broadcast
        </Button>
      </div>
      <div className="mt-2 flex items-center gap-1.5">
        {quick.map((q) => (
          <button
            key={q.label}
            disabled={!enabled}
            onClick={() => send(q.cmd)}
            title={q.cmd}
            className="flex h-6 items-center gap-1 rounded-md border border-line bg-panel px-2 text-[11px] text-fg-3 hover:border-line-strong hover:text-fg disabled:opacity-40 [&_svg]:size-3"
          >
            {q.icon}
            {q.label}
          </button>
        ))}
        <span className="ml-auto text-[10.5px] text-fg-4">{enabled ? (st?.pid ? 'Commands go to the server console' : 'Sent over RCON') : ''}</span>
      </div>
    </div>
  );
}

export function Console({ id }: { id: string }) {
  const inst = useInstance(id)!;
  const live = useLogLines(id);
  const [query, setQuery] = useState('');
  const [level, setLevel] = useState<Level>('all');
  const [hideManager, setHideManager] = useState(false);
  const [follow, setFollow] = useState(true);
  const [files, setFiles] = useState<LogFile[]>([]);
  const [fileView, setFileView] = useState<{ name: string; lines: LogLine[] } | null>(null);
  const toast = useApp((s) => s.toast);

  useEffect(() => {
    api.logTail(id, 1500).then((l) => seedLines(id, l)).catch(() => {});
    api.logFiles(id).then(setFiles).catch(() => {});
  }, [id]);

  const base = fileView?.lines ?? live;
  const filtered = useMemo(() => {
    const q = query.toLowerCase();
    return base.filter((l) => {
      if (hideManager && l.source !== 'server' && l.source !== 'rcon') return false;
      if (level !== 'all') {
        const lv = lineLevel(l.text);
        if (level === 'error' && lv !== 'error') return false;
        if (level === 'warn' && lv !== 'error' && lv !== 'warn') return false;
        if (level === 'chat' && lv !== 'chat') return false;
      }
      return !q || l.text.toLowerCase().includes(q);
    });
  }, [base, query, level, hideManager]);

  const counts = useMemo(() => {
    let e = 0;
    let w = 0;
    let c = 0;
    for (const l of base) {
      const lv = lineLevel(l.text);
      if (lv === 'error') e++;
      else if (lv === 'warn') w++;
      else if (lv === 'chat') c++;
    }
    return { e, w, c };
  }, [base]);

  const openFile = async (path: string) => {
    if (!path) return setFileView(null);
    try {
      const text = await api.readTextFile(id, path);
      const f = files.find((x) => x.path === path);
      setFileView({ name: f?.name ?? path, lines: text.split(/\r?\n/).map((t, i) => ({ n: i, ts: f?.modified ?? Date.now(), source: 'server', text: t })) });
      setFollow(false);
    } catch (e) {
      toast('error', errMsg(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-4 py-2.5">
        <div className="relative w-64">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-4" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter lines…" className="pl-8" />
          {query && (
            <button onClick={() => setQuery('')} className="absolute top-1/2 right-2 -translate-y-1/2 text-fg-4 hover:text-fg">
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <Segmented
          size="sm"
          value={level}
          onChange={setLevel}
          options={[
            { value: 'all', label: 'All' },
            { value: 'chat', label: <>Chat <span className="font-mono text-info">{counts.c}</span></> },
            { value: 'warn', label: <>Warnings <span className="font-mono text-warn">{counts.w + counts.e}</span></> },
            { value: 'error', label: <>Errors <span className="font-mono text-err">{counts.e}</span></> },
          ]}
        />
        <button
          onClick={() => setHideManager(!hideManager)}
          className={cn('h-6 rounded-md border px-2 text-[11px] font-medium', !hideManager ? 'border-line-strong bg-panel-2 text-fg-2' : 'border-transparent text-fg-4 line-through')}
        >
          Manager lines
        </button>
        <div className="ml-auto flex items-center gap-1">
          <Select value={fileView ? files.find((f) => f.name === fileView.name)?.path ?? '' : ''} onChange={(e) => openFile(e.target.value)} className="w-60">
            <option value="">Live console</option>
            {files.map((f) => (
              <option key={f.path} value={f.path}>
                {f.kind === 'crash' ? '💥 ' : ''}
                {f.name} · {bytes(f.size)} · {dateTime(f.modified)}
              </option>
            ))}
          </Select>
          <IconButton label="Copy visible lines" onClick={() => copy(filtered.map((l) => l.text).join('\n')).then(() => toast('success', `Copied ${filtered.length} lines`))}>
            <Copy />
          </IconButton>
          <IconButton label="Clear console" disabled={!!fileView} onClick={() => clearLines(id)}>
            <Eraser />
          </IconButton>
          <IconButton label="Open logs folder" onClick={() => reveal(`${inst.installDir}\\logs`)}>
            <FolderOpen />
          </IconButton>
          <IconButton label={follow ? 'Following new lines' : 'Follow new lines'} active={follow} onClick={() => setFollow(!follow)}>
            <ArrowDownToLine />
          </IconButton>
        </div>
      </div>
      {fileView && (
        <div className="flex items-center gap-2 border-b border-line bg-info/5 px-4 py-1.5 text-[11.5px] text-info">
          <FileText className="size-3.5" /> Viewing {fileView.name} ({fileView.lines.length.toLocaleString()} lines)
          <button className="ml-auto text-fg-3 hover:text-fg" onClick={() => setFileView(null)}>
            Back to live
          </button>
        </div>
      )}
      {filtered.length === 0 ? (
        <div className="flex flex-1 items-center justify-center bg-bg/70 text-[12.5px] text-fg-4">{base.length ? 'No lines match the filter.' : 'No output yet. Server log lines appear here as soon as it starts.'}</div>
      ) : (
        <VirtualLines lines={filtered} follow={follow} onUserScroll={(b) => b !== follow && !fileView && setFollow(b)} query={query} />
      )}
      <CommandBar id={id} />
      <div className="flex shrink-0 items-center gap-4 border-t border-line px-4 py-1.5 font-mono text-[10.5px] text-fg-4">
        <span>
          {filtered.length.toLocaleString()} / {base.length.toLocaleString()} lines
        </span>
        <span className="truncate">{inst.installDir}\.mcsm\console.log</span>
      </div>
    </div>
  );
}
